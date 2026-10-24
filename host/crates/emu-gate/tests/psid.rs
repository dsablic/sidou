// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! .sid tunes on the device (M3 Task 9, docs/plans/2026-10-19-m3-sid-files.md), end to end in
//! fm1-emu: a tune put into the store's flash before the boot plays through the firmware's
//! test hook (platform/fm1/main.c `psid_test_hook`, until Task 11's views) over the song's
//! memory, and the firmware's audio equals the host build's render of the same tune through the
//! same chain (common::render_host_psid: psid_ctl, a frame ahead, each write at its cycle)
//! sample for sample, for a VBI, a CIA, an NTSC and a 2SID tune, with LIVE showing (the tap)
//! and without; the switch between the song and a tune both ways, 20 times, the song back from
//! the store byte for byte each time; the keys on chip B during a 1SID tune, silent during a
//! 2SID tune and back after; a tune whose play never returns stops with PLAY HUNG and the
//! device stays responsive.

mod common;

use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::psid::packed;
use sidou_tests::psid_fixtures;
use sidou_tests::song::Song;

/// How long a comparison plays a tune.
const PLAY_MS: u64 = 10_000;
/// The DMA plays the zeroed buffer before the first half the interrupt filled.
const MAX_SILENT_HALVES: usize = 4;
const HOME: u8 = PANEL_BTN_HOME as u8;
/// The hook's ops (main.c psid_test_req): play, and end for good.
const OP_PLAY: u32 = 1;
const OP_END: u32 = 3;
/// psid_test_req's layout: op, song, name[17], result.
const HOOK_SONG: u32 = 4;
const HOOK_NAME: u32 = 8;
const HOOK_RESULT: u32 = 28;
/// audio.h AUDIO_SONG, AUDIO_PSID, AUDIO_NONE.
const AUDIO_SONG: u8 = 0;
const AUDIO_PSID: u8 = 1;
/// main.c's tune states: TUNE_OFF, TUNE_ON.
const TUNE_OFF: u8 = 0;
const TUNE_ON: u8 = 5;
/// The song-I/O hook (main.c song_io_test_req): SAVE, LOAD.
const IO_SAVE: u32 = 2;
const IO_LOAD: u32 = 3;
const WAIT_MS: u64 = 15_000;

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

fn example(name: &str) -> Vec<u8> {
    gt2_ref::examples()
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("{name}"))
        .1
}

/// A GoatTracker example packed as a PSID v2 tune (sidou_tests::psid::packed), song 1 timed by
/// CIA 1 (the KERNAL's latch: its init leaves it) if `cia`, at the NTSC clock if `ntsc`.
fn tune(sng: &str, sid: u32, cia: bool, ntsc: bool) -> Vec<u8> {
    let mut f = packed(&example(sng), sid);
    if cia {
        f[0x12..0x16].copy_from_slice(&1u32.to_be_bytes());
    }
    if ntsc {
        let flags = u16::from_be_bytes([f[0x76], f[0x77]]) & !(3 << 2);
        f[0x76..0x78].copy_from_slice(&(flags | psid_fixtures::CLOCK_NTSC).to_be_bytes());
    }
    f
}

fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
}

fn until(m: &mut Machine, what: &str, mut done: impl FnMut(&mut Machine) -> bool) {
    let start = m.ms();
    while !done(m) {
        assert!(m.ms() - start < WAIT_MS, "timed out: {what}");
        m.run_ms(1);
    }
}

/// A request to the firmware's tune hook, taken by the main loop: its result.
fn request(m: &mut Machine, fw: &Firmware, op: u32, name: &str, song: u32) -> i32 {
    let hook = fw.symbol("psid_test_hook");
    assert_eq!(m.soc.read32(hook), 0, "a request still pending");
    for i in 0..=STORE_NAME_MAX {
        let b = name.as_bytes().get(i as usize).copied().unwrap_or(0);
        m.soc.write8(hook + HOOK_NAME + i, b);
    }
    m.soc.write32(hook + HOOK_SONG, song);
    m.soc.write32(hook + HOOK_RESULT, u32::MAX);
    m.soc.write32(hook, op);
    until(m, "the main loop takes the tune request", |m| {
        m.soc.read32(hook) == 0
    });
    m.soc.read32(hook + HOOK_RESULT) as i32
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
    assert_eq!(
        m.soc.read32(hook + 24) as i32,
        1,
        "song I/O op {op} on {name}"
    );
    song_idle(m, fw);
}

fn song_idle(m: &mut Machine, fw: &Firmware) {
    let state = fw.symbol("songs") + ilp32::song_io::state as u32;
    until(m, "song I/O idle", |m| {
        m.soc.read8(state) == SONG_IO_IDLE as u8
    });
}

/// main.c's tune state (`sid_tune`'s first byte).
fn tune_state(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc.read8(fw.symbol("sid_tune"))
}

/// The tune's psid_ctl (audio.c: the first member of the interrupt's union, `player_isr`).
fn ctl(fw: &Firmware) -> u32 {
    fw.symbol("player_isr")
}

fn ctl32(m: &mut Machine, fw: &Firmware, offset: usize) -> u32 {
    m.soc.read32(ctl(fw) + offset as u32)
}

fn ctl8(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    m.soc.read8(ctl(fw) + offset as u32)
}

/// A flash with the store holding `file` as the .sid file TUNE.
fn flash_with(fw: &Firmware, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut flash = settled_flash(&fw.image);
    for (name, file) in files {
        put_file(&mut flash, name, STORE_FILE_SID, file);
    }
    flash
}

fn boot_with(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    run_past_boot(&mut m, fw);
    song_idle(&mut m, fw);
    m
}

/// What the firmware did with a tune: its audio, and the blocks its events reached the
/// interrupt at (common::PsidPlay's).
struct Played {
    audio: Vec<i16>,
    psid_block: usize,
    fresh_block: [Option<usize>; 2],
    start_block: usize,
    b_use: Option<usize>,
    late: u32,
    writes: u32,
}

/// Boots with `file` in the store, `before` run past the boot (a view opened, say), plays it
/// through the hook for `ms`.
fn play(
    fw: &Firmware,
    file: &[u8],
    song: u32,
    ms: u64,
    before: impl FnOnce(&mut Machine),
) -> Played {
    let flash = flash_with(fw, &[("TUNE", file)]);
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(&flash);
    let chips = fw.chips();
    let active = chips + ilp32::reg_ring_chips::active as u32;
    m.snapshot_bytes = vec![
        fw.symbol("player_now"),
        ctl(fw) + ilp32::psid_ctl::blocks as u32,
        fw.symbol("b_on"),
        active,
        active + 1,
    ];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 1)));
    run_past_boot(&mut m, fw);
    song_idle(&mut m, fw);
    before(&mut m);
    assert_eq!(
        request(&mut m, fw, OP_PLAY, "TUNE", song),
        1,
        "the tune refused"
    );
    m.run_ms(ms);
    assert_eq!(tune_state(&mut m, fw), TUNE_ON, "the tune is not on");
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
    let b_use = snaps.iter().position(|s| s.bytes[2] != 0);
    Played {
        audio: m.soc.periph.audio.clone(),
        psid_block,
        fresh_block: [fresh(0), fresh(1)],
        start_block,
        b_use,
        late: ctl32(&mut m, fw, ilp32::psid_ctl::late),
        writes: ctl32(&mut m, fw, ilp32::psid_ctl::writes),
    }
}

/// The device's audio against the host's render, over the silent halves the DMA may lead with
/// and the block the keys' registers reach chip B in (around the one it goes in use).
fn matches(p: &Played, file: &[u8], song: u32, two: bool) -> Result<usize, i32> {
    let mut closest = i32::MAX;
    let rewrites: Vec<Option<usize>> = match (p.b_use, two) {
        (Some(b), false) => (b.saturating_sub(1)..=b + 1).map(Some).collect(),
        _ => vec![None],
    };
    for silent in 0..=MAX_SILENT_HALVES {
        if p.audio[..2 * silent * HALF_FRAMES].iter().any(|&v| v != 0) {
            break;
        }
        let played = &p.audio[2 * silent * HALF_FRAMES..];
        for &r in &rewrites {
            let want = render_host_psid(
                played.len() / 2,
                &PsidPlay {
                    file,
                    song,
                    psid_block: p.psid_block,
                    fresh_block: p.fresh_block,
                    start_block: p.start_block,
                    b_use_block: p.b_use,
                    b_rewrite_block: r,
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

fn check(name: &str, fw: &Firmware, file: &[u8], before: impl FnOnce(&mut Machine)) {
    let two = sidou_tests::psid::header(file).sid2 != 0;
    let p = play(fw, file, 1, PLAY_MS, before);
    let peak = p
        .audio
        .iter()
        .map(|&v| i32::from(v).abs())
        .max()
        .unwrap_or(0);
    let r = matches(&p, file, 1, two);
    println!(
        "{name}: on AUDIO_PSID at block {}, chips fresh at {:?}, started at {}, chip B in use \
         at {:?}; {} writes, {} late; {} frames, peak {peak}: {r:?}",
        p.psid_block,
        p.fresh_block,
        p.start_block,
        p.b_use,
        p.writes,
        p.late,
        p.audio.len() / 2
    );
    assert!(peak > 1000, "{name}: the tune is silent (peak {peak})");
    assert_eq!(p.late, 0, "{name}: writes applied late");
    assert!(
        r.is_ok(),
        "{name}: the firmware's tune differs from the host's: closest {r:?} LSB"
    );
}

#[test]
fn a_vbi_tune_plays_bit_exact_with_the_host_while_live_shows() {
    let Some(fw) = fixture("the .sid test") else {
        return;
    };
    check(
        "dojo, VBI, LIVE",
        &fw,
        &tune("dojo.sng", 0xD400, false, false),
        |m| {
            assert!(tap_on(m, &fw), "LIVE's tap is off");
        },
    );
}

#[test]
fn a_vbi_tune_plays_bit_exact_with_the_host_under_the_menu() {
    let Some(fw) = fixture("the .sid test") else {
        return;
    };
    check(
        "dojo, VBI, MENU",
        &fw,
        &tune("dojo.sng", 0xD400, false, false),
        |m| {
            press(m, HOME);
            m.run_ms(500);
            assert!(!tap_on(m, &fw), "the tap runs under the menu");
        },
    );
}

#[test]
fn a_cia_tune_plays_bit_exact_with_the_host() {
    let Some(fw) = fixture("the .sid test") else {
        return;
    };
    check(
        "funktest, CIA",
        &fw,
        &tune("funktest.sng", 0xD400, true, false),
        |_| {},
    );
}

#[test]
fn an_ntsc_tune_plays_bit_exact_with_the_host() {
    let Some(fw) = fixture("the .sid test") else {
        return;
    };
    check(
        "wavecmdtest, NTSC",
        &fw,
        &tune("wavecmdtest.sng", 0xD400, false, true),
        |_| {},
    );
}

#[test]
fn a_2sid_tune_plays_bit_exact_with_the_host() {
    let Some(fw) = fixture("the .sid test") else {
        return;
    };
    check(
        "dojo, 2SID at $D420",
        &fw,
        &tune("dojo.sng", 0xD420, false, false),
        |_| {},
    );
}

/// The song the device holds, as its file (the model in its region, written as the store keeps
/// it: its free arena is no part of it).
fn song_file(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let mut song = Song::device();
    let base = fw.symbol("__song_region");
    assert_eq!(ilp32::gt2_song::SIZE, size_of::<gt2_song>());
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

/// The keys' chip (live.keys_chip) and whether live's writes reach a chip (keys_route.silent).
fn keys(m: &mut Machine, fw: &Firmware) -> (u8, u8) {
    let kr = fw.symbol("keys_router");
    let l = m.soc.read32(kr + ilp32::keys_route::l as u32);
    (
        m.soc.read8(l + ilp32::live::keys_chip as u32),
        m.soc.read8(kr + ilp32::keys_route::silent as u32),
    )
}

/// A4's key id.
const A4_KEY: u8 = 30;

/// The sum of chip B's three envelope counters (the sid it renders now).
fn chip_b_envelope(m: &mut Machine, fw: &Firmware) -> u32 {
    let chips = fw.chips();
    let active = m
        .soc
        .read8(chips + ilp32::reg_ring_chips::active as u32 + 1);
    let s =
        chips + ilp32::reg_ring_chips::chip as u32 + u32::from(active) * ilp32::sid::SIZE as u32;
    (0..3u32)
        .map(|v| {
            let e = s + ilp32::sid::env as u32 + v * ilp32::sid_env::SIZE as u32;
            u32::from(m.soc.read8(e + ilp32::sid_env::counter as u32))
        })
        .sum()
}

/// PLAY's press plays the song again once it is back (its transport made again and bound).
fn song_plays(m: &mut Machine, fw: &Firmware) -> bool {
    let calls = fw.symbol("song_transport") + ilp32::song_ctl::calls as u32;
    let before = m.soc.read32(calls);
    press(m, PANEL_BTN_PLAY as u8);
    m.run_ms(1500);
    let n = m.soc.read32(calls).wrapping_sub(before);
    press(m, PANEL_BTN_PLAY as u8);
    let state = fw.symbol("song_transport") + ilp32::song_ctl::state as u32;
    until(m, "the song stops", |m| {
        m.soc.read8(state) != SONG_PLAYING as u8
    });
    m.run_ms(300);
    n > 20
}

/// The song and a tune, both ways, 20 times: with LIVE showing and with the song playing as the
/// tune is asked for, alternately; each time the tune plays (writes, none late), the keys are
/// where they should be (chip B under a 1SID tune; silent under a 2SID tune), the song comes
/// back from the store byte for byte with its transport (PLAY plays it), the keys back on chip A,
/// and nothing faults (the machine's steps panic on a fault; its stacks are checked as it goes).
#[test]
fn the_song_and_a_tune_switch_both_ways_twenty_times() {
    let Some(fw) = fixture("the .sid switch test") else {
        return;
    };
    let one = tune("dojo.sng", 0xD400, false, false);
    let two = tune("funktest.sng", 0xD420, false, false);
    let flash = flash_with(&fw, &[("ONE", &one), ("TWO", &two)]);
    let mut m = boot_with(&fw, &flash);
    // A song of the user's on the store, loaded: the one that must come back.
    song_io(&mut m, &fw, IO_SAVE, "NIGHT");
    song_io(&mut m, &fw, IO_LOAD, "NIGHT");
    let song = song_file(&mut m, &fw);
    assert_eq!(
        keys(&mut m, &fw),
        (0, 0),
        "the keys play chip A at the start"
    );
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    for round in 0..20u32 {
        let two_sid = round % 2 == 1;
        let with_song = round % 4 >= 2;
        if with_song {
            // the song plays as the tune is asked for: it stops first
            press(&mut m, PANEL_BTN_PLAY as u8);
            m.run_ms(600);
        }
        assert!(tap_on(&mut m, &fw), "round {round}: LIVE's tap is off");
        let name = if two_sid { "TWO" } else { "ONE" };
        assert_eq!(
            request(&mut m, &fw, OP_PLAY, name, 0),
            1,
            "round {round}: refused"
        );
        let start = m.ms();
        while !(m.soc.read8(fw.symbol("sid_tune")) == TUNE_ON
            && ctl32(&mut m, &fw, ilp32::psid_ctl::writes) > 500)
        {
            assert!(
                m.ms() - start < WAIT_MS,
                "round {round}: the tune does not play: tune {} psid {} writes {} io {} song {} \
                 keys {:?} made {} {} preparing {} {} active {} {} prep_on {} chip {} lost {}",
                m.soc.read8(fw.symbol("sid_tune")),
                ctl8(&mut m, &fw, ilp32::psid_ctl::state),
                ctl32(&mut m, &fw, ilp32::psid_ctl::writes),
                m.soc
                    .read8(fw.symbol("songs") + ilp32::song_io::state as u32),
                m.soc
                    .read8(fw.symbol("song_transport") + ilp32::song_ctl::state as u32),
                keys(&mut m, &fw),
                m.soc.read8(
                    fw.symbol("keys_router")
                        + (ilp32::keys_route::ctl + ilp32::chip_ctl::made) as u32
                ),
                m.soc.read8(
                    fw.symbol("keys_router")
                        + (ilp32::keys_route::ctl + ilp32::chip_ctl::SIZE + ilp32::chip_ctl::made)
                            as u32
                ),
                m.soc.read8(
                    fw.symbol("keys_router")
                        + (ilp32::keys_route::ctl + ilp32::chip_ctl::preparing) as u32
                ),
                m.soc.read8(
                    fw.symbol("keys_router")
                        + (ilp32::keys_route::ctl
                            + ilp32::chip_ctl::SIZE
                            + ilp32::chip_ctl::preparing) as u32
                ),
                m.soc
                    .read8(fw.chips() + ilp32::reg_ring_chips::active as u32),
                m.soc
                    .read8(fw.chips() + ilp32::reg_ring_chips::active as u32 + 1),
                m.soc
                    .read8(fw.chips() + ilp32::reg_ring_chips::prep_on as u32),
                m.soc
                    .read8(fw.chips() + ilp32::reg_ring_chips::prep_chip as u32),
                m.soc
                    .read32(fw.chips() + ilp32::reg_ring_chips::lost_resets as u32),
            );
            m.run_ms(1);
        }
        m.run_ms(300);
        assert_eq!(
            ctl32(&mut m, &fw, ilp32::psid_ctl::late),
            0,
            "round {round}: late"
        );
        let (chip, silent) = keys(&mut m, &fw);
        if two_sid {
            assert_eq!(silent, 1, "round {round}: the keys sound under a 2SID tune");
        } else {
            assert_eq!(
                (chip, silent),
                (1, 0),
                "round {round}: the keys not on chip B"
            );
        }
        assert_eq!(request(&mut m, &fw, OP_END, "", 0), 1);
        until(&mut m, "the tune ends", |m| {
            m.soc.read8(fw.symbol("sid_tune")) == TUNE_OFF
        });
        assert_eq!(m.soc.read8(fw.symbol("player_now")), AUDIO_SONG);
        song_idle(&mut m, &fw);
        assert!(
            song_file(&mut m, &fw) == song,
            "round {round}: the song did not come back byte for byte"
        );
        until(&mut m, "the keys back on chip A", |m| {
            keys(m, &fw) == (0, 0)
        });
        if round % 5 == 4 {
            assert!(
                song_plays(&mut m, &fw),
                "round {round}: the song does not play back"
            );
        }
    }
    let worst = m.passes.iter().copied().max().unwrap_or(0);
    println!(
        "20 switches: {} passes, the busiest {worst} instructions (POLL_INSTRUCTIONS \
         {POLL_INSTRUCTIONS})",
        m.passes.len()
    );
    assert!(worst <= POLL_INSTRUCTIONS);
}

/// A tune whose play call never returns stops with PLAY HUNG (one second of 6502 time in), the
/// toast says so, and the device goes on: its keys play, and the tune ends for good with the
/// song back.
#[test]
fn a_tune_whose_play_hangs_stops_with_play_hung() {
    let Some(fw) = fixture("the .sid hang test") else {
        return;
    };
    let hung = psid_fixtures::play_forever().bytes();
    let flash = flash_with(&fw, &[("HUNG", &hung)]);
    let mut m = boot_with(&fw, &flash);
    song_io(&mut m, &fw, IO_SAVE, "NIGHT");
    song_io(&mut m, &fw, IO_LOAD, "NIGHT");
    let song = song_file(&mut m, &fw);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    assert_eq!(request(&mut m, &fw, OP_PLAY, "HUNG", 0), 1);
    until(&mut m, "the tune fails", |m| {
        ctl8(m, &fw, ilp32::psid_ctl::state) == PSID_FAILED as u8
    });
    assert_eq!(
        u32::from(ctl8(&mut m, &fw, ilp32::psid_ctl::error)),
        PSID_RUN_PLAY_HUNG
    );
    m.run_ms(50);
    let toast: String = (0..SCREEN_TOAST_CHARS)
        .map(|i| {
            m.soc
                .read8(fw.symbol("ui") + ilp32::screen::toast as u32 + i) as char
        })
        .take_while(|&c| c != '\0')
        .collect();
    assert_eq!(toast, "PLAY HUNG");
    // responsive: a key plays on chip B (the keys' chip under the tune)
    assert_eq!(keys(&mut m, &fw), (1, 0), "the keys are not on chip B");
    assert_eq!(chip_b_envelope(&mut m, &fw), 0);
    m.key(A4_KEY, true);
    m.run_ms(200);
    assert!(
        chip_b_envelope(&mut m, &fw) > 0,
        "a key does not sound on chip B"
    );
    m.key(A4_KEY, false);
    assert_eq!(request(&mut m, &fw, OP_END, "", 0), 1);
    until(&mut m, "the tune ends", |m| {
        m.soc.read8(fw.symbol("sid_tune")) == TUNE_OFF
    });
    song_idle(&mut m, &fw);
    assert!(song_file(&mut m, &fw) == song, "the song did not come back");
    let worst = m.passes.iter().copied().max().unwrap_or(0);
    println!("PLAY HUNG: the busiest pass {worst} instructions");
    assert!(worst <= POLL_INSTRUCTIONS);
}
