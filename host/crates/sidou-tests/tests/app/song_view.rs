// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::c_void;

use sidou_sys::*;

use crate::drain;
use crate::views::Ui;

const COLS: usize = GRID_COLS as usize;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Call {
    Play(u32),
    Stop,
    Resume,
    Mute(u32),
    Jam(u32, u32, u32),
}

pub(crate) struct Fake {
    pub(crate) calls: Vec<Call>,
    pub(crate) st: song_status,
    keys_left_chip_a: bool,
    pub(crate) refuse: bool,
    statuses: u32,
    song: *mut gt2_song,
    pub(crate) edit_guard: i32,
    pub(crate) cell_writes: u32,
    pub(crate) index_swaps: u32,
    pub(crate) last_subtune: u32,
    pub(crate) calls_since_play: u32,
    pub(crate) held_step_locks: Vec<(u32, Vec<(u8, u16)>)>,
}

impl Fake {
    fn cmd(&mut self, c: Call) -> i32 {
        if self.refuse {
            return 0;
        }
        self.calls.push(c);
        match c {
            Call::Play(_) => {
                self.st.state = if self.keys_left_chip_a {
                    SONG_T_PLAYING
                } else {
                    SONG_T_STARTING
                } as u8
            }
            Call::Stop => self.st.state = SONG_T_STOPPED as u8,
            Call::Resume => self.st.state = SONG_T_PLAYING as u8,
            Call::Mute(m) => self.st.mute = m as u8,
            Call::Jam(..) => {}
        }
        1
    }
}

fn fake(ctx: *mut c_void) -> &'static mut Fake {
    unsafe { &mut *ctx.cast::<Fake>() }
}

unsafe extern "C" fn play(ctx: *mut c_void, pos: u32) -> i32 {
    fake(ctx).cmd(Call::Play(pos))
}
unsafe extern "C" fn play_from(ctx: *mut c_void, sub: u32, pos: u32) -> i32 {
    let accepted = unsafe { play(ctx, pos) };
    if accepted != 0 {
        fake(ctx).last_subtune = sub;
        fake(ctx).st.subtune = sub as u8;
    }
    accepted
}
unsafe extern "C" fn stop(ctx: *mut c_void) -> i32 {
    fake(ctx).cmd(Call::Stop)
}
unsafe extern "C" fn resume(ctx: *mut c_void) -> i32 {
    fake(ctx).cmd(Call::Resume)
}
unsafe extern "C" fn mute(ctx: *mut c_void, mask: u32) -> i32 {
    fake(ctx).cmd(Call::Mute(mask))
}
unsafe extern "C" fn jam(ctx: *mut c_void, voice: u32, note: u32, instr: u32) -> i32 {
    fake(ctx).cmd(Call::Jam(voice, note, instr))
}
unsafe extern "C" fn held(ctx: *mut c_void, voice: u32, locks: *const u8, n: u32) -> i32 {
    let f = fake(ctx);
    if f.refuse {
        return 0;
    }
    let b = if n == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(locks, 3 * n as usize) }
    };
    let l = b
        .chunks(3)
        .map(|c| (c[0], u16::from(c[1]) | u16::from(c[2]) << 8))
        .collect();
    f.held_step_locks.push((voice, l));
    1
}
unsafe extern "C" fn status(ctx: *mut c_void, out: *mut song_status) {
    let f = fake(ctx);
    f.statuses += 1;
    if f.st.state == SONG_T_STARTING as u8 && f.keys_left_chip_a {
        f.st.state = SONG_T_PLAYING as u8;
    }
    unsafe { *out = f.st };
}

unsafe extern "C" fn clock(ctx: *mut c_void, out: *mut song_clock) {
    let f = fake(ctx);
    let o = unsafe { &mut *out };
    o.state = f.st.state;
    o.speed = f.st.speed;
    o.calls = f.calls_since_play;
}

unsafe extern "C" fn edit(ctx: *mut c_void, edit: *const gt2_edit, ticket: *mut u32) -> i32 {
    let f = fake(ctx);
    if f.refuse {
        return 0;
    }
    let e = unsafe { &*edit };
    match u32::from(e.kind) {
        GT2_EDIT_CELL => f.cell_writes += 1,
        GT2_EDIT_SWAP => f.index_swaps += 1,
        _ => panic!("an edit is not a cell or swap"),
    }
    unsafe { gt2_edit_commit(f.song, edit) };
    f.st.applied += 1;
    unsafe { *ticket = f.st.applied };
    1
}

unsafe extern "C" fn edit_done(ctx: *mut c_void, ticket: u32) -> i32 {
    i32::from(fake(ctx).st.applied >= ticket)
}

unsafe extern "C" fn edit_guard(ctx: *mut c_void, _: *const song_edit_request) -> i32 {
    fake(ctx).edit_guard
}

pub(crate) struct Song {
    pub(crate) ui: Ui,
    pub(crate) f: Box<Fake>,
    _t: Box<song_transport>,
    _song: Option<sidou_tests::song::Song>,
}

impl Song {
    pub(crate) fn new() -> Self {
        Self::playing(None)
    }

    pub(crate) fn playing(song: Option<sidou_tests::song::Song>) -> Self {
        let mut ui = Ui::without_cards();
        let mut f = Box::new(Fake {
            calls: Vec::new(),
            st: song_status {
                speed: [6, 6],
                multispeed: 1,
                order_pos: [SONG_T_NOWHERE as u8; 3],
                row: [SONG_T_NOWHERE as u8; 3],
                ..Default::default()
            },
            keys_left_chip_a: true,
            refuse: false,
            statuses: 0,
            song: song
                .as_ref()
                .map_or(std::ptr::null_mut(), |s| s.ptr().cast_mut()),
            edit_guard: SONG_EDIT_GUARD_OK as i32,
            cell_writes: 0,
            index_swaps: 0,
            last_subtune: 0,
            calls_since_play: 0,
            held_step_locks: Vec::new(),
        });
        let ctx: *mut Fake = &mut *f;
        let t = Box::new(song_transport {
            play: Some(play),
            play_from: Some(play_from),
            stop: Some(stop),
            resume: Some(resume),
            mute: Some(mute),
            jam: Some(jam),
            status: Some(status),
            clock: Some(clock),
            ctx: ctx.cast(),
            song: song.as_ref().map_or(std::ptr::null(), |s| s.ptr()),
            edit: Some(edit),
            edit_done: Some(edit_done),
            edit_guard: Some(edit_guard),
            held: Some(held),
            ..Default::default()
        });
        unsafe { views_set_song(&mut *ui.v, &*t) };
        let mut s = Song {
            ui,
            f,
            _t: t,
            _song: song,
        };
        s.ui.frame();
        s.f.calls.clear();
        s
    }

    pub(crate) fn calls(&mut self) -> Vec<Call> {
        std::mem::take(&mut self.f.calls)
    }
}

pub(crate) fn settle(ui: &mut Ui) {
    for _ in 0..200 {
        ui.frame();
        drain(&mut ui.g);
        if unsafe { screen_animating(&ui.v.s) } == 0
            && ui.v.song.dirty == 0
            && ui.v.song.unbuilt == 0
            && ui.v.pages.unbuilt == 0
        {
            ui.frame();
            drain(&mut ui.g);
            return;
        }
    }
    panic!("the screen kept changing");
}

pub(crate) fn open_tracker(ui: &mut Ui) {
    for _ in 0..SONG_VIEW_COUNT {
        ui.press(PANEL_BTN_SEQ);
        settle(ui);
        if ui.top() == "TRACKER" {
            break;
        }
    }
    assert_eq!(ui.top(), "TRACKER");
}

pub(crate) fn attr(ui: &Ui, col: usize, row: usize) -> u8 {
    ui.g.attr[row * COLS + col]
}

pub(crate) fn ch(ui: &Ui, col: usize, row: usize) -> u8 {
    ui.g.ch[row * COLS + col]
}

pub(crate) const fn attr_of(fg: u32, bg: u32) -> u8 {
    (bg << 4 | fg) as u8
}

fn key(k: u32) -> u32 {
    1 << k
}

fn gated(ui: &Ui) -> usize {
    (0..3).filter(|&v| ui.rig.gated(v)).count()
}

const PAL_CLOCK_HZ: f64 = 985_248.0;
const NTSC_CLOCK_HZ: f64 = 1_022_727.0;
const PAL_CIA_PERIOD: u32 = 0x4cc7;
const NTSC_CIA_PERIOD: u32 = 0x42c6;
const SECONDS_A_MINUTE_IN_TENTHS: f64 = 600.0;

fn cia_period(frame_period: u32, multispeed: u32) -> u32 {
    frame_period
        .checked_div(multispeed)
        .map_or(2 * frame_period + 1, |p| p + 1)
}

#[test]
fn the_bpm_follows_the_frame_rate_the_multispeed_and_the_rows_a_beat() {
    let bpm = |sum, ms, ntsc, rows| unsafe { song_view_bpm10(sum, ms, ntsc, rows) };
    assert_eq!(bpm(12, 1, 0, 4), 1253, "tempo 6 on PAL: 125.3 BPM");
    assert_eq!(bpm(12, 2, 0, 4), 2506, "multispeed 2: 250.6");
    assert_eq!(bpm(5 + 7, 1, 0, 4), 1253, "funktempo 5 and 7 play as 6");
    for ntsc in 0..2u32 {
        let (hz, cia) = if ntsc == 1 {
            (NTSC_CLOCK_HZ, NTSC_CIA_PERIOD)
        } else {
            (PAL_CLOCK_HZ, PAL_CIA_PERIOD)
        };
        for ms in 0..=4u32 {
            let period = cia_period(cia, ms);
            for (sum, rows) in [(4, 4), (12, 4), (12, 8), (31, 3), (510, 255), (2, 1)] {
                let want = (SECONDS_A_MINUTE_IN_TENTHS * hz / f64::from(period) * 2.0
                    / f64::from(sum * rows))
                .round() as u32;
                let got = bpm(sum, ms, ntsc, rows);
                assert!(
                    got.abs_diff(want) <= 1,
                    "speeds {sum}, multispeed {ms}, NTSC {ntsc}, {rows} rows a beat: {got} for {want}"
                );
            }
        }
    }
    assert_eq!(bpm(0, 1, 0, 4), 0, "no tempo yet");
}

#[test]
fn seq_opens_the_tracker_from_any_view_and_home_goes_back() {
    let mut ui = Ui::without_cards();
    open_tracker(&mut ui);
    for page in ["ORDER", "INSTR", "TABLE", "STEP", "TRACKER"] {
        ui.press(PANEL_BTN_SEQ);
        settle(&mut ui);
        assert_eq!(ui.top(), page);
    }
    assert_eq!(ui.v.s.depth, 2);
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "LIVE");
    crate::views::open_status(&mut ui);
    settle(&mut ui);
    open_tracker(&mut ui);
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "STATUS");
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "MENU");
    ui.choose_menu_item(VIEWS_MENU_TRACKER);
    settle(&mut ui);
    assert_eq!(ui.top(), "TRACKER");
}

#[test]
fn seq_reopens_the_song_view_used_last() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_SEQ);
    settle(&mut ui);
    assert_eq!(ui.top(), "STEP", "the first time: STEP");
    for page in ["TRACKER", "ORDER"] {
        ui.press(PANEL_BTN_SEQ);
        settle(&mut ui);
        assert_eq!(ui.top(), page, "SEQ in a song view steps");
    }
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "LIVE");
    ui.press(PANEL_BTN_SEQ);
    settle(&mut ui);
    assert_eq!(ui.top(), "ORDER", "the view used last");
    ui.press(PANEL_BTN_SEQ);
    settle(&mut ui);
    assert_eq!(ui.top(), "INSTR");
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    ui.press(PANEL_BTN_HOME);
    ui.choose_menu_item(VIEWS_MENU_TRACKER);
    settle(&mut ui);
    assert_eq!(ui.top(), "TRACKER");
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "MENU");
    ui.press(PANEL_BTN_SEQ);
    settle(&mut ui);
    assert_eq!(ui.top(), "TRACKER", "from the menu too");
}

#[test]
fn seq_steps_to_the_next_song_view_and_round() {
    let step: *const view = unsafe { &step_view_view };
    let tracker: *const view = unsafe { &tracker_view_view };
    unsafe {
        assert_eq!(song_view_next(std::ptr::null()), step);
        assert_eq!(song_view_next(&views_menu), step);
        let order: *const view = &song_pages_order_view;
        let instr: *const view = &song_pages_instr_view;
        let table: *const view = &song_pages_table_view;
        assert_eq!(song_view_next(step), tracker);
        assert_eq!(song_view_next(tracker), order);
        assert_eq!(song_view_next(order), instr);
        assert_eq!(song_view_next(instr), table);
        assert_eq!(song_view_next(table), step);
        assert_eq!(song_view_is(tracker), 1);
        assert_eq!(song_view_is(step), 1);
        for page in [order, instr, table] {
            assert_eq!(song_view_is(page), 1);
        }
        assert_eq!(song_view_is(&views_menu), 0);
        assert_eq!(song_view_is(std::ptr::null()), 0);
    }
}

#[test]
fn the_header_shows_the_transport_the_tempo_the_position_and_rec() {
    let mut ui = Ui::without_cards();
    open_tracker(&mut ui);
    let alert = unsafe { (*theme_get(THEME_HIGH_CONTRAST)).alert };
    assert_eq!(ui.small(0, 26, 4), " ---");
    assert_eq!(ui.small(29, 17, 13), "ORD 00 ROW 00");
    assert_eq!(ui.big(0, 24, 2, 1), (FONT_SQUARE as u8 as char).to_string());
    assert_eq!(ui.small(1, 18, 6), "      ");
    ui.press(PANEL_BTN_REC);
    settle(&mut ui);
    assert_eq!(ui.big(0, 18, 2, 3), "REC");
    for c in 18..24 {
        assert_eq!(attr(&ui, c, 1) & 15, alert);
    }
    ui.press(PANEL_BTN_REC);
    settle(&mut ui);
    assert_eq!(ui.small(1, 18, 6), "      ");

    let mut s = Song::new();
    open_tracker(&mut s.ui);
    assert_eq!(s.ui.small(0, 26, 4), " 125");
    assert_eq!(s.ui.small(29, 17, 13), "ORD 00 ROW 00");
    s.ui.press(PANEL_BTN_PLAY);
    s.f.st.order_pos = [3, 4, 5];
    s.f.st.row = [0x1a, 0x1b, 0x1c];
    settle(&mut s.ui);
    assert_eq!(s.ui.small(0, 26, 4), " 125");
    assert_eq!(s.ui.small(29, 17, 13), "ORD 03 ROW 1A");
    assert_eq!(s.ui.big(0, 24, 2, 1), (FONT_PLAY as u8 as char).to_string());
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(29, 17, 13), "ORD 04 ROW 1B");
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    settle(&mut s.ui);
    s.f.st.multispeed = 2;
    s.f.st.speed = [5, 7];
    settle(&mut s.ui);
    assert_eq!(s.ui.small(0, 26, 4), " 251");
    s.ui.press(PANEL_BTN_PLAY);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(29, 17, 13), "ORD 00 ROW 00");
    assert_eq!(
        s.ui.big(0, 24, 2, 1),
        (FONT_SQUARE as u8 as char).to_string()
    );
}

#[test]
fn a_loaded_song_discards_old_edit_history_and_pending_transport_intent() {
    let mut s = Song::playing(Some(sidou_tests::song::Song::device()));
    open_tracker(&mut s.ui);
    s.ui.press(PANEL_BTN_REC);
    s.ui.note_keys(key(16), key(16));
    s.ui.note_keys(0, 0);
    settle(&mut s.ui);
    assert_ne!(s.ui.v.editor.can_undo, 0);
    let binding = s.ui.v.song.t;
    let mode = s.ui.v.song.rec;
    s.ui.v.song.row = 31;
    s.ui.v.song.order_pos = 1;
    s.ui.v.song.instr = 3;
    s.ui.v.song.want = 1;
    unsafe { song_view_loaded(&mut s.ui.v.song) };
    assert_eq!(s.ui.v.song.t, binding);
    assert_eq!(s.ui.v.song.rec, mode);
    assert_eq!(s.ui.v.song.row, 0);
    assert_eq!(s.ui.v.song.order_pos, 0);
    assert_eq!(s.ui.v.song.instr, 1);
    assert_eq!(s.ui.v.song.want, 0);
    assert_eq!(s.ui.v.editor.can_undo, 0);
    assert_eq!(s.ui.v.song.edit_pending, 0);
    assert_eq!(s.ui.v.song.jam_keys, 0);
}

#[test]
fn play_plays_from_the_cursor_stops_and_with_edit_goes_on() {
    let mut s = Song::new();
    open_tracker(&mut s.ui);
    s.calls();
    s.ui.v.song.order_pos = 2;
    s.ui.press(PANEL_BTN_PLAY);
    assert_eq!(
        s.calls(),
        [Call::Play(2)],
        "PLAY plays at once, from the cursor's entry"
    );
    s.ui.frame();
    s.ui.press(PANEL_BTN_PLAY);
    assert_eq!(s.calls(), [Call::Stop]);
    s.ui.frame();
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.hold(PANEL_BTN_EDIT, false);
    assert_eq!(s.calls(), [Call::Resume], "EDIT + PLAY goes on");
    s.ui.frame();
    s.ui.press(PANEL_BTN_PLAY);
    assert_eq!(s.calls(), [Call::Stop]);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "LIVE");
    s.ui.press(PANEL_BTN_PLAY);
    assert_eq!(s.calls(), [Call::Play(2)]);
}

#[test]
fn edit_and_play_with_nothing_to_go_on_from_plays() {
    let mut s = Song::new();
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.press(PANEL_BTN_PLAY);
    assert_eq!(s.calls(), [Call::Play(0)]);
}

#[test]
fn play_waits_for_the_keys_to_leave_chip_a_and_a_second_press_stops() {
    let mut s = Song::new();
    open_tracker(&mut s.ui);
    s.calls();
    s.f.keys_left_chip_a = false;
    s.ui.press(PANEL_BTN_PLAY);
    assert_eq!(s.calls(), [Call::Play(0)]);
    s.ui.frames(5);
    assert_eq!(s.ui.v.song.st.state, SONG_T_STARTING as u8);
    assert_eq!(
        s.ui.big(0, 24, 2, 1),
        (FONT_PLAY as u8 as char).to_string(),
        "starting shows as playing"
    );
    s.ui.press(PANEL_BTN_PLAY);
    assert_eq!(s.calls(), [Call::Stop]);
    s.ui.frame();
    s.f.keys_left_chip_a = true;
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.frame();
    assert_eq!(s.calls(), [Call::Play(0)]);
    assert_eq!(s.ui.v.song.st.state, SONG_T_PLAYING as u8);
}

#[test]
fn a_command_the_player_cannot_take_is_asked_again_at_the_next_frame() {
    let mut s = Song::new();
    s.f.refuse = true;
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(key(0), key(0));
    assert!(s.calls().is_empty());
    s.ui.frame();
    s.ui.frame();
    assert!(s.calls().is_empty());
    s.f.refuse = false;
    s.ui.frame();
    assert_eq!(s.calls(), [Call::Play(0), Call::Mute(1)]);
    s.ui.frame();
    assert!(s.calls().is_empty(), "each once");
    s.f.refuse = true;
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.press(PANEL_BTN_PLAY);
    s.f.refuse = false;
    s.ui.frame();
    assert_eq!(s.calls(), [], "stop then play again: nothing to ask");
    s.f.refuse = true;
    s.ui.press(PANEL_BTN_PLAY);
    s.f.refuse = false;
    s.ui.frame();
    assert_eq!(s.calls(), [Call::Stop]);
}

#[test]
fn glo_and_the_leftmost_keys_mute_and_solo_and_sound_nothing() {
    let mut s = Song::new();
    s.ui.rig.clear();
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(key(0), key(0));
    assert_eq!(s.calls(), [Call::Mute(0b001)]);
    s.ui.note_keys(0, 0);
    s.ui.note_keys(key(3), key(3));
    assert_eq!(s.calls(), [Call::Mute(0b110)]);
    s.ui.note_keys(0, 0);
    s.ui.note_keys(key(4), key(4));
    assert_eq!(s.calls(), [Call::Mute(0b100)]);
    s.ui.note_keys(0, 0);
    s.ui.note_keys(key(16), key(16));
    assert!(s.calls().is_empty());
    assert_eq!(
        gated(&s.ui),
        1,
        "a key beyond the leftmost six plays under GLO"
    );
    s.ui.note_keys(0, 0);
    s.ui.note_keys(key(2), key(2));
    assert!(
        s.calls().is_empty(),
        "voices 1 and 2 are soloed: the mute of 3 changes nothing yet"
    );
    s.ui.hold(PANEL_BTN_GLO, false);
    s.ui.note_keys(key(2), 0);
    assert_eq!(gated(&s.ui), 0);
    s.ui.note_keys(0, 0);
    s.ui.note_keys(key(2), key(2));
    assert_eq!(gated(&s.ui), 1, "pressed again without GLO: it plays");
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(key(2), key(2));
    assert!(s.calls().is_empty());
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(key(3) | key(4), key(3) | key(4));
    assert_eq!(s.calls(), [Call::Mute(0b001)]);
    s.ui.note_keys(0, 0);
    s.ui.note_keys(key(0), key(0));
    assert_eq!(s.calls(), [Call::Mute(0)]);
    assert_eq!(unsafe { song_view_mute_mask(&s.ui.v.song) }, 0);
}

#[test]
fn glo_lets_a_key_already_sounding_sound_on() {
    let mut ui = Ui::without_cards();
    ui.note_keys(key(0), key(0));
    assert_eq!(gated(&ui), 1);
    ui.hold(PANEL_BTN_GLO, true);
    ui.note_keys(key(0), 0);
    assert_eq!(gated(&ui), 1, "held from before GLO: it sounds on");
    assert_eq!(ui.v.song.mute, 0, "and toggles nothing");
    ui.note_keys(0, 0);
    assert_eq!(gated(&ui), 0);
}

#[test]
fn the_mutes_show_in_the_header_the_strips_and_lives_header() {
    let mut s = Song::new();
    open_tracker(&mut s.ui);
    let t = unsafe { &*theme_get(THEME_HIGH_CONTRAST) };
    assert_eq!(attr(&s.ui, 0, 0) & 15, t.voice[0]);
    assert_eq!(s.ui.small(0, 18, 3), "   ");
    assert_eq!(attr(&s.ui, 26, 2) & 15, t.voice[1]);
    assert_eq!(attr(&s.ui, 28, 2) & 15, t.voice[2]);
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(key(0) | key(5), key(0) | key(5));
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    settle(&mut s.ui);
    assert_eq!(s.ui.big(0, 18, 2, 3), "MUT");
    assert_eq!(attr(&s.ui, 18, 0) & 15, t.alert);
    assert_eq!(attr(&s.ui, 0, 0) & 15, t.dim, "V1 grey");
    assert_eq!(
        attr(&s.ui, 26, 2) & 15,
        t.dim,
        "voice 2 is silent under the solo"
    );
    assert_eq!(
        (attr(&s.ui, 28, 2) & 15, attr(&s.ui, 28, 2) >> 4),
        (t.bg, t.hot),
        "voice 3 sounds, soloed: its number reversed"
    );
    s.ui.turn(PANEL_ENC_ALGORITHM, 2);
    settle(&mut s.ui);
    assert_eq!(s.ui.big(0, 18, 2, 3), "SOL");
    assert_eq!(attr(&s.ui, 18, 0) & 15, t.value);
    assert_eq!(attr(&s.ui, 0, 0) & 15, t.voice[2]);
    assert_eq!(
        s.calls(),
        [Call::Mute(0b011)],
        "both in one call: the solo wins"
    );
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "LIVE");
    let name = crate::preset_name(crate::preset(s.ui.live().preset));
    assert_eq!(s.ui.row(0)[..name.len()], name, "the preset's name stays");
    let badge = |s: &Song, v: usize| (ch(&s.ui, 24 + 2 * v, 20), attr(&s.ui, 24 + 2 * v, 20));
    assert_eq!(badge(&s, 0), (b'1', attr_of(u32::from(t.dim), GRID_BLACK)));
    assert_eq!(badge(&s, 1), (b'2', attr_of(u32::from(t.dim), GRID_BLACK)));
    assert_eq!(badge(&s, 2), (b'3', attr_of(GRID_BLACK, u32::from(t.hot))));
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(key(0) | key(5), key(0) | key(5));
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    settle(&mut s.ui);
    assert_eq!(ch(&s.ui, 24, 20), b' ', "none, and no song: no badges");
}

#[test]
fn a_key_in_a_song_view_jams_the_current_instrument_on_the_cursors_voice() {
    let mut s = Song::new();
    open_tracker(&mut s.ui);
    s.calls();
    s.ui.rig.clear();
    s.ui.note_keys(key(16), key(16));
    assert_eq!(s.calls(), [Call::Jam(0, 57, 1)]);
    assert_eq!(gated(&s.ui), 0, "the instrument does not sound");
    s.ui.note_keys(key(16), 0);
    assert!(s.calls().is_empty(), "held: nothing more");
    s.ui.note_keys(0, 0);
    assert_eq!(s.calls(), [Call::Jam(0, SONG_T_RELEASE, 1)]);
    s.ui.press(PANEL_BTN_OCT_UP);
    s.ui.v.song.voice = 2;
    s.ui.v.song.instr = 7;
    s.ui.note_keys(key(0), key(0));
    assert_eq!(s.calls(), [Call::Jam(2, 53 + 12 - 12, 7)]);
    s.ui.note_keys(key(0) | key(2), key(2));
    assert_eq!(s.calls(), [Call::Jam(2, 55, 7)]);
    s.ui.note_keys(key(2), 0);
    assert!(s.calls().is_empty());
    s.ui.note_keys(0, 0);
    assert_eq!(s.calls(), [Call::Jam(2, SONG_T_RELEASE, 7)]);
    s.ui.note_keys(0, key(4));
    assert_eq!(
        s.calls(),
        [Call::Jam(2, 57, 7), Call::Jam(2, SONG_T_RELEASE, 7)]
    );
    assert!(
        s.ui.rig.log().iter().all(|&(r, _)| r % 7 != 4),
        "{:?}",
        s.ui.rig.log()
    );
}

#[test]
fn a_key_held_into_a_song_view_plays_on_and_a_jam_key_releases_outside() {
    let mut s = Song::new();
    s.ui.note_keys(key(10), key(10));
    assert_eq!(gated(&s.ui), 1);
    open_tracker(&mut s.ui);
    s.calls();
    s.ui.note_keys(key(10), 0);
    assert!(s.calls().is_empty());
    assert_eq!(gated(&s.ui), 1);
    s.ui.note_keys(0, 0);
    assert_eq!(gated(&s.ui), 0);
    s.ui.note_keys(key(12), key(12));
    assert_eq!(s.calls(), [Call::Jam(0, 53, 1)]);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    s.ui.note_keys(key(12), 0);
    assert_eq!(gated(&s.ui), 0);
    s.ui.note_keys(0, 0);
    assert_eq!(s.calls(), [Call::Jam(0, SONG_T_RELEASE, 1)]);
    s.ui.note_keys(key(12), key(12));
    assert!(s.calls().is_empty());
    assert_eq!(gated(&s.ui), 1);
}

#[test]
fn without_a_player_the_keys_play_the_instrument_in_a_song_view() {
    let mut ui = Ui::without_cards();
    open_tracker(&mut ui);
    ui.note_keys(key(16), key(16));
    assert_eq!(gated(&ui), 1);
    ui.press(PANEL_BTN_PLAY);
    assert_eq!(ui.v.song.st.state, SONG_T_IDLE as u8);
    ui.note_keys(0, 0);
    ui.hold(PANEL_BTN_GLO, true);
    ui.note_keys(key(1), key(1));
    ui.note_keys(0, 0);
    ui.hold(PANEL_BTN_GLO, false);
    settle(&mut ui);
    assert_eq!(ui.small(0, 18, 3), "   ");
    assert_eq!(attr(&ui, 26, 2) & 15, unsafe { (*theme_get(0)).dim });
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut ui);
    assert_eq!(ui.big(0, 18, 2, 3), "MUT");
}

const ROW_STEP_CELLS_MAX: usize = 80;

#[test]
fn the_rows_show_the_page_of_the_row_and_the_bar_on_it() {
    let mut s = Song::new();
    open_tracker(&mut s.ui);
    let t = unsafe { &*theme_get(THEME_HIGH_CONTRAST) };
    let top = |j: usize| TRACKER_VIEW_TOP as usize + 2 * j;
    for j in 0..4 {
        assert_eq!(s.ui.small(top(j), 0, 25).trim(), "", "row {j}");
    }
    assert_eq!(s.ui.small(top(4) + 1, 0, 2), "00");
    assert_eq!(
        attr(&s.ui, 0, top(4) + 1),
        attr_of(u32::from(t.hot), u32::from(t.cursor))
    );
    assert_eq!(
        attr(&s.ui, 17, top(4)) >> 4,
        t.cursor,
        "the bar's last column"
    );
    assert_eq!(attr(&s.ui, 18, top(4)) >> 4, t.bg, "and no further");
    assert_eq!(
        attr(&s.ui, 0, top(5) + 1),
        attr_of(u32::from(t.label), u32::from(t.bg))
    );
    assert_eq!(s.ui.small(top(11) + 1, 0, 2), "07", "twelve rows");
    s.ui.press(PANEL_BTN_PLAY);
    s.f.st.row = [0x1a, 0, 0];
    s.f.st.order_pos = [0, 0, 0];
    settle(&mut s.ui);
    assert_eq!(s.ui.small(top(0) + 1, 0, 2), "14");
    assert_eq!(s.ui.small(top(6) + 1, 0, 2), "1A");
    assert_eq!(attr(&s.ui, 0, top(6) + 1) & 15, t.hot);
    assert_eq!(attr(&s.ui, 0, top(6) + 1) >> 4, t.cursor);
    s.f.st.row = [0x1b, 0, 0];
    s.ui.frame();
    let sent = drain(&mut s.ui.g);
    assert!(
        sent.cells > 0 && sent.cells <= ROW_STEP_CELLS_MAX,
        "{sent:?}"
    );
}

#[test]
fn an_idle_song_view_composes_nothing_and_a_page_turn_stays_within_the_budget() {
    let mut s = Song::new();
    open_tracker(&mut s.ui);
    let before = s.f.statuses;
    for _ in 0..10 {
        s.ui.frame();
        assert_eq!(drain(&mut s.ui.g).cells, 0, "idle");
    }
    assert_eq!(s.f.statuses - before, 10, "the status once a frame");
    s.ui.press(PANEL_BTN_PLAY);
    s.f.st.order_pos = [0, 0, 0];
    for row in 0..64u8 {
        s.f.st.row = [row, 0, 0];
        s.f.st.env = [row * 4, 0, 0];
        s.ui.frame();
        let sent = drain(&mut s.ui.g);
        assert!(
            sent.cells as u32 <= SONG_VIEW_FRAME_CELLS,
            "row {row}: {sent:?}"
        );
    }
}

#[test]
fn glo_double_tap_latches_and_a_tap_releases() {
    let mut s = Song::new();
    s.ui.rig.clear();
    s.ui.settle();
    s.ui.press(PANEL_BTN_GLO);
    assert_eq!(s.ui.p.glo_latch, 0, "one tap is no latch");
    s.ui.frames(12);
    s.ui.press(PANEL_BTN_GLO);
    assert_eq!(s.ui.p.glo_latch, 0, "the second tap came 500 ms later");
    s.ui.frame();
    s.ui.press(PANEL_BTN_GLO);
    assert_eq!(s.ui.p.glo_latch, 1, "two taps within 400 ms");
    s.ui.frame();
    assert_eq!(s.ui.toast().as_deref(), Some("GLO LATCHED"));
    let leds = unsafe { panel_get_leds(&*s.ui.p, &*s.ui.rig.live, std::ptr::null()) };
    assert_eq!(leds.buttons >> PANEL_BTN_GLO & 1, 1, "GLO is lit");
    s.ui.note_keys(key(1) | key(7), key(1) | key(7));
    assert_eq!(s.calls(), [Call::Mute(0b010)]);
    assert_eq!(gated(&s.ui), 1, "only key 8 sounds");
    s.ui.note_keys(0, 0);
    s.ui.press(PANEL_BTN_GLO);
    assert_eq!(s.ui.p.glo_latch, 0);
    s.ui.frame();
    assert_eq!(s.ui.toast().as_deref(), Some("GLO OFF"));
    s.ui.note_keys(key(1), key(1));
    assert_eq!(gated(&s.ui), 1, "key 2 plays again");
}

#[test]
fn live_shows_the_song_and_the_mutes() {
    let mut s = Song::new();
    s.ui.settle();
    let t = unsafe { &*theme_get(THEME_HIGH_CONTRAST) };
    assert_eq!(s.ui.top(), "LIVE");
    assert_eq!(ch(&s.ui, 22, 0), b' ');
    assert_eq!(s.ui.small(20, 0, 7), "       ");
    assert_eq!(ch(&s.ui, 24, 20), b' ');
    s.ui.press(PANEL_BTN_PLAY);
    s.f.st.row = [0, 0, 0];
    s.ui.frame();
    s.ui.frame();
    assert_eq!(
        ch(&s.ui, 22, 0),
        GRID_BIG_FIRST as u8,
        "the play mark at 2x"
    );
    assert_eq!(s.ui.g.src[22], FONT_PLAY as u8);
    assert_eq!(attr(&s.ui, 22, 0) & 15, t.hot, "a bar's first beat");
    assert_eq!(s.ui.small(20, 0, 7), "125 BPM");
    for v in 0..3 {
        assert_eq!(ch(&s.ui, 24 + 2 * v, 20), b'1' + v as u8);
        assert_eq!(attr(&s.ui, 24 + 2 * v, 20) & 15, t.label);
    }
    s.ui.frames(4);
    assert_eq!(attr(&s.ui, 22, 0) & 15, t.label, "between beats");
    s.f.st.row = [4, 4, 4];
    for _ in 0..20 {
        s.ui.frame();
        if s.ui.v.live.row_seen == 4 {
            break;
        }
    }
    assert_eq!(s.ui.v.live.row_seen, 4, "the row reached LIVE");
    assert_eq!(
        attr(&s.ui, 22, 0) & 15,
        t.value,
        "a beat that is not a bar's first"
    );
    s.ui.press(PANEL_BTN_GLO);
    s.ui.press(PANEL_BTN_GLO);
    s.ui.note_keys(key(1), key(1));
    s.ui.note_keys(0, 0);
    s.ui.settle();
    assert_eq!(s.ui.small(21, 0, 11), "GLO LATCHED");
    assert_eq!(s.ui.big(22, 0, 2, 2), "M1");
    assert_eq!(attr(&s.ui, 5, 22) >> 4, t.alert, "M2 is lit");
    assert_ne!(attr(&s.ui, 0, 22) >> 4, t.alert, "M1 is not");
    assert_eq!(attr(&s.ui, 26, 20) & 15, t.dim, "voice 2's badge");
    assert_eq!(&s.ui.row(0)[..8], "SAW LEAD");
    s.ui.press(PANEL_BTN_GLO);
    s.ui.settle();
    assert_eq!(s.ui.small(21, 0, 6), "FILTER");
}
