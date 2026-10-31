// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::panel::{press, wait_for_within, A4_KEY, PLAY};
use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::psid::packed;
use sidou_tests::psid_fixtures;
use sidou_tests::song::Song;

const PLAY_MS: u64 = 10_000;
const MAX_SILENT_HALVES: usize = 4;
const AUDIO_SONG: u8 = 0;
const AUDIO_PSID: u8 = 1;
const TUNE_OFF: u8 = 0;
const TUNE_ON: u8 = 5;
const IO_SAVE: u32 = 2;
const IO_LOAD: u32 = 3;
const WAIT_MS: u64 = 15_000;
const SONG_IO_HOOK_NAME: u32 = 4;
const SONG_IO_HOOK_RESULT: u32 = 24;
const PSID_SPEED: usize = 0x12;
const PSID_FLAGS: usize = 0x76;
const PSID_CLOCK_BITS: u16 = 3 << 2;
const SONG_1_ON_CIA: u32 = 1;
const AUDIBLE_PEAK: i32 = 1000;
const SWITCH_ROUNDS: u32 = 20;
const TUNE_PLAYING_WRITES: u32 = 500;
const SONG_PLAYS_CALLS: u32 = 20;
const SONG_PLAYS_MS: u64 = 1_500;
const SONG_STOPS_MS: u64 = 600;
const AFTER_STOP_MS: u64 = 300;
const TUNE_SETTLE_MS: u64 = 300;
const KEY_SOUNDS_MS: u64 = 200;
const TOAST_SHOWN_MS: u64 = 50;

fn example(name: &str) -> Vec<u8> {
    gt2_ref::examples()
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("{name}"))
        .1
}

fn packed_tune(sng: &str, sid: u32, cia: bool, ntsc: bool) -> Vec<u8> {
    let mut f = packed(&example(sng), sid);
    if cia {
        f[PSID_SPEED..PSID_SPEED + 4].copy_from_slice(&SONG_1_ON_CIA.to_be_bytes());
    }
    if ntsc {
        let flags = u16::from_be_bytes([f[PSID_FLAGS], f[PSID_FLAGS + 1]]) & !PSID_CLOCK_BITS;
        f[PSID_FLAGS..PSID_FLAGS + 2]
            .copy_from_slice(&(flags | psid_fixtures::CLOCK_NTSC).to_be_bytes());
    }
    f
}

fn until(m: &mut Machine, what: &str, done: impl FnMut(&mut Machine) -> bool) {
    wait_for_within(m, WAIT_MS, what, done);
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

fn tune_state(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc.read8(fw.symbol("sid_tune"))
}

fn psid_ctl(fw: &Firmware) -> u32 {
    fw.symbol("player_isr")
}

fn ctl32(m: &mut Machine, fw: &Firmware, offset: usize) -> u32 {
    m.soc.read32(psid_ctl(fw) + offset as u32)
}

fn ctl8(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    m.soc.read8(psid_ctl(fw) + offset as u32)
}

fn flash_with(fw: &Firmware, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut flash = settled_flash(&fw.image);
    for (name, file) in files {
        put_file(&mut flash, name, STORE_FILE_SID, file);
    }
    flash
}

fn powered_on_with(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    m
}

fn past_boot_and_song_load(m: &mut Machine, fw: &Firmware) {
    run_past_boot(m, fw);
    song_idle(m, fw);
}

fn boot_with(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = powered_on_with(fw, flash);
    past_boot_and_song_load(&mut m, fw);
    m
}

struct Played {
    audio: Vec<i16>,
    psid_block: usize,
    fresh_block: [Option<usize>; 2],
    start_block: usize,
    b_use: Option<usize>,
    late: u32,
    writes: u32,
}

fn play(fw: &Firmware, file: &[u8], ms: u64, after: impl FnOnce(&mut Machine)) -> Played {
    let flash = flash_with(fw, &[("TUNE", file)]);
    let mut m = powered_on_with(fw, &flash);
    let chips = fw.chips();
    let active = chips + ilp32::reg_ring_chips::active as u32;
    m.snapshot_bytes = vec![
        fw.symbol("player_now"),
        psid_ctl(fw) + ilp32::psid_ctl::blocks as u32,
        fw.symbol("b_on"),
        active,
        active + 1,
    ];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 1)));
    past_boot_and_song_load(&mut m, fw);
    let start = m.ms();
    ui_play_tune(&mut m, fw, 0);
    after(&mut m);
    m.run_ms(ms.saturating_sub(m.ms() - start));
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
                    mute_block: None,
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

fn assert_plays_bit_exact(
    name: &str,
    fw: &Firmware,
    file: &[u8],
    after: impl FnOnce(&mut Machine),
) {
    let two = sidou_tests::psid::header(file).sid2 != 0;
    let p = play(fw, file, PLAY_MS, after);
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
    assert!(
        peak > AUDIBLE_PEAK,
        "{name}: the tune is silent (peak {peak})"
    );
    assert_eq!(p.late, 0, "{name}: writes applied late");
    assert!(
        r.is_ok(),
        "{name}: the firmware's tune differs from the host's: closest {r:?} LSB"
    );
}

#[test]
fn a_vbi_tune_plays_bit_exact_with_the_host_while_live_shows() {
    let Some(fw) = firmware("the .sid test") else {
        return;
    };
    assert_plays_bit_exact(
        "dojo, VBI, LIVE",
        &fw,
        &packed_tune("dojo.sng", 0xD400, false, false),
        |m| {
            ui_back_to(m, &fw, "live_view_view");
            assert!(tap_on(m, &fw), "LIVE's tap is off");
        },
    );
}

#[test]
fn a_vbi_tune_plays_bit_exact_with_the_host_under_the_menu() {
    let Some(fw) = firmware("the .sid test") else {
        return;
    };
    assert_plays_bit_exact(
        "dojo, VBI, MENU",
        &fw,
        &packed_tune("dojo.sng", 0xD400, false, false),
        |m| {
            ui_back_to(m, &fw, "views_menu");
            assert!(!tap_on(m, &fw), "the tap runs under the menu");
        },
    );
}

#[test]
fn a_cia_tune_plays_bit_exact_with_the_host() {
    let Some(fw) = firmware("the .sid test") else {
        return;
    };
    assert_plays_bit_exact(
        "funktest, CIA",
        &fw,
        &packed_tune("funktest.sng", 0xD400, true, false),
        |_| {},
    );
}

#[test]
fn an_ntsc_tune_plays_bit_exact_with_the_host() {
    let Some(fw) = firmware("the .sid test") else {
        return;
    };
    assert_plays_bit_exact(
        "wavecmdtest, NTSC",
        &fw,
        &packed_tune("wavecmdtest.sng", 0xD400, false, true),
        |_| {},
    );
}

#[test]
fn a_2sid_tune_plays_bit_exact_with_the_host() {
    let Some(fw) = firmware("the .sid test") else {
        return;
    };
    assert_plays_bit_exact(
        "dojo, 2SID at $D420",
        &fw,
        &packed_tune("dojo.sng", 0xD420, false, false),
        |_| {},
    );
}

fn device_song_file(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let mut song = Song::device();
    let base = fw.symbol("__song_region");
    assert_eq!(ilp32::gt2_song::SIZE, size_of::<gt2_song>());
    for i in 0..SONG_REGION_BYTES {
        unsafe {
            song.ptr_mut()
                .cast::<u8>()
                .add(i as usize)
                .write(m.soc.read8(base + i))
        };
    }
    song.write(true)
}

fn keys_chip_and_silence(m: &mut Machine, fw: &Firmware) -> (u8, u8) {
    let kr = fw.symbol("keys_router");
    let l = m.soc.read32(kr + ilp32::keys_route::l as u32);
    (
        m.soc.read8(l + ilp32::live::keys_chip as u32),
        m.soc.read8(kr + ilp32::keys_route::silent as u32),
    )
}

fn chip_b_envelope(m: &mut Machine, fw: &Firmware) -> u32 {
    let s = rendered_sid(m, fw, 1);
    (0..3u32)
        .map(|v| {
            let e = s + ilp32::sid::env as u32 + v * ilp32::sid_env::SIZE as u32;
            u32::from(m.soc.read8(e + ilp32::sid_env::counter as u32))
        })
        .sum()
}

fn a_users_song_saved_and_loaded(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    song_io(m, fw, IO_SAVE, "NIGHT");
    song_io(m, fw, IO_LOAD, "NIGHT");
    device_song_file(m, fw)
}

fn tune_diagnostics(m: &mut Machine, fw: &Firmware) -> String {
    let router_ctl = |m: &mut Machine, chip: usize, field: usize| {
        m.soc.read8(
            fw.symbol("keys_router")
                + (ilp32::keys_route::ctl + chip * ilp32::chip_ctl::SIZE + field) as u32,
        )
    };
    let chips = |m: &mut Machine, field: usize| m.soc.read8(fw.chips() + field as u32);
    format!(
        "tune {} psid {} writes {} io {} song {} keys {:?} made {} {} preparing {} {} active {} {} \
         prep_on {} chip {} lost {}",
        tune_state(m, fw),
        ctl8(m, fw, ilp32::psid_ctl::state),
        ctl32(m, fw, ilp32::psid_ctl::writes),
        m.soc.read8(fw.symbol("songs") + ilp32::song_io::state as u32),
        m.soc
            .read8(fw.symbol("song_transport") + ilp32::song_ctl::state as u32),
        keys_chip_and_silence(m, fw),
        router_ctl(m, 0, ilp32::chip_ctl::made),
        router_ctl(m, 1, ilp32::chip_ctl::made),
        router_ctl(m, 0, ilp32::chip_ctl::preparing),
        router_ctl(m, 1, ilp32::chip_ctl::preparing),
        chips(m, ilp32::reg_ring_chips::active),
        chips(m, ilp32::reg_ring_chips::active + 1),
        chips(m, ilp32::reg_ring_chips::prep_on),
        chips(m, ilp32::reg_ring_chips::prep_chip),
        m.soc
            .read32(fw.chips() + ilp32::reg_ring_chips::lost_resets as u32),
    )
}

fn play_plays_the_song_back(m: &mut Machine, fw: &Firmware) -> bool {
    let calls = fw.symbol("song_transport") + ilp32::song_ctl::calls as u32;
    let before = m.soc.read32(calls);
    press(m, PLAY);
    m.run_ms(SONG_PLAYS_MS);
    let n = m.soc.read32(calls).wrapping_sub(before);
    press(m, PLAY);
    let state = fw.symbol("song_transport") + ilp32::song_ctl::state as u32;
    until(m, "the song stops", |m| {
        m.soc.read8(state) != SONG_PLAYING as u8
    });
    m.run_ms(AFTER_STOP_MS);
    n > SONG_PLAYS_CALLS
}

#[test]
fn the_song_and_a_tune_switch_both_ways_twenty_times_with_the_keys_routed_and_the_song_back_byte_for_byte(
) {
    let Some(fw) = firmware("the .sid switch test") else {
        return;
    };
    let one = packed_tune("dojo.sng", 0xD400, false, false);
    let two = packed_tune("funktest.sng", 0xD420, false, false);
    let flash = flash_with(&fw, &[("ONE", &one), ("TWO", &two)]);
    let mut m = boot_with(&fw, &flash);
    let song = a_users_song_saved_and_loaded(&mut m, &fw);
    assert_eq!(
        keys_chip_and_silence(&mut m, &fw),
        (0, 0),
        "the keys play chip A at the start"
    );
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    for round in 0..SWITCH_ROUNDS {
        let two_sid = round % 2 == 1;
        let song_playing_at_the_ask = round % 4 >= 2;
        let checks_the_song_plays_back = round % 5 == 4;
        if song_playing_at_the_ask {
            press(&mut m, PLAY);
            m.run_ms(SONG_STOPS_MS);
        }
        assert!(tap_on(&mut m, &fw), "round {round}: LIVE's tap is off");
        let browser_index = u32::from(two_sid);
        ui_play_tune(&mut m, &fw, browser_index);
        ui_back_to(&mut m, &fw, "live_view_view");
        let start = m.ms();
        while !(tune_state(&mut m, &fw) == TUNE_ON
            && ctl32(&mut m, &fw, ilp32::psid_ctl::writes) > TUNE_PLAYING_WRITES)
        {
            assert!(
                m.ms() - start < WAIT_MS,
                "round {round}: the tune does not play: {}",
                tune_diagnostics(&mut m, &fw)
            );
            m.run_ms(1);
        }
        m.run_ms(TUNE_SETTLE_MS);
        assert_eq!(
            ctl32(&mut m, &fw, ilp32::psid_ctl::late),
            0,
            "round {round}: late"
        );
        let (chip, silent) = keys_chip_and_silence(&mut m, &fw);
        if two_sid {
            assert_eq!(silent, 1, "round {round}: the keys sound under a 2SID tune");
        } else {
            assert_eq!(
                (chip, silent),
                (1, 0),
                "round {round}: the keys not on chip B"
            );
        }
        ui_end_tune(&mut m, &fw);
        until(&mut m, "the tune ends", |m| tune_state(m, &fw) == TUNE_OFF);
        assert_eq!(m.soc.read8(fw.symbol("player_now")), AUDIO_SONG);
        song_idle(&mut m, &fw);
        assert!(
            device_song_file(&mut m, &fw) == song,
            "round {round}: the song did not come back byte for byte"
        );
        until(&mut m, "the keys back on chip A", |m| {
            keys_chip_and_silence(m, &fw) == (0, 0)
        });
        if checks_the_song_plays_back {
            assert!(
                play_plays_the_song_back(&mut m, &fw),
                "round {round}: the song does not play back"
            );
        }
    }
    let worst = m.passes.iter().copied().max().unwrap_or(0);
    println!(
        "{SWITCH_ROUNDS} switches: {} passes, the busiest {worst} instructions",
        m.passes.len()
    );
    assert_within_budget("the busiest pass", worst, POLL_INSTRUCTIONS);
}

#[test]
fn a_tune_whose_play_hangs_stops_with_play_hung_and_the_keys_still_play() {
    let Some(fw) = firmware("the .sid hang test") else {
        return;
    };
    let hung = psid_fixtures::play_forever().bytes();
    let flash = flash_with(&fw, &[("HUNG", &hung)]);
    let mut m = boot_with(&fw, &flash);
    let song = a_users_song_saved_and_loaded(&mut m, &fw);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    ui_play_tune(&mut m, &fw, 0);
    until(&mut m, "the tune fails", |m| {
        ctl8(m, &fw, ilp32::psid_ctl::state) == PSID_FAILED as u8
    });
    assert_eq!(
        u32::from(ctl8(&mut m, &fw, ilp32::psid_ctl::error)),
        PSID_RUN_PLAY_HUNG
    );
    m.run_ms(TOAST_SHOWN_MS);
    let toast: String = (0..SCREEN_TOAST_CHARS)
        .map(|i| {
            m.soc
                .read8(fw.symbol("ui") + ilp32::screen::toast as u32 + i) as char
        })
        .take_while(|&c| c != '\0')
        .collect();
    assert_eq!(toast, "PLAY HUNG");
    assert_eq!(
        keys_chip_and_silence(&mut m, &fw),
        (1, 0),
        "the keys are not on chip B"
    );
    assert_eq!(chip_b_envelope(&mut m, &fw), 0);
    m.key(A4_KEY, true);
    m.run_ms(KEY_SOUNDS_MS);
    assert!(
        chip_b_envelope(&mut m, &fw) > 0,
        "a key does not sound on chip B"
    );
    m.key(A4_KEY, false);
    ui_end_tune(&mut m, &fw);
    until(&mut m, "the tune ends", |m| tune_state(m, &fw) == TUNE_OFF);
    song_idle(&mut m, &fw);
    assert!(
        device_song_file(&mut m, &fw) == song,
        "the song did not come back"
    );
    let worst = m.passes.iter().copied().max().unwrap_or(0);
    println!("PLAY HUNG: the busiest pass {worst} instructions");
    assert_within_budget("the busiest pass", worst, POLL_INSTRUCTIONS);
}
