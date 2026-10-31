// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::panel::wait_for_within;
use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::psid::{full_size, packed};

const STORE_POLL_WORST: u64 = 30_000;
const TRS_GAP_MS: u64 = 1_000;
const TRS_US_PER_BYTE: u64 = 320;
const WAIT_MS: u64 = 20_000;
const STAT_PROGS: u32 = 0;
const TUNE_OFF: u8 = 0;
const TUNE_ON: u8 = 5;
const SYSEX_IDLE_STATE: u8 = 0;
const SYSEX_UPLOAD_STATE: u8 = 1;
const SYSEX_SEQUENCES: usize = 128;
const PROGRAMMED_POLL_LEAD_MS: u64 = 2;
const TOAST_LOOK_MS: u64 = 50;
const PSID_DATA_OFFSET: usize = 6;
const PSID_FLAGS_LOW: usize = 0x77;
const PSID_FLAG_MUS: u8 = 1;
const PSID_V2_HEADER: usize = 0x80;
const PAST_THE_HEADER: u16 = 0x0200;
const AUDIBLE_PEAK: i32 = 1000;
const PLAYING_WRITES: u32 = 100;
const SID_IO_CLEAR_PASSES: usize = 4;
const PACK_STORE_SHARE_MAX: u32 = 4;
const PACK_KEYS_LEFT_MIN: u32 = 20;
const FILLER_KEY0: u32 = 0x1000;
const FILLER_TYPE: u8 = 1;
const FILLER_BYTES: usize = 8;
const FRAMES_BEFORE_THE_REFUSAL: usize = 2;
const EIGHTH_IN_THE_BROWSER: u32 = SID_VIEW_FILES - 1;

fn until(m: &mut Machine, what: &str, done: impl FnMut(&mut Machine) -> bool) {
    wait_for_within(m, WAIT_MS, what, done);
}

fn wire_ms(frame: &[u8]) -> u64 {
    (frame.len() as u64 * TRS_US_PER_BYTE).div_ceil(1000)
}

fn goat() -> Vec<u8> {
    let (_, sng) = gt2_ref::examples()
        .into_iter()
        .find(|(n, _)| n == "dojo.sng")
        .expect("dojo.sng");
    packed(&sng, 0xD400)
}

fn boot(fw: &Firmware, flash: Option<&[u8]>) -> Machine {
    let mut m = Machine::boot(&fw.image);
    if let Some(flash) = flash {
        m.soc.flash.mem.copy_from_slice(flash);
    }
    run_past_boot(&mut m, fw);
    let songs = fw.symbol("songs") + ilp32::song_io::state as u32;
    until(&mut m, "song I/O idle", |m| {
        m.soc.read8(songs) == SONG_IO_IDLE as u8
    });
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    m
}

fn frames(syx: &[u8]) -> Vec<&[u8]> {
    syx.split_inclusive(|&b| b == 0xf7).collect()
}

fn send_paced(m: &mut Machine, syx: &[u8], gap_ms: u64) {
    for f in frames(syx) {
        m.midi(f);
        m.run_ms(wire_ms(f) + gap_ms);
    }
}

fn send_ready(m: &mut Machine, fw: &Firmware, syx: &[u8]) {
    let s = fw.symbol("sidou_sysex");
    let fill =
        fw.symbol("store_fm1_store") + (ilp32::store::f + ilp32::store_file_writer::fill) as u32;
    for (i, f) in frames(syx).iter().enumerate() {
        m.midi(f);
        m.run_ms(wire_ms(f) + PROGRAMMED_POLL_LEAD_MS);
        let next_sequence = ((i + 1) % SYSEX_SEQUENCES) as u8;
        until(m, "the packet programmed", |m| {
            let state = m.soc.read8(s + ilp32::sysex::state as u32);
            let ready_for_the_next = state == SYSEX_UPLOAD_STATE
                && m.soc.read8(s + ilp32::sysex::sequence as u32) == next_sequence
                && m.soc.read16(fill) == 0;
            let end_committed =
                state == SYSEX_IDLE_STATE && m.soc.read8(s + ilp32::sysex::claimed as u32) == 0;
            ready_for_the_next || end_committed
        });
    }
}

fn stored(m: &Machine, name: &str) -> Option<(u8, Vec<u8>)> {
    stored_file(&m.soc.flash.mem, name)
}

fn toast(m: &mut Machine, fw: &Firmware) -> String {
    let at = fw.symbol("ui") + ilp32::screen::toast as u32;
    (0..SCREEN_TOAST_CHARS)
        .map(|i| m.soc.read8(at + i))
        .take_while(|&b| b != 0)
        .map(char::from)
        .collect()
}

fn tune_state(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc.read8(fw.symbol("sid_tune"))
}

struct TunePlayed {
    writes: u32,
    late: u32,
    peak: i32,
}

fn plays_from_the_browser(
    m: &mut Machine,
    fw: &Firmware,
    index: u32,
    ms: u64,
    name: &str,
) -> TunePlayed {
    let audio = m.soc.periph.audio.len();
    ui_play_tune(m, fw, index);
    until(m, "the tune on", |m| tune_state(m, fw) == TUNE_ON);
    m.run_ms(ms);
    assert_eq!(tune_state(m, fw), TUNE_ON, "{name}: the tune stopped");
    let ctl = fw.symbol("player_isr");
    let played = TunePlayed {
        writes: m.soc.read32(ctl + ilp32::psid_ctl::writes as u32),
        late: m.soc.read32(ctl + ilp32::psid_ctl::late as u32),
        peak: m.soc.periph.audio[audio..]
            .iter()
            .map(|&v| i32::from(v).abs())
            .max()
            .unwrap_or(0),
    };
    println!(
        "{name}: {} writes, {} late, peak {}",
        played.writes, played.late, played.peak
    );
    assert!(played.peak > AUDIBLE_PEAK, "{name}: silent");
    assert_eq!(played.late, 0, "{name}: writes applied late");
    ui_end_tune(m, fw);
    until(m, "the song back", |m| tune_state(m, fw) == TUNE_OFF);
    played
}

fn plays(m: &mut Machine, fw: &Firmware, name: &str) {
    let played = plays_from_the_browser(m, fw, 0, 3_000, name);
    assert!(played.writes > PLAYING_WRITES, "{name}: silent");
}

fn check_passes(m: &Machine, what: &str) {
    let worst = *m.passes.iter().max().expect("passes");
    assert_within_budget(
        &format!("{what}: the busiest pass"),
        worst,
        POLL_INSTRUCTIONS,
    );
    assert_eq!(
        m.soc.periph.midi_dropped(),
        0,
        "the TRS queue dropped bytes"
    );
}

#[test]
fn the_clis_syx_over_trs_stores_a_tune_byte_for_byte_and_it_loads_and_plays() {
    let Some(fw) = firmware("the .sid store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    let file = goat();
    let h = sidou_sysex::sid::check(&file).expect("the tune checks");
    let mut taken = Default::default();
    let name = sidou_sysex::sid::store_name(&sidou_sysex::sid::title(&h), "dojo", &mut taken);
    let syx = sidou_sysex::sid::syx(&name, &file).unwrap();
    let start = m.ms();
    send_paced(&mut m, &syx, TRS_GAP_MS);
    println!(
        "{name}: {} bytes in {} frames over TRS, {} ms",
        file.len(),
        frames(&syx).len(),
        m.ms() - start
    );
    assert_eq!(toast(&mut m, &fw), "SAVED");
    assert_eq!(stored(&m, &name), Some((STORE_FILE_SID as u8, file)));
    plays(&mut m, &fw, &name);
    check_passes(&m, "a tune over TRS");
}

#[test]
fn rsid_mus_and_crafted_files_are_refused_over_trs_before_any_flash_program() {
    let Some(fw) = firmware("the .sid store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    let good = goat();
    let mut rsid = good.clone();
    rsid[..4].copy_from_slice(b"RSID");
    let mut mus = good.clone();
    mus[PSID_FLAGS_LOW] |= PSID_FLAG_MUS;
    let mut crafted = good[..PSID_V2_HEADER].to_vec();
    crafted[PSID_DATA_OFFSET..PSID_DATA_OFFSET + 2].copy_from_slice(&PAST_THE_HEADER.to_be_bytes());
    let counts = fw.symbol("store_fm1_counts");
    for (name, file, says) in [
        ("RSID", &rsid, "RSID: NOT YET"),
        ("MUS", &mus, "MUS: NOT SUPPORTED"),
        ("CRAFTED", &crafted, "BROKEN .SID"),
    ] {
        let before = m.soc.flash.mem.clone();
        let progs = m.soc.read32(counts + STAT_PROGS);
        let syx = sidou_sysex::put_frames(name, sidou_sysex::FILE_SID, file)
            .unwrap()
            .concat();
        m.soc
            .write8(fw.symbol("ui") + ilp32::screen::toast as u32, 0);
        let mut shown = String::new();
        for f in frames(&syx) {
            m.midi(f);
            m.run_ms(wire_ms(f) + TOAST_LOOK_MS);
            if shown.is_empty() {
                let t = toast(&mut m, &fw);
                if !t.is_empty() && t != "RECEIVING" {
                    shown = t;
                }
            }
            m.run_ms(TRS_GAP_MS - TOAST_LOOK_MS);
        }
        assert_eq!(shown, says, "{name}");
        assert_eq!(
            m.soc.read32(counts + STAT_PROGS),
            progs,
            "{name}: a program"
        );
        assert!(m.soc.flash.mem == before, "{name}: the flash changed");
        assert_eq!(stored(&m, name), None);
    }
    check_passes(&m, "refusals over TRS");
}

#[test]
fn a_64_kib_tune_goes_in_and_loads_within_the_stores_pass_share() {
    let Some(fw) = firmware("the .sid store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    let file = full_size(&goat());
    assert_eq!(file.len(), STORE_FILE_MAX as usize);
    let syx = sidou_sysex::sid::syx("BIG TUNE", &file).unwrap();
    send_ready(&mut m, &fw, &syx);
    assert_eq!(stored(&m, "BIG TUNE"), Some((STORE_FILE_SID as u8, file)));
    check_passes(&m, "64 KiB over TRS");
    for (function, calls) in [("sid_io_poll", 36), ("sid_io_open", 2)] {
        m.watch_calls(fw.symbol(function));
        plays(&mut m, &fw, "BIG TUNE");
        let worst = *m.watched_calls.iter().max().expect("called");
        println!(
            "{function}: {} calls, at most {worst} instructions",
            m.watched_calls.len()
        );
        assert_eq!(m.watched_calls.len(), calls, "{function}'s calls");
        if function == "sid_io_poll" {
            let (clear, data) = m.watched_calls.split_at(SID_IO_CLEAR_PASSES);
            println!(
                "  the clear's passes at most {}, the data's at most {}",
                clear.iter().max().unwrap(),
                data.iter().max().unwrap()
            );
        }
        assert_within_budget(function, worst, STORE_POLL_WORST);
        m.watched_calls.clear();
    }
    check_passes(&m, "64 KiB loaded and played");
}

fn starter() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/starter");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("assets/starter")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "sid"))
        .collect();
    paths.sort();
    let mut taken = Default::default();
    paths
        .iter()
        .map(|p| {
            let file = std::fs::read(p).unwrap();
            let h = sidou_sysex::sid::check(&file).expect("a starter tune checks");
            let stem = p.file_stem().unwrap().to_string_lossy();
            let name =
                sidou_sysex::sid::store_name(&sidou_sysex::sid::title(&h), &stem, &mut taken);
            (name, file)
        })
        .collect()
}

#[test]
fn the_starter_pack_goes_into_the_store_over_trs_whole_with_room_to_spare() {
    let Some(fw) = firmware("the .sid store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    let before = u32::from(mount_dump(&mut m.soc.flash.mem.clone()).nkeys);
    let tunes = starter();
    assert!(
        tunes.len() > EIGHTH_IN_THE_BROWSER as usize,
        "the starter pack fills less than the browser's first page"
    );
    let syx: Vec<u8> = tunes
        .iter()
        .flat_map(|(name, file)| sidou_sysex::sid::syx(name, file).unwrap())
        .collect();
    let start = m.ms();
    send_paced(&mut m, &syx, TRS_GAP_MS);
    for (name, file) in &tunes {
        let got = stored(&m, name);
        assert!(
            got.as_ref() == Some(&(STORE_FILE_SID as u8, file.clone())),
            "{name}: stored {:?} bytes",
            got.map(|g| g.1.len())
        );
    }
    let bytes: usize = tunes.iter().map(|(_, f)| f.len()).sum();
    let keys = u32::from(mount_dump(&mut m.soc.flash.mem.clone()).nkeys);
    println!(
        "the starter pack: {} tunes, {bytes} bytes (the store holds {STORE_CAPACITY}), {} of the \
         store's {STORE_KEYS_MAX} keys used ({before} before, {} for the tunes), {} frames in {} \
         ms of device time",
        tunes.len(),
        keys,
        keys - before,
        frames(&syx).len(),
        m.ms() - start
    );
    for (name, file) in &tunes {
        println!("  {name}: {} bytes", file.len());
    }
    assert!(
        bytes as u32 * PACK_STORE_SHARE_MAX <= STORE_CAPACITY,
        "the pack takes over a quarter of the store"
    );
    assert!(
        keys + PACK_KEYS_LEFT_MIN <= STORE_KEYS_MAX,
        "the pack leaves {} keys",
        STORE_KEYS_MAX - keys
    );
    for index in [0, EIGHTH_IN_THE_BROWSER] {
        plays_from_the_browser(
            &mut m,
            &fw,
            index,
            2_000,
            &format!("tune {index} of the browser"),
        );
    }
    check_passes(&m, "the starter pack over TRS");
}

fn fill_keys_with_small_values(flash: &mut [u8], n: u32) {
    for k in 0..n {
        put_value(
            flash,
            FILLER_KEY0 + k,
            FILLER_TYPE,
            &[k as u8; FILLER_BYTES],
        );
    }
}

#[test]
fn with_the_stores_keys_used_up_a_tunes_put_is_refused_store_full() {
    let Some(fw) = firmware("the .sid store tests") else {
        return;
    };
    let mut flash = settled_flash(&fw.image);
    let taken = {
        let st = mount_dump(&mut flash);
        u32::from(st.nkeys)
    };
    fill_keys_with_small_values(&mut flash, STORE_KEYS_MAX - taken);
    assert_eq!(
        u32::from(mount_dump(&mut flash.clone()).nkeys),
        STORE_KEYS_MAX
    );
    let mut m = boot(&fw, Some(&flash));
    let counts = fw.symbol("store_fm1_counts");
    let progs = m.soc.read32(counts + STAT_PROGS);
    let syx = sidou_sysex::sid::syx("NO ROOM", &goat()).unwrap();
    let all = frames(&syx);
    for f in &all[..FRAMES_BEFORE_THE_REFUSAL] {
        m.midi(f);
        m.run_ms(wire_ms(f) + TOAST_LOOK_MS);
    }
    assert_eq!(toast(&mut m, &fw), "STORE FULL");
    for f in &all[FRAMES_BEFORE_THE_REFUSAL..] {
        m.midi(f);
        m.run_ms(wire_ms(f) + TOAST_LOOK_MS);
        assert_eq!(toast(&mut m, &fw), "STORE FULL", "the refusal stands");
    }
    assert_eq!(m.soc.read32(counts + STAT_PROGS), progs, "a program");
    assert_eq!(stored(&m, "NO ROOM"), None);
    check_passes(&m, "STORE FULL");
}
