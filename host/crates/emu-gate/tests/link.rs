// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! FM-1 Live Link end to end in fm1-emu (docs/reference/live-link.md): fm1-emu's USB host
//! (common::usb_host) starts a session, keeps it with heartbeats and rebuilds the screen from
//! the stream with the reference decoder (the fm1-live-link crate), while the views run:
//! - the mirror draws what the LCD shows, pixel for pixel, once a view is at rest (LIVE,
//!   TRACKER, the menu, SCENE's way out), and while one moves (LIVE's scopes, TRACKER playing,
//!   SCENE) every cell of it shows what the LCD showed within the last two seconds;
//! - LINK in the LCD's bottom-left corner while the session runs, gone after it;
//! - the costs: each view's stream (bytes a second, under the 22 KB/s cap), the main loop's
//!   passes within the poll allowance, MIDI out's CCs and clock out's song position as they go
//!   without a session, and no packet inside a SysEx frame;
//! - remote control: a PANEL frame's press opens the menu, its steps turn PRESETS, and its
//!   held EDIT is let go half a second after the last PANEL frame.

mod common;

use common::usb_host::UsbHost;
use common::{build_dir, run_past_boot, skip_message, Firmware, Machine, POLL_INSTRUCTIONS};
use fm1_core::Bus;
use fm1_live_link as ll;
use sidou_sys::ilp32;

const W: usize = 240;
const COLS: usize = 30;
const HOME: u8 = 8;
const EDIT: u8 = 6;
const SEQ: u8 = 11;
const PLAY: u8 = 12;
const SELECT: u8 = 0;
const ALGORITHM: u8 = 1;
const K1: u8 = 2;
const PRESETS: u8 = 6;
/// A4 at octave 0 (note key 0 = F3 = MIDI 53, id 14).
const A4_KEY: u8 = 14 + 69 - 53;
/// The LINK stamp's cells (platform/fm1/ui.c): the bottom row's first two.
const STAMP: [(usize, usize); 2] = [(0, 29), (1, 29)];
/// How long a moving view's mirror may lag the LCD.
const LAG_MS: usize = 2000;

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

fn host(m: &mut Machine) -> &mut UsbHost {
    m.usb.as_mut().expect("a host")
}

/// Booted past the boot, a host on the cable with the device configured.
fn usb_machine(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    m.usb = Some(UsbHost::plug(&mut m.soc));
    run_past_boot(&mut m, fw);
    host(&mut m).start();
    for _ in 0..2_000 {
        if UsbHost::configured(&m.soc) {
            break;
        }
        m.usb_frames(1);
    }
    assert!(UsbHost::configured(&m.soc), "not enumerated");
    m.usb_frames(200);
    host(&mut m).ep1_in.clear();
    m
}

/// Each cell's 64 LCD pixels hashed, as the LCD has them now.
fn cell_hashes(fb: &[u16]) -> Vec<u64> {
    (0..COLS * COLS)
        .map(|i| {
            let (x0, y0) = (i % COLS * 8, i / COLS * 8);
            let mut h = 0xcbf2_9ce4_8422_2325u64;
            for y in 0..8 {
                for x in 0..8 {
                    h = (h ^ u64::from(fb[(y0 + y) * W + x0 + x])).wrapping_mul(0x100_0000_01b3);
                }
            }
            h
        })
        .collect()
}

fn stamp_cell(i: usize) -> bool {
    STAMP.iter().any(|&(c, r)| r * COLS + c == i)
}

/// The host's side: the session's frames out of EP1 IN, the mirror, the heartbeat.
struct Host {
    taken: usize,
    split: ll::Splitter,
    mirror: ll::Mirror,
    /// (ms, command, payload bytes) of every frame.
    log: Vec<(u64, u8, usize)>,
    beat_at: u64,
    beating: bool,
    /// The LCD's cells at each ms while `history` is on, the newest last.
    history: Option<std::collections::VecDeque<Vec<u64>>>,
    /// Packets that were not SysEx (channel, system common, real time), with their ms.
    others: Vec<(u64, [u8; 4])>,
    open: bool,
}

impl Host {
    fn new() -> Self {
        Host {
            taken: 0,
            split: ll::Splitter::default(),
            mirror: ll::Mirror::default(),
            log: Vec::new(),
            beat_at: 0,
            beating: true,
            history: None,
            others: Vec::new(),
            open: false,
        }
    }

    fn send(&mut self, m: &mut Machine, frame: &[u8]) {
        UsbHost::send(&mut m.soc, frame);
    }

    /// `ms` milliseconds a USB frame at a time: what came in decoded, a heartbeat a second.
    fn run(&mut self, m: &mut Machine, ms: u64) {
        for _ in 0..ms {
            if self.beating && self.mirror.on && m.ms() >= self.beat_at + 1000 {
                self.beat_at = m.ms();
                self.send(m, &ll::heartbeat());
            }
            m.usb_frames(1);
            self.take(m);
            if let Some(h) = &mut self.history {
                h.push_back(cell_hashes(&m.soc.lcd.fb));
                if h.len() > LAG_MS {
                    h.pop_front();
                }
            }
        }
    }

    fn take(&mut self, m: &mut Machine) {
        let now = m.ms();
        let in1 = &m.usb.as_ref().expect("a host").ep1_in;
        let mut frames = Vec::new();
        for p in in1[self.taken..].concat().as_chunks::<4>().0 {
            let cin = p[0] & 15;
            if (4..=7).contains(&cin) {
                if p[1] == 0xf0 {
                    assert!(!self.open, "a SysEx frame cut by another: {p:02x?}");
                }
                self.open = cin == 4;
            } else {
                assert!(
                    !self.open || (cin == 0xf && p[1] >= 0xf8),
                    "a packet inside a SysEx frame: {p:02x?}"
                );
                self.others.push((now, [p[0], p[1], p[2], p[3]]));
            }
            self.split.packets(p, &mut frames);
        }
        self.taken = in1.len();
        for f in frames {
            let Some((cmd, p)) = ll::parse(&f) else {
                continue; // SIDOU's own SysEx or the updater's
            };
            assert!(p.len() <= ll::PAYLOAD_MAX, "a {}-byte payload", p.len());
            assert!(
                self.mirror.apply(cmd, &p),
                "the mirror refused {cmd:#x} {p:02x?}"
            );
            self.log.push((now, cmd, p.len()));
        }
    }

    /// A session: START, then the palette and the font, until all of them are in.
    fn start(&mut self, m: &mut Machine) {
        self.send(m, &ll::start());
        self.send(m, &ll::palette());
        self.send(m, &ll::font());
        self.beat_at = m.ms();
        for _ in 0..3000 {
            self.run(m, 1);
            let glyphs = self.log.iter().filter(|e| e.1 == ll::GLYPHS).count();
            if self.mirror.on && glyphs * 5 > sidou_sys::FONT_LAST as usize {
                return;
            }
        }
        panic!(
            "no HELLO and font in 3 s: {:?}",
            &self.log[..self.log.len().min(8)]
        );
    }

    /// Payload bytes of the cell frames from `from` ms on, and their frames.
    fn cell_traffic(&self, from: u64) -> (usize, usize) {
        self.log
            .iter()
            .filter(|e| e.0 >= from && (e.1 == ll::CELLS || e.1 == ll::PIXELS))
            .fold((0, 0), |(b, n), e| (b + e.2, n + 1))
    }

    fn press(&mut self, m: &mut Machine, id: u8) {
        m.key(id, true);
        self.run(m, 30);
        m.key(id, false);
        self.run(m, 30);
    }

    /// Every cell of the mirror shows what the LCD showed in the last LAG_MS (the stamp's
    /// aside): how many cells were checked.
    fn assert_lags(&self, what: &str) -> usize {
        let h = self.history.as_ref().expect("the LCD's history");
        let fb = self.mirror.render();
        let now = cell_hashes(&fb);
        for (i, c) in now.iter().enumerate() {
            if stamp_cell(i) {
                continue;
            }
            assert!(
                h.iter().any(|past| past[i] == *c),
                "{what}: the mirror's cell ({}, {}) is nothing the LCD showed in {LAG_MS} ms",
                i % COLS,
                i / COLS
            );
        }
        now.len()
    }
}

/// The link with nothing left to send and the panel showing the composed frame (a run rendered
/// and not yet sent is not seen here: assert_mirror asks for this over 80 ms, in which any
/// such run reaches the panel and marks the link).
fn at_rest(m: &mut Machine, fw: &Firmware) -> bool {
    let link = fw.symbol("live_link");
    let g = fw.symbol("cells");
    let rows = m.soc.read32(link + ilp32::link_state::rows as u32);
    let pending = m.soc.read32(g + ilp32::grid::rows as u32);
    rows == 0 && pending == 0
}

/// Until the view and the link are at rest (a frame at a time), then the mirror against the
/// LCD, pixel for pixel but the stamp's cells, and against the grid the panel shows.
fn assert_mirror(h: &mut Host, m: &mut Machine, fw: &Firmware, what: &str) {
    let mut quiet = 0;
    for _ in 0..200 {
        h.run(m, 20);
        quiet = if at_rest(m, fw) { quiet + 1 } else { 0 };
        if quiet >= 4 {
            break;
        }
    }
    assert!(quiet >= 4, "{what}: never at rest");
    let fb = h.mirror.render();
    let lcd = &m.soc.lcd.fb;
    for i in 0..fb.len() {
        let cell = i / W / 8 * COLS + i % W / 8;
        if stamp_cell(cell) {
            continue;
        }
        assert_eq!(
            fb[i],
            lcd[i],
            "{what}: the mirror's pixel ({}, {}) against the LCD's",
            i % W,
            i / W
        );
    }
    let g = fw.symbol("cells");
    for i in 0..COLS * COLS {
        let ch = m.soc.read8(g + (ilp32::grid::shown_ch + i) as u32);
        let attr = m.soc.read8(g + (ilp32::grid::shown_attr + i) as u32);
        let c = h.mirror.cells[i];
        assert_eq!(
            (c.ch, c.attr),
            (ch, attr),
            "{what}: cell ({}, {})",
            i % COLS,
            i / COLS
        );
    }
}

/// The LCD's stamp cells: LINK, light green on black, or not.
fn stamped(m: &Machine) -> bool {
    let (green, black) = (0x9e90u16, 0x0000u16);
    let rows = [0u16, 0x452a, 0x45ac, 0x4568, 0x452c, 0x752a, 0, 0];
    (0..8).all(|y| {
        (0..16).all(|x| {
            let want = if rows[y] >> (15 - x) & 1 != 0 {
                green
            } else {
                black
            };
            m.soc.lcd.fb[(232 + y) * W + x] == want
        })
    })
}

/// Over a moving phase of `ms`: the cell bytes a second, the cell frames, and the lag check
/// every 250 ms.
fn moving(
    h: &mut Host,
    m: &mut Machine,
    ms: u64,
    what: &str,
    mut act: impl FnMut(&mut Machine, u64),
) -> serde_json::Value {
    h.history = Some(std::collections::VecDeque::new());
    h.run(m, 500); // the history filled a little, the stream under way
    let from = m.ms();
    let mut checked = 0;
    for step in 0..ms / 250 {
        act(m, step);
        h.run(m, 250);
        checked += h.assert_lags(what);
    }
    h.history = None;
    let (bytes, frames) = h.cell_traffic(from);
    let secs = (m.ms() - from) as f64 / 1000.0;
    let rate = bytes as f64 / secs;
    println!("{what}: {rate:.0} payload bytes/s in {frames} frames over {secs:.1} s");
    assert!(
        rate <= f64::from(sidou_sys::LINK_RATE) * 1000.0 * 1.02,
        "{what}: {rate:.0} bytes/s, over the cap"
    );
    serde_json::json!({
        "bytes_per_s": rate.round(),
        "frames_per_s": (frames as f64 / secs).round(),
        "cells_checked": checked,
    })
}

/// LIVE, scopes playing, TRACKER scrolling, the menu, SCENE: the mirror against the LCD, the
/// stream's rates, the passes' cost, the stamp.
#[test]
fn the_mirror_follows_live_tracker_the_menu_and_scene() {
    let Some(fw) = fixture("the Live Link mirror test") else {
        return;
    };
    let mut m = usb_machine(&fw);
    let mut h = Host::new();
    assert!(!stamped(&m), "LINK before a session");
    h.start(&mut m);
    let hello = h.mirror.hello.clone().expect("HELLO");
    assert_eq!((hello.cols, hello.rows, hello.version), (30, 30, 1));
    assert!(hello.name.starts_with("SIDOU "), "{hello:?}");
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    m.watch_calls(fw.symbol("midi_link_run"));
    let mut report = serde_json::Map::new();

    assert_mirror(&mut h, &mut m, &fw, "LIVE at rest");
    assert!(stamped(&m), "no LINK on the LCD during a session");

    // a note a quarter second, each a new attack and pitch: the scopes redraw throughout
    m.key(A4_KEY, true);
    let live = moving(&mut h, &mut m, 2000, "LIVE, notes' scopes", |m, step| {
        m.key(A4_KEY + ((step + 3) % 4) as u8 * 2, false);
        m.key(A4_KEY + (step % 4) as u8 * 2, true);
    });
    report.insert("live_scopes".into(), live);
    for k in 0..4 {
        m.key(A4_KEY + k * 2, false);
    }
    h.run(&mut m, 2000);
    assert_mirror(&mut h, &mut m, &fw, "LIVE after the note");

    h.press(&mut m, SEQ); // STEP, then TRACKER
    h.press(&mut m, SEQ);
    h.run(&mut m, 700);
    assert_eq!(
        common::top_view(&mut m, &fw),
        fw.symbol("tracker_view_view")
    );
    assert_mirror(&mut h, &mut m, &fw, "TRACKER");
    h.press(&mut m, PLAY);
    report.insert(
        "tracker_playing".into(),
        moving(&mut h, &mut m, 3000, "TRACKER playing", |_, _| {}),
    );
    h.press(&mut m, PLAY);
    assert_mirror(&mut h, &mut m, &fw, "TRACKER stopped");

    for _ in 0..3 {
        if common::top_view(&mut m, &fw) == fw.symbol("views_menu") {
            break;
        }
        h.press(&mut m, HOME);
        h.run(&mut m, 700);
    }
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("views_menu"));
    assert_mirror(&mut h, &mut m, &fw, "the menu");

    // SCENE from the menu, past its title card
    m.turn(SELECT, -(sidou_sys::VIEWS_MENU_ITEMS as i32));
    h.run(&mut m, 10 * u64::from(sidou_sys::VIEWS_MENU_ITEMS));
    m.turn(SELECT, sidou_sys::VIEWS_MENU_SCENE as i32);
    h.run(&mut m, 10 * u64::from(sidou_sys::VIEWS_MENU_SCENE) + 100);
    m.turn(ALGORITHM, 1);
    h.run(&mut m, 4000);
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("scene_view_view"));
    report.insert(
        "scene".into(),
        moving(&mut h, &mut m, 3000, "SCENE", |_, _| {}),
    );
    h.press(&mut m, HOME);
    h.run(&mut m, 1000);
    assert_mirror(&mut h, &mut m, &fw, "back from SCENE");

    let pass = *m.passes.iter().max().unwrap();
    let link = *m.watched_calls.iter().max().unwrap();
    println!("the busiest pass {pass} instructions, midi_link_run at most {link}");
    assert!(pass <= POLL_INSTRUCTIONS, "a pass took {pass} instructions");
    report.insert("max_pass_instructions".into(), pass.into());
    report.insert("max_link_run_instructions".into(), link.into());
    report.insert("poll_instructions".into(), POLL_INSTRUCTIONS.into());

    // STOP: no more frames, the stamp gone
    h.beating = false;
    h.send(&mut m, &ll::stop());
    h.run(&mut m, 200);
    assert!(!h.mirror.on, "no STOPPED");
    let after = h.log.len();
    h.press(&mut m, HOME);
    h.run(&mut m, 500);
    assert_eq!(h.log.len(), after, "frames after STOP");
    assert!(!stamped(&m), "LINK after the session");
    println!("{}", serde_json::Value::Object(report.clone()));
    if let Some(build) = build_dir() {
        std::fs::write(
            build.join("link_mirror.json"),
            serde_json::Value::Object(report).to_string(),
        )
        .unwrap();
    }
}

/// No heartbeat for 3 s: the session ends by itself, STOPPED says so, LINK goes.
#[test]
fn a_session_ends_three_seconds_after_the_last_heartbeat() {
    let Some(fw) = fixture("the Live Link timeout test") else {
        return;
    };
    let mut m = usb_machine(&fw);
    let mut h = Host::new();
    h.start(&mut m);
    h.beating = false;
    h.run(&mut m, 200);
    assert!(h.mirror.on && stamped(&m));
    let last = h.beat_at;
    let wait = last + 2990 - m.ms();
    h.run(&mut m, wait);
    assert!(h.mirror.on, "ended before 3 s");
    h.run(&mut m, 200);
    assert!(!h.mirror.on, "still on 3.5 s after the last heartbeat");
    let stopped = h.log.iter().find(|e| e.1 == ll::STOPPED).unwrap().0;
    println!("STOPPED {} ms after the last heartbeat", stopped - last);
    assert!((3000..3100).contains(&(stopped - last)));
    h.run(&mut m, 200);
    assert!(!stamped(&m), "LINK after the timeout");
}

/// A PANEL frame: HOME pressed opens the menu, PRESETS' steps step the preset, EDIT held is
/// held until half a second after the last PANEL frame. The note keys stay MIDI's: none of it
/// plays a note.
#[test]
fn a_hosts_panel_frame_presses_turns_and_is_let_go() {
    let Some(fw) = fixture("the Live Link remote test") else {
        return;
    };
    let mut m = usb_machine(&fw);
    let mut h = Host::new();
    h.start(&mut m);
    let preset = fw.symbol("instrument") + ilp32::live::preset as u32;
    let before = m.soc.read8(preset);
    let mut steps = [0i8; 7];
    steps[usize::from(PRESETS)] = 1;
    h.send(&mut m, &ll::panel(0, 0, &steps));
    h.run(&mut m, 200);
    assert_eq!(
        m.soc.read8(preset),
        before.wrapping_add(1),
        "PRESETS a step on"
    );
    h.send(&mut m, &ll::panel(1 << HOME, 0, &[]));
    h.run(&mut m, 100);
    h.send(&mut m, &ll::panel(0, 0, &[]));
    h.run(&mut m, 700);
    assert_eq!(
        common::top_view(&mut m, &fw),
        fw.symbol("views_menu"),
        "HOME from the host opened the menu"
    );
    let held = fw.symbol("front") + ilp32::panel::held as u32;
    h.send(&mut m, &ll::panel(1 << EDIT, 0, &[]));
    let sent = m.ms();
    h.run(&mut m, 50);
    assert_eq!(
        m.soc.read32(held) & 1 << EDIT,
        1 << EDIT,
        "EDIT held from the host"
    );
    let mut let_go = None;
    for _ in 0..800 {
        h.run(&mut m, 1);
        if m.soc.read32(held) & 1 << EDIT == 0 {
            let_go = Some(m.ms() - sent);
            break;
        }
    }
    let let_go = let_go.expect("EDIT never let go");
    println!("EDIT let go {let_go} ms after the last PANEL frame");
    // (the frame reaches the device in the next USB frame, the panel in the next key scan)
    assert!((500..=510).contains(&let_go), "{let_go} ms");
}

/// K1 turned fast on LIVE with notes' scopes streaming: MIDI out's CCs go as they do with no
/// session (as many, the same values, none deferred for room), and none inside a frame.
#[test]
fn midi_outs_ccs_go_as_without_a_session() {
    let Some(fw) = fixture("the Live Link MIDI out test") else {
        return;
    };
    let cc = |with: bool| {
        let mut m = usb_machine(&fw);
        let mut h = Host::new();
        if with {
            h.start(&mut m);
        }
        h.run(&mut m, 500);
        let from = m.ms();
        for step in 0..20u8 {
            // a new note every turn: the scopes redraw while the CCs go
            m.key(A4_KEY + (step + 3) % 4 * 2, false);
            m.key(A4_KEY + step % 4 * 2, true);
            m.turn(K1, 3);
            h.run(&mut m, 40);
        }
        h.run(&mut m, 300);
        let out = fw.symbol("midi_tx");
        let deferred = m.soc.read32(out + ilp32::midi_out::deferred as u32);
        let ccs: Vec<[u8; 4]> = h
            .others
            .iter()
            .filter(|(t, p)| *t >= from && p[0] & 15 == 0xb)
            .map(|e| e.1)
            .collect();
        let frames = h.cell_traffic(from).1;
        (ccs, deferred, frames)
    };
    let (alone, deferred_alone, _) = cc(false);
    let (with, deferred_with, frames) = cc(true);
    println!(
        "{} CCs alone, {} with {frames} cell frames; deferred {deferred_alone} / {deferred_with}",
        alone.len(),
        with.len()
    );
    assert!(frames > 50, "the scopes streamed only {frames} frames");
    assert!(!alone.is_empty());
    // the same last value, and no fewer of them (which detents a 10 ms tick catches can differ)
    assert_eq!(
        with.last(),
        alone.last(),
        "MIDI out's last CC changed under a session"
    );
    assert!(
        with.len() + 1 >= alone.len(),
        "{with:02x?} against {alone:02x?}"
    );
    assert_eq!(deferred_with, deferred_alone, "MIDI out deferred for room");
}

/// Clock out on, TRACKER's song played, stopped part way and continued (EDIT + PLAY: a song
/// position, then a continue) while a session streams: each of clock out's messages reaches the
/// host as soon after the device starts or resumes the song as without a session (the song
/// itself starts on its next player frame, whenever that is), its clocks as evenly, and none
/// inside a frame (Host::take checks every packet).
#[test]
fn clock_outs_song_position_goes_as_without_a_session() {
    let Some(fw) = fixture("the Live Link clock out test") else {
        return;
    };
    let run = |with: bool| {
        let mut m = usb_machine(&fw);
        let mut h = Host::new();
        let syncing = fw.symbol("syncing");
        m.soc
            .write8(syncing + ilp32::midi_sync::clock_out as u32, 1);
        if with {
            h.start(&mut m);
        }
        h.press(&mut m, SEQ); // STEP, then TRACKER
        h.press(&mut m, SEQ);
        h.run(&mut m, 700);
        let calls = fw.symbol("song_transport") + ilp32::song_ctl::calls as u32;
        // The ms the song's calls first move after `keys` go down, and the ms each of `bytes`
        // first reaches the host from then on.
        let go = |h: &mut Host, m: &mut Machine, keys: &[u8], bytes: &[u8]| {
            let c0 = m.soc.read32(calls);
            let from = m.ms();
            for &k in keys {
                m.key(k, true);
            }
            let mut began = None;
            for _ in 0..60 {
                h.run(m, 1);
                if began.is_none() && m.soc.read32(calls) != c0 {
                    began = Some(m.ms());
                }
            }
            for &k in keys {
                m.key(k, false);
            }
            let began = began.expect("the song never moved");
            let arrived: Vec<i64> = bytes
                .iter()
                .map(|&b| {
                    let t = h.others.iter().find(|(t, p)| *t >= from && p[1] == b);
                    t.expect("never reached the host").0 as i64 - began as i64
                })
                .collect();
            arrived
        };
        let start = go(&mut h, &mut m, &[PLAY], &[0xfa]);
        let from = m.ms();
        h.run(&mut m, 2000);
        // the clocks while it plays, a frame a beat or more streaming beside them
        let clocks: Vec<u64> = h
            .others
            .iter()
            .filter(|(t, p)| *t >= from && p[1] == 0xf8)
            .map(|e| e.0)
            .collect();
        let gaps: Vec<i64> = clocks.windows(2).map(|w| (w[1] - w[0]) as i64).collect();
        let mut sorted = gaps.clone();
        sorted.sort_unstable();
        let median = sorted[sorted.len() / 2];
        let jitter = gaps.iter().map(|g| (g - median).abs()).max().unwrap();
        let frames = h.cell_traffic(from).1;
        h.press(&mut m, PLAY);
        h.run(&mut m, 300);
        m.key(EDIT, true);
        h.run(&mut m, 10);
        let cont = go(&mut h, &mut m, &[PLAY], &[0xf2, 0xfb]);
        m.key(EDIT, false);
        (start, cont, jitter, median, frames)
    };
    let alone = run(false);
    let with = run(true);
    println!(
        "start, song position and continue after the song moved: {:?} {:?} ms alone, {:?} {:?} \
         with; clocks {} ms apart, at most {} / {} off; {} cell frames while playing",
        alone.0, alone.1, with.0, with.1, alone.3, alone.2, with.2, with.4
    );
    assert!(with.4 > 20, "nothing streamed while the song played");
    for (w, a) in with
        .0
        .iter()
        .chain(&with.1)
        .zip(alone.0.iter().chain(&alone.1))
    {
        assert!(
            *w <= a + 1,
            "a transport message {w} ms after the song moved, {a} alone"
        );
    }
    assert!(
        with.2 <= alone.2 + 1,
        "the clocks' jitter {} against {}",
        with.2,
        alone.2
    );
}
