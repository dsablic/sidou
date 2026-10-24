// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The SID views on the device (M3 Task 11, docs/plans/2026-10-19-m3-sid-files.md, "The browser
//! and the transport"; app/sid_view.h), end to end in fm1-emu: the menu's SIDS, a tune, ALGORITHM
//! clockwise plays it; GLO mutes voice 2 (the firmware's audio equal to the host's render with
//! the mute at the same block, sample for sample, and unequal to the render without it), and
//! solos voice 3; ALGORITHM steps the song; EDIT + PLAY pauses and resumes; PLAY stops; the
//! unsaved-song question each way (ALGORITHM again discards, SAVE saves it first; HOME cancels);
//! MIDI start, stop and continue over TRS; and through all of it, and over the browser's pages
//! and NOW PLAYING with a tune moving, every ui_poll within SIDS_WORST_POLL, every main-loop pass
//! within POLL_INSTRUCTIONS and every frame within the song views' 200 cells.

mod common;

use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::psid::packed;
use sidou_tests::song::Song;

const WAIT_MS: u64 = 15_000;
const SELECT: u8 = 0;
const ALGORITHM: u8 = 1;
const EDIT: u8 = PANEL_BTN_EDIT as u8;
const GLO: u8 = PANEL_BTN_GLO as u8;
const HOME: u8 = PANEL_BTN_HOME as u8;
const PLAY: u8 = PANEL_BTN_PLAY as u8;
/// The first note key's id (app/panel.h: note keys from 14).
const KEY0: u8 = 14;
/// The DMA plays the zeroed buffer before the first half the interrupt filled.
const MAX_SILENT_HALVES: usize = 4;
/// main.c's tune states: TUNE_OFF, TUNE_ON.
const TUNE_OFF: u8 = 0;
const TUNE_ON: u8 = 5;
/// audio.h AUDIO_PSID.
const AUDIO_PSID: u8 = 1;
/// The song-I/O hook (main.c song_io_test_req): SAVE, LOAD.
const IO_SAVE: u32 = 2;
const IO_LOAD: u32 = 3;
/// The SID views' busiest ui_poll, tied to the main loop's allowance as every view's is
/// (screen.rs): measured 35,909 (the browser's first frames as it opens from the menu: its canvas
/// built whole and copied; a page read 35,489; NOW PLAYING as a tune plays 18,048), plus 10 %.
const SIDS_WORST_POLL: u64 = 39_500;
const _: () = assert!(SIDS_WORST_POLL <= POLL_INSTRUCTIONS);

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

fn until(m: &mut Machine, what: &str, mut done: impl FnMut(&mut Machine) -> bool) {
    let start = m.ms();
    while !done(m) {
        assert!(m.ms() - start < WAIT_MS, "timed out: {what}");
        m.run_ms(1);
    }
}

fn ctl(fw: &Firmware) -> u32 {
    fw.symbol("player_isr")
}

fn psid_state(m: &mut Machine, fw: &Firmware) -> u32 {
    u32::from(m.soc.read8(ctl(fw) + ilp32::psid_ctl::state as u32))
}

fn writes(m: &mut Machine, fw: &Firmware) -> u32 {
    m.soc.read32(ctl(fw) + ilp32::psid_ctl::writes as u32)
}

fn tune_state(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc.read8(fw.symbol("sid_tune"))
}

/// The song psid_ctl's environment plays (psid_ctl.w, its psid_env's song).
fn env_song(m: &mut Machine, fw: &Firmware) -> u32 {
    let w = m.soc.read32(ctl(fw) + ilp32::psid_ctl::w as u32);
    u32::from(
        m.soc
            .read16(w + (ilp32::psid_work::env + ilp32::psid_env::song) as u32),
    )
}

fn ui_field(fw: &Firmware, offset: usize) -> u32 {
    fw.symbol("ui") + offset as u32
}

/// The first GoatTracker example with more than one subtune, packed as a PSID: a tune of songs.
fn songs_tune() -> Vec<u8> {
    let (_, sng) = gt2_ref::examples()
        .into_iter()
        .find(|(_, s)| gt2_ref::subtunes(s) >= 2)
        .expect("an example with two subtunes");
    packed(&sng, 0xD400)
}

fn dojo() -> Vec<u8> {
    let (_, sng) = gt2_ref::examples()
        .into_iter()
        .find(|(n, _)| n == "dojo.sng")
        .expect("dojo.sng");
    packed(&sng, 0xD400)
}

fn boot_with(fw: &Firmware, files: &[(&str, &[u8])]) -> Machine {
    let mut flash = settled_flash(&fw.image);
    for (name, file) in files {
        put_file(&mut flash, name, STORE_FILE_SID, file);
    }
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(&flash);
    m
}

fn song_idle(m: &mut Machine, fw: &Firmware) {
    let state = fw.symbol("songs") + ilp32::song_io::state as u32;
    until(m, "song I/O idle", |m| {
        m.soc.read8(state) == SONG_IO_IDLE as u8
    });
}

/// A request to the song-I/O hook (main.c song_io_test_hook), and its wait for song_io's idle.
fn song_io(m: &mut Machine, fw: &Firmware, op: u32, name: &str) {
    let hook = fw.symbol("song_io_test_hook");
    for i in 0..=STORE_NAME_MAX {
        let b = name.as_bytes().get(i as usize).copied().unwrap_or(0);
        m.soc.write8(hook + 4 + i, b);
    }
    m.soc.write32(hook + 24, u32::MAX);
    m.soc.write32(hook, op);
    until(m, "the main loop takes the song request", |m| {
        m.soc.read32(hook) == 0
    });
    assert_eq!(m.soc.read32(hook + 24) as i32, 1, "song I/O op {op}");
    song_idle(m, fw);
}

/// The song the device holds, as its file (psid.rs's).
fn song_file(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let mut song = Song::device();
    let base = fw.symbol("__song_region");
    for i in 0..SONG_REGION_BYTES {
        // SAFETY: the model holds offsets, not pointers, laid out alike on both builds.
        unsafe {
            song.ptr_mut()
                .cast::<u8>()
                .add(i as usize)
                .write(m.soc.read8(base + i))
        };
    }
    song.write(true)
}

/// An edit of the song (platform/fm1/main.c `song_test_hook`: op, pattern, row, cell): pattern 0,
/// row 5 written with a note the song has.
fn edit_song(m: &mut Machine, fw: &Firmware) {
    let hook = fw.symbol("song_test_hook");
    m.soc.write32(hook + 4, 0);
    m.soc.write32(hook + 8, 5);
    for (k, &c) in [0x60u8 + 12 + 9, 1, 0, 0].iter().enumerate() {
        m.soc.write8(hook + 12 + k as u32, c);
    }
    m.soc.write32(hook, 1);
    until(m, "the edit made", |m| m.soc.read32(hook) == 0);
    m.run_ms(200);
}

/// What a stretch of time cost: each compose's cells (screen.rs's Composes: from a compose to
/// the next, read from platform/fm1/lcd.c `lcd_count`) and the dearest ui_poll in it, with the
/// views on top (their `view` tables' addresses) as the stretch began and as it ended.
#[derive(Default)]
struct Cost {
    cells: Vec<u32>,
    polls: Vec<(u32, u32, u64)>,
}

/// `ms` of running a tenth of a millisecond at a time, its frames and ui_polls into `cost`.
fn frames(m: &mut Machine, fw: &Firmware, ms: u64, cost: &mut Cost) {
    let top = top_view(m, fw);
    let polls = m.watched_calls.len();
    let cells = &mut cost.cells;
    let last = fw.symbol("last_compose");
    let count = fw.symbol("lcd_count");
    let sent = |m: &mut Machine| (m.soc.read32(count), m.soc.read32(count + 4));
    let mut stamp = m.soc.read32(last);
    let mut from = sent(m);
    let step = m.soc.periph.cycle_of_ms(1) / 10;
    let end = m.soc.periph.cycles + m.soc.periph.cycle_of_ms(ms);
    while m.soc.periph.cycles < end {
        m.run_cycles(step);
        let s = m.soc.read32(last);
        if s != stamp {
            stamp = s;
            let now = sent(m);
            let (windows, bytes) = (now.0 - from.0, now.1 - from.1);
            cells.push((bytes - windows * 11) / 128);
            from = now;
        }
    }
    let worst = m.watched_calls[polls..].iter().copied().max().unwrap_or(0);
    let end = top_view(m, fw);
    cost.polls.push((top, end, worst));
}

/// The device's audio against the host's render of `file` (psid.rs's matching: over the silent
/// halves the DMA may lead with and the block the keys' registers reach chip B in), with GLO's
/// mutes `mute` at their block; Ok, or the closest it came.
fn matches(
    audio: &[i16],
    file: &[u8],
    snaps: &Snaps,
    mute: Option<(usize, u32)>,
) -> Result<usize, i32> {
    let mut closest = i32::MAX;
    let rewrites: Vec<Option<usize>> = match snaps.b_use {
        Some(b) => (b.saturating_sub(1)..=b + 1).map(Some).collect(),
        None => vec![None],
    };
    for silent in 0..=MAX_SILENT_HALVES {
        if audio[..2 * silent * HALF_FRAMES].iter().any(|&v| v != 0) {
            break;
        }
        let played = &audio[2 * silent * HALF_FRAMES..];
        for &r in &rewrites {
            let want = render_host_psid(
                played.len() / 2,
                &PsidPlay {
                    file,
                    song: 1,
                    psid_block: snaps.psid_block,
                    fresh_block: snaps.fresh_block,
                    start_block: snaps.start_block,
                    b_use_block: snaps.b_use,
                    b_rewrite_block: r,
                    mute_block: mute,
                },
            );
            let d = max_diff(played, &want);
            closest = closest.min(d);
            if d == 0 {
                return Ok(silent);
            }
        }
    }
    Err(closest)
}

/// The blocks the firmware's events reached the interrupt at (psid.rs's Played), and the block
/// chip A's mute became `mask`.
struct Snaps {
    psid_block: usize,
    fresh_block: [Option<usize>; 2],
    start_block: usize,
    b_use: Option<usize>,
}

/// The snapshot bytes: the player's mode, psid_ctl's blocks, chip B in use, the two pairs' active
/// sids, and chip A's pair's two mutes.
fn watch(m: &mut Machine, fw: &Firmware) {
    let chips = fw.chips();
    let active = chips + ilp32::reg_ring_chips::active as u32;
    let sid0 = chips + ilp32::reg_ring_chips::chip as u32;
    m.snapshot_bytes = vec![
        fw.symbol("player_now"),
        ctl(fw) + ilp32::psid_ctl::blocks as u32,
        fw.symbol("b_on"),
        active,
        active + 1,
        sid0 + ilp32::sid::mute as u32,
        sid0 + (ilp32::sid::SIZE + ilp32::sid::mute) as u32,
    ];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 1)));
}

fn snaps(m: &Machine) -> Snaps {
    let snaps = &m.buffer_snapshots;
    let psid_block = snaps
        .iter()
        .position(|s| s.bytes[0] == AUDIO_PSID)
        .expect("the interrupt never took the tune");
    let fresh = |k: usize| {
        let was = snaps[psid_block].bytes[3 + k];
        snaps[psid_block..]
            .iter()
            .position(|s| s.bytes[3 + k] != was)
            .map(|i| i + psid_block)
    };
    let start_block = snaps[psid_block..]
        .iter()
        .position(|s| s.bytes[1] != 0)
        .expect("the tune never started")
        + psid_block;
    Snaps {
        psid_block,
        fresh_block: [fresh(0), None],
        start_block,
        b_use: snaps.iter().position(|s| s.bytes[2] != 0),
    }
}

/// The first block chip A (the sid its pair plays then) has mute `mask`, after `from`.
fn mute_block(m: &Machine, from: usize, mask: u8) -> Option<usize> {
    m.buffer_snapshots[from..]
        .iter()
        .position(|s| s.bytes[5 + usize::from(s.bytes[3])] == mask)
        .map(|i| i + from)
}

/// GLO held and note key `k` (the mutes 0-2, the solos 3-5).
fn glo_key(m: &mut Machine, k: u8) {
    m.key(GLO, true);
    m.run_ms(30);
    m.key(KEY0 + k, true);
    m.run_ms(30);
    m.key(KEY0 + k, false);
    m.run_ms(30);
    m.key(GLO, false);
    m.run_ms(450); /* (GLO again sooner would latch its layer: a double tap) */
}

/// Every pass within POLL_INSTRUCTIONS, every frame within the song views' 200 cells, every
/// ui_poll under the SID views within SIDS_WORST_POLL (and under the others, the menu, STATUS
/// and LIVE, theirs: within POLL_INSTRUCTIONS here, screen.rs has each one's own).
fn check_budgets(m: &Machine, fw: &Firmware, cost: &Cost, what: &str) {
    let pass = m.passes.iter().copied().max().unwrap_or(0);
    let sids = [fw.symbol("sid_view_browser"), fw.symbol("sid_view_playing")];
    let theirs = |&&(from, to, _): &&(u32, u32, u64)| sids.contains(&from) || sids.contains(&to);
    let ours = cost
        .polls
        .iter()
        .filter(theirs)
        .map(|p| p.2)
        .max()
        .unwrap_or(0);
    let others = cost.polls.iter().map(|p| p.2).max().unwrap_or(0);
    let most = cost.cells.iter().copied().max().unwrap_or(0);
    let name = |v: u32| {
        [
            "sid_view_browser",
            "sid_view_playing",
            "views_menu",
            "views_status",
            "live_view_view",
        ]
        .into_iter()
        .find(|n| fw.symbol(n) == v)
        .unwrap_or("?")
    };
    for &(from, to, p) in &cost.polls {
        println!("  {} to {}: the dearest ui_poll {p}", name(from), name(to));
    }
    println!(
        "{what}: the busiest pass {pass} (POLL_INSTRUCTIONS {POLL_INSTRUCTIONS}), the dearest \
         ui_poll under the SID views {ours} (SIDS_WORST_POLL {SIDS_WORST_POLL}), elsewhere \
         {others}; {} frames, the most cells {most}",
        cost.cells.len()
    );
    assert!(pass <= POLL_INSTRUCTIONS, "{what}: a pass of {pass}");
    assert!(ours <= SIDS_WORST_POLL, "{what}: a ui_poll of {ours}");
    assert!(others <= POLL_INSTRUCTIONS, "{what}: a ui_poll of {others}");
    assert!(
        most <= SONG_VIEW_FRAME_CELLS,
        "{what}: a frame of {most} cells"
    );
}

/// The menu's SIDS, the tune, ALGORITHM clockwise: it plays. GLO + the second key mutes voice 2 on
/// the tune's chip: the audio equals the host's render muted at that block, sample for sample
/// (and the unmuted render differs); GLO + the sixth key solos voice 3 (voices 1 and 2 muted on
/// the chip). ALGORITHM steps to song 2, which plays from init; EDIT + PLAY pauses and resumes;
/// PLAY stops. Every pass, ui_poll and frame within its budget throughout.
#[test]
fn a_tune_from_the_menu_plays_mutes_solos_steps_pauses_and_stops() {
    let Some(fw) = fixture("the SID views test") else {
        return;
    };
    let file = songs_tune();
    let mut m = boot_with(&fw, &[("SONGS", &file)]);
    watch(&mut m, &fw);
    run_past_boot(&mut m, &fw);
    song_idle(&mut m, &fw);
    m.watch_calls(fw.symbol("ui_poll"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut cells = Cost::default();
    ui_play_tune(&mut m, &fw, 0);
    until(&mut m, "the tune plays", |m| {
        tune_state(m, &fw) == TUNE_ON && writes(m, &fw) > 200
    });
    frames(&mut m, &fw, 1_500, &mut cells);
    // GLO: voice 2 muted
    let before = m.buffer_snapshots.len();
    glo_key(&mut m, 1);
    frames(&mut m, &fw, 1_500, &mut cells);
    let audio = m.soc.periph.audio.clone();
    let s = snaps(&m);
    let muted = mute_block(&m, before, 2).expect("voice 2 never muted on chip A");
    let with = matches(&audio, &file, &s, Some((muted, 2)));
    let without = matches(&audio, &file, &s, None);
    println!(
        "on AUDIO_PSID at block {}, started at {}, muted at {muted}: with the mute {with:?}, \
         without {without:?}",
        s.psid_block, s.start_block
    );
    assert!(
        with.is_ok(),
        "the firmware's audio differs from the host's muted render"
    );
    assert!(without.is_err(), "the mute changed nothing");
    // GLO: voice 3 soloed (with 2 muted: the solo wins, voices 1 and 2 silent)
    let before = m.buffer_snapshots.len();
    glo_key(&mut m, 5);
    frames(&mut m, &fw, 300, &mut cells);
    assert!(
        mute_block(&m, before, 3).is_some(),
        "the solo never reached chip A"
    );
    glo_key(&mut m, 5);
    glo_key(&mut m, 1);
    frames(&mut m, &fw, 300, &mut cells);
    // ALGORITHM: song 2, from init
    m.turn(ALGORITHM, 1);
    until(&mut m, "song 2 plays", |m| {
        tune_state(m, &fw) == TUNE_ON
            && env_song(m, &fw) == 2
            && psid_state(m, &fw) == PSID_PLAYING
            && writes(m, &fw) > 100
    });
    frames(&mut m, &fw, 1_000, &mut cells);
    // EDIT + PLAY: paused, then on
    m.key(EDIT, true);
    press(&mut m, PLAY);
    m.key(EDIT, false);
    until(&mut m, "paused", |m| psid_state(m, &fw) == PSID_PAUSED);
    let w = writes(&mut m, &fw);
    frames(&mut m, &fw, 500, &mut cells);
    assert_eq!(writes(&mut m, &fw), w, "the 6502 runs while paused");
    m.key(EDIT, true);
    press(&mut m, PLAY);
    m.key(EDIT, false);
    until(&mut m, "playing again", |m| {
        psid_state(m, &fw) == PSID_PLAYING
    });
    frames(&mut m, &fw, 500, &mut cells);
    assert!(writes(&mut m, &fw) > w, "not resumed");
    // PLAY: stopped, silent
    press(&mut m, PLAY);
    until(&mut m, "stopped", |m| psid_state(m, &fw) == PSID_STOPPED);
    frames(&mut m, &fw, 300, &mut cells);
    let at = m.soc.periph.audio.len();
    frames(&mut m, &fw, 500, &mut cells);
    let peak = m.soc.periph.audio[at..]
        .iter()
        .map(|&v| i32::from(v).abs())
        .max()
        .unwrap_or(0);
    assert!(peak < 100, "the stopped tune still sounds: {peak}");
    check_budgets(
        &m,
        &fw,
        &cells,
        "MENU, SIDS, a tune, GLO, a song step, pause, stop",
    );
}

/// The browser over two pages of tunes (its scan a file a frame, SELECT to the second page and
/// back), NOW PLAYING with a tune playing from LIVE's tap to the time, and STATUS's 6502 rows:
/// every ui_poll within SIDS_WORST_POLL, every pass within POLL_INSTRUCTIONS, every frame within
/// 200 cells.
#[test]
fn the_sid_views_stay_within_their_budgets() {
    let Some(fw) = fixture("the SID views budget test") else {
        return;
    };
    let file = dojo();
    let names: Vec<String> = (0..12).map(|i| format!("TUNE {:02}", i + 1)).collect();
    let files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &file[..])).collect();
    let mut m = boot_with(&fw, &files);
    run_past_boot(&mut m, &fw);
    song_idle(&mut m, &fw);
    m.watch_calls(fw.symbol("ui_poll"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut cells = Cost::default();
    press(&mut m, HOME);
    frames(&mut m, &fw, 400, &mut cells);
    m.turn(SELECT, -(VIEWS_MENU_ITEMS as i32));
    frames(&mut m, &fw, 200, &mut cells);
    m.turn(SELECT, VIEWS_MENU_SIDS as i32);
    frames(&mut m, &fw, 100, &mut cells);
    m.turn(ALGORITHM, 1);
    frames(&mut m, &fw, 1_200, &mut cells);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("sid_view_browser"));
    m.turn(SELECT, 9); // the second page
    frames(&mut m, &fw, 1_000, &mut cells);
    let page = ui_field(&fw, ilp32::views::sids + ilp32::sid_view::page);
    assert_eq!(m.soc.read8(page), 1, "the second page");
    m.turn(SELECT, -2);
    frames(&mut m, &fw, 1_000, &mut cells);
    assert_eq!(m.soc.read8(page), 0, "the first page again");
    m.turn(ALGORITHM, 1);
    frames(&mut m, &fw, 4_000, &mut cells);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("sid_view_playing"));
    assert_eq!(tune_state(&mut m, &fw), TUNE_ON);
    // to STATUS (its 6502 rows) with the tune on, and LIVE
    ui_back_to(&mut m, &fw, "views_menu");
    m.turn(SELECT, -(VIEWS_MENU_ITEMS as i32));
    frames(&mut m, &fw, 200, &mut cells);
    m.turn(SELECT, VIEWS_MENU_STATUS as i32);
    frames(&mut m, &fw, 100, &mut cells);
    m.turn(ALGORITHM, 1);
    frames(&mut m, &fw, 2_500, &mut cells);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("views_status"));
    ui_back_to(&mut m, &fw, "live_view_view");
    frames(&mut m, &fw, 1_000, &mut cells);
    check_budgets(&m, &fw, &cells, "SIDS' pages, NOW PLAYING, STATUS");
}

/// The unsaved-song question each way, over a named song with an edit (NIGHT): HOME cancels (no
/// tune); ALGORITHM again discards the edit, the tune plays, and once it ends the song comes back
/// as it was saved; SAVE saves it first, then the tune plays, and the song comes back with its
/// edit, from the store.
#[test]
fn the_unsaved_song_question_each_way() {
    let Some(fw) = fixture("the SID views' question test") else {
        return;
    };
    let file = dojo();
    let mut m = boot_with(&fw, &[("TUNE", &file)]);
    run_past_boot(&mut m, &fw);
    song_idle(&mut m, &fw);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let asking = ui_field(&fw, ilp32::views::sids + ilp32::sid_view::asking);
    song_io(&mut m, &fw, IO_SAVE, "NIGHT");
    song_io(&mut m, &fw, IO_LOAD, "NIGHT");
    let saved = song_file(&mut m, &fw);
    for keep in [false, true] {
        edit_song(&mut m, &fw);
        let edited = song_file(&mut m, &fw);
        assert!(edited != saved, "the edit changed nothing");
        open_sids(&mut m, &fw);
        m.turn(ALGORITHM, 1);
        m.run_ms(200);
        assert_eq!(m.soc.read8(asking), 1, "no question");
        assert_eq!(tune_state(&mut m, &fw), TUNE_OFF);
        if !keep {
            press(&mut m, HOME); // cancelled
            m.run_ms(200);
            assert_eq!(m.soc.read8(asking), 0);
            assert_eq!(top_view(&mut m, &fw), fw.symbol("sid_view_browser"));
            assert_eq!(tune_state(&mut m, &fw), TUNE_OFF);
            m.turn(ALGORITHM, 1);
            m.run_ms(200);
            assert_eq!(m.soc.read8(asking), 1);
            m.turn(ALGORITHM, 1); // discarded
        } else {
            press(&mut m, PANEL_BTN_SAVE as u8); // saved first
        }
        until(&mut m, "the tune plays", |m| tune_state(m, &fw) == TUNE_ON);
        m.run_ms(1_000);
        ui_end_tune(&mut m, &fw);
        until(&mut m, "the song back", |m| tune_state(m, &fw) == TUNE_OFF);
        song_idle(&mut m, &fw);
        let back = song_file(&mut m, &fw);
        if keep {
            assert!(back == edited, "the song did not come back with its edit");
        } else {
            assert!(back == saved, "the discarded edit came back");
        }
    }
    let worst = m.passes.iter().copied().max().unwrap_or(0);
    println!("the question: the busiest pass {worst}");
    assert!(worst <= POLL_INSTRUCTIONS);
}

/// MIDI over the TRS jack while a tune plays: stop holds it (paused, the 6502 still), continue
/// goes on, start restarts its song from init; clocks change nothing.
#[test]
fn midi_start_stop_and_continue_over_trs_drive_the_tune() {
    let Some(fw) = fixture("the SID views' MIDI test") else {
        return;
    };
    let file = dojo();
    let mut m = boot_with(&fw, &[("TUNE", &file)]);
    run_past_boot(&mut m, &fw);
    song_idle(&mut m, &fw);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    ui_play_tune(&mut m, &fw, 0);
    until(&mut m, "the tune plays", |m| {
        tune_state(m, &fw) == TUNE_ON && writes(m, &fw) > 500
    });
    m.midi(&[0xFC]);
    until(&mut m, "MIDI stop: paused", |m| {
        psid_state(m, &fw) == PSID_PAUSED
    });
    let w = writes(&mut m, &fw);
    for _ in 0..24 {
        m.midi(&[0xF8]);
        m.run_ms(20);
    }
    assert_eq!(writes(&mut m, &fw), w, "the 6502 runs after MIDI stop");
    assert_eq!(psid_state(&mut m, &fw), PSID_PAUSED, "a clock moved it");
    m.midi(&[0xFB]);
    until(&mut m, "MIDI continue: playing", |m| {
        psid_state(m, &fw) == PSID_PLAYING && writes(m, &fw) > w
    });
    m.run_ms(1_000);
    let before = writes(&mut m, &fw);
    m.midi(&[0xFA]);
    until(&mut m, "MIDI start: from init", |m| {
        writes(m, &fw) < before && tune_state(m, &fw) == TUNE_ON
    });
    until(&mut m, "and playing", |m| {
        psid_state(m, &fw) == PSID_PLAYING && writes(m, &fw) > 200
    });
    let worst = m.passes.iter().copied().max().unwrap_or(0);
    println!("MIDI's transport: the busiest pass {worst}");
    assert!(worst <= POLL_INSTRUCTIONS);
}
