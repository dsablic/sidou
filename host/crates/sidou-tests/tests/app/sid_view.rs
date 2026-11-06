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
    Tweak(i16, i8, [u8; 3]),
}

pub(crate) struct Tune {
    pub(crate) calls: Vec<TuneCall>,
    pub(crate) st: sid_status,
    pub(crate) h: Box<psid_header>,
    pub(crate) refuse: bool,
    pub(crate) loading: bool,
    pub(crate) gti: Vec<u8>,
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
    let lifting = voice < SID_LIFT_NO_VOICE;
    t.st.lift = if lifting {
        SID_LIFT_TAKING as u8
    } else {
        SID_LIFT_OFF as u8
    };
    t.st.lift_voice = if lifting { voice as u8 } else { 0 };
    t.st.lift_gti = std::ptr::null();
    t.st.lift_len = 0;
    1
}

unsafe extern "C" fn t_tweak(ctx: *mut c_void, w: *const psid_tweak) -> i32 {
    let t = tune(ctx);
    let w = unsafe { *w };
    t.calls.push(TuneCall::Tweak(w.cut, w.res, w.wave));
    t.st.tweak = w;
    1
}

pub(crate) const LIFTED_AD: u8 = 0x2a;
pub(crate) const LIFTED_SR: u8 = 0xb9;
pub(crate) const LIFTED_WAVE: u8 = 0x21;

pub(crate) fn lifted_gti() -> Vec<u8> {
    let mut ins = [0u8; GT2_INSTR_BYTES as usize];
    ins[GT2_INS_AD as usize] = LIFTED_AD;
    ins[GT2_INS_SR as usize] = LIFTED_SR;
    ins[GT2_INS_WAVE as usize] = 1;
    ins[GT2_INS_GATETIMER as usize] = 2;
    ins[GT2_INS_FIRSTWAVE as usize] = LIFTED_WAVE;
    let mut f = b"GTI5".to_vec();
    f.extend_from_slice(&ins);
    f.extend_from_slice(&[2, 0x00, 0xff, 0x00, 0x00, 0, 0, 0]);
    f
}

pub(crate) fn lifted(t: &mut Tune, voice: u8) {
    t.gti = lifted_gti();
    t.st.lift = SID_LIFT_READY as u8;
    t.st.lift_voice = voice;
    t.st.lift_gti = t.gti.as_ptr();
    t.st.lift_len = t.gti.len() as u16;
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
        gti: Vec::new(),
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
        tweak: Some(t_tweak),
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
    _model: Box<gt2_song>,
}

impl Sids {
    pub(crate) fn new(names: &[&str]) -> Sids {
        let ui = Ui::new();
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
            _model: model,
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

fn edited_song_named(name: &[u8]) -> Sids {
    let mut s = Sids::new(&["ALPHA"]);
    s.io.dirty = 1;
    for (i, b) in name.iter().enumerate() {
        s.io.name[i] = *b as c_char;
    }
    open_sids(&mut s.ui);
    s
}

fn asked_with(s: &mut Sids, name: &str) {
    assert_eq!(s.ui.v.sids.asking, 1, "no question");
    assert_eq!(s.ui.big(6, 2, 2, 7), "DISCARD");
    assert_eq!(s.ui.big(8, 2, 2, 13), "UNSAVED SONG?");
    assert_eq!(s.ui.big(10, 2, 2, name.len()), name);
    assert_eq!(s.ui.big(14, 2, 2, 10), "SAVE AGAIN");
    assert!(s.calls().is_empty(), "a tune before the answer");
}

#[test]
fn an_unsaved_song_is_asked_about_and_save_again_discards_it_for_the_tune() {
    let mut s = edited_song_named(b"NIGHT");
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    asked_with(&mut s, "NIGHT");
    golden(&s.ui, "sids-unsaved");
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!((s.ui.top().as_str(), s.ui.v.sids.asking), ("SIDS", 0));
    assert_eq!(s.ui.big(6, 2, 2, 5), "ALPHA");
    assert!(s.calls().is_empty());
    s.ui.press(SAVE_KEY_BUTTON);
    settle(&mut s.ui);
    asked_with(&mut s, "NIGHT");
    s.ui.press(SAVE_KEY_BUTTON);
    settle(&mut s.ui);
    assert_eq!(s.calls(), vec![TuneCall::Play("ALPHA".into())]);
    assert_eq!(s.ui.top(), "NOW PLAYING");
    assert_eq!(s.io.state, SONG_IO_IDLE as u8, "the song was not saved");
}

#[test]
fn no_encoder_answers_the_unsaved_question_nor_the_control_that_asked_it() {
    for opener in ["ALGORITHM", "SAVE"] {
        let mut s = edited_song_named(b"NIGHT");
        if opener == "SAVE" {
            s.ui.press(SAVE_KEY_BUTTON);
        } else {
            s.ui.turn(PANEL_ENC_ALGORITHM, 1);
        }
        settle(&mut s.ui);
        asked_with(&mut s, "NIGHT");
        for (enc, steps) in [
            (PANEL_ENC_ALGORITHM, 1),
            (PANEL_ENC_ALGORITHM, -1),
            (PANEL_ENC_SELECT, 1),
            (PANEL_ENC_SELECT, -1),
        ] {
            s.ui.turn(enc, steps);
            settle(&mut s.ui);
            assert_eq!(
                s.ui.top(),
                "SIDS",
                "{opener}, then encoder {enc} by {steps}"
            );
            asked_with(&mut s, "NIGHT");
        }
        assert_eq!(s.io.state, SONG_IO_IDLE as u8, "{opener} saved the song");
    }
}

#[test]
fn the_unsaved_question_names_a_new_song() {
    let mut s = edited_song_named(b"");
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    asked_with(&mut s, "NEW SONG");
}

#[test]
fn the_unsaved_question_names_the_built_in_loop() {
    let mut s = Sids::new(&["ALPHA"]);
    s.io.dirty = 1;
    s.io.builtin = 1;
    open_sids(&mut s.ui);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    asked_with(&mut s, "SIDOU LOOP");
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

const TWEAK_ROW: usize = 27;
const SAW: u8 = SID_CTRL_SAW as u8;
const K2_RES: u32 = PANEL_ENC_K1 + 1;
const K3_VOICE: u32 = PANEL_ENC_K1 + 2;

fn turned(s: &mut Sids, enc: u32, steps: i32) -> (String, String) {
    s.ui.turn(enc, steps);
    settle(&mut s.ui);
    let text = |b: &[c_char]| {
        unsafe { CStr::from_ptr(b.as_ptr()) }
            .to_str()
            .unwrap()
            .to_owned()
    };
    (text(&s.ui.v.popup.name), text(&s.ui.v.popup.value))
}

fn says(name: &str, value: &str) -> (String, String) {
    (name.to_owned(), value.to_owned())
}

fn tweak_line(s: &Sids) -> String {
    let words = s.ui.big(TWEAK_ROW + 1, 2, 2, 14);
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let label = s.ui.small(TWEAK_ROW, 2, 7);
    assert_eq!(
        label == "TWEAKED",
        !words.is_empty(),
        "the label with the words"
    );
    words
}

#[test]
fn k1_to_k4_tweak_a_playing_tune_each_with_its_popup_and_the_tweak_line_says_what_is_bent() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    now_playing(&mut s);
    assert_eq!(tweak_line(&s), "");
    assert_eq!(turned(&mut s, PANEL_ENC_K1, 3), says("TWEAK", "CUT +96"));
    assert_eq!(tweak_line(&s), "CUT");
    assert_eq!(turned(&mut s, K2_RES, -2), says("TWEAK", "RESO -2"));
    assert_eq!(tweak_line(&s), "CUT RES");
    assert_eq!(turned(&mut s, K3_VOICE, 1), says("TWEAK", "VOICE 2"));
    assert_eq!(turned(&mut s, PANEL_ENC_K4, 2), says("TWEAK", "V2 SAW"));
    assert_eq!(tweak_line(&s), "CUT RES WAVE");
    s.ui.popup_gone();
    drain(&mut s.ui.g);
    settle(&mut s.ui);
    golden(&s.ui, "playing-tweak");
    assert_eq!(
        s.calls(),
        vec![
            TuneCall::Tweak(96, 0, [0; 3]),
            TuneCall::Tweak(96, -2, [0; 3]),
            TuneCall::Tweak(96, -2, [0, SAW, 0]),
        ]
    );
    assert_eq!(turned(&mut s, PANEL_ENC_K4, -9), says("TWEAK", "V2 TUNE'S"));
    assert_eq!(turned(&mut s, PANEL_ENC_K1, 99), says("TWEAK", "CUT +1024"));
    turned(&mut s, PANEL_ENC_K1, -32);
    turned(&mut s, K2_RES, 2);
    assert_eq!(tweak_line(&s), "", "every tweak back at 0");
    assert_eq!(s.calls().last(), Some(&TuneCall::Tweak(0, 0, [0; 3])));
}

#[test]
fn edit_with_a_knob_and_a_stopped_tune_leave_the_knobs_to_live() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    now_playing(&mut s);
    s.ui.down(PANEL_BTN_EDIT);
    s.ui.turn(PANEL_ENC_K1, 1);
    s.ui.up(PANEL_BTN_EDIT);
    s.tune.st.state = SID_T_STOPPED as u8;
    s.ui.turn(K2_RES, 1);
    assert!(s.calls().is_empty());
}

#[test]
fn leaving_now_playing_gives_the_tune_its_own_back() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    now_playing(&mut s);
    turned(&mut s, PANEL_ENC_K1, 1);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "SIDS");
    assert_eq!(
        s.calls(),
        vec![
            TuneCall::Tweak(32, 0, [0; 3]),
            TuneCall::Tweak(0, 0, [0; 3])
        ]
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
fn glo_mutes_and_solos_reach_the_tune_and_the_song_gets_its_own_back() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    s.ui.v.song.mute = 0b001;
    now_playing(&mut s);
    unsafe {
        views_set_song(&mut *s.ui.v, std::ptr::null());
        views_tune(&mut *s.ui.v, 1);
    }
    assert_eq!(
        s.calls(),
        vec![TuneCall::Mute(0)],
        "the tune starts with every voice"
    );
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(GLO_MUTE_VOICE_2_KEY, GLO_MUTE_VOICE_2_KEY);
    s.ui.note_keys(0, 0);
    s.ui.note_keys(GLO_SOLO_VOICE_3_KEY, GLO_SOLO_VOICE_3_KEY);
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    assert_eq!(s.calls(), vec![TuneCall::Mute(2), TuneCall::Mute(3)]);
    unsafe { views_tune(&mut *s.ui.v, 0) };
    assert_eq!(
        (s.ui.v.song.mute, s.ui.v.song.solo),
        (0b001, 0),
        "the song's own"
    );
    unsafe { views_tune(&mut *s.ui.v, 1) };
    assert_eq!(
        s.calls(),
        vec![TuneCall::Mute(0)],
        "the next tune starts afresh"
    );
}

#[test]
fn the_song_views_show_sid_playing_while_a_tune_has_the_song() {
    let mut song = crate::song_view::Song::new();
    song.ui.press(PANEL_BTN_SEQ);
    crate::song_view::settle(&mut song.ui);
    assert_eq!(song.ui.top(), "STEP");
    let mut s = Sids::with(std::mem::replace(&mut song.ui, Ui::new()), &["NIGHT"]);
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
    let mut ui = Ui::new();
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
    assert_eq!(heading, "SETTINGS#  9/9");
    unsafe { views_settings(&mut *ui.v, std::ptr::null_mut()) };
}

#[test]
fn the_sid_views_help_cards_match_their_goldens() {
    for (card, asks, discards) in [
        ("SIDS", false, false),
        ("PLAYING", true, true),
        ("UNSAVED", true, false),
        ("LIFT", true, true),
    ] {
        let mut s = Sids::new(&["NIGHT DRIVE"]);
        s.io.dirty = 1;
        open_sids(&mut s.ui);
        if asks {
            s.ui.turn(PANEL_ENC_ALGORITHM, 1);
        }
        if discards {
            s.ui.press(SAVE_KEY_BUTTON);
        }
        if card == "LIFT" {
            s.ui.press(PANEL_BTN_REC);
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

const LIFT_LINE_ROW: usize = 9;
const LIFT_HINT_ROW: usize = 15;
const VOICE_2_KEY: u32 = 1 << 1;
const VOICE_KEYS: u32 = 0b111;

fn lift_line(s: &Sids) -> String {
    s.ui.big(LIFT_LINE_ROW, 2, 2, 14).trim_end().to_string()
}

fn asked_which_voice(s: &mut Sids) {
    now_playing(s);
    s.ui.press(PANEL_BTN_REC);
    settle(&mut s.ui);
    assert_eq!(lift_line(s), "LIFT WHICH");
}

fn key_lights(s: &mut Sids) -> u32 {
    let mut i = panel_led_in::default();
    unsafe { views_leds(&mut *s.ui.v, s.ui.now, &mut i) };
    unsafe { panel_get_leds(&*s.ui.p, &*s.ui.rig.live, &i) }.notes
}

#[test]
fn rec_on_now_playing_asks_which_voice_and_a_key_lifts_it() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    asked_which_voice(&mut s);
    assert_eq!(s.ui.big(12, 2, 2, 12), "VOICE? 1 2 3");
    assert_eq!(
        s.ui.small(LIFT_HINT_ROW, 2, 27),
        "KEY 1-3 PICKS  HOME: CANCEL"
    );
    assert_eq!(
        key_lights(&mut s) & VOICE_KEYS,
        VOICE_KEYS,
        "keys 1 to 3 lit"
    );
    assert!(s.calls().is_empty(), "nothing lifted yet");
    golden(&s.ui, "playing-lift-ask");
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert!(
        s.calls().is_empty(),
        "ALGORITHM does not change the song under the question"
    );
    s.ui.note_keys(VOICE_2_KEY, VOICE_2_KEY);
    s.ui.note_keys(0, 0);
    assert_eq!(s.calls(), vec![TuneCall::Lift(1)]);
    assert_eq!(s.ui.live().note_ons, 0, "the answer does not sound");
    assert_eq!(key_lights(&mut s) & VOICE_KEYS, 0);
    settle(&mut s.ui);
    assert_eq!(lift_line(&s), "LIFTING V2");
    s.tune.st.lift = SID_LIFT_ARMED as u8;
    settle(&mut s.ui);
    assert_eq!(lift_line(&s), "WAITING FOR V2");
    assert_eq!(s.ui.small(LIFT_HINT_ROW, 2, 19), "REC OR HOME: CANCEL");
    golden(&s.ui, "playing-lift-waiting");
    s.ui.press(PANEL_BTN_REC);
    assert_eq!(s.calls(), vec![TuneCall::Lift(SID_LIFT_NO_VOICE)]);
    settle(&mut s.ui);
    assert_eq!(s.ui.big(LIFT_LINE_ROW, 2, 2, 8), "SONG 1/3");
    assert_eq!(s.ui.v.song.rec, 0, "not the song's REC");
}

#[test]
fn rec_or_home_cancels_and_a_paused_tune_says_why() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    asked_which_voice(&mut s);
    s.ui.press(PANEL_BTN_REC);
    settle(&mut s.ui);
    assert_eq!(
        s.ui.big(LIFT_LINE_ROW, 2, 2, 8),
        "SONG 1/3",
        "REC again: gone"
    );
    s.ui.press(PANEL_BTN_REC);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(
        (s.ui.top(), s.ui.big(LIFT_LINE_ROW, 2, 2, 8)),
        ("NOW PLAYING".to_string(), "SONG 1/3".to_string()),
        "HOME: the question gone, the tune's page stays"
    );
    assert!(s.calls().is_empty());
    s.ui.press(PANEL_BTN_REC);
    s.ui.note_keys(VOICE_2_KEY, VOICE_2_KEY);
    s.ui.note_keys(0, 0);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(
        s.calls(),
        vec![TuneCall::Lift(1), TuneCall::Lift(SID_LIFT_NO_VOICE)]
    );
    assert_eq!(
        s.ui.top(),
        "NOW PLAYING",
        "HOME: the wait cancelled, the page stays"
    );
    s.tune.st.state = SID_T_PAUSED as u8;
    s.ui.press(PANEL_BTN_REC);
    assert_eq!(s.ui.toast().as_deref(), Some("LIFT: PLAY THE TUNE"));
    settle(&mut s.ui);
    assert_eq!(
        s.ui.big(LIFT_LINE_ROW, 2, 2, 8),
        "SONG 1/3",
        "nothing asked"
    );
}

#[test]
fn a_lifted_sound_opens_lab_named_for_its_voice_and_home_goes_back_to_the_tune() {
    let mut s = Sids::new(&["NIGHT DRIVE"]);
    asked_which_voice(&mut s);
    s.ui.note_keys(VOICE_2_KEY, VOICE_2_KEY);
    s.ui.note_keys(0, 0);
    s.calls();
    lifted(&mut s.tune, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "LAB");
    assert_eq!(
        s.calls(),
        vec![TuneCall::Lift(SID_LIFT_NO_VOICE)],
        "the lift taken"
    );
    assert_eq!(s.ui.toast().as_deref(), Some("LIFTED V2"));
    assert_eq!(
        s.ui.row(LAB_VIEW_NAME_ROW as usize).trim_end(),
        "LIFTED V2*",
        "LAB names the sound for its voice, starred till it is kept"
    );
    let l = s.ui.live();
    assert_eq!(
        (l.attack_decay, l.sustain_release, l.control & 0xf0),
        (LIFTED_AD, LIFTED_SR, LIFTED_WAVE & 0xf0),
        "the lifted envelope and wave"
    );
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "NOW PLAYING");
    assert_eq!(s.tune.st.state, SID_T_PLAYING as u8, "the tune plays on");
    assert!(s.calls().is_empty(), "HOME asks the tune nothing");
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "SIDS", "and on, as before the lift");
}
