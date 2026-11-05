// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::panel::{
    chord, press, wait_for_within, ALGORITHM, EDIT, GLO, HOME, K1, K4, KEY0, PLAY, PRESETS, REC,
    SAVE, SELECT,
};
use common::screen_animating;
use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::psid::packed;
use sidou_tests::song::Song;

const WAIT_MS: u64 = 15_000;
const MAX_SILENT_HALVES: usize = 4;
const TUNE_OFF: u8 = 0;
const TUNE_ON: u8 = 5;
const AUDIO_PSID: u8 = 1;
const IO_SAVE: u32 = 2;
const IO_LOAD: u32 = 3;
const SONG_IO_HOOK_NAME: u32 = 4;
const SONG_IO_HOOK_RESULT: u32 = 24;
const SONG_HOOK_WRITE_ROW: u32 = 1;
const SONG_HOOK_PATTERN: u32 = 4;
const SONG_HOOK_ROW: u32 = 8;
const SONG_HOOK_CELL: u32 = 12;
const EDITED_ROW: u32 = 5;
const GT2_FIRST_NOTE: u8 = 0x60;
const A_NOTE_THE_SONG_HAS: u8 = GT2_FIRST_NOTE + 12 + 9;
const EDIT_SETTLE_MS: u64 = 200;
const SIDS_WORST_POLL: u64 = 39_500;
const _: () = assert!(SIDS_WORST_POLL <= POLL_INSTRUCTIONS);
const LIFT_CONVERT_WORST: u64 = 40_000;
const LCD_WINDOW_OVERHEAD_BYTES: u32 = 11;
const LCD_CELL_BYTES: u32 = 128;
const COST_STEPS_A_MS: u64 = 10;
const SNAP_PLAYER: usize = 0;
const SNAP_PSID_BLOCKS: usize = 1;
const SNAP_B_ON: usize = 2;
const SNAP_ACTIVE: usize = 3;
const SNAP_MUTE: usize = 5;
const GLO_DOUBLE_TAP_MS: u64 = 450;
const GLO_MUTE_VOICE_2: u8 = 1;
const GLO_SOLO_VOICE_3: u8 = 5;
const VOICE_2_MUTED: u8 = 0b010;
const VOICES_1_AND_2_MUTED: u8 = 0b011;
const PLAYING_WRITES: u32 = 200;
const SILENT_PEAK: i32 = 100;
const TWO_PAGES_OF_TUNES: usize = 12;
const TO_THE_SECOND_PAGE: i32 = SID_VIEW_FILES as i32 + 1;
const BACK_TO_THE_FIRST_PAGE: i32 = -2;
const STATUS_LOOK_LOAD: u32 = 12;
const STATUS_LOOK_TICKS_K: u32 = 14;
const MIDI_START: u8 = 0xFA;
const MIDI_CONTINUE: u8 = 0xFB;
const MIDI_STOP: u8 = 0xFC;
const MIDI_CLOCK: u8 = 0xF8;
const CLOCKS_WHILE_STOPPED: usize = 24;
const FRAME_POLL_MS: u64 = 20;
const GTI_MAGIC: usize = 4;
const GTI_AD: usize = GTI_MAGIC + GT2_INS_AD as usize;
const GTI_SR: usize = GTI_MAGIC + GT2_INS_SR as usize;
const WAVEFORMS: u8 = 0xf0;
const VOICE_2: u8 = 1;
const HISTORY_FILL_MS: u64 = 1_500;
const LIFTED_V2: &str = "LIFTED V2";
const LATE_VOICE_AD: u8 = 0x09;
const LATE_VOICE_SR: u8 = 0xa0;
const TUNE_PLAYS_A_WHILE_MS: u64 = 500;
const CUT_DETENTS: i32 = 3;
const SAW_DETENTS: i32 = 2;
const REG_CONTROL_V1: usize = 0x04;
const REG_CUTOFF_LO: usize = 0x15;
const REG_CUTOFF_HI: usize = 0x16;
const CUTOFF_HI_MAX: u32 = 0xff;

fn until(m: &mut Machine, what: &str, done: impl FnMut(&mut Machine) -> bool) {
    wait_for_within(m, WAIT_MS, what, done);
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

fn powered_on_with(fw: &Firmware, files: &[(&str, &[u8])]) -> Machine {
    let mut flash = settled_flash(&fw.image);
    for (name, file) in files {
        put_file(&mut flash, name, STORE_FILE_SID, file);
    }
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(&flash);
    m
}

fn past_boot_and_song_load(m: &mut Machine, fw: &Firmware) {
    run_past_boot(m, fw);
    song_idle(m, fw);
}

fn booted_with(fw: &Firmware, files: &[(&str, &[u8])]) -> Machine {
    let mut m = powered_on_with(fw, files);
    past_boot_and_song_load(&mut m, fw);
    m
}

fn song_idle(m: &mut Machine, fw: &Firmware) {
    let state = fw.symbol("songs") + ilp32::song_io::state as u32;
    until(m, "song I/O idle", |m| {
        m.soc.read8(state) == SONG_IO_IDLE as u8
    });
}

fn song_io(m: &mut Machine, fw: &Firmware, op: u32, name: &str) {
    let hook = fw.symbol("song_io_test_hook");
    for i in 0..=STORE_NAME_MAX {
        let b = name.as_bytes().get(i as usize).copied().unwrap_or(0);
        m.soc.write8(hook + SONG_IO_HOOK_NAME + i, b);
    }
    m.soc.write32(hook + SONG_IO_HOOK_RESULT, u32::MAX);
    m.soc.write32(hook, op);
    until(m, "the main loop takes the song request", |m| {
        m.soc.read32(hook) == 0
    });
    assert_eq!(
        m.soc.read32(hook + SONG_IO_HOOK_RESULT) as i32,
        1,
        "song I/O op {op}"
    );
    song_idle(m, fw);
}

fn device_song(m: &mut Machine, fw: &Firmware) -> Song {
    let mut song = Song::device();
    let base = fw.symbol("__song_region");
    for i in 0..SONG_REGION_BYTES {
        unsafe {
            song.ptr_mut()
                .cast::<u8>()
                .add(i as usize)
                .write(m.soc.read8(base + i))
        };
    }
    song
}

fn song_file(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    device_song(m, fw).write(true)
}

fn song_gt2_part(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    device_song(m, fw).write(false)
}

fn edit_a_row(m: &mut Machine, fw: &Firmware) {
    let hook = fw.symbol("song_test_hook");
    m.soc.write32(hook + SONG_HOOK_PATTERN, 0);
    m.soc.write32(hook + SONG_HOOK_ROW, EDITED_ROW);
    for (k, &c) in [A_NOTE_THE_SONG_HAS, 1, 0, 0].iter().enumerate() {
        m.soc.write8(hook + SONG_HOOK_CELL + k as u32, c);
    }
    m.soc.write32(hook, SONG_HOOK_WRITE_ROW);
    until(m, "the edit made", |m| m.soc.read32(hook) == 0);
    m.run_ms(EDIT_SETTLE_MS);
}

#[derive(Default)]
struct Cost {
    cells: Vec<u32>,
    polls: Vec<(u32, u32, u64)>,
}

fn frames(m: &mut Machine, fw: &Firmware, ms: u64, cost: &mut Cost) {
    let top = top_view(m, fw);
    let polls = m.watched_calls.len();
    let cells = &mut cost.cells;
    let last = fw.symbol("last_compose");
    let count = fw.symbol("lcd_count");
    let sent = |m: &mut Machine| (m.soc.read32(count), m.soc.read32(count + 4));
    let mut stamp = m.soc.read32(last);
    let mut from = sent(m);
    let step = m.soc.periph.cycle_of_ms(1) / COST_STEPS_A_MS;
    let end = m.soc.periph.cycles + m.soc.periph.cycle_of_ms(ms);
    while m.soc.periph.cycles < end {
        m.run_cycles(step);
        let s = m.soc.read32(last);
        if s != stamp {
            stamp = s;
            let now = sent(m);
            let (windows, bytes) = (now.0 - from.0, now.1 - from.1);
            cells.push((bytes - windows * LCD_WINDOW_OVERHEAD_BYTES) / LCD_CELL_BYTES);
            from = now;
        }
    }
    let worst = m.watched_calls[polls..].iter().copied().max().unwrap_or(0);
    let end = top_view(m, fw);
    cost.polls.push((top, end, worst));
}

fn frames_until(
    m: &mut Machine,
    fw: &Firmware,
    cost: &mut Cost,
    what: &str,
    mut done: impl FnMut(&mut Machine) -> bool,
) {
    let start = m.ms();
    while !done(m) {
        assert!(m.ms() - start < WAIT_MS, "timed out: {what}");
        frames(m, fw, FRAME_POLL_MS, cost);
    }
}

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

struct Snaps {
    psid_block: usize,
    fresh_block: [Option<usize>; 2],
    start_block: usize,
    b_use: Option<usize>,
}

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
        .position(|s| s.bytes[SNAP_PLAYER] == AUDIO_PSID)
        .expect("the interrupt never took the tune");
    let fresh = |k: usize| {
        let was = snaps[psid_block].bytes[SNAP_ACTIVE + k];
        snaps[psid_block..]
            .iter()
            .position(|s| s.bytes[SNAP_ACTIVE + k] != was)
            .map(|i| i + psid_block)
    };
    let start_block = snaps[psid_block..]
        .iter()
        .position(|s| s.bytes[SNAP_PSID_BLOCKS] != 0)
        .expect("the tune never started")
        + psid_block;
    Snaps {
        psid_block,
        fresh_block: [fresh(0), None],
        start_block,
        b_use: snaps.iter().position(|s| s.bytes[SNAP_B_ON] != 0),
    }
}

fn mute_block(m: &Machine, from: usize, mask: u8) -> Option<usize> {
    m.buffer_snapshots[from..]
        .iter()
        .position(|s| s.bytes[SNAP_MUTE + usize::from(s.bytes[SNAP_ACTIVE])] == mask)
        .map(|i| i + from)
}

fn glo_key(m: &mut Machine, k: u8) {
    chord(m, GLO, KEY0 + k);
    m.run_ms(GLO_DOUBLE_TAP_MS);
}

fn open_from_the_menu(m: &mut Machine, fw: &Firmware, item: u32, open_ms: u64, cost: &mut Cost) {
    m.turn(SELECT, -(VIEWS_MENU_ITEMS as i32));
    frames(m, fw, 200, cost);
    m.turn(SELECT, item as i32);
    frames(m, fw, 100, cost);
    m.turn(ALGORITHM, 1);
    frames(m, fw, open_ms, cost);
}

fn busiest_pass(m: &Machine) -> u64 {
    m.passes.iter().copied().max().unwrap_or(0)
}

fn check_budgets(m: &Machine, fw: &Firmware, cost: &Cost, what: &str) {
    let pass = busiest_pass(m);
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
            "lab_view_view",
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
        "{what}: the busiest pass {pass}, the dearest ui_poll under the SID views {ours}, \
         elsewhere {others}; {} frames, the most cells {most}",
        cost.cells.len()
    );
    assert_within_budget(
        &format!("{what}: the busiest pass"),
        pass,
        POLL_INSTRUCTIONS,
    );
    assert_within_budget(
        &format!("{what}: a SID view's ui_poll"),
        ours,
        SIDS_WORST_POLL,
    );
    assert_within_budget(
        &format!("{what}: another view's ui_poll"),
        others,
        POLL_INSTRUCTIONS,
    );
    assert_within_budget(
        &format!("{what}: a frame's cells"),
        most,
        SONG_VIEW_FRAME_CELLS,
    );
}

#[test]
fn a_tune_from_the_menu_plays_mutes_solos_steps_pauses_and_stops() {
    let Some(fw) = firmware("the SID views test") else {
        return;
    };
    let file = songs_tune();
    let mut m = powered_on_with(&fw, &[("SONGS", &file)]);
    watch(&mut m, &fw);
    past_boot_and_song_load(&mut m, &fw);
    m.watch_calls(fw.symbol("ui_poll"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut cells = Cost::default();
    ui_play_tune(&mut m, &fw, 0);
    until(&mut m, "the tune plays", |m| {
        tune_state(m, &fw) == TUNE_ON && writes(m, &fw) > PLAYING_WRITES
    });
    frames(&mut m, &fw, 1_500, &mut cells);
    let before = m.buffer_snapshots.len();
    glo_key(&mut m, GLO_MUTE_VOICE_2);
    frames(&mut m, &fw, 1_500, &mut cells);
    let audio = m.soc.periph.audio.clone();
    let s = snaps(&m);
    let muted = mute_block(&m, before, VOICE_2_MUTED).expect("voice 2 never muted on chip A");
    let with = matches(&audio, &file, &s, Some((muted, u32::from(VOICE_2_MUTED))));
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
    let before = m.buffer_snapshots.len();
    glo_key(&mut m, GLO_SOLO_VOICE_3);
    frames(&mut m, &fw, 300, &mut cells);
    assert!(
        mute_block(&m, before, VOICES_1_AND_2_MUTED).is_some(),
        "the solo never reached chip A"
    );
    glo_key(&mut m, GLO_SOLO_VOICE_3);
    glo_key(&mut m, GLO_MUTE_VOICE_2);
    frames(&mut m, &fw, 300, &mut cells);
    m.turn(ALGORITHM, 1);
    until(&mut m, "song 2 plays", |m| {
        tune_state(m, &fw) == TUNE_ON
            && env_song(m, &fw) == 2
            && psid_state(m, &fw) == PSID_PLAYING
            && writes(m, &fw) > 100
    });
    frames(&mut m, &fw, 1_000, &mut cells);
    chord(&mut m, EDIT, PLAY);
    until(&mut m, "paused", |m| psid_state(m, &fw) == PSID_PAUSED);
    let w = writes(&mut m, &fw);
    frames(&mut m, &fw, 500, &mut cells);
    assert_eq!(writes(&mut m, &fw), w, "the 6502 runs while paused");
    chord(&mut m, EDIT, PLAY);
    until(&mut m, "playing again", |m| {
        psid_state(m, &fw) == PSID_PLAYING
    });
    frames(&mut m, &fw, 500, &mut cells);
    assert!(writes(&mut m, &fw) > w, "not resumed");
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
    assert!(peak < SILENT_PEAK, "the stopped tune still sounds: {peak}");
    check_budgets(
        &m,
        &fw,
        &cells,
        "MENU, SIDS, a tune, GLO, a song step, pause, stop",
    );
}

fn bent_cutoff(tunes: u32, bend: u32) -> u32 {
    ((tunes >> 3) + (bend >> 3)).min(CUTOFF_HI_MAX) << 3 | (tunes & 7)
}

struct Heard {
    cutoff: u32,
    control: u8,
    tunes_cutoff: u32,
    tunes_control: u8,
}

fn heard(m: &mut Machine, fw: &Firmware) -> Heard {
    let raw =
        |m: &mut Machine, reg: usize| m.soc.read8(ctl(fw) + (ilp32::psid_ctl::raw + reg) as u32);
    let chip = chip_a(m, fw);
    Heard {
        cutoff: m.soc.read32(chip + ilp32::sid::fc as u32),
        control: m
            .soc
            .read8(chip + (ilp32::sid::osc + ilp32::sid_osc::control) as u32),
        tunes_cutoff: u32::from(raw(m, REG_CUTOFF_LO) & 7) | u32::from(raw(m, REG_CUTOFF_HI)) << 3,
        tunes_control: raw(m, REG_CONTROL_V1),
    }
}

#[test]
fn k1_and_k4_bend_a_playing_tunes_cutoff_and_waveform_and_home_gives_them_back() {
    let Some(fw) = firmware("the SID tweak test") else {
        return;
    };
    let file = consultant();
    let mut m = booted_with(&fw, &[("CONSULTANT", &file)]);
    ui_play_tune(&mut m, &fw, 0);
    until(&mut m, "the tune plays", |m| {
        tune_state(m, &fw) == TUNE_ON && writes(m, &fw) > PLAYING_WRITES
    });
    let bend = (CUT_DETENTS as u32) * PSID_TWEAK_CUT_STEP;
    m.turn(K1, CUT_DETENTS);
    until(&mut m, "the cutoff bent", |m| {
        let h = heard(m, &fw);
        h.cutoff == bent_cutoff(h.tunes_cutoff, bend) && h.cutoff != h.tunes_cutoff
    });
    m.turn(K4, SAW_DETENTS);
    until(&mut m, "voice 1 on the sawtooth, its own gate kept", |m| {
        let h = heard(m, &fw);
        h.control == (h.tunes_control & 0x0f) | SID_CTRL_SAW as u8
    });
    m.run_ms(TUNE_PLAYS_A_WHILE_MS);
    let h = heard(&mut m, &fw);
    assert_eq!(
        h.cutoff,
        bent_cutoff(h.tunes_cutoff, bend),
        "the tune's writes stay bent"
    );
    press(&mut m, HOME);
    until(&mut m, "the tune's own cutoff and waveform", |m| {
        let h = heard(m, &fw);
        top_view(m, &fw) == fw.symbol("sid_view_browser")
            && h.cutoff == h.tunes_cutoff
            && h.control == h.tunes_control
    });
}

#[test]
fn the_sid_views_stay_within_their_budgets() {
    let Some(fw) = firmware("the SID views budget test") else {
        return;
    };
    let file = dojo();
    let names: Vec<String> = (0..TWO_PAGES_OF_TUNES)
        .map(|i| format!("TUNE {:02}", i + 1))
        .collect();
    let files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &file[..])).collect();
    let mut m = booted_with(&fw, &files);
    m.watch_calls(fw.symbol("ui_poll"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut cells = Cost::default();
    press(&mut m, HOME);
    frames(&mut m, &fw, 400, &mut cells);
    open_from_the_menu(&mut m, &fw, VIEWS_MENU_SIDS, 1_200, &mut cells);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("sid_view_browser"));
    m.turn(SELECT, TO_THE_SECOND_PAGE);
    frames(&mut m, &fw, 1_000, &mut cells);
    let page = ui_field(&fw, ilp32::views::sids + ilp32::sid_view::page);
    assert_eq!(m.soc.read8(page), 1, "the second page");
    m.turn(SELECT, BACK_TO_THE_FIRST_PAGE);
    frames(&mut m, &fw, 1_000, &mut cells);
    assert_eq!(m.soc.read8(page), 0, "the first page again");
    m.turn(ALGORITHM, 1);
    frames(&mut m, &fw, 4_000, &mut cells);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("sid_view_playing"));
    assert_eq!(tune_state(&mut m, &fw), TUNE_ON);
    ui_back_to(&mut m, &fw, "views_menu");
    open_from_the_menu(&mut m, &fw, VIEWS_MENU_STATUS, 2_500, &mut cells);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("views_status"));
    let meter = |m: &mut Machine| {
        let look = ui_field(&fw, ilp32::views::tune_drawn);
        (
            m.soc.read16(look + STATUS_LOOK_LOAD),
            m.soc.read16(look + STATUS_LOOK_TICKS_K),
        )
    };
    let (load, ticks_k) = meter(&mut m);
    eprintln!("the meter: {load} tenths of a percent, {ticks_k} ticks a thousand cycles");
    assert!(
        load > 0 && ticks_k > 0,
        "the meter reads while a tune plays"
    );
    press(&mut m, PLAY);
    frames(&mut m, &fw, 2_500, &mut cells);
    assert_eq!(
        meter(&mut m),
        (0, 0),
        "the meter reads 0 while no tune runs"
    );
    ui_back_to(&mut m, &fw, "live_view_view");
    frames(&mut m, &fw, 1_000, &mut cells);
    check_budgets(&m, &fw, &cells, "SIDS' pages, NOW PLAYING, STATUS");
}

#[test]
fn the_unsaved_song_question_cancels_on_home_ignores_algorithm_and_discards_on_save() {
    let Some(fw) = firmware("the SID views' question test") else {
        return;
    };
    let file = dojo();
    let mut m = booted_with(&fw, &[("TUNE", &file)]);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let asking = ui_field(&fw, ilp32::views::sids + ilp32::sid_view::asking);
    song_io(&mut m, &fw, IO_SAVE, "NIGHT");
    song_io(&mut m, &fw, IO_LOAD, "NIGHT");
    let saved = song_file(&mut m, &fw);
    edit_a_row(&mut m, &fw);
    assert!(song_file(&mut m, &fw) != saved, "the edit changed nothing");
    open_sids(&mut m, &fw);
    m.turn(ALGORITHM, 1);
    m.run_ms(200);
    assert_eq!(m.soc.read8(asking), 1, "no question");
    press(&mut m, HOME);
    m.run_ms(200);
    assert_eq!(m.soc.read8(asking), 0);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("sid_view_browser"));
    m.turn(ALGORITHM, 1);
    m.run_ms(200);
    assert_eq!(m.soc.read8(asking), 1);
    m.turn(ALGORITHM, 1);
    m.run_ms(500);
    assert_eq!(m.soc.read8(asking), 1, "ALGORITHM answered the question");
    assert_eq!(tune_state(&mut m, &fw), TUNE_OFF);
    press(&mut m, SAVE);
    until(&mut m, "the tune plays", |m| tune_state(m, &fw) == TUNE_ON);
    m.run_ms(1_000);
    ui_end_tune(&mut m, &fw);
    until(&mut m, "the song back", |m| tune_state(m, &fw) == TUNE_OFF);
    song_idle(&mut m, &fw);
    assert!(
        song_file(&mut m, &fw) == saved,
        "the discarded edit came back"
    );
    assert_within_budget(
        "the question's busiest pass",
        busiest_pass(&m),
        POLL_INSTRUCTIONS,
    );
}

#[test]
fn a_sound_edit_asks_nothing_and_survives_a_tune() {
    let Some(fw) = firmware("the SID views' sound-edit test") else {
        return;
    };
    let file = dojo();
    let mut m = booted_with(&fw, &[("TUNE", &file)]);
    let asking = ui_field(&fw, ilp32::views::sids + ilp32::sid_view::asking);
    let live = fw.symbol("instrument");
    let preset = live + ilp32::live::preset as u32;
    let cutoff = live + ilp32::live::cutoff as u32;
    let song = song_gt2_part(&mut m, &fw);
    let was = m.soc.read8(preset);
    m.turn(PRESETS, 1);
    m.run_ms(100);
    m.turn(K1, -6);
    m.run_ms(200);
    let edited = (m.soc.read8(preset), m.soc.read16(cutoff));
    assert_ne!(edited.0, was, "PRESETS changed the sound");
    open_sids(&mut m, &fw);
    m.turn(ALGORITHM, 1);
    m.run_ms(200);
    assert_eq!(m.soc.read8(asking), 0, "a sound edit asks nothing");
    until(&mut m, "the tune plays", |m| tune_state(m, &fw) == TUNE_ON);
    m.run_ms(1_000);
    ui_end_tune(&mut m, &fw);
    until(&mut m, "the song back", |m| tune_state(m, &fw) == TUNE_OFF);
    song_idle(&mut m, &fw);
    assert!(
        song_gt2_part(&mut m, &fw) == song,
        "the loop came back as it was"
    );
    assert_eq!(
        (m.soc.read8(preset), m.soc.read16(cutoff)),
        edited,
        "the sound as edited, not the loop's"
    );
}

#[test]
fn midi_start_stop_and_continue_over_trs_drive_the_tune() {
    let Some(fw) = firmware("the SID views' MIDI test") else {
        return;
    };
    let file = dojo();
    let mut m = booted_with(&fw, &[("TUNE", &file)]);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    ui_play_tune(&mut m, &fw, 0);
    until(&mut m, "the tune plays", |m| {
        tune_state(m, &fw) == TUNE_ON && writes(m, &fw) > 500
    });
    m.midi(&[MIDI_STOP]);
    until(&mut m, "MIDI stop: paused", |m| {
        psid_state(m, &fw) == PSID_PAUSED
    });
    let w = writes(&mut m, &fw);
    for _ in 0..CLOCKS_WHILE_STOPPED {
        m.midi(&[MIDI_CLOCK]);
        m.run_ms(20);
    }
    assert_eq!(writes(&mut m, &fw), w, "the 6502 runs after MIDI stop");
    assert_eq!(psid_state(&mut m, &fw), PSID_PAUSED, "a clock moved it");
    m.midi(&[MIDI_CONTINUE]);
    until(&mut m, "MIDI continue: playing", |m| {
        psid_state(m, &fw) == PSID_PLAYING && writes(m, &fw) > w
    });
    m.run_ms(1_000);
    let before = writes(&mut m, &fw);
    m.midi(&[MIDI_START]);
    until(&mut m, "MIDI start: from init", |m| {
        writes(m, &fw) < before && tune_state(m, &fw) == TUNE_ON
    });
    until(&mut m, "and playing", |m| {
        psid_state(m, &fw) == PSID_PLAYING && writes(m, &fw) > PLAYING_WRITES
    });
    assert_within_budget(
        "MIDI's transport's busiest pass",
        busiest_pass(&m),
        POLL_INSTRUCTIONS,
    );
}

fn lift_at(fw: &Firmware, field: usize) -> u32 {
    fw.symbol("song_space") + field as u32
}

fn live_at(fw: &Firmware, field: usize) -> u32 {
    fw.symbol("instrument") + field as u32
}

fn sound_name(m: &mut Machine, fw: &Firmware) -> String {
    text_at(m, live_at(fw, ilp32::live::base_name), LAB_SOUND_NAME)
}

fn lifted_gti(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let at = lift_at(fw, ilp32::psid_lift::file);
    (0..PSID_LIFT_FILE_MAX)
        .map(|i| m.soc.read8(at + i))
        .collect()
}

fn lift_voice(m: &mut Machine, key: u8) {
    press(m, REC);
    m.run_ms(100);
    press(m, KEY0 + key);
}

fn in_lab(m: &mut Machine, fw: &Firmware) -> bool {
    top_view(m, fw) == fw.symbol("lab_view_view")
}

fn first_audible_wave(m: &mut Machine, fw: &Firmware) -> u8 {
    let frames = u32::from(m.soc.read8(lift_at(fw, ilp32::psid_lift::frames)));
    let at = lift_at(fw, ilp32::psid_lift::frame);
    (0..frames)
        .map(|k| m.soc.read8(at + k * PSID_LIFT_REGS + PSID_LIFT_CTRL) & WAVEFORMS)
        .find(|&w| w != 0)
        .expect("a waveform")
}

fn assert_lab_holds_the_lifted_sound(m: &mut Machine, fw: &Firmware, ad: u8, sr: u8) {
    assert_eq!(sound_name(m, fw), LIFTED_V2);
    let gti = lifted_gti(m, fw);
    assert_eq!(&gti[..GTI_MAGIC], b"GTI5");
    assert_eq!(
        (gti[GTI_AD], gti[GTI_SR]),
        (ad, sr),
        "the voice's envelope lifted"
    );
    let wave = first_audible_wave(m, fw);
    assert_eq!(
        (
            m.soc.read8(live_at(fw, ilp32::live::attack_decay)),
            m.soc.read8(live_at(fw, ilp32::live::sustain_release)),
            m.soc.read8(live_at(fw, ilp32::live::control)) & WAVEFORMS
        ),
        (ad, sr, wave),
        "LAB plays the lifted envelope and the wave the voice sounded"
    );
}

fn consultant() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/starter/consultant.sid"),
    )
    .expect("assets/starter/consultant.sid")
}

#[test]
fn a_voice_lifted_from_a_tune_opens_in_lab_save_keeps_it_and_home_goes_back() {
    let Some(fw) = firmware("the lift test") else {
        return;
    };
    let mut m = booted_with(&fw, &[("CONSULTANT", &consultant())]);
    m.watch_calls(fw.symbol("ui_poll"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut cells = Cost::default();
    ui_play_tune(&mut m, &fw, 0);
    until(&mut m, "the tune plays", |m| {
        tune_state(m, &fw) == TUNE_ON && writes(m, &fw) > PLAYING_WRITES
    });
    frames(&mut m, &fw, HISTORY_FILL_MS, &mut cells);
    let polls = m.watched_calls.len();
    m.watch_calls(fw.symbol("psid_lift_convert"));
    lift_voice(&mut m, VOICE_2);
    frames_until(&mut m, &fw, &mut cells, "LAB with the lifted sound", |m| {
        in_lab(m, &fw)
    });
    let convert = m.watched_calls[polls..].to_vec();
    println!("psid_lift_convert: {convert:?} instructions");
    assert!(!convert.is_empty(), "converted");
    for pass in convert {
        assert_within_budget("a pass of psid_lift_convert", pass, LIFT_CONVERT_WORST);
    }
    m.watch_calls(fw.symbol("ui_poll"));
    let gti = lifted_gti(&mut m, &fw);
    assert_lab_holds_the_lifted_sound(&mut m, &fw, gti[GTI_AD], gti[GTI_SR]);
    assert_eq!(
        m.soc.read8(live_at(&fw, ilp32::live::user_sound)),
        0,
        "a new sound, no USER sound yet"
    );
    press(&mut m, SAVE);
    frames_until(&mut m, &fw, &mut cells, "kept as a USER sound", |m| {
        m.soc.read8(live_at(&fw, ilp32::live::user_sound)) != 0
    });
    assert_eq!(sound_name(&mut m, &fw), LIFTED_V2, "kept under its name");
    let w = writes(&mut m, &fw);
    press(&mut m, HOME);
    frames_until(&mut m, &fw, &mut cells, "back on NOW PLAYING", |m| {
        top_view(m, &fw) == fw.symbol("sid_view_playing") && !screen_animating(m, &fw)
    });
    frames(&mut m, &fw, TUNE_PLAYS_A_WHILE_MS, &mut cells);
    assert_eq!(
        (tune_state(&mut m, &fw), psid_state(&mut m, &fw)),
        (TUNE_ON, PSID_PLAYING),
        "the tune plays on"
    );
    assert!(writes(&mut m, &fw) > w, "and writes");
    until(&mut m, "the USER sounds' file", |m| {
        stored_file(&m.soc.flash.mem, "USER SOUNDS").is_some_and(|(_, f)| {
            f.windows(LIFTED_V2.len())
                .any(|w| w == LIFTED_V2.as_bytes())
        })
    });
    assert!(
        stored_file(&m.soc.flash.mem, "LIFTED SOUND").is_none(),
        "no file is lifted to the store"
    );
    check_budgets(&m, &fw, &cells, "a tune, REC, key 2, LAB, SAVE, HOME");
}

fn a_voice_starting_late() -> Vec<u8> {
    let init = format!(
        "lda #$0f\nsta $d418\nlda #${LATE_VOICE_AD:02x}\nsta $d40c\nlda #${LATE_VOICE_SR:02x}\n\
         sta $d40d\nlda #$11\nsta $d404\nlda #0\nsta <$fb\nrts\n"
    );
    let play = "inc <$fb\nlda <$fb\ncmp #150\nbne wait\nlda #$21\nsta $d40b\nwait: rts\n";
    sidou_tests::psid_fixtures::Psid::new(&[(0x1000, &init), (0x1100, play)]).bytes()
}

fn digi() -> Vec<u8> {
    let init = "lda #$09\nsta $d405\nrts\n";
    let play = "lda #$10\nsta $d404\nlda #$11\nsta $d404\nldx #20\n\
                burst: stx $d418\ndex\nbne burst\nrts\n";
    sidou_tests::psid_fixtures::Psid::new(&[(0x1000, init), (0x1100, play)]).bytes()
}

#[test]
fn a_voice_silent_so_far_waits_and_is_lifted_at_its_next_note() {
    let Some(fw) = firmware("the lift wait test") else {
        return;
    };
    let mut m = booted_with(&fw, &[("LATE", &a_voice_starting_late())]);
    ui_play_tune(&mut m, &fw, 0);
    until(&mut m, "the tune plays", |m| tune_state(m, &fw) == TUNE_ON);
    lift_voice(&mut m, VOICE_2);
    m.run_ms(200);
    assert_eq!(
        u32::from(m.soc.read8(lift_at(&fw, ilp32::psid_lift::state))),
        PSID_LIFT_ARMED,
        "voice 2 has played nothing: it waits"
    );
    assert_eq!(top_view(&mut m, &fw), fw.symbol("sid_view_playing"));
    until(&mut m, "LAB at voice 2's first note", |m| in_lab(m, &fw));
    assert_lab_holds_the_lifted_sound(&mut m, &fw, LATE_VOICE_AD, LATE_VOICE_SR);
}

#[test]
fn a_digi_tune_refuses_the_lift() {
    let Some(fw) = firmware("the digi lift test") else {
        return;
    };
    let mut m = booted_with(&fw, &[("DIGI", &digi())]);
    ui_play_tune(&mut m, &fw, 0);
    until(&mut m, "the tune plays", |m| {
        tune_state(m, &fw) == TUNE_ON && writes(m, &fw) > PLAYING_WRITES
    });
    lift_voice(&mut m, 0);
    until(&mut m, "the refusal", |m| {
        toast_text(m, &fw) == "DIGI: CAN'T LIFT"
    });
    m.run_ms(300);
    assert_eq!(
        top_view(&mut m, &fw),
        fw.symbol("sid_view_playing"),
        "no LAB"
    );
    assert_eq!(
        u32::from(m.soc.read8(lift_at(&fw, ilp32::psid_lift::state))),
        PSID_LIFT_OFF
    );
}
