// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The song views' frame (app/song_view.c, M2d Task 1): SEQ, PLAY, REC and GLO through the
//! views, the jam, the header and the voice heads, against a fake of the player's binding
//! (app/song_transport.h) that stands for M2c Task 6's song control until it is bound.

use std::ffi::c_void;

use sidou_sys::*;

use crate::drain;
use crate::views::Ui;

const COLS: usize = GRID_COLS as usize;

/// What the views asked of the player.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Call {
    Play(u32),
    Stop,
    Resume,
    Mute(u32),
    Jam(u32, u32, u32),
}

/// The player, as the binding over song_ctl would show it: PLAY waits for the keys to leave
/// chip A (`ready`, keys_route_ready_for_song) before it plays; `refuse` stands for its
/// command ring full.
struct Fake {
    calls: Vec<Call>,
    st: song_status,
    ready: bool,
    refuse: bool,
    statuses: u32,
}

impl Fake {
    fn cmd(&mut self, c: Call) -> i32 {
        if self.refuse {
            return 0;
        }
        self.calls.push(c);
        match c {
            Call::Play(_) => {
                self.st.state = if self.ready {
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
unsafe extern "C" fn status(ctx: *mut c_void, out: *mut song_status) {
    let f = fake(ctx);
    f.statuses += 1;
    if f.st.state == SONG_T_STARTING as u8 && f.ready {
        f.st.state = SONG_T_PLAYING as u8;
    }
    unsafe { *out = f.st };
}

/// The views with the fake bound (both boxed: C keeps pointers to them).
struct Song {
    ui: Ui,
    f: Box<Fake>,
    _t: Box<song_transport>,
}

impl Song {
    fn new() -> Self {
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
            ready: true,
            refuse: false,
            statuses: 0,
        });
        let ctx: *mut Fake = &mut *f;
        let t = Box::new(song_transport {
            play: Some(play),
            stop: Some(stop),
            resume: Some(resume),
            mute: Some(mute),
            jam: Some(jam),
            status: Some(status),
            ctx: ctx.cast(),
        });
        unsafe { views_set_song(&mut *ui.v, &*t) };
        let mut s = Song { ui, f, _t: t };
        s.ui.frame();
        s.f.calls.clear();
        s
    }

    fn calls(&mut self) -> Vec<Call> {
        std::mem::take(&mut self.f.calls)
    }
}

/// Frames until no transition runs and the song view is whole on the grid.
fn settle(ui: &mut Ui) {
    for _ in 0..200 {
        ui.frame();
        drain(&mut ui.g);
        if unsafe { screen_animating(&ui.v.s) } == 0 && ui.v.song.copying == 0 {
            ui.frame();
            drain(&mut ui.g);
            return;
        }
    }
    panic!("the screen kept changing");
}

fn open_tracker(ui: &mut Ui) {
    ui.press(PANEL_BTN_SEQ);
    settle(ui);
    assert_eq!(ui.top(), "TRACKER");
}

fn attr(ui: &Ui, col: usize, row: usize) -> u8 {
    ui.g.attr[row * COLS + col]
}

fn ch(ui: &Ui, col: usize, row: usize) -> u8 {
    ui.g.ch[row * COLS + col]
}

const fn attr_of(fg: u32, bg: u32) -> u8 {
    (bg << 4 | fg) as u8
}

fn key(k: u32) -> u32 {
    1 << k
}

fn gated(ui: &Ui) -> usize {
    (0..3).filter(|&v| ui.rig.gated(v)).count()
}

/// The BPM in tenths for tempo 6 (two rows: 12 calls), 4 rows a beat.
#[test]
fn the_bpm_follows_the_frame_rate_the_multispeed_and_the_rows_a_beat() {
    let bpm = |sum, ms, ntsc, rows| unsafe { song_view_bpm10(sum, ms, ntsc, rows) };
    assert_eq!(bpm(12, 1, 0, 4), 1253, "tempo 6 on PAL: 125.3 BPM");
    assert_eq!(bpm(12, 2, 0, 4), 2506, "multispeed 2: 250.6");
    // Funktempo's two speeds, averaged: 5 and 7 play as 6.
    assert_eq!(bpm(5 + 7, 1, 0, 4), 1253);
    // Against the formula: the CIA period for multispeed (0x4cc7 / N + 1, 25 Hz 2 x 0x4cc7 + 1;
    // NTSC 0x42c6), the clock over it, 60 s, over the calls a beat.
    for ntsc in 0..2u32 {
        let (hz, cia) = if ntsc == 1 {
            (1_022_727.0, 0x42c6u32)
        } else {
            (985_248.0, 0x4cc7u32)
        };
        for ms in 0..=4u32 {
            let period = cia.checked_div(ms).map_or(2 * cia + 1, |p| p + 1);
            for (sum, rows) in [(4, 4), (12, 4), (12, 8), (31, 3), (510, 255), (2, 1)] {
                let want =
                    (600.0 * hz / f64::from(period) * 2.0 / f64::from(sum * rows)).round() as u32;
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
    // The only song view so far: SEQ stays on it.
    ui.press(PANEL_BTN_SEQ);
    settle(&mut ui);
    assert_eq!(ui.top(), "TRACKER");
    assert_eq!(ui.v.s.depth, 2);
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "LIVE");
    // From the status screen: HOME goes back to it.
    crate::views::open_status(&mut ui);
    settle(&mut ui);
    open_tracker(&mut ui);
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "STATUS");
    // The menu's TRACKER (STATUS was opened from the menu).
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "MENU");
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_TRACKER as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut ui);
    assert_eq!(ui.top(), "TRACKER");
}

#[test]
fn seq_steps_to_the_next_song_view_and_round() {
    let tracker: *const view = unsafe { &song_view_tracker };
    unsafe {
        assert_eq!(song_view_next(std::ptr::null()), tracker);
        assert_eq!(song_view_next(&views_menu), tracker);
        assert_eq!(song_view_next(tracker), tracker, "the only one: itself");
        assert_eq!(song_view_is(tracker), 1);
        assert_eq!(song_view_is(&views_menu), 0);
        assert_eq!(song_view_is(std::ptr::null()), 0);
    }
}

#[test]
fn the_header_shows_the_transport_the_tempo_the_position_and_rec() {
    // No player: stopped, no tempo, the cursor's position.
    let mut ui = Ui::without_cards();
    open_tracker(&mut ui);
    assert_eq!(ui.row(0), "TRACKER# --- ORD 00 ROW 00    ");
    assert_eq!(ch(&ui, 7, 0), FONT_SQUARE as u8);
    let header = attr_of(GRID_BLUE, GRID_LIGHT_BLUE);
    assert!((0..COLS).all(|c| attr(&ui, c, 0) == header));
    // REC, in red.
    ui.press(PANEL_BTN_REC);
    settle(&mut ui);
    assert_eq!(&ui.row(0)[26..], " REC");
    for c in 27..30 {
        assert_eq!(attr(&ui, c, 0), attr_of(GRID_RED, GRID_LIGHT_BLUE));
    }
    ui.press(PANEL_BTN_REC);
    settle(&mut ui);
    assert_eq!(&ui.row(0)[26..], "    ");

    // With the player: its tempo, and while it plays its position (the cursor's voice's).
    let mut s = Song::new();
    open_tracker(&mut s.ui);
    assert_eq!(s.ui.row(0), "TRACKER# 125 ORD 00 ROW 00    ");
    s.ui.press(PANEL_BTN_PLAY);
    s.f.st.order_pos = [3, 4, 5];
    s.f.st.row = [0x1a, 0x1b, 0x1c];
    settle(&mut s.ui);
    assert_eq!(s.ui.row(0), "TRACKER# 125 ORD 03 ROW 1A    ");
    assert_eq!(ch(&s.ui, 7, 0), FONT_PLAY as u8);
    // Multispeed 2 and funktempo 5/7: 250.6 rounded.
    s.f.st.multispeed = 2;
    s.f.st.speed = [5, 7];
    settle(&mut s.ui);
    assert_eq!(&s.ui.row(0)[8..13], " 251 ");
    // Stopped: the cursor's position again.
    s.ui.press(PANEL_BTN_PLAY);
    settle(&mut s.ui);
    assert_eq!(s.ui.row(0), "TRACKER# 251 ORD 00 ROW 00    ");
    assert_eq!(ch(&s.ui, 7, 0), FONT_SQUARE as u8);
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
    // In LIVE as well.
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
    s.f.ready = false; // keys_route_ready_for_song: not yet
    s.ui.press(PANEL_BTN_PLAY);
    assert_eq!(s.calls(), [Call::Play(0)]);
    for _ in 0..5 {
        s.ui.frame();
    }
    assert_eq!(s.ui.v.song.st.state, SONG_T_STARTING as u8);
    assert_eq!(
        ch(&s.ui, 7, 0),
        FONT_PLAY as u8,
        "starting shows as playing"
    );
    // A second PLAY while it starts stops it.
    s.ui.press(PANEL_BTN_PLAY);
    assert_eq!(s.calls(), [Call::Stop]);
    s.ui.frame();
    s.f.ready = true;
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
    // A PLAY not yet taken counts as done: a second press undoes it, nothing goes.
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
    // Key 1 (the leftmost): voice 1 muted, within the call.
    s.ui.note_keys(key(0), key(0));
    assert_eq!(s.calls(), [Call::Mute(0b001)]);
    s.ui.note_keys(0, 0);
    // Key 4: voice 1 soloed, so voices 2 and 3 are silent.
    s.ui.note_keys(key(3), key(3));
    assert_eq!(s.calls(), [Call::Mute(0b110)]);
    s.ui.note_keys(0, 0);
    // Key 5 too: voices 1 and 2 soloed.
    s.ui.note_keys(key(4), key(4));
    assert_eq!(s.calls(), [Call::Mute(0b100)]);
    s.ui.note_keys(0, 0);
    // Any other key: nothing, and it sounds nothing.
    s.ui.note_keys(key(16), key(16));
    assert!(s.calls().is_empty());
    assert_eq!(gated(&s.ui), 0, "a key under GLO sounds nothing");
    assert!(s.ui.rig.log().is_empty(), "{:?}", s.ui.rig.log());
    // GLO let go with the key still down: it stays silent until it goes up.
    s.ui.hold(PANEL_BTN_GLO, false);
    s.ui.note_keys(key(16), 0);
    assert_eq!(gated(&s.ui), 0);
    s.ui.note_keys(0, 0);
    s.ui.note_keys(key(16), key(16));
    assert_eq!(gated(&s.ui), 1, "pressed again without GLO: it plays");
    s.ui.note_keys(0, 0);
    // Unsolo both: the mute of voice 1 is back.
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
fn the_mutes_show_in_the_heads_and_in_lives_header() {
    let mut s = Song::new();
    s.f.st.wave = [0x41, 0x21, 0x11];
    s.f.st.env = [255, 128, 0];
    open_tracker(&mut s.ui);
    assert_eq!(s.ui.row(1), "   V1 PUL # V2 SAW # V3 TRI # ");
    let black = GRID_BLACK;
    for (v, color) in [GRID_CYAN, GRID_YELLOW, GRID_LIGHT_GREEN]
        .iter()
        .enumerate()
    {
        let x = 3 + 9 * v;
        assert_eq!(attr(&s.ui, x, 1), attr_of(*color, black), "V{}", v + 1);
        assert_eq!(attr(&s.ui, x + 3, 1), attr_of(*color, black));
        assert_eq!(attr(&s.ui, x + 7, 1), attr_of(*color, GRID_DARK_GREY));
    }
    // The meters: ENV 255 full, 128 half, 0 empty.
    assert_eq!(ch(&s.ui, 10, 1), FONT_V8 as u8);
    assert_eq!(ch(&s.ui, 19, 1), FONT_V4 as u8);
    assert_eq!(ch(&s.ui, 28, 1), FONT_V0 as u8);
    // Voice 1 muted, voice 3 soloed: voices 1 and 2 silent.
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(key(0) | key(5), key(0) | key(5));
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    settle(&mut s.ui);
    assert_eq!(s.ui.row(1), "   V1 MUT # V2 SAW # V3 SOL # ");
    assert_eq!(attr(&s.ui, 3, 1), attr_of(GRID_GREY, black));
    assert_eq!(attr(&s.ui, 6, 1), attr_of(GRID_LIGHT_RED, black));
    assert_eq!(
        attr(&s.ui, 12, 1),
        attr_of(GRID_GREY, black),
        "silent under the solo"
    );
    assert_eq!(attr(&s.ui, 21, 1), attr_of(GRID_LIGHT_GREEN, black));
    assert_eq!(attr(&s.ui, 24, 1), attr_of(GRID_WHITE, black));
    assert_eq!(
        s.calls(),
        [Call::Mute(0b011)],
        "both in one call: the solo wins"
    );
    // LIVE: the preset's place says so.
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "LIVE");
    assert_eq!(&s.ui.row(0)[13..23], "M1 S3     ");
    assert_eq!(attr(&s.ui, 13, 0), attr_of(GRID_RED, GRID_LIGHT_BLUE));
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(key(0) | key(5), key(0) | key(5));
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    settle(&mut s.ui);
    let name = crate::preset_name(crate::preset(s.ui.live().preset));
    assert_eq!(
        s.ui.row(0)[13..13 + name.len()],
        name,
        "none: the preset again"
    );
}

#[test]
fn a_key_in_a_song_view_jams_the_current_instrument_on_the_cursors_voice() {
    let mut s = Song::new();
    open_tracker(&mut s.ui);
    s.calls();
    s.ui.rig.clear();
    // Key 16 is A4 (MIDI 69): GoatTracker's A-4 is 57 (C-0 is 0).
    s.ui.note_keys(key(16), key(16));
    assert_eq!(s.calls(), [Call::Jam(0, 57, 1)]);
    assert_eq!(gated(&s.ui), 0, "the instrument does not sound");
    s.ui.note_keys(key(16), 0);
    assert!(s.calls().is_empty(), "held: nothing more");
    s.ui.note_keys(0, 0);
    assert_eq!(s.calls(), [Call::Jam(0, SONG_T_RELEASE, 1)]);
    // The panel's octave, the cursor's voice and instrument.
    s.ui.press(PANEL_BTN_OCT_UP);
    s.ui.v.song.voice = 2;
    s.ui.v.song.instr = 7;
    s.ui.note_keys(key(0), key(0));
    assert_eq!(s.calls(), [Call::Jam(2, 53 + 12 - 12, 7)]);
    // The last key wins: the first one's release does nothing, the second's lets it go.
    s.ui.note_keys(key(0) | key(2), key(2));
    assert_eq!(s.calls(), [Call::Jam(2, 55, 7)]);
    s.ui.note_keys(key(2), 0);
    assert!(s.calls().is_empty());
    s.ui.note_keys(0, 0);
    assert_eq!(s.calls(), [Call::Jam(2, SONG_T_RELEASE, 7)]);
    // Pressed and let go between two calls: the note and its release.
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
    // Held in LIVE: the instrument, and it stays the panel's in TRACKER.
    s.ui.note_keys(key(10), key(10));
    assert_eq!(gated(&s.ui), 1);
    open_tracker(&mut s.ui);
    s.calls();
    s.ui.note_keys(key(10), 0);
    assert!(s.calls().is_empty());
    assert_eq!(gated(&s.ui), 1);
    s.ui.note_keys(0, 0);
    assert_eq!(gated(&s.ui), 0);
    // A jam key held while HOME goes back to LIVE: its release still reaches the player, and
    // it does not sound the instrument there.
    s.ui.note_keys(key(12), key(12));
    assert_eq!(s.calls(), [Call::Jam(0, 53, 1)]);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    s.ui.note_keys(key(12), 0);
    assert_eq!(gated(&s.ui), 0);
    s.ui.note_keys(0, 0);
    assert_eq!(s.calls(), [Call::Jam(0, SONG_T_RELEASE, 1)]);
    // In LIVE the keys play the instrument.
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
    // PLAY does nothing, GLO's mutes are kept and shown.
    ui.press(PANEL_BTN_PLAY);
    assert_eq!(ui.v.song.st.state, SONG_T_IDLE as u8);
    ui.note_keys(0, 0);
    ui.hold(PANEL_BTN_GLO, true);
    ui.note_keys(key(1), key(1));
    ui.note_keys(0, 0);
    ui.hold(PANEL_BTN_GLO, false);
    settle(&mut ui);
    assert_eq!(&ui.row(1)[12..18], "V2 MUT");
}

#[test]
fn the_rows_show_the_page_of_the_row_and_the_bar_on_it() {
    let mut s = Song::new();
    open_tracker(&mut s.ui);
    // Stopped: the cursor's row 0, the page from row -4 (four blank rows).
    for j in 0..4 {
        assert_eq!(s.ui.row(2 + j).trim(), "", "row {j}");
    }
    assert_eq!(&s.ui.row(6)[..2], "00");
    assert_eq!(attr(&s.ui, 0, 6), attr_of(GRID_WHITE, GRID_BLUE), "the bar");
    assert_eq!(attr(&s.ui, 28, 6), attr_of(GRID_LIGHT_BLUE, GRID_BLUE));
    assert_eq!(attr(&s.ui, 0, 7), attr_of(GRID_GREY, GRID_BLACK));
    assert_eq!(
        attr(&s.ui, 0, 10),
        attr_of(GRID_LIGHT_GREY, GRID_BLACK),
        "row 04"
    );
    assert_eq!(&s.ui.row(26)[..2], "14");
    // Playing row $1A: the page from $0C, the bar on $1A, $10 in yellow.
    s.ui.press(PANEL_BTN_PLAY);
    s.f.st.row = [0x1a, 0, 0];
    s.f.st.order_pos = [0, 0, 0];
    settle(&mut s.ui);
    assert_eq!(&s.ui.row(2)[..2], "0C");
    assert_eq!(&s.ui.row(6)[..2], "10");
    assert_eq!(attr(&s.ui, 0, 6), attr_of(GRID_YELLOW, GRID_BLACK));
    assert_eq!(&s.ui.row(16)[..2], "1A");
    assert_eq!(attr(&s.ui, 0, 16), attr_of(GRID_WHITE, GRID_BLUE));
    // A row step moves the bar: a few cells, within the budget.
    s.f.st.row = [0x1b, 0, 0];
    s.ui.frame();
    let sent = drain(&mut s.ui.g);
    assert!(sent.cells > 0 && sent.cells <= 70, "{sent:?}");
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
