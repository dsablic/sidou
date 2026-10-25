// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! .sid files on the store (M3 Task 10, docs/plans/2026-10-19-m3-sid-files.md), in fm1-emu
//! over the emulated TRS jack (no physical MIDI or USB device): the `.syx` `sidou-sysex sid`
//! writes, sent a frame a second as docs/manual/sid-tunes.md says, stores the tune byte for
//! byte, and it loads (app/sid_io.c) and plays from the panel (the menu's SIDS: Task 11's views,
//! common::ui_play_tune); an RSID, a MUS and a crafted file are refused with
//! their toasts before any flash program; the largest file the store holds (a 64 KiB tune) goes
//! in and plays, every sid_io_poll and sid_io_open within the store's share of a pass
//! (STORE_POLL_WORST) and every pass within POLL_INSTRUCTIONS; with the store's keys used up,
//! a tune's PUT is refused (STORE FULL) before any program; and the starter pack (Task 13,
//! assets/starter), `sidou-sysex sid --pack`'s one `.syx`, goes in over TRS whole with room to
//! spare (the bytes and keys it uses are printed) and its tunes play from the browser.

mod common;

use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::psid::{full_size, packed};

/// The store's share of a main-loop pass (emu-gate store.rs, usb.rs).
const STORE_POLL_WORST: u64 = 30_000;
/// docs/reference/sysex.md: over TRS, one second after each frame.
const TRS_GAP_MS: u64 = 1_000;
const WAIT_MS: u64 = 20_000;
/// store_fm1_counts: programs.
const PROGS: u32 = 0;
/// main.c's tune states: TUNE_OFF, TUNE_ON.
const TUNE_OFF: u8 = 0;
const TUNE_ON: u8 = 5;

fn fixture() -> Option<Firmware> {
    let Some(build) = build_dir() else {
        skip_message("the .sid store tests");
        return None;
    };
    Some(Firmware::load(&build))
}

fn until(m: &mut Machine, what: &str, mut done: impl FnMut(&mut Machine) -> bool) {
    let start = m.ms();
    while !done(m) {
        assert!(m.ms() - start < WAIT_MS, "timed out: {what}");
        m.run_ms(1);
    }
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

/// A `.syx`'s frames, as a SysEx tool sends them: each ends at its F7.
fn frames(syx: &[u8]) -> Vec<&[u8]> {
    syx.split_inclusive(|&b| b == 0xf7).collect()
}

/// The frames into the TRS jack, each followed by `gap_ms` (from its last byte on the wire).
fn send_paced(m: &mut Machine, syx: &[u8], gap_ms: u64) {
    for f in frames(syx) {
        m.midi(f);
        m.run_ms((f.len() as u64 * 320).div_ceil(1000) + gap_ms);
    }
}

/// The frames into the TRS jack, each once the last is programmed (the transfer back in
/// UPLOAD, the store's staging empty): the fastest a sender may go, for the 64 KiB file.
fn send_ready(m: &mut Machine, fw: &Firmware, syx: &[u8]) {
    let s = fw.symbol("sidou_sysex");
    let fill =
        fw.symbol("store_fm1_store") + (ilp32::store::f + ilp32::store_file_writer::fill) as u32;
    for (i, f) in frames(syx).iter().enumerate() {
        m.midi(f);
        m.run_ms((f.len() as u64 * 320).div_ceil(1000) + 2);
        until(m, "the packet programmed", |m| {
            let state = m.soc.read8(s + ilp32::sysex::state as u32);
            // UPLOAD (1) with the next sequence, or idle once END has committed.
            (state == 1
                && m.soc.read8(s + ilp32::sysex::sequence as u32) == ((i + 1) & 127) as u8
                && m.soc.read16(fill) == 0)
                || (state == 0 && m.soc.read8(s + ilp32::sysex::claimed as u32) == 0)
        });
    }
}

fn stored(m: &Machine, name: &str) -> Option<(u8, Vec<u8>)> {
    let mut flash = m.soc.flash.mem.clone();
    let st = mount_dump(&mut flash);
    let mut file = store_file::default();
    let name = std::ffi::CString::new(name).unwrap();
    // SAFETY: st, file and the buffer outlive each store call.
    unsafe {
        if store_file_open(&*st, name.as_ptr(), &mut file) != STORE_OK {
            return None;
        }
        let mut data = vec![0; file.length as usize];
        assert_eq!(
            store_file_read(&*st, &file, 0, data.as_mut_ptr().cast(), file.length),
            file.length as i32
        );
        Some((file.type_, data))
    }
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

/// Plays the store's tune `name` (its only one) from the panel for 3 s: it is on, it sounds,
/// nothing late; then it ends for good through the panel.
fn plays(m: &mut Machine, fw: &Firmware, name: &str) {
    let audio = m.soc.periph.audio.len();
    ui_play_tune(m, fw, 0);
    until(m, "the tune on", |m| tune_state(m, fw) == TUNE_ON);
    m.run_ms(3_000);
    assert_eq!(tune_state(m, fw), TUNE_ON, "{name}: the tune stopped");
    let ctl = fw.symbol("player_isr");
    let writes = m.soc.read32(ctl + ilp32::psid_ctl::writes as u32);
    let late = m.soc.read32(ctl + ilp32::psid_ctl::late as u32);
    let peak = m.soc.periph.audio[audio..]
        .iter()
        .map(|&v| i32::from(v).abs())
        .max()
        .unwrap_or(0);
    println!("{name}: {writes} writes, {late} late, peak {peak}");
    assert!(writes > 100 && peak > 1000, "{name}: silent");
    assert_eq!(late, 0, "{name}: writes applied late");
    ui_end_tune(m, fw);
    until(m, "the song back", |m| tune_state(m, fw) == TUNE_OFF);
}

fn check_passes(m: &Machine, what: &str) {
    let worst = *m.passes.iter().max().expect("passes");
    println!("{what}: the busiest pass {worst} (POLL_INSTRUCTIONS {POLL_INSTRUCTIONS})");
    assert!(worst <= POLL_INSTRUCTIONS, "{what}: a pass of {worst}");
    assert_eq!(
        m.soc.periph.midi_dropped(),
        0,
        "the TRS queue dropped bytes"
    );
}

#[test]
fn the_clis_syx_over_trs_stores_a_tune_byte_for_byte_and_it_loads_and_plays() {
    let Some(fw) = fixture() else { return };
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
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, None);
    let good = goat();
    let mut rsid = good.clone();
    rsid[..4].copy_from_slice(b"RSID");
    let mut mus = good.clone();
    mus[0x77] |= 1;
    // FM-1 SID's bounds bug's fixture (docs/upstream-reports.md, 1): a data offset past the
    // file's end.
    let mut crafted = good[..0x80].to_vec();
    crafted[6..8].copy_from_slice(&0x0200u16.to_be_bytes());
    let counts = fw.symbol("store_fm1_counts");
    for (name, file, says) in [
        ("RSID", &rsid, "RSID: NOT YET"),
        ("MUS", &mus, "MUS: NOT SUPPORTED"),
        ("CRAFTED", &crafted, "BROKEN .SID"),
    ] {
        let before = m.soc.flash.mem.clone();
        let progs = m.soc.read32(counts + PROGS);
        // The CLI refuses them, so the frames are made by hand.
        let syx = sidou_sysex::put_frames(name, sidou_sysex::FILE_SID, file)
            .unwrap()
            .concat();
        // The last file's toast out of the way (the test's own write to the toast's text).
        m.soc
            .write8(fw.symbol("ui") + ilp32::screen::toast as u32, 0);
        let mut shown = String::new();
        for f in frames(&syx) {
            m.midi(f);
            m.run_ms((f.len() as u64 * 320).div_ceil(1000) + 50);
            if shown.is_empty() {
                let t = toast(&mut m, &fw);
                if !t.is_empty() && t != "RECEIVING" {
                    shown = t;
                }
            }
            m.run_ms(TRS_GAP_MS - 50);
        }
        assert_eq!(shown, says, "{name}");
        assert_eq!(m.soc.read32(counts + PROGS), progs, "{name}: a program");
        assert!(m.soc.flash.mem == before, "{name}: the flash changed");
        assert_eq!(stored(&m, name), None);
    }
    check_passes(&m, "refusals over TRS");
}

#[test]
fn a_64_kib_tune_goes_in_and_loads_within_the_stores_pass_share() {
    let Some(fw) = fixture() else { return };
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
            "{function}: {} calls, at most {worst} instructions (STORE_POLL_WORST {STORE_POLL_WORST})",
            m.watched_calls.len()
        );
        assert_eq!(m.watched_calls.len(), calls, "{function}'s calls");
        if function == "sid_io_poll" {
            // The clear's four passes (16 KiB each), then the data's (2 KiB each).
            let (clear, data) = m.watched_calls.split_at(4);
            println!(
                "  the clear's passes at most {}, the data's at most {}",
                clear.iter().max().unwrap(),
                data.iter().max().unwrap()
            );
        }
        assert!(worst <= STORE_POLL_WORST, "{function}: {worst}");
        m.watched_calls.clear();
    }
    check_passes(&m, "64 KiB loaded and played");
}

/// The starter pack's tunes as `sidou-sysex sid --pack assets/starter` makes them: the store
/// name and the file, in file-name order.
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
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, None);
    let before = u32::from(mount_dump(&mut m.soc.flash.mem.clone()).nkeys);
    let tunes = starter();
    assert_eq!(tunes.len(), 11);
    // `pack.syx` as a SysEx tool sends it over TRS: every message, a second after the last.
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
        bytes as u32 * 4 <= STORE_CAPACITY,
        "the pack takes over a quarter of the store"
    );
    assert!(
        keys + 20 <= STORE_KEYS_MAX,
        "the pack leaves {} keys",
        STORE_KEYS_MAX - keys
    );
    // The browser lists them by name, eight a page: the first and the eighth play.
    for index in [0, 7] {
        let audio = m.soc.periph.audio.len();
        ui_play_tune(&mut m, &fw, index);
        until(&mut m, "the tune on", |m| tune_state(m, &fw) == TUNE_ON);
        m.run_ms(2_000);
        assert_eq!(tune_state(&mut m, &fw), TUNE_ON, "tune {index} stopped");
        let ctl = fw.symbol("player_isr");
        let late = m.soc.read32(ctl + ilp32::psid_ctl::late as u32);
        let peak = m.soc.periph.audio[audio..]
            .iter()
            .map(|&v| i32::from(v).abs())
            .max()
            .unwrap_or(0);
        println!("tune {index} of the browser: {late} late, peak {peak}");
        assert!(peak > 1000, "tune {index}: silent");
        assert_eq!(late, 0, "tune {index}: writes applied late");
        ui_end_tune(&mut m, &fw);
        until(&mut m, "the song back", |m| tune_state(m, &fw) == TUNE_OFF);
    }
    check_passes(&m, "the starter pack over TRS");
}

unsafe extern "C" fn dump_map(ctx: *mut std::ffi::c_void, off: u32) -> *const u8 {
    unsafe { ctx.cast::<u8>().add(off as usize) }
}

unsafe extern "C" fn dump_prog(
    ctx: *mut std::ffi::c_void,
    off: u32,
    data: *const std::ffi::c_void,
    n: u32,
) -> i32 {
    for i in 0..n as usize {
        unsafe { *ctx.cast::<u8>().add(off as usize + i) &= *data.cast::<u8>().add(i) };
    }
    STORE_OK
}

unsafe extern "C" fn dump_erase(ctx: *mut std::ffi::c_void, off: u32) -> i32 {
    for i in 0..STORE_SECTOR as usize {
        unsafe { *ctx.cast::<u8>().add(off as usize + i) = 0xff };
    }
    STORE_OK
}

/// `n` small values (keys 0x1000 on) written into a flash dump's store by the firmware's C
/// core, as settings and maps take keys.
fn put_values(flash: &mut [u8], n: u32) {
    let hooks = store_flash {
        map: Some(dump_map),
        prog: Some(dump_prog),
        erase: Some(dump_erase),
        ctx: flash.as_mut_ptr().cast(),
    };
    let mut st = Box::<store>::default();
    // SAFETY: the store and the value outlive every call; the flash outlives the store.
    unsafe {
        store_init(&mut *st, &hooks, STORE_WINDOW_MAX);
        store_mount(&mut *st);
        store_quiet(&mut *st, 1);
        while store_poll(&mut *st) != 0 {}
        for k in 0..n {
            let value = [k as u8; 8];
            assert_eq!(
                store_put(&mut *st, 0x1000 + k, 1, value.as_ptr().cast(), 8),
                STORE_OK
            );
            while store_poll(&mut *st) != 0 {}
        }
    }
}

#[test]
fn with_the_stores_keys_used_up_a_tunes_put_is_refused_store_full() {
    let Some(fw) = fixture() else { return };
    let mut flash = settled_flash(&fw.image);
    let taken = {
        let st = mount_dump(&mut flash);
        u32::from(st.nkeys)
    };
    put_values(&mut flash, STORE_KEYS_MAX - taken);
    assert_eq!(
        u32::from(mount_dump(&mut flash.clone()).nkeys),
        STORE_KEYS_MAX
    );
    let mut m = boot(&fw, Some(&flash));
    let counts = fw.symbol("store_fm1_counts");
    let progs = m.soc.read32(counts + PROGS);
    let syx = sidou_sysex::sid::syx("NO ROOM", &goat()).unwrap();
    let all = frames(&syx);
    for f in &all[..2] {
        m.midi(f);
        m.run_ms((f.len() as u64 * 320).div_ceil(1000) + 50);
    }
    assert_eq!(toast(&mut m, &fw), "STORE FULL");
    for f in &all[2..] {
        m.midi(f);
        m.run_ms((f.len() as u64 * 320).div_ceil(1000) + 50);
        assert_eq!(toast(&mut m, &fw), "STORE FULL", "the refusal stands");
    }
    assert_eq!(m.soc.read32(counts + PROGS), progs, "a program");
    assert_eq!(stored(&m, "NO ROOM"), None);
    check_passes(&m, "STORE FULL");
}
