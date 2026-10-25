// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! .sid tunes on the store (M3 Task 10, app/sid_io.h): the browser's list, the header's
//! refusals before anything loads, and the loading in parts (the 64 KiB cleared, the data read
//! straight into it) equal to psid_ctl_load of the whole file, for the largest file the store
//! holds too; a file replaced under a loading, a song change loaded again.
use std::ffi::CString;

use super::store::{fresh, Rig};
use sidou_sys::*;
use sidou_tests::psid::{full_size, packed, FILE_MAX};
use sidou_tests::psid_fixtures::vbi;

/// A file of `file_type` written whole into the store.
pub fn put(rig: &mut Rig, name: &str, file_type: u32, data: &[u8]) {
    let c = CString::new(name).unwrap();
    let begin = |rig: &mut Rig| {
        rig.api(|st| unsafe {
            store_file_begin(st, c.as_ptr(), file_type as u8, data.len() as u32)
        })
    };
    let mut rc = begin(rig);
    if rc == STORE_BUSY {
        rig.maintain(); // the pool refilled, as the device does when quiet
        rc = begin(rig);
    }
    assert_eq!(rc, STORE_OK, "{name}: store_file_begin");
    assert_eq!(rig.file_stream(data, 1000, None), STORE_OK, "{name}");
}

fn goat() -> Vec<u8> {
    let (_, sng) = gt2_ref::examples().into_iter().next().unwrap();
    packed(&sng, 0xD400)
}

fn listed(rig: &Rig) -> Vec<String> {
    let mut f = store_file::default();
    let mut out = Vec::new();
    while unsafe { sid_io_next(&*rig.st, &mut f) } == 1 {
        let name = unsafe { std::ffi::CStr::from_ptr(f.name.as_ptr()) };
        out.push(name.to_str().unwrap().to_owned());
        assert!(out.len() <= STORE_KEYS_MAX as usize, "the list loops");
    }
    out
}

fn message(r: u32) -> String {
    let p = unsafe { sid_io_message(r as i32) };
    unsafe { std::ffi::CStr::from_ptr(p) }
        .to_str()
        .unwrap()
        .to_owned()
}

/// psid_ctl over its own 64 KiB, filled with garbage first (the song's bytes on the device),
/// with sid_io loading into it.
struct Loader {
    ctl: Box<psid_ctl>,
    _work: Box<psid_work>,
    mem: Box<[u8; 0x10000]>,
    h: Box<psid_header>,
    io: Box<sid_io>,
}

impl Loader {
    fn new() -> Loader {
        let mut l = Loader {
            ctl: Box::default(),
            _work: Box::default(),
            mem: Box::new([0x5a; 0x10000]),
            h: Box::default(),
            io: Box::default(),
        };
        unsafe {
            psid_ctl_init(
                &mut *l.ctl,
                &mut *l._work,
                l.mem.as_mut_ptr(),
                std::ptr::null(),
            )
        };
        l
    }

    fn open(&mut self, rig: &Rig, name: &str) -> u32 {
        let c = CString::new(name).unwrap();
        unsafe { sid_io_open(&mut *self.io, &*rig.st, c.as_ptr(), &mut *self.h) as u32 }
    }

    /// Opens and loads `name`: the polls it took.
    fn load(&mut self, rig: &Rig, name: &str) -> u32 {
        assert_eq!(self.open(rig, name), SID_IO_OK, "{name}");
        assert_eq!(
            unsafe { sid_io_load(&mut *self.io, &mut *self.ctl, &*self.h) },
            1
        );
        let mut polls = 0;
        loop {
            let r = unsafe { sid_io_poll(&mut *self.io) };
            polls += 1;
            if r == 0 {
                break;
            }
            assert_eq!(r, 1, "{name}: poll {polls}");
            assert!(polls < 100, "{name}: the loading never ends");
        }
        assert_eq!(unsafe { sid_io_busy(&*self.io) }, 0);
        assert_eq!(unsafe { psid_ctl_state(&*self.ctl) }, PSID_LOADED);
        polls
    }
}

/// The tune `file` loaded whole by psid_ctl_load, as a reference.
struct Whole {
    ctl: Box<psid_ctl>,
    _work: Box<psid_work>,
    mem: Box<[u8; 0x10000]>,
    _h: Box<psid_header>,
    _file: Vec<u8>,
}

impl Whole {
    fn new(file: &[u8]) -> Whole {
        let mut w = Whole {
            ctl: Box::default(),
            _work: Box::default(),
            mem: Box::new([0; 0x10000]),
            _h: Box::new(sidou_tests::psid::header(file)),
            _file: file.to_vec(),
        };
        unsafe {
            psid_ctl_init(
                &mut *w.ctl,
                &mut *w._work,
                w.mem.as_mut_ptr(),
                std::ptr::null(),
            );
            assert_eq!(psid_ctl_load(&mut *w.ctl, &*w._h, w._file.as_ptr()), 1);
        }
        w
    }
}

/// Plays song 1 of both for `polls` passes: the same writes at the same cycles.
fn plays_alike(a: &mut psid_ctl, b: &mut psid_ctl, polls: u32) {
    unsafe {
        assert_eq!(psid_ctl_play(a, 1), 1);
        assert_eq!(psid_ctl_play(b, 1), 1);
        for _ in 0..polls {
            psid_ctl_poll(a);
            psid_ctl_poll(b);
        }
    }
    assert!(a.head > 0, "the tune wrote nothing");
    assert_eq!(a.head, b.head);
    assert_eq!(a.ring_entry[..], b.ring_entry[..]);
    assert_eq!(a.ring_cycle[..], b.ring_cycle[..]);
}

#[test]
fn the_list_has_only_sid_files_in_name_order_and_never_sysex_staging() {
    let mut rig = fresh(0x5349_4401, 256);
    let tune = vbi().bytes();
    put(&mut rig, "ZED", STORE_FILE_SID, &tune);
    put(&mut rig, "A SONG", STORE_FILE_SONG, &[1; 40]);
    put(&mut rig, "ALPHA", STORE_FILE_SID, &tune);
    // MIDI INSTRUMENT's staging was type 2 before .sid files were: still never a tune.
    put(&mut rig, "MIDI INSTRUMENT", STORE_FILE_SID, &[2; 40]);
    put(&mut rig, "MIDI SONG", STORE_FILE_SONG, &[3; 40]);
    put(&mut rig, "M", STORE_FILE_SID, &tune);
    assert_eq!(listed(&rig), ["ALPHA", "M", "ZED"]);
    let mut l = Loader::new();
    assert_eq!(l.open(&rig, "MIDI INSTRUMENT"), SID_IO_NOT_FOUND);
    assert_eq!(l.open(&rig, "A SONG"), SID_IO_NOT_FOUND);
    assert_eq!(l.open(&rig, "NONE"), SID_IO_NOT_FOUND);
    assert_eq!(message(SID_IO_NOT_FOUND), "SID NOT FOUND");
    assert_eq!(l.open(&rig, "M"), SID_IO_OK);
    assert_eq!(l.h.load, vbi().load);
    assert_eq!(listed(&fresh(0x5349_4402, 256)), Vec::<String>::new());
}

#[test]
fn rsid_mus_and_broken_headers_are_refused_with_their_toasts_and_nothing_loads() {
    let mut rig = fresh(0x5349_4403, 256);
    let good = vbi().bytes();
    let mut rsid = good.clone();
    rsid[..4].copy_from_slice(b"RSID");
    let mut mus = good.clone();
    mus[0x77] |= 1;
    // FM-1 SID's bounds bug's fixture (docs/upstream-reports.md, 1): a data offset past the
    // file's end.
    let mut past = good[..0x7C].to_vec();
    past[6..8].copy_from_slice(&0x0200u16.to_be_bytes());
    let mut long = good.clone();
    long[8..10].copy_from_slice(&0xFFF0u16.to_be_bytes());
    let cases: [(&str, &[u8], u32, &str); 6] = [
        ("RSID", &rsid, SID_IO_RSID, "RSID: NOT YET"),
        ("MUS", &mus, SID_IO_MUS, "MUS: NOT SUPPORTED"),
        ("PAST", &past, SID_IO_BROKEN, "BROKEN .SID"),
        ("SHORT", &good[..0x40], SID_IO_BROKEN, "BROKEN .SID"),
        ("WAV", b"RIFF....WAVEfmt ", SID_IO_BROKEN, "BROKEN .SID"),
        ("LONG", &long, SID_IO_TOO_LONG, "TOO LONG"),
    ];
    for (name, file, _, _) in &cases {
        put(&mut rig, name, STORE_FILE_SID, file);
    }
    let mut l = Loader::new();
    for (name, _, why, toast) in cases {
        assert_eq!(l.open(&rig, name), why, "{name}");
        assert_eq!(message(why), toast);
        assert_eq!(l.h.songs, 0, "{name}: the header left empty");
    }
    assert_eq!(message(SID_IO_READ), "SID: READ ERROR");
    assert!(
        l.mem.iter().all(|&b| b == 0x5a),
        "a refusal touched the memory"
    );
    assert_eq!(unsafe { psid_ctl_state(&*l.ctl) }, PSID_EMPTY);
}

#[test]
fn a_tune_loads_in_parts_as_the_whole_file_loads_the_largest_file_too() {
    let mut rig = fresh(0x5349_4404, 256);
    // The header's load address 0: the data's first two bytes are the load address.
    let mut prefixed = vbi().bytes();
    let load = vbi().load;
    prefixed[8..10].copy_from_slice(&[0, 0]);
    prefixed.splice(0x7C..0x7C, load.to_le_bytes());
    let big = full_size(&goat());
    let files = [
        ("VBI", vbi().bytes()),
        ("PREFIXED", prefixed),
        ("GOAT", goat()),
        ("BIG", big),
    ];
    assert_eq!(files[3].1.len(), FILE_MAX);
    for (name, file) in &files {
        put(&mut rig, name, STORE_FILE_SID, file);
    }
    for (name, file) in &files {
        let mut l = Loader::new();
        let polls = l.load(&rig, name);
        let mut w = Whole::new(file);
        assert!(l.mem[..] == w.mem[..], "{name}: the memory differs");
        // The clear's passes, then the data's.
        let clear = (0x10000 - 2u32).div_ceil(SID_IO_CLEAR_BYTES);
        let data = w._h.data_len.div_ceil(SID_IO_PASS_BYTES);
        assert_eq!(polls, clear + data, "{name}");
        println!("{name}: {} data bytes in {polls} polls", w._h.data_len);
        plays_alike(&mut l.ctl, &mut w.ctl, 40);
    }
}

#[test]
fn a_song_change_loads_the_tune_again_from_the_store() {
    let mut rig = fresh(0x5349_4405, 256);
    let file = goat();
    put(&mut rig, "GOAT", STORE_FILE_SID, &file);
    let mut l = Loader::new();
    l.load(&rig, "GOAT");
    unsafe {
        assert_eq!(psid_ctl_play(&mut *l.ctl, 1), 1);
        for _ in 0..40 {
            psid_ctl_poll(&mut *l.ctl);
        }
        // Loaded in parts, the tune has no file to load a song change from: the caller's.
        assert_eq!(psid_ctl_subtune(&mut *l.ctl, 1), 0);
        // Playing: not quiet, so no load.
        assert_eq!(sid_io_load(&mut *l.io, &mut *l.ctl, &*l.h), 0);
        assert_eq!(psid_ctl_stop(&mut *l.ctl), 1);
        // The interrupt's half plays the stop out (its acknowledgement), then a poll sees it.
        l.ctl.done_seq = l.ctl.want_seq;
        psid_ctl_poll(&mut *l.ctl);
    }
    let w = Whole::new(&file);
    assert!(
        l.mem[..] != w.mem[..],
        "the tune changed nothing in its memory"
    );
    l.load(&rig, "GOAT");
    assert!(
        l.mem[..] == w.mem[..],
        "loaded again: the memory as at the first load"
    );
}

#[test]
fn a_file_replaced_or_deleted_under_a_loading_stops_it() {
    let mut rig = fresh(0x5349_4406, 256);
    let file = full_size(&goat());
    for replace in [true, false] {
        put(&mut rig, "GOAT", STORE_FILE_SID, &file);
        let mut l = Loader::new();
        assert_eq!(l.open(&rig, "GOAT"), SID_IO_OK);
        assert_eq!(unsafe { sid_io_load(&mut *l.io, &mut *l.ctl, &*l.h) }, 1);
        assert!(
            l.h.data_len > 2 * SID_IO_PASS_BYTES,
            "a file of three parts at least"
        );
        let mut polls = 0;
        // Through the clear and into the data.
        while polls < 5 {
            assert_eq!(unsafe { sid_io_poll(&mut *l.io) }, 1);
            polls += 1;
        }
        if replace {
            put(&mut rig, "GOAT", STORE_FILE_SID, &file);
        } else {
            assert_eq!(rig.file_delete("GOAT"), STORE_OK);
            rig.settle();
        }
        assert_eq!(
            unsafe { sid_io_poll(&mut *l.io) },
            -(SID_IO_READ as i32),
            "replaced {replace}"
        );
        assert_eq!(unsafe { sid_io_busy(&*l.io) }, 0);
    }
}
