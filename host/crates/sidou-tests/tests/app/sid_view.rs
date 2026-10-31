// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::{c_char, c_void, CStr};
use std::path::{Path, PathBuf};

use sidou_sys::*;

use super::sid_io::put;
use super::store::{fresh, Rig};
use crate::views::{ascii_or_hash, Ui};
use crate::{c_text, drain, pixels};

const COLS: usize = GRID_COLS as usize;
const GLO_MUTE_VOICE_2_KEY: u32 = 1 << 1;
const GLO_SOLO_VOICE_3_KEY: u32 = 1 << 5;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TuneCall {
    Play(String),
    Start(u32),
    Stop,
    Pause,
    Resume,
    End,
    Mute(u32),
    Lift(u32),
    Import(i32),
}

pub(crate) struct Tune {
    pub(crate) calls: Vec<TuneCall>,
    pub(crate) st: sid_status,
    pub(crate) h: Box<psid_header>,
    pub(crate) refuse: bool,
    pub(crate) loading: bool,
}

fn tune(ctx: *mut c_void) -> &'static mut Tune {
    unsafe { &mut *ctx.cast::<Tune>() }
}

unsafe extern "C" fn t_play(ctx: *mut c_void, name: *const c_char) -> i32 {
    let t = tune(ctx);
    if t.refuse {
        return 0;
    }
    let n = c_text(name);
    t.calls.push(TuneCall::Play(n.clone()));
    t.st.name = [0; STORE_NAME_MAX as usize + 1];
    for (i, b) in n.bytes().enumerate() {
        t.st.name[i] = b as c_char;
    }
    t.st.state = if t.loading {
        SID_T_LOADING
    } else {
        SID_T_PLAYING
    } as u8;
    t.st.h = &*t.h;
    t.st.song = t.h.start;
    t.st.chips = if t.h.sid2 != 0 { 2 } else { 1 };
    t.st.seconds = 0;
    1
}
unsafe extern "C" fn t_start(ctx: *mut c_void, song: u32) -> i32 {
    let t = tune(ctx);
    t.calls.push(TuneCall::Start(song));
    t.st.song = song as u16;
    t.st.state = SID_T_PLAYING as u8;
    t.st.seconds = 0;
    1
}
unsafe extern "C" fn t_stop(ctx: *mut c_void) -> i32 {
    let t = tune(ctx);
    t.calls.push(TuneCall::Stop);
    t.st.state = SID_T_STOPPED as u8;
    1
}
unsafe extern "C" fn t_pause(ctx: *mut c_void) -> i32 {
    let t = tune(ctx);
    t.calls.push(TuneCall::Pause);
    t.st.state = SID_T_PAUSED as u8;
    1
}
unsafe extern "C" fn t_resume(ctx: *mut c_void) -> i32 {
    let t = tune(ctx);
    t.calls.push(TuneCall::Resume);
    t.st.state = SID_T_PLAYING as u8;
    1
}
unsafe extern "C" fn t_end(ctx: *mut c_void) -> i32 {
    let t = tune(ctx);
    t.calls.push(TuneCall::End);
    t.st.state = SID_T_NONE as u8;
    t.st.h = std::ptr::null();
    t.st.name = [0; STORE_NAME_MAX as usize + 1];
    1
}
unsafe extern "C" fn t_mute(ctx: *mut c_void, mask: u32) -> i32 {
    tune(ctx).calls.push(TuneCall::Mute(mask));
    1
}
unsafe extern "C" fn t_status(ctx: *mut c_void, out: *mut sid_status) {
    unsafe { *out = tune(ctx).st };
}
unsafe extern "C" fn t_lift(ctx: *mut c_void, voice: u32) -> i32 {
    let t = tune(ctx);
    t.calls.push(TuneCall::Lift(voice));
    t.st.lift = if voice < 3 { SID_LIFT_ARMED as u8 } else { 0 };
    t.st.lift_voice = if voice < 3 { voice as u8 } else { 0 };
    1
}
unsafe extern "C" fn t_import(ctx: *mut c_void, go: i32) -> i32 {
    let t = tune(ctx);
    t.calls.push(TuneCall::Import(go));
    t.st.offer = if go != 0 {
        SID_OFFER_IMPORTING as u8
    } else {
        SID_OFFER_NONE as u8
    };
    1
}

pub(crate) fn offer(t: &mut Tune, name: &str, instr: u8) {
    t.st.offer = SID_OFFER_READY as u8;
    t.st.offer_instr = instr;
    text(&mut t.st.offer_name, name);
}

fn text(field: &mut [c_char], s: &str) {
    field.fill(0);
    for (i, b) in s.bytes().take(field.len() - 1).enumerate() {
        field[i] = b as c_char;
    }
}

pub(crate) fn pal_6581_header(
    title: &str,
    author: &str,
    released: &str,
    songs: u16,
) -> psid_header {
    let mut h = psid_header {
        version: 2,
        load: 0x1000,
        init: 0x1000,
        play: 0x1003,
        songs,
        start: 1,
        clock: 1,
        model1: 1,
        model2: 1,
        sids: 1,
        ..Default::default()
    };
    text(&mut h.name, title);
    text(&mut h.author, author);
    text(&mut h.released, released);
    h
}

pub(crate) struct Bound {
    _t: Box<sid_transport>,
    pub(crate) rig: Rig,
}

pub(crate) fn bind(ui: &mut Ui, names: &[&str]) -> (Box<Tune>, Bound) {
    let mut rig = fresh(1103, STORE_WINDOW_MAX);
    for n in names {
        put(&mut rig, n, STORE_FILE_SID, &[0x50, 0x53, 0x49, 0x44]);
    }
    let mut tune = Box::new(Tune {
        calls: Vec::new(),
        st: sid_status::default(),
        h: Box::new(pal_6581_header(
            "NIGHT DRIVE ON THE MOTORWAY",
            "A. Composer",
            "2026 SIDOU",
            3,
        )),
        refuse: false,
        loading: false,
    });
    let ctx: *mut Tune = &mut *tune;
    let t = Box::new(sid_transport {
        play: Some(t_play),
        start: Some(t_start),
        stop: Some(t_stop),
        pause: Some(t_pause),
        resume: Some(t_resume),
        end: Some(t_end),
        mute: Some(t_mute),
        status: Some(t_status),
        lift: Some(t_lift),
        import: Some(t_import),
        ctx: ctx.cast(),
    });
    unsafe { views_set_tune(&mut *ui.v, &*t) };
    ui.v.sids.st = &*rig.st;
    (tune, Bound { _t: t, rig })
}

pub(crate) struct Sids {
    pub(crate) ui: Ui,
    pub(crate) tune: Box<Tune>,
    _bound: Bound,
    pub(crate) io: Box<song_io>,
    _ctl: Box<song_ctl>,
    pub(crate) model: Box<gt2_song>,
}

impl Sids {
    pub(crate) fn new(names: &[&str]) -> Sids {
        let ui = Ui::without_cards();
        Sids::with(ui, names)
    }

    pub(crate) fn with(mut ui: Ui, names: &[&str]) -> Sids {
        let (tune, mut b) = bind(&mut ui, names);
        let mut model = Box::<gt2_song>::default();
        let mut ctl = Box::new(song_ctl {
            song: &mut *model,
            ..Default::default()
        });
        let mut io = Box::new(song_io {
            ctl: &mut *ctl,
            st: &mut *b.rig.st,
            ..Default::default()
        });
        unsafe { views_song_io(&mut *ui.v, &mut *io) };
        Sids {
            ui,
            tune,
            _bound: b,
            io,
            _ctl: ctl,
            model,
        }
    }

    pub(crate) fn calls(&mut self) -> Vec<TuneCall> {
        std::mem::take(&mut self.tune.calls)
    }
}

pub(crate) fn settle(ui: &mut Ui) -> usize {
    let mut most = 0;
    for _ in 0..200 {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(
            sent.cells <= SONG_VIEW_FRAME_CELLS as usize,
            "a frame of {} sent {} cells (animating {}, scanning {}, page {})",
            ui.top(),
            sent.cells,
            unsafe { screen_animating(&ui.v.s) },
            ui.v.sids.scanning,
            ui.v.sids.page
        );
        most = most.max(sent.cells);
        if unsafe { screen_animating(&ui.v.s) } == 0
            && ui.v.song.dirty == 0
            && ui.v.sids.scanning == 0
            && sent.cells == 0
        {
            return most;
        }
    }
    panic!("the screen kept changing");
}

pub(crate) fn open_sids(ui: &mut Ui) {
    if ui.top() != "MENU" {
        ui.press(PANEL_BTN_HOME);
        ui.settle();
    }
    ui.choose_menu_item(VIEWS_MENU_SIDS);
    settle(ui);
    assert_eq!(ui.top(), "SIDS");
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/golden/lcd")
}

const W: usize = COLS * 8;

fn write_png(path: &Path, idx: &[u8]) {
    let file =
        std::fs::File::create(path).unwrap_or_else(|e| panic!("create {}: {e}", path.display()));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W as u32, W as u32);
    enc.set_color(png::ColorType::Indexed);
    enc.set_depth(png::BitDepth::Four);
    enc.set_palette(sidou_tests::logo::PALETTE.concat());
    let packed: Vec<u8> = idx.chunks(2).map(|p| p[0] << 4 | p[1]).collect();
    let mut w = enc.write_header().expect("PNG header");
    w.write_image_data(&packed).expect("PNG data");
    w.finish().expect("PNG end");
}

fn read_png(path: &Path) -> Vec<u8> {
    let file = std::fs::File::open(path).unwrap_or_else(|e| {
        panic!(
            "open {}: {e}; create it with SIDOU_LCD_GOLDEN_UPDATE=1",
            path.display()
        )
    });
    let mut dec = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .expect("PNG header");
    let info = dec.info();
    assert_eq!((info.width, info.height), (W as u32, W as u32));
    assert_eq!(info.color_type, png::ColorType::Indexed);
    assert_eq!(info.bit_depth, png::BitDepth::Four);
    let mut buf = vec![0; dec.output_buffer_size().expect("PNG size")];
    dec.next_frame(&mut buf).expect("PNG data");
    buf.iter().flat_map(|&b| [b >> 4, b & 15]).collect()
}

pub(crate) fn golden(ui: &Ui, name: &str) {
    let got = pixels(&ui.g);
    let path = golden_dir().join(format!("{name}.png"));
    if std::env::var("SIDOU_LCD_GOLDEN_UPDATE").as_deref() == Ok("1") {
        write_png(&path, &got);
        println!("wrote {}", path.display());
        return;
    }
    let want = read_png(&path);
    let diff = (0..got.len()).filter(|&i| got[i] != want[i]).count();
    if diff != 0 {
        let actual = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.actual.png"));
        write_png(&actual, &got);
        panic!(
            "{name}: {diff} pixels differ from {}; the screen is in {}",
            path.display(),
            actual.display()
        );
    }
}

fn names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("TUNE {:02}", i + 1)).collect()
}

#[test]
fn the_browser_lists_the_stores_tunes_eight_a_page() {
    let mut s = Sids::new(&[]);
    open_sids(&mut s.ui);
    assert_eq!(s.ui.big(6, 2, 2, 9), "NO TUNES:");
    assert_eq!(s.ui.big(8, 2, 2, 10), "SEE MANUAL");
    golden(&s.ui, "sids-empty");
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!((s.ui.top().as_str(), s.calls()), ("SIDS", vec![]));

    let mut s = Sids::new(&["PAPER LANTERNS 2"]);
    open_sids(&mut s.ui);
    assert_eq!(s.ui.big(6, 2, 2, 13), "PAPER LANTERN");
    assert_eq!(
        s.ui.g.src[6 * COLS + 28],
        FONT_ARROW_RIGHT as u8,
        "a long name's arrow"
    );
    golden(&s.ui, "sids-one");

    let all = names(11);
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    let mut s = Sids::new(&refs);
    open_sids(&mut s.ui);
    for (i, n) in all.iter().take(8).enumerate() {
        assert_eq!(s.ui.big(6 + 2 * i, 2, 2, n.len()), *n);
    }
    assert_eq!(s.ui.big(22, 2, 2, 6), "PAGE 1");
    golden(&s.ui, "sids-full");
    s.ui.turn(PANEL_ENC_SELECT, 8);
    settle(&mut s.ui);
    assert_eq!(s.ui.v.sids.page, 1);
    for (i, n) in all.iter().skip(8).enumerate() {
        assert_eq!(s.ui.big(6 + 2 * i, 2, 2, n.len()), *n);
    }
    assert_eq!(s.ui.big(22, 2, 2, 6), "PAGE 2");
    golden(&s.ui, "sids-scrolled");
    s.ui.turn(PANEL_ENC_SELECT, -1);
    settle(&mut s.ui);
    assert_eq!((s.ui.v.sids.page, s.ui.v.sids.cursor), (0, 7));
}

#[test]
fn choosing_a_tune_plays_it_and_opens_now_playing() {
    let mut s = Sids::new(&["ALPHA", "BETA", "GAMMA"]);
    open_sids(&mut s.ui);
    s.ui.turn(PANEL_ENC_SELECT, 1);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.calls(), vec![TuneCall::Play("BETA".into())]);
    assert_eq!(s.ui.top(), "NOW PLAYING");
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "SIDS");
    assert_eq!(s.ui.v.sids.cursor, 1, "its place kept");
    assert_eq!(s.ui.g.src[8 * COLS], FONT_PLAY as u8, "the play mark");
    s.ui.press(SAVE_KEY_BUTTON);
    settle(&mut s.ui);
    assert_eq!((s.ui.top().as_str(), s.calls()), ("NOW PLAYING", vec![]));
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.calls(), vec![TuneCall::Stop]);
    assert_eq!(s.ui.g.src[8 * COLS], FONT_SQUARE as u8, "stopped: a square");
    s.io.dirty = 1;
    s.ui.turn(PANEL_ENC_SELECT, 1);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.calls(), vec![TuneCall::Play("GAMMA".into())]);
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.calls(), vec![TuneCall::Stop]);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(
        (s.ui.top().as_str(), s.calls()),
        ("MENU", vec![TuneCall::End])
    );
    s.io.dirty = 0;
    open_sids(&mut s.ui);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert!(matches!(s.calls()[..], [TuneCall::Play(_)]));
    s.ui.press(PANEL_BTN_HOME);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!((s.ui.top().as_str(), s.calls()), ("MENU", vec![]));
    s.tune.st.state = SID_T_NONE as u8;
    s.tune.refuse = true;
    s.io.dirty = 0;
    open_sids(&mut s.ui);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "SIDS");
}

#[test]
fn an_unsaved_song_is_asked_about_before_a_tune_takes_its_memory() {
    let mut s = Sids::new(&["ALPHA"]);
    s.io.dirty = 1;
    for (i, b) in b"NIGHT".iter().enumerate() {
        s.io.name[i] = *b as c_char;
    }
    open_sids(&mut s.ui);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.big(6, 2, 2, 14), "SONG NOT SAVED");
    assert_eq!(s.ui.big(8, 2, 2, 5), "NIGHT");
    assert!(s.calls().is_empty());
    golden(&s.ui, "sids-unsaved");
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!((s.ui.top().as_str(), s.ui.v.sids.asking), ("SIDS", 0));
    assert_eq!(s.ui.big(6, 2, 2, 5), "ALPHA");
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.calls(), vec![TuneCall::Play("ALPHA".into())]);
    assert_eq!(s.ui.top(), "NOW PLAYING");

    let mut s = Sids::new(&["ALPHA"]);
    s.io.dirty = 1;
    for (i, b) in b"NIGHT".iter().enumerate() {
        s.io.name[i] = *b as c_char;
    }
    open_sids(&mut s.ui);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.press(SAVE_KEY_BUTTON);
    settle(&mut s.ui);
    assert_eq!(s.io.state, SONG_IO_SAVE_WAIT as u8);
    assert_eq!(c_text(s.io.target.as_ptr()), "NIGHT");
    assert!(s.calls().is_empty(), "not before it is saved");
    assert_eq!(s.ui.big(4, 2, 2, 6), "SAVING");
    s.io.state = SONG_IO_IDLE as u8;
    s.model.frozen = 0;
    settle(&mut s.ui);
    assert_eq!((s.ui.v.sids.asking, s.ui.v.sids.saving), (1, 0));
    assert!(s.calls().is_empty());
    s.ui.press(SAVE_KEY_BUTTON);
    settle(&mut s.ui);
    s.io.state = SONG_IO_IDLE as u8;
    s.model.frozen = 0;
    s.io.dirty = 0;
    settle(&mut s.ui);
    assert_eq!(s.calls(), vec![TuneCall::Play("ALPHA".into())]);
    assert_eq!(s.ui.top(), "NOW PLAYING");

    let mut s = Sids::new(&["ALPHA"]);
    s.io.dirty = 1;
    open_sids(&mut s.ui);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.big(8, 2, 2, 8), "NEW SONG");
    s.ui.press(SAVE_KEY_BUTTON);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "SONG");
    assert_eq!(s.ui.big(2, 2, 2, 7), "SAVE AS");
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!((s.ui.top().as_str(), s.ui.v.sids.asking), ("SIDS", 1));
    assert!(s.calls().is_empty());
    s.ui.press(SAVE_KEY_BUTTON);
    settle(&mut s.ui);
    s.ui.press(SAVE_KEY_BUTTON);
    settle(&mut s.ui);
    assert_eq!(s.io.state, SONG_IO_SAVE_WAIT as u8);
    assert_eq!(s.ui.top(), "SIDS");
    s.io.state = SONG_IO_IDLE as u8;
    s.model.frozen = 0;
    s.io.dirty = 0;
    settle(&mut s.ui);
    assert_eq!(s.calls(), vec![TuneCall::Play("ALPHA".into())]);
    assert_eq!(s.ui.top(), "NOW PLAYING");
}

fn now_playing(s: &mut Sids) {
    open_sids(&mut s.ui);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "NOW PLAYING");
    s.calls();
}

#[test]
fn now_playing_shows_the_tune_and_its_state() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    now_playing(&mut s);
    s.tune.st.seconds = 83;
    s.tune.st.song = 2;
    s.tune.st.env = [200, 90, 0];
    settle(&mut s.ui);
    assert_eq!(s.ui.big(1, 2, 2, 14), "NIGHT DRIVE ON");
    assert_eq!(s.ui.big(3, 2, 2, 12), "THE MOTORWAY");
    assert_eq!(s.ui.small(6, 2, 11), "A. Composer");
    assert_eq!(s.ui.small(7, 2, 10), "2026 SIDOU");
    assert_eq!(s.ui.big(9, 2, 2, 8), "SONG 2/3");
    assert_eq!(s.ui.big(12, 2, 2, 4), "1:23");
    assert_eq!(s.ui.big(12, 16, 2, 7), "PLAYING");
    assert_eq!(s.ui.small(15, 2, 3), "PAL");
    assert_eq!(s.ui.small(15, 8, 4), "6581");
    golden(&s.ui, "playing-pal");

    s.tune.h.clock = 2;
    s.tune.st.model = [SID_MODEL_8580 as u8, 0];
    s.tune.st.state = SID_T_PAUSED as u8;
    settle(&mut s.ui);
    assert_eq!(s.ui.small(15, 2, 4), "NTSC");
    assert_eq!(s.ui.small(15, 8, 4), "8580");
    assert_eq!(s.ui.big(12, 18, 2, 6), "PAUSED");
    golden(&s.ui, "playing-ntsc");

    s.tune.h.clock = 1;
    s.tune.h.sid2 = 0xD420;
    s.tune.st.chips = 2;
    s.tune.st.model = [SID_MODEL_6581 as u8, SID_MODEL_8580 as u8];
    s.tune.st.state = SID_T_PLAYING as u8;
    settle(&mut s.ui);
    assert_eq!(s.ui.small(15, 13, 4), "8580");
    assert_eq!(s.ui.small(15, 18, 4), "2SID");
    golden(&s.ui, "playing-2sid");

    s.tune.h.sid3 = 0xD440;
    settle(&mut s.ui);
    assert_eq!(s.ui.small(16, 2, 16), "3SID: 3RD SILENT");
    golden(&s.ui, "playing-3sid");

    s.tune.h.sid2 = 0;
    s.tune.h.sid3 = 0;
    s.tune.st.chips = 1;
    s.tune.st.overruns = 12;
    s.tune.st.late = 34_567;
    s.tune.st.state = SID_T_STOPPED as u8;
    settle(&mut s.ui);
    assert_eq!(s.ui.small(25, 2, 11), "OVERRUNS 12");
    assert_eq!(s.ui.small(25, 17, 9), "LATE 9999");
    assert_eq!(s.ui.big(12, 16, 2, 7), "STOPPED");
    golden(&s.ui, "playing-counts");

    s.tune.st.overruns = 0;
    s.tune.st.late = 0;
    s.tune.st.state = SID_T_PLAYING as u8;
    s.ui.v.song.mute = 2;
    s.ui.v.song.solo = 4;
    settle(&mut s.ui);
    assert_eq!(s.ui.big(20, 6, 2, 3), "MUT");
    assert_eq!(s.ui.big(22, 6, 2, 3), "SOL");
    golden(&s.ui, "playing-mutes");
}

#[test]
fn now_playing_composes_what_moves_within_the_frame_budget() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    now_playing(&mut s);
    settle(&mut s.ui);
    s.ui.frame();
    assert_eq!(drain(&mut s.ui.g).cells, 0, "idle: nothing");
    let mut most = 0;
    for f in 0..200u32 {
        s.tune.st.seconds = f / 25;
        for v in 0..3 {
            s.tune.st.env[v] = ((f * 37 + v as u32 * 90) % 256) as u8;
        }
        s.ui.frame();
        let sent = drain(&mut s.ui.g);
        most = most.max(sent.cells);
    }
    println!("NOW PLAYING, moving: at most {most} cells a frame");
    assert!(most <= SONG_VIEW_FRAME_CELLS as usize);
    assert!(most > 0);
}

#[test]
fn algorithm_steps_and_restarts_the_song_on_now_playing_within_the_headers_songs() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    now_playing(&mut s);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.turn(PANEL_ENC_ALGORITHM, 5);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.turn(PANEL_ENC_ALGORITHM, -9);
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    s.ui.turn(PANEL_ENC_SELECT, 1);
    assert_eq!(
        s.calls(),
        vec![TuneCall::Start(2), TuneCall::Start(3), TuneCall::Start(1)]
    );
}

#[test]
fn play_and_edit_play_drive_the_tune() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    now_playing(&mut s);
    s.ui.down(PANEL_BTN_EDIT);
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.up(PANEL_BTN_EDIT);
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.down(PANEL_BTN_EDIT);
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.up(PANEL_BTN_EDIT);
    assert_eq!(
        s.calls(),
        vec![
            TuneCall::Pause,
            TuneCall::Resume,
            TuneCall::Stop,
            TuneCall::Start(1),
            TuneCall::Pause,
            TuneCall::Resume,
        ]
    );
    s.ui.press(PANEL_BTN_HOME);
    s.ui.press(PANEL_BTN_HOME);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!((s.ui.top().as_str(), s.calls()), ("LIVE", vec![]));
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.press(PANEL_BTN_PLAY);
    assert_eq!(s.calls(), vec![TuneCall::Stop, TuneCall::End]);
    s.ui.press(PANEL_BTN_PLAY);
    assert!(s.calls().is_empty());
}

#[test]
fn glo_mutes_and_solos_reach_the_tune() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    now_playing(&mut s);
    unsafe {
        views_set_song(&mut *s.ui.v, std::ptr::null());
        views_tune(&mut *s.ui.v, 1);
    }
    assert_eq!(s.calls(), vec![TuneCall::Mute(0)], "GLO's mask to the tune");
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(GLO_MUTE_VOICE_2_KEY, GLO_MUTE_VOICE_2_KEY);
    s.ui.note_keys(0, 0);
    s.ui.note_keys(GLO_SOLO_VOICE_3_KEY, GLO_SOLO_VOICE_3_KEY);
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    assert_eq!(s.calls(), vec![TuneCall::Mute(2), TuneCall::Mute(3)]);
    unsafe { views_tune(&mut *s.ui.v, 0) };
    unsafe { views_tune(&mut *s.ui.v, 1) };
    assert_eq!(s.calls(), vec![TuneCall::Mute(3)], "again, to the new tune");
}

#[test]
fn the_song_views_show_sid_playing_while_a_tune_has_the_song() {
    let mut song = crate::song_view::Song::new();
    song.ui.press(PANEL_BTN_SEQ);
    crate::song_view::settle(&mut song.ui);
    assert_eq!(song.ui.top(), "STEP");
    let mut s = Sids::with(
        std::mem::replace(&mut song.ui, Ui::without_cards()),
        &["NIGHT"],
    );
    s.tune.st.state = SID_T_PLAYING as u8;
    unsafe {
        views_set_song(&mut *s.ui.v, std::ptr::null());
        views_tune(&mut *s.ui.v, 1);
    }
    settle(&mut s.ui);
    assert_eq!(s.ui.big(12, 4, 2, 11), "SID PLAYING");
    assert_eq!(s.ui.big(15, 4, 2, 12), "STOP TO EDIT");
    golden(&s.ui, "song-sid-playing");
    assert_eq!(s.ui.top(), "STEP");
    assert_eq!(s.ui.v.song.away, 1);
    s.ui.turn(PANEL_ENC_SELECT, 1);
    assert_eq!(s.ui.toast().as_deref(), Some("SID PLAYING: STOP TO EDIT"));
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    s.ui.press(PANEL_BTN_SEQ);
    assert_eq!(s.ui.toast().as_deref(), Some("SID PLAYING: STOP TO EDIT"));
    settle(&mut s.ui);
    assert_ne!(s.ui.top(), "STEP");
    s.tune.st.state = SID_T_STOPPED as u8;
    s.ui.press(PANEL_BTN_SEQ);
    assert_eq!(
        s.calls(),
        vec![TuneCall::Mute(0), TuneCall::End],
        "GLO's mask, then the end"
    );
    drop(song);
}

#[test]
fn status_shows_the_6502s_rows_while_a_tune_is_loaded() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    s.ui.press(PANEL_BTN_HOME);
    s.ui.settle();
    s.ui.choose_menu_item(VIEWS_MENU_STATUS);
    s.ui.settle();
    assert_eq!(s.ui.top(), "STATUS");
    assert_eq!(s.ui.big(5, 2, 2, 6), "CUTOFF");
    s.tune.st.state = SID_T_PLAYING as u8;
    s.tune.st.cycles = 985_248;
    s.tune.st.overruns = 3;
    s.tune.st.shadow = 1;
    s.tune.st.chips = 1;
    s.ui.settle();
    assert_eq!(s.ui.big(8, 24, 2, 2), "--", "no meter reading yet");
    assert_eq!(s.ui.big(11, 24, 2, 2), "--");
    s.tune.st.load = 32;
    s.tune.st.ticks_k = 412;
    s.ui.settle();
    assert_eq!(s.ui.big(2, 22, 2, 3), "1/3");
    assert_eq!(s.ui.big(5, 2, 2, 4), "6502");
    assert_eq!(s.ui.big(5, 14, 2, 7), "PLAYING");
    assert_eq!(s.ui.big(8, 2, 2, 3), "CPU");
    assert_eq!(s.ui.big(8, 20, 2, 4), "3.2%");
    assert_eq!(s.ui.big(11, 2, 2, 4), "T/KC");
    assert_eq!(s.ui.big(11, 22, 2, 3), "412");
    assert_eq!(s.ui.big(14, 2, 2, 4), "LATE");
    assert_eq!(s.ui.big(14, 26, 2, 1), "0");
    assert_eq!(s.ui.big(17, 2, 2, 8), "OVERRUNS");
    assert_eq!(s.ui.big(17, 26, 2, 1), "3");
    assert_eq!(s.ui.big(20, 2, 2, 5), "CYC/S");
    assert_eq!(s.ui.big(20, 16, 2, 6), "985248");
    assert_eq!(s.ui.big(23, 2, 2, 6), "SHADOW");
    assert_eq!(s.ui.big(23, 24, 2, 2), "ON");
    assert_eq!(s.ui.big(26, 2, 2, 7), "3RD SID");
    assert_eq!(s.ui.big(26, 26, 2, 1), "0");
    golden(&s.ui, "status-tune");
    drain(&mut s.ui.g);
    s.tune.st.cycles = 985_249;
    s.ui.frame();
    let sent = drain(&mut s.ui.g);
    assert!(
        sent.cells > 0 && sent.cells <= 8 * 28,
        "{} cells",
        sent.cells
    );
    s.tune.st.state = SID_T_NONE as u8;
    s.ui.settle();
    assert_eq!(s.ui.big(5, 2, 2, 6), "CUTOFF");
    s.tune.st.state = SID_T_PLAYING as u8;
    s.ui.settle();
    s.ui.turn(PANEL_ENC_SELECT, 1);
    s.ui.settle();
    assert_eq!(s.ui.big(5, 2, 2, 4), "CHIP");
    assert_eq!(s.ui.big(17, 2, 2, 5), "NOTES");
    s.ui.turn(PANEL_ENC_SELECT, -1);
    s.ui.settle();
    assert_eq!(s.ui.big(5, 2, 2, 4), "6502");
}

#[test]
fn sid_model_is_a_persisted_setting_of_auto_or_keys() {
    let mut ui = Ui::without_cards();
    let mut set = Box::<settings>::default();
    set.l = &mut *ui.rig.live;
    unsafe { views_settings(&mut *ui.v, &mut *set) };
    crate::views::open_setting(&mut ui, VIEWS_SETTINGS_SID_MODEL);
    let line = |ui: &Ui| ui.menu_line(VIEWS_SETTINGS_SID_MODEL as usize);
    assert_eq!(line(&ui), "SID MODEL AUTO");
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    assert_eq!(line(&ui), "SID MODEL KEYS");
    assert_eq!(u32::from(set.sid_model), SETTINGS_SID_KEYS);
    assert_ne!(ui.live().settings_dirty, 0);
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    ui.settle();
    assert_eq!(line(&ui), "SID MODEL AUTO");
    ui.turn(PANEL_ENC_SELECT, 9);
    ui.settle();
    let heading = ascii_or_hash(ui.big(1, 1, 2, 14));
    assert_eq!(heading, "SETTINGS#10/10");
    unsafe { views_settings(&mut *ui.v, std::ptr::null_mut()) };
}

#[test]
fn the_sid_views_help_cards_match_their_goldens() {
    for (card, algorithm_turns) in [("SIDS", 0), ("PLAYING", 2), ("UNSAVED", 1), ("LIFT", 0)] {
        let mut s = Sids::new(&["NIGHT DRIVE"]);
        s.io.dirty = 1;
        open_sids(&mut s.ui);
        for _ in 0..algorithm_turns {
            s.ui.turn(PANEL_ENC_ALGORITHM, 1);
        }
        if card == "LIFT" {
            offer(&mut s.tune, "NIGHT DRIVE V2", 5);
        }
        settle(&mut s.ui);
        s.ui.down(PANEL_BTN_HOME);
        for _ in 0..20 {
            unsafe { views_flow(&mut *s.ui.v, s.ui.now + 700) };
            settle(&mut s.ui);
        }
        assert_eq!(s.ui.top(), "HELP");
        let name = unsafe { CStr::from_ptr(help_cards[s.ui.v.help.card as usize].name) };
        assert_eq!(name.to_str().unwrap(), card);
        golden(&s.ui, &format!("help-{}", card.to_lowercase()));
    }
}

#[test]
fn rec_on_now_playing_lifts_the_soloed_voice() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    now_playing(&mut s);
    s.calls();
    s.ui.press(PANEL_BTN_REC);
    assert_eq!(s.ui.toast().as_deref(), Some("LIFT: SOLO ONE VOICE"));
    s.ui.v.song.solo = 6;
    s.ui.press(PANEL_BTN_REC);
    assert_eq!(s.ui.toast().as_deref(), Some("LIFT: SOLO ONE VOICE"));
    assert!(s.calls().is_empty());
    s.ui.v.song.solo = 2;
    s.ui.press(PANEL_BTN_REC);
    assert_eq!(s.ui.toast().as_deref(), Some("LIFT: V2'S NEXT NOTE"));
    assert_eq!(s.calls(), vec![TuneCall::Lift(1)]);
    while s.ui.toast().is_some() {
        s.ui.frame();
    }
    settle(&mut s.ui);
    assert_eq!(s.ui.big(20, 6, 2, 3), "ARM");
    golden(&s.ui, "playing-lift-armed");
    s.tune.st.lift = SID_LIFT_TAKING as u8;
    settle(&mut s.ui);
    assert_eq!(s.ui.big(20, 6, 2, 3), "REC");
    s.ui.press(PANEL_BTN_REC);
    assert_eq!(s.ui.toast().as_deref(), Some("LIFT OFF"));
    assert_eq!(s.calls(), vec![TuneCall::Lift(3)]);
    settle(&mut s.ui);
    assert_eq!(s.ui.big(20, 6, 2, 3), "SOL");
    s.tune.st.state = SID_T_PAUSED as u8;
    s.ui.press(PANEL_BTN_REC);
    assert_eq!(s.ui.toast().as_deref(), Some("LIFT: PLAY THE TUNE"));
    assert!(s.calls().is_empty());
    assert_eq!(s.ui.v.song.rec, 0, "not the song's REC");
}

#[test]
fn the_lifted_instrument_is_offered_once_the_song_is_back() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    offer(&mut s.tune, "NIGHT DRIVE V2", 5);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "LIFTED SOUND");
    assert_eq!(s.ui.big(6, 2, 2, 14), "NIGHT DRIVE V2");
    assert_eq!(s.ui.big(9, 2, 2, 8), "INTO I05");
    golden(&s.ui, "lift-offer");
    s.ui.press(SAVE_KEY_BUTTON);
    assert_eq!(s.calls(), vec![TuneCall::Import(1)]);
    settle(&mut s.ui);
    assert_eq!(s.ui.big(9, 2, 2, 9), "IMPORTING");
    s.tune.st.offer = SID_OFFER_NONE as u8;
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "MENU", "answered: gone");

    offer(&mut s.tune, "NIGHT DRIVE V3", 0);
    s.tune.st.offer_playing = 1;
    settle(&mut s.ui);
    assert_eq!(s.ui.big(9, 2, 2, 13), "NO FREE INSTR");
    assert_eq!(s.ui.big(15, 2, 2, 13), "STOP IT FIRST");
    s.ui.press(SAVE_KEY_BUTTON);
    assert_eq!(s.ui.toast().as_deref(), Some("STOP THE SONG FIRST"));
    assert!(s.calls().is_empty());
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.calls(), vec![TuneCall::Import(0)]);
    assert_eq!(s.ui.top(), "MENU");
}
