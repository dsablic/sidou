// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The song model, the `.sng` reader and writer and the trailer (M2c Task 2; decision record
//! 0007, docs/reference/song-format.md).
use sidou_sys::*;
use sidou_tests::song::*;

fn trailer_start(file: &[u8], info: &ReadInfo) -> usize {
    let e = info.gt2_bytes as usize;
    assert_eq!(&file[e..e + 8], b"SIDOU-X\0");
    e
}

/// A song with every M2 chunk set.
fn with_chunks(s: &mut Song) {
    let mut o = gt2_options::default();
    unsafe { gt2_options_default(&mut o) };
    o.multispeed = 2;
    o.hr_adsr = 0x0a00;
    assert_eq!(unsafe { song_ext_set_options(s.ptr_mut(), &o) }, 0);
    let mut c = song_chip_setup {
        model: 0,
        profile: 3,
        age: 10,
        dac: 20,
        thump: 30,
        bleed: 40,
        envelope: 1,
        expert_count: 2,
        ..Default::default()
    };
    c.expert[0] = 7;
    c.expert[1] = 9;
    assert_eq!(unsafe { song_ext_set_chip(s.ptr_mut(), TAG_CHPA, &c) }, 0);
    c.model = 1;
    assert_eq!(unsafe { song_ext_set_chip(s.ptr_mut(), TAG_CHPB, &c) }, 0);
    let mut sl = song_slots {
        count: 3,
        ..Default::default()
    };
    sl.slot[1] = [2, 64, 32, 16, 100];
    assert_eq!(unsafe { song_ext_set_slots(s.ptr_mut(), &sl) }, 0);
    let locks = [
        song_lock {
            pattern: 0,
            row: 1,
            param: 2,
            value: 1000,
        },
        song_lock {
            pattern: 1,
            row: 0,
            param: 0,
            value: 7,
        },
    ];
    assert_eq!(
        unsafe { song_ext_set_locks(s.ptr_mut(), locks.as_ptr(), 2) },
        0
    );
    let m = song_midi {
        mode: 1,
        rows_per_beat: 4,
        count: 3,
        channel: [0, 1, 2, 0, 0, 0],
    };
    assert_eq!(unsafe { song_ext_set_midi(s.ptr_mut(), &m) }, 0);
    let sc = song_scene {
        piece: 1,
        count: 3,
        role: [1, 2, 3, 0, 0, 0],
        ..Default::default()
    };
    assert_eq!(unsafe { song_ext_set_scene(s.ptr_mut(), &sc) }, 0);
    let mut k = song_keys {
        count: 42,
        ..Default::default()
    };
    for (i, p) in k.param.iter_mut().take(42).enumerate() {
        *p = i as u16 * 100;
    }
    assert_eq!(unsafe { song_ext_set_keys(s.ptr_mut(), &k) }, 0);
}

#[test]
fn crc32_is_zlibs() {
    assert_eq!(
        unsafe { song_crc32(0, b"123456789".as_ptr(), 9) },
        0xcbf4_3926
    );
    let a = unsafe { song_crc32(0, b"1234".as_ptr(), 4) };
    assert_eq!(unsafe { song_crc32(a, b"56789".as_ptr(), 5) }, 0xcbf4_3926);
}

#[test]
fn empty_song_is_goattrackers_cleared_song() {
    let s = Song::device();
    let f = s.write(false);
    // clearsong, then savesong: one subtune, channel c plays pattern c; patterns 0-2 empty
    // with 64 rows, and the three named so all are written.
    let mut want = SngSpec::new();
    want.instr.clear();
    assert_eq!(f, want.encode());
}

#[test]
fn canonical_gts5_round_trips_byte_for_byte() {
    let mut rng = Rng(0x5eed_0001);
    for seed in 0..400 {
        let spec = random_canonical(&mut rng, seed % 10 == 9);
        let file = spec.encode();
        let mut s = Song::device();
        let info = s
            .read(&file, 4)
            .unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));
        assert_eq!(
            info.channels,
            if seed % 10 == 9 { 6 } else { 3 },
            "seed {seed}"
        );
        assert_eq!(s.write(false), file, "seed {seed}");
    }
}

#[test]
fn sidou_output_round_trips_with_and_without_trailer() {
    let mut rng = Rng(0x5eed_0002);
    for seed in 0..100 {
        let file = random_canonical(&mut rng, seed % 7 == 0).encode();
        let mut s = Song::device();
        s.read(&file, 4).unwrap();
        with_chunks(&mut s);
        for trailer in [false, true] {
            let out = s.write(trailer);
            let mut t = Song::device();
            let info = t.read(&out, 4).unwrap();
            assert_eq!(
                info.trailer,
                if trailer {
                    GT2_TRAILER_READ
                } else {
                    GT2_TRAILER_NONE
                }
            );
            assert_eq!(info.file_bytes as usize, out.len());
            assert_eq!(t.write(trailer), out, "seed {seed} trailer {trailer}");
            if trailer {
                assert_eq!(t.ext_chunks(), s.ext_chunks());
            }
        }
    }
}

#[test]
fn name_only_instruments_are_dropped_on_save() {
    let mut spec = SngSpec::new();
    let mut used = [0u8; 25];
    used[0] = 0x09;
    let mut name_only = [0u8; 25];
    name_only[9..14].copy_from_slice(b"EMPTY");
    let mut timer_only = [0u8; 25];
    timer_only[7] = 2;
    timer_only[8] = 0x09;
    // Unused in the middle: kept. Unused after the last used: dropped.
    spec.instr = vec![used, name_only, used, name_only, timer_only];
    let mut s = Song::device();
    s.read(&spec.encode(), 4).unwrap();
    let mut want = spec.clone();
    want.instr.truncate(3);
    assert_eq!(s.write(false), want.encode());
    // An instrument a pattern names counts as used, empty or not.
    spec.patterns[1].0[5][1] = 5;
    s.read(&spec.encode(), 4).unwrap();
    assert_eq!(s.write(false), spec.encode());
}

#[test]
fn counts_are_trimmed_as_savesong_trims_them() {
    let mut spec = SngSpec::new();
    // A trailing table row of zeros, a trailing empty unnamed pattern, a trailing subtune
    // with an empty channel, an orderlist's length byte past its $FF.
    spec.tables[1] = vec![(0x88, 0), (0xff, 0), (0, 0)];
    spec.patterns
        .push((vec![[0xbd, 0, 0, 0]; 8], [0xff, 0, 0, 0]));
    spec.orders
        .push(vec![(vec![0], 0), (vec![], 0), (vec![1], 0)]);
    let mut file = spec.encode();
    let mut s = Song::device();
    s.read(&file, 4).unwrap();
    let mut want = spec.clone();
    want.tables[1].truncate(2);
    want.patterns.pop();
    want.orders.pop();
    assert_eq!(s.write(false), want.encode());
    // Rows after a pattern's end row are dropped.
    let mut spec2 = SngSpec::new();
    spec2.patterns[2].0[10] = [0xff, 1, 2, 3];
    s.read(&spec2.encode(), 4).unwrap();
    let mut want2 = SngSpec::new();
    want2.patterns[2] = (vec![[0xbd, 0, 0, 0]; 10], [0xff, 1, 2, 3]);
    assert_eq!(s.write(false), want2.encode());
    // A pattern named but not in the file is an empty 64-row one.
    let mut spec3 = SngSpec::new();
    spec3.orders[0][2].0 = vec![5];
    s.read(&spec3.encode(), 4).unwrap();
    let mut want3 = spec3.clone();
    while want3.patterns.len() < 6 {
        want3
            .patterns
            .push((vec![[0xbd, 0, 0, 0]; 64], [0xff, 0, 0, 0]));
    }
    assert_eq!(s.write(false), want3.encode());
    // A length byte past the orderlist's $FF: the rest of it is ignored.
    file.clear();
    let mut spec4 = SngSpec::new();
    spec4.orders[0][0] = (vec![0, 0xff, 0x00, 0x01], 2);
    file.extend(spec4.encode());
    s.read(&file, 4).unwrap();
    let mut want4 = SngSpec::new();
    want4.orders[0][0] = (vec![0], 0);
    assert_eq!(s.write(false), want4.encode());
}

#[test]
fn older_idents_are_converted_and_saved_as_gts5() {
    // GTS4: the old legato and no-hard-restart parameters.
    let mut spec = SngSpec::new();
    spec.ident = *b"GTS4";
    let mut a = [0u8; 25];
    a[0] = 0x09;
    a[8] = 0x89; // first-frame wave $89: no hard restart, wave $09
    let mut b = [0u8; 25];
    b[0] = 0x09;
    b[8] = 0x00; // legato
    spec.instr = vec![a, b];
    let mut s = Song::device();
    let info = s.read(&spec.encode(), 4).unwrap();
    assert_eq!(info.version, 4);
    let out = s.write(false);
    assert_eq!(&out[..4], b"GTS5");
    let mut want = spec.clone();
    want.ident = *b"GTS5";
    want.instr[0][7] |= 0x80;
    want.instr[0][8] = 0x09;
    want.instr[1][7] |= 0x40;
    assert_eq!(out, want.encode());
    // GTS3: the pulse modulation speeds doubled as well.
    let mut spec3 = SngSpec::new();
    spec3.ident = *b"GTS3";
    spec3.tables[1] = vec![(0x88, 0x00), (0x20, 0x40), (0x10, 0xc0), (0x10, 0x50)];
    s.read(&spec3.encode(), 4).unwrap();
    let mut want3 = spec3.clone();
    want3.ident = *b"GTS5";
    want3.tables[1] = vec![(0x88, 0x00), (0x20, 0x7f), (0x10, 0x80), (0x10, 0x7f)];
    assert_eq!(s.write(false), want3.encode());
}

#[test]
fn gts2_speed_parameters_become_speedtable_entries() {
    let mut spec = SngSpec::new();
    spec.ident = *b"GTS2";
    spec.tables.truncate(3);
    let mut i = [0u8; 25];
    i[0] = 0x09;
    i[5] = 3; // GTS2: the vibrato delay ...
    i[6] = 0x34; // ... then the old vibrato parameter
    i[8] = 0x09;
    spec.instr = vec![i];
    spec.patterns[0].0[0] = [0x60, 1, 1, 0x08]; // portamento up 8
    spec.patterns[0].0[1] = [0x60, 0, 4, 0x34]; // vibrato 34: the instrument's entry again
    spec.patterns[0].0[2] = [0x60, 0, 14, 0x96]; // funktempo 9/6
    let mut s = Song::device();
    s.read(&spec.encode(), 4).unwrap();
    let raw = s.raw();
    // Fine vibrato $34: left 3, right $40; portamento 8: $0020; funktempo $96: 9, 6.
    assert_eq!(&raw.ltable[3][..3], &[0x03, 0x00, 0x09]);
    assert_eq!(&raw.rtable[3][..3], &[0x40, 0x20, 0x06]);
    assert_eq!(raw.instr[0][5], 1);
    assert_eq!(raw.instr[0][6], 3);
    let p = s.pattern(0).unwrap();
    assert_eq!(p[0], [0x60, 1, 1, 2]);
    assert_eq!(p[1], [0x60, 0, 4, 1]);
    assert_eq!(p[2], [0x60, 0, 14, 3]);
    assert_eq!(&s.write(false)[..4], b"GTS5");
}

#[test]
fn refusals_have_messages_and_leave_the_song() {
    let mut s = Song::device();
    let good = random_canonical(&mut Rng(77), false).encode();
    s.read(&good, 4).unwrap();
    let before = s.bytes().to_vec();
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (b"MOD!xxxx".to_vec(), "SNG: NOT A SONG"),
        (b"GTS!".to_vec(), "SNG: GT1 FORMAT"),
        (good[..50].to_vec(), "SNG: SHORT FILE"),
        (
            {
                let mut f = good.clone();
                f[100] = 0;
                f
            },
            "SNG: BAD SUBTUNES",
        ),
        (
            {
                let mut f = good.clone();
                f[100] = 33;
                f
            },
            "SNG: BAD SUBTUNES",
        ),
        (
            {
                let mut sp = SngSpec::new();
                sp.orders[0][1] = (vec![], 0);
                let mut f = sp.encode();
                // Channel 2's orderlist: length 1, data [$00, $00]: no $FF.
                let at = 101 + 4;
                f[at] = 1;
                f[at + 1] = 0;
                f
            },
            "SNG: BAD ORDERLIST V2",
        ),
        (
            {
                let mut sp = SngSpec::new();
                sp.instr = vec![[0; 25]; 64];
                sp.encode()
            },
            "SNG: BAD INSTRUMENTS",
        ),
        (
            {
                let mut sp = SngSpec::new();
                sp.patterns[1].0[3] = [0x40, 0, 0, 0];
                sp.encode()
            },
            "SNG: BAD PATTERN 1",
        ),
        (
            {
                let mut sp = SngSpec::new();
                sp.patterns[2].0 = vec![[0xbd, 0, 0, 0]; 129];
                sp.encode()
            },
            "SNG: BAD PATTERN 2",
        ),
        (
            {
                let mut sp = SngSpec::new();
                sp.patterns[0].0[0] = [0x60, 0x40, 0, 0];
                sp.encode()
            },
            "SNG: BAD PATTERN 0",
        ),
        (
            {
                let mut sp = SngSpec::new();
                sp.patterns[0].0[0] = [0x60, 0, 0x10, 0];
                sp.encode()
            },
            "SNG: BAD PATTERN 0",
        ),
        (
            {
                let mut sp = SngSpec::new();
                sp.patterns = vec![(vec![], [0xff, 0, 0, 0]); 209];
                sp.encode()
            },
            "SNG: BAD PATTERNS",
        ),
    ];
    for (file, msg) in cases {
        let e = s.read(&file, 4).expect_err(msg);
        assert_eq!(e.msg, msg);
        assert!(s.bytes() == &before[..], "{msg}: the song changed");
    }
}

#[test]
fn caps_size_and_multispeed() {
    // Over the device's arena: refused with the sizes; the host's region loads it.
    let mut spec = SngSpec::new();
    spec.orders[0][0].0 = (0..120).map(|p| p as u8).collect();
    spec.patterns = (0..120)
        .map(|_| (vec![[0x60, 0, 0, 0]; 127], [0xff, 0, 0, 0]))
        .collect();
    let file = spec.encode();
    let mut d = Song::device();
    let e = d.read(&file, 4).unwrap_err();
    let need = e.need_bytes.div_ceil(1024);
    let cap = unsafe { gt2_arena_capacity(d.ptr()) } / 1024;
    assert_eq!(e.msg, format!("SNG: TOO BIG ({need}K OF {cap}K)"));
    assert!(need > cap);
    let mut h = Song::host();
    h.read(&file, 0).unwrap();
    assert_eq!(h.write(false), file);
    // Multispeed 8: refused on the device's caps, read on the host's.
    let mut s = Song::device();
    let mut o = gt2_options::default();
    unsafe { gt2_options_default(&mut o) };
    o.multispeed = 8;
    assert_eq!(unsafe { song_ext_set_options(s.ptr_mut(), &o) }, 0);
    let f = s.write(true);
    assert_eq!(s.read(&f, 4).unwrap_err().msg, "SNG: MULTISPEED 8 (MAX 4)");
    s.read(&f, 0).unwrap();
    o.multispeed = 0;
    assert_eq!(unsafe { song_ext_set_options(s.ptr_mut(), &o) }, 0);
    let f = s.write(true);
    assert_eq!(s.read(&f, 4).unwrap_err().msg, "SNG: MULTISPEED 0 (MAX 4)");
    o.multispeed = 4;
    assert_eq!(unsafe { song_ext_set_options(s.ptr_mut(), &o) }, 0);
    let f = s.write(true);
    s.read(&f, 4).unwrap();
}

#[test]
fn trailer_layout_crc_rules_and_unknown_chunks() {
    let file = random_canonical(&mut Rng(5), false).encode();
    let mut s = Song::device();
    s.read(&file, 4).unwrap();
    with_chunks(&mut s);
    // An unknown chunk, between known ones, is kept in its place.
    assert_eq!(s.ext_set(tag(b"ZZZ9"), &[3, 1, 2, 3]), 0);
    assert_eq!(s.ext_set(TAG_MIDI, &[1, 9, 9, 0]), 0); // replaced in place
    let chunks = s.ext_chunks();
    let out = s.write(true);
    let mut t = Song::device();
    let info = t.read(&out, 4).unwrap();
    let e = trailer_start(&out, &info);
    assert_eq!(&out[..e], &file[..], "the GT2 part is the plain file");
    assert_eq!(t.ext_chunks(), chunks);
    // The fixed header and lead-in: bytes 0-254 hold no $FF.
    assert!(out[e..e + 255].iter().all(|&b| b != 0xff));
    assert_eq!(&out[e + 8..e + 10], &[1, 0]);
    let len = u32::from_le_bytes(out[e + 10..e + 14].try_into().unwrap()) as usize;
    assert_eq!(e + len, out.len());
    assert_eq!(out[e + 14], 3, "the GT2 part's channel count");
    assert!(out[e + 15..e + 255].iter().all(|&b| b == 0));
    let gt2_crc = unsafe { song_crc32(0, out.as_ptr(), e as u32) };
    assert_eq!(
        u32::from_le_bytes(out[e + 255..e + 259].try_into().unwrap()),
        gt2_crc
    );
    let crc = unsafe { song_crc32(0, out[e..].as_ptr(), len as u32 - 4) };
    assert_eq!(
        u32::from_le_bytes(out[out.len() - 4..].try_into().unwrap()),
        crc
    );
    // Each chunk model reads back.
    let mut o = gt2_options::default();
    assert_eq!(unsafe { song_ext_get_options(t.ptr(), &mut o) }, 1);
    assert_eq!(
        (o.multispeed, o.hr_adsr, o.sid_address),
        (2, 0x0a00, 0xd400)
    );
    let mut c = song_chip_setup::default();
    assert_eq!(unsafe { song_ext_get_chip(t.ptr(), TAG_CHPB, &mut c) }, 1);
    assert_eq!(
        (c.model, c.profile, c.bleed, c.expert_count, c.expert[1]),
        (1, 3, 40, 2, 9)
    );
    let mut locks = [song_lock::default(); 4];
    assert_eq!(
        unsafe { song_ext_get_locks(t.ptr(), locks.as_mut_ptr(), 4) },
        2
    );
    assert_eq!(locks[0].value, 1000);
    let mut k = song_keys::default();
    assert_eq!(unsafe { song_ext_get_keys(t.ptr(), &mut k) }, 1);
    assert_eq!((k.count, k.param[41]), (42, 4100));

    // A flipped byte in the GT2 part (the song's name): LOCK dropped, the rest kept.
    let mut f = out.clone();
    f[10] ^= 0x20;
    let info = t.read(&f, 4).unwrap();
    assert_eq!(info.trailer, GT2_TRAILER_LOCKS_DROPPED);
    assert_eq!(info.msg, "SNG: LOCKS DROPPED");
    assert!(t.ext_find(TAG_LOCK).is_none());
    let mut kept = chunks.clone();
    kept.retain(|c| c.0 != TAG_LOCK);
    assert_eq!(t.ext_chunks(), kept);
    // A flipped byte in the trailer: the whole trailer dropped, the song still loads.
    for at in [e + 3, e + 12, e + 100, e + 300, out.len() - 1] {
        let mut f = out.clone();
        f[at] ^= 0x01;
        let info = t.read(&f, 4).unwrap();
        if at == e + 3 {
            // Not the magic: no trailer at all, the bytes ignored.
            assert_eq!(info.trailer, GT2_TRAILER_NONE);
        } else {
            assert_eq!(info.trailer, GT2_TRAILER_DROPPED, "byte {at}");
            assert_eq!(info.msg, "SNG: EXTRAS DROPPED");
        }
        assert_eq!(t.raw().ext_used, 0);
        assert_eq!(t.write(false), file);
    }
    // Truncated: dropped. Bytes after it: ignored.
    let info = t.read(&out[..out.len() - 1], 4).unwrap();
    assert_eq!(info.trailer, GT2_TRAILER_DROPPED);
    let mut longer = out.clone();
    longer.extend_from_slice(b"junk");
    let info = t.read(&longer, 4).unwrap();
    assert_eq!(info.trailer, GT2_TRAILER_READ);
    assert_eq!(t.write(true), out);
    // Bytes after a plain GT2 part that are not a trailer: ignored, as GoatTracker does.
    let mut junk = file.clone();
    junk.extend_from_slice(&[0xff; 40]);
    let info = t.read(&junk, 4).unwrap();
    assert_eq!(info.trailer, GT2_TRAILER_NONE);
    assert_eq!(t.write(false), file);
}

#[test]
fn trailer_length_never_holds_ff() {
    // Lengths 263 + n for every extension size up to the area's: the writer pads with PAD
    // where the length would hold $FF, and the reader drops the PAD again.
    let file = SngSpec::new().encode();
    let mut padded = 0;
    for n in 9..=4096u32 {
        let mut s = Song::device();
        s.read(&file, 4).unwrap();
        assert_eq!(s.ext_set(tag(b"ZZZZ"), &vec![1u8; n as usize - 8]), 0);
        let out = s.write(true);
        let e = file.len();
        let len = &out[e + 10..e + 14];
        assert!(len.iter().all(|&b| b != 0xff), "n {n}: {len:?}");
        if out.len() - e != 263 + n as usize {
            padded += 1;
        }
        let mut t = Song::device();
        t.read(&out, 4).unwrap();
        assert_eq!(t.ext_chunks(), s.ext_chunks(), "n {n}");
        assert_eq!(t.write(true), out, "n {n}");
    }
    assert!(padded > 0);
}

/// The smallest valid song grown a byte at a time: an orderlist entry adds one, a table
/// row two, a pattern row four.
fn grown(size: u32, end_row: [u8; 4], stereo: bool) -> Vec<u8> {
    let mut sp = SngSpec::new();
    let channels = if stereo { 6 } else { 3 };
    sp.orders = vec![(0..channels).map(|c| (vec![c as u8 % 3], 0)).collect()];
    sp.patterns = (0..3).map(|_| (vec![], end_row)).collect();
    let mut left = size;
    let extra_entries = (left % 2).min(1) + (left / 2 % 3);
    left -= extra_entries;
    for _ in 0..extra_entries {
        sp.orders[0][0].0.push(0);
    }
    let table_rows = (left / 2).min(255);
    left -= table_rows * 2;
    sp.tables[0] = (0..table_rows)
        .map(|i| (1 + (i % 200) as u8, i as u8))
        .collect();
    sp.patterns[0].0 = vec![[0xbd, 0, 0, 1]; (left / 4).min(128) as usize];
    sp.encode()
}

#[test]
fn stereo_detection_with_a_trailer() {
    for end_row in [
        [0xff, 0, 0, 0],
        [0xff, 0xff, 0xff, 0xff],
        [0xff, 0x3f, 0x0f, 0xff],
    ] {
        for size in 0..700 {
            let file = grown(size, end_row, false);
            let plain = determinechannels(&file);
            let mut s = Song::device();
            let info = s.read(&file, 4).unwrap();
            assert_eq!(info.channels as u32, plain, "size {size}");
            with_chunks(&mut s);
            for t in [s.write(true), {
                let mut big = Song::device();
                big.read(&file, 4).unwrap();
                big.ext_set(tag(b"ZZZZ"), &vec![0xff; 3000]);
                big.write(true)
            }] {
                assert_eq!(determinechannels(&t), 3, "size {size} end {end_row:?}");
                let mut u = Song::device();
                assert_eq!(u.read(&t, 4).unwrap().channels, 3, "size {size}");
            }
            assert_eq!(
                plain, 3,
                "size {size}: a mono GT2 part that reads as stereo"
            );
        }
    }
    let mut rng = Rng(0x5eed_0003);
    for seed in 0..200 {
        let stereo = seed % 2 == 0;
        let file = random_canonical(&mut rng, stereo).encode();
        let mut s = Song::device();
        s.read(&file, 4).unwrap();
        with_chunks(&mut s);
        let t = s.write(true);
        let want = if stereo { 6 } else { 3 };
        assert_eq!(determinechannels(&file), want, "seed {seed}");
        assert_eq!(determinechannels(&t), want, "seed {seed}");
        let mut u = Song::device();
        assert_eq!(u.read(&t, 4).unwrap().channels as u32, want, "seed {seed}");
        assert_eq!(u.write(true), t);
    }
}

// Edits.

const EMPTY: [u8; 4] = [0xbd, 0, 0, 0];

/// An independent model of the song's orderlists and patterns.
#[derive(Clone, Debug, PartialEq)]
struct Model {
    orders: Vec<Vec<(Vec<u8>, u8)>>,
    patterns: Vec<Option<Vec<[u8; 4]>>>,
}

impl Model {
    fn of(s: &Song) -> Model {
        let raw = s.raw();
        Model {
            orders: (0..raw.subtunes as u32)
                .map(|d| {
                    (0..raw.channels as u32)
                        .map(|c| s.order(d, c).unwrap())
                        .collect()
                })
                .collect(),
            patterns: (0..208).map(|p| s.pattern(p)).collect(),
        }
    }

    fn named(&self, p: u8) -> bool {
        self.orders.iter().flatten().any(|o| o.0.contains(&p))
    }
}

fn order_valid(e: &[u8]) -> bool {
    let mut i = 0;
    while i < e.len() {
        if (0xe0..0xff).contains(&e[i]) {
            i += 1;
        }
        if i < e.len() && (0xd0..0xe0).contains(&e[i]) {
            i += 1;
        }
        if i >= e.len() || e[i] >= 0xd0 {
            return false;
        }
        i += 1;
    }
    true
}

/// Every index entry's bytes in the arena, for the "rebuilds touch only free space" check.
fn in_use(s: &Song) -> Vec<(u32, Vec<u8>)> {
    let raw = s.raw();
    let arena = &s.bytes()[raw.arena_offset as usize..];
    let mut out = Vec::new();
    for sp in raw.order.iter().flatten().filter(|&&x| x != 0) {
        let at = span_off4(*sp) as usize * 4;
        out.push((*sp, arena[at..at + span_len(*sp) as usize].to_vec()));
    }
    for sp in raw.pattern.iter().filter(|&&x| x != 0) {
        let at = span_off4(*sp) as usize * 4;
        out.push((*sp, arena[at..at + span_len(*sp) as usize * 4].to_vec()));
    }
    out
}

/// The bitmap is exactly the union of the index's blocks, which never overlap, and the
/// spare is kept.
fn check_arena(s: &Song) {
    let raw = s.raw();
    let mut want = vec![false; raw.arena_units as usize];
    let mut mark = |off4: u32, units: u32| {
        for u in off4..off4 + units {
            assert!(!want[u as usize], "blocks overlap at unit {u}");
            want[u as usize] = true;
        }
    };
    for sp in raw.order.iter().flatten().filter(|&&x| x != 0) {
        mark(span_off4(*sp), span_len(*sp).div_ceil(4));
    }
    for sp in raw.pattern.iter().filter(|&&x| x != 0) {
        mark(span_off4(*sp), span_len(*sp));
    }
    assert!(s.bitmap() == want, "the bitmap and the index disagree");
    assert_eq!(raw.used_units as usize, want.iter().filter(|&&b| b).count());
    let free = raw.arena_units * 4 - raw.used_units * 4;
    assert!(free >= SONG_ARENA_SPARE, "spare {free}");
}

/// Runs a rebuild, checks it touched no byte in use, commits it; compacts and retries on
/// FRAGMENTED.
fn rebuild(s: &mut Song, f: &mut dyn FnMut(*mut gt2_song, &mut gt2_edit) -> gt2_err) -> gt2_err {
    for _ in 0..1000 {
        let before = in_use(s);
        let mut e = gt2_edit::default();
        let r = f(s.ptr_mut(), &mut e);
        assert_eq!(in_use(s), before, "a primitive changed bytes in use");
        if r == GT2_ERR_FRAGMENTED {
            let mut c = gt2_edit::default();
            let before = in_use(s);
            assert_eq!(
                unsafe { gt2_compact_step(s.ptr_mut(), &mut c) },
                1,
                "compaction stuck"
            );
            assert_eq!(in_use(s), before, "compaction changed bytes in use");
            unsafe { gt2_edit_commit(s.ptr_mut(), &c) };
            continue;
        }
        if r == GT2_OK {
            unsafe { gt2_edit_commit(s.ptr_mut(), &e) };
        }
        return r;
    }
    panic!("no room after 1000 compactions");
}

#[test]
fn edit_refusals() {
    let mut s = Song::device();
    let mut e = gt2_edit::default();
    let p = s.ptr_mut();
    unsafe {
        // Cell writes: ranges.
        assert_eq!(
            gt2_set_row(p, 0, 63, [0x60, 0, 0, 0].as_ptr(), &mut e),
            GT2_OK
        );
        gt2_edit_cancel(p, &e);
        assert_eq!(
            gt2_set_row(p, 0, 64, [0x60, 0, 0, 0].as_ptr(), &mut e),
            GT2_ERR_RANGE
        );
        assert_eq!(
            gt2_set_row(p, 0, 0, [0xff, 0, 0, 0].as_ptr(), &mut e),
            GT2_ERR_RANGE
        );
        assert_eq!(
            gt2_set_row(p, 0, 0, [0x60, 0x40, 0, 0].as_ptr(), &mut e),
            GT2_ERR_RANGE
        );
        assert_eq!(
            gt2_set_row(p, 9, 0, [0x60, 0, 0, 0].as_ptr(), &mut e),
            GT2_ERR_RANGE
        );
        assert_eq!(gt2_set_instr(p, 0, 0, 1, &mut e), GT2_ERR_RANGE);
        assert_eq!(gt2_set_instr(p, 63, 24, 1, &mut e), GT2_OK);
        assert_eq!(gt2_set_table(p, 4, 0, 0, 1, &mut e), GT2_ERR_RANGE);
        // Orderlists: transpose before repeat, a pattern last; a held pattern; restart.
        assert_eq!(gt2_order_insert(p, 0, 0, 0, 0xd2, &mut e), GT2_OK);
        gt2_edit_commit(p, &e);
        assert_eq!(
            gt2_order_insert(p, 0, 0, 1, 0xe3, &mut e),
            GT2_ERR_ORDER_RULE
        );
        assert_eq!(gt2_order_insert(p, 0, 0, 0, 0xe3, &mut e), GT2_OK);
        gt2_edit_commit(p, &e);
        assert_eq!(
            gt2_order_insert(p, 0, 0, 3, 0xd1, &mut e),
            GT2_ERR_ORDER_RULE
        );
        assert_eq!(gt2_order_delete(p, 0, 0, 2, &mut e), GT2_ERR_ORDER_RULE);
        assert_eq!(gt2_set_order(p, 0, 0, 2, 0xe0, &mut e), GT2_ERR_ORDER_RULE);
        assert_eq!(gt2_set_order(p, 0, 0, 2, 9, &mut e), GT2_ERR_RANGE);
        assert_eq!(gt2_set_order(p, 0, 0, 2, 1, &mut e), GT2_OK);
        gt2_edit_cancel(p, &e);
        assert_eq!(gt2_set_order(p, 0, 0, 3, 3, &mut e), GT2_ERR_RANGE); // restart past end
        assert_eq!(gt2_set_order(p, 0, 0, 3, 2, &mut e), GT2_OK);
        gt2_edit_cancel(p, &e);
        assert_eq!(gt2_set_order(p, 1, 0, 0, 1, &mut e), GT2_ERR_RANGE); // no subtune 1
                                                                         // Rows.
        assert_eq!(gt2_pattern_resize(p, 0, 129, &mut e), GT2_ERR_ROWS);
        assert_eq!(gt2_pattern_resize(p, 0, 0, &mut e), GT2_ERR_ROWS);
        assert_eq!(gt2_pattern_resize(p, 0, 128, &mut e), GT2_OK);
        gt2_edit_commit(p, &e);
        assert_eq!(gt2_pattern_insert_row(p, 0, 0, &mut e), GT2_ERR_ROWS);
        assert_eq!(gt2_pattern_resize(p, 0, 1, &mut e), GT2_OK);
        gt2_edit_commit(p, &e);
        assert_eq!(gt2_pattern_delete_row(p, 0, 0, &mut e), GT2_ERR_ROWS);
        // Patterns: one in use is not removed; 208 at most.
        assert_eq!(gt2_pattern_remove(p, 1, &mut e), GT2_ERR_IN_USE);
        let mut n = 0;
        for _ in 3..208 {
            assert_eq!(gt2_pattern_add(p, 1, &mut n, &mut e), GT2_OK);
            gt2_edit_commit(p, &e);
        }
        assert_eq!(gt2_pattern_add(p, 1, &mut n, &mut e), GT2_ERR_PATTERNS);
        // 254 entries at most.
        for _ in 3..254 {
            assert_eq!(gt2_order_insert(p, 0, 1, 0, 1, &mut e), GT2_OK);
            gt2_edit_commit(p, &e);
        }
        assert_eq!(gt2_order_entries(p, 0, 1), 252);
        assert_eq!(gt2_order_insert(p, 0, 1, 0, 1, &mut e), GT2_OK);
        gt2_edit_commit(p, &e);
        assert_eq!(gt2_order_insert(p, 0, 1, 0, 1, &mut e), GT2_OK);
        gt2_edit_commit(p, &e);
        assert_eq!(gt2_order_insert(p, 0, 1, 0, 1, &mut e), GT2_ERR_ORDER_FULL);
    }
    check_arena(&s);
    // Full: a song's content may not eat into the spare.
    let mut s = Song::device();
    let mut r = GT2_OK;
    let mut added = 0;
    while r == GT2_OK && added < 300 {
        let mut n = 0;
        r = rebuild(&mut s, &mut |p, e| unsafe {
            gt2_pattern_add(p, 128, &mut n, e)
        });
        added += 1;
    }
    assert_eq!(r, GT2_ERR_FULL);
    check_arena(&s);
}

#[test]
fn random_edits_keep_the_arena_consistent() {
    random_edits(40, 400);
}

#[test]
#[ignore = "long: cargo test --release -p sidou-tests --test song -- --ignored"]
fn random_edits_long() {
    random_edits(2000, 2000);
}

fn random_edits(seeds: u64, steps: u32) {
    for seed in 0..seeds {
        let mut rng = Rng(0x5eed_1000 + seed);
        let mut s = Song::device();
        let file = random_canonical(&mut rng, false).encode();
        s.read(&file, 4).unwrap();
        let mut m = Model::of(&s);
        for step in 0..steps {
            let held: Vec<u8> = (0..208u8)
                .filter(|&p| m.patterns[p as usize].is_some())
                .collect();
            let op = if held.is_empty() { 4 } else { rng.below(9) };
            let p = if held.is_empty() {
                0
            } else {
                held[rng.below(held.len() as u32) as usize]
            };
            let rows = m.patterns[p as usize]
                .as_ref()
                .map_or(0, |v| v.len() as u32);
            let d = rng.below(m.orders.len() as u32);
            let c = rng.below(3);
            let (entries, _) = m.orders[d as usize][c as usize].clone();
            let n = entries.len() as u32;
            let what;
            let r = match op {
                0 => {
                    let row = rng.below(rows);
                    let cell = [
                        0x60 + rng.below(0x60) as u8,
                        rng.below(64) as u8,
                        rng.below(16) as u8,
                        rng.byte(),
                    ];
                    what = format!("set_row {p} {row} {cell:?}");
                    let mut e = gt2_edit::default();
                    let r =
                        unsafe { gt2_set_row(s.ptr_mut(), p as u32, row, cell.as_ptr(), &mut e) };
                    if r == GT2_OK {
                        unsafe { gt2_edit_commit(s.ptr_mut(), &e) };
                        m.patterns[p as usize].as_mut().unwrap()[row as usize] = cell;
                    }
                    assert_eq!(r == GT2_OK, row + 1 < rows, "{what}");
                    r
                }
                1 => {
                    let row = rng.below(rows + 1);
                    what = format!("insert_row {p} {row}");
                    let r = rebuild(&mut s, &mut |q, e| unsafe {
                        gt2_pattern_insert_row(q, p as u32, row, e)
                    });
                    let ok = row < rows && rows <= 128;
                    if ok && r == GT2_OK {
                        m.patterns[p as usize]
                            .as_mut()
                            .unwrap()
                            .insert(row as usize, EMPTY);
                    }
                    assert!(
                        if ok {
                            r == GT2_OK || r == GT2_ERR_FULL
                        } else {
                            r != GT2_OK
                        },
                        "{what}: {r}"
                    );
                    r
                }
                2 => {
                    let row = rng.below(rows);
                    what = format!("delete_row {p} {row}");
                    let r = rebuild(&mut s, &mut |q, e| unsafe {
                        gt2_pattern_delete_row(q, p as u32, row, e)
                    });
                    let ok = row + 1 < rows && rows >= 3;
                    if ok {
                        assert_eq!(r, GT2_OK, "{what}");
                        m.patterns[p as usize]
                            .as_mut()
                            .unwrap()
                            .remove(row as usize);
                    } else {
                        assert_ne!(r, GT2_OK, "{what}");
                    }
                    r
                }
                3 => {
                    let new_rows = 1 + rng.below(128);
                    what = format!("resize {p} {new_rows}");
                    let r = rebuild(&mut s, &mut |q, e| unsafe {
                        gt2_pattern_resize(q, p as u32, new_rows, e)
                    });
                    if r == GT2_OK {
                        let v = m.patterns[p as usize].as_mut().unwrap();
                        let end = *v.last().unwrap();
                        v.pop();
                        v.resize(new_rows as usize, EMPTY);
                        v.push(end);
                    } else {
                        assert_eq!(r, GT2_ERR_FULL, "{what}");
                    }
                    r
                }
                4 => {
                    let new_rows = 1 + rng.below(128);
                    let mut got = 0u32;
                    what = format!("add {new_rows}");
                    let r = rebuild(&mut s, &mut |q, e| unsafe {
                        gt2_pattern_add(q, new_rows, &mut got, e)
                    });
                    if r == GT2_OK {
                        let want = (0..208u8)
                            .find(|&q| m.patterns[q as usize].is_none() && !m.named(q))
                            .unwrap();
                        assert_eq!(got, want as u32);
                        let mut v = vec![EMPTY; new_rows as usize];
                        v.push([0xff, 0, 0, 0]);
                        m.patterns[got as usize] = Some(v);
                    } else {
                        assert!(r == GT2_ERR_FULL || r == GT2_ERR_PATTERNS, "{what}: {r}");
                    }
                    r
                }
                5 => {
                    what = format!("remove {p}");
                    let r = rebuild(&mut s, &mut |q, e| unsafe {
                        gt2_pattern_remove(q, p as u32, e)
                    });
                    assert_eq!(r == GT2_OK, !m.named(p), "{what}");
                    if r == GT2_OK {
                        m.patterns[p as usize] = None;
                    }
                    r
                }
                6 => {
                    let pos = rng.below(n + 1);
                    let value = match rng.below(3) {
                        0 => p,
                        1 => 0xd0 + rng.below(16) as u8,
                        _ => 0xe0 + rng.below(31) as u8,
                    };
                    what = format!("order_insert {d} {c} {pos} {value:#x}");
                    let r = rebuild(&mut s, &mut |q, e| unsafe {
                        gt2_order_insert(q, d, c, pos, value, e)
                    });
                    let mut t = entries.clone();
                    t.insert(pos as usize, value);
                    let ok = n < 254 && order_valid(&t);
                    if ok && r == GT2_OK {
                        let o = &mut m.orders[d as usize][c as usize];
                        if pos <= o.1 as u32 && (o.1 as u32) < n {
                            o.1 += 1;
                        }
                        o.0 = t;
                    }
                    assert!(
                        r == GT2_OK && ok || r != GT2_OK && (!ok || r == GT2_ERR_FULL),
                        "{what}: {r}"
                    );
                    r
                }
                7 => {
                    let pos = rng.below(n.max(1));
                    what = format!("order_delete {d} {c} {pos}");
                    let r = rebuild(&mut s, &mut |q, e| unsafe {
                        gt2_order_delete(q, d, c, pos, e)
                    });
                    let mut t = entries.clone();
                    let ok = pos < n && {
                        t.remove(pos as usize);
                        order_valid(&t)
                    };
                    assert_eq!(r == GT2_OK, ok, "{what}: {r}");
                    if ok {
                        let o = &mut m.orders[d as usize][c as usize];
                        let mut restart = o.1;
                        if (pos as u8) < restart {
                            restart -= 1;
                        }
                        if t.is_empty() {
                            restart = 0;
                        } else if restart as usize >= t.len() {
                            restart = t.len() as u8 - 1;
                        }
                        *o = (t, restart);
                    }
                    r
                }
                _ => {
                    // Compaction on its own: one step, the content unchanged.
                    what = "compact".into();
                    let mut e = gt2_edit::default();
                    let before = in_use(&s);
                    if unsafe { gt2_compact_step(s.ptr_mut(), &mut e) } == 1 {
                        assert_eq!(in_use(&s), before);
                        unsafe { gt2_edit_commit(s.ptr_mut(), &e) };
                    }
                    GT2_OK
                }
            };
            let _ = r;
            check_arena(&s);
            assert_eq!(Model::of(&s), m, "seed {seed} step {step}: {what}");
        }
        // Whatever the edits, the song saves and reads back the same.
        let out = s.write(true);
        let mut t = Song::device();
        t.read(&out, 4).unwrap();
        let again = t.write(true);
        if unsafe { gt2_saved_subtunes(s.ptr()) } < s.raw().subtunes as u32 {
            // A trailing subtune with an empty channel is dropped, but savesong still counts
            // the patterns only it names (0007): the next save drops them too, then holds.
            let mut u = Song::device();
            u.read(&again, 4).unwrap();
            assert_eq!(u.write(true), again, "seed {seed}");
        } else {
            assert_eq!(again, out, "seed {seed}");
        }
    }
}

// Fuzz: random bytes and mutated songs never crash the reader (run under UBSan and ASan as
// the sanitizer builds do), and whatever it accepts it writes and reads back.

fn fuzz(seeds: u64, per_seed: u32) {
    let mut s = Song::device();
    for seed in 0..seeds {
        let mut rng = Rng(0xf022_0000 + seed);
        let base = {
            let stereo = rng.chance(20);
            let mut sp = random_canonical(&mut rng, stereo);
            sp.ident = *[b"GTS2", b"GTS3", b"GTS4", b"GTS5"][rng.below(4) as usize];
            if &sp.ident == b"GTS2" {
                sp.tables.truncate(3);
            }
            let mut t = Song::device();
            t.read(&sp.encode(), 0).ok();
            with_chunks(&mut t);
            t.write(true)
        };
        for _ in 0..per_seed {
            let mut f = if rng.chance(10) {
                (0..rng.below(2000))
                    .map(|_| rng.byte())
                    .collect::<Vec<u8>>()
            } else {
                base.clone()
            };
            if rng.chance(15) && f.len() >= 4 {
                f[..4].copy_from_slice(b"GTS5");
            }
            for _ in 0..1 + rng.below(8) {
                if f.is_empty() {
                    break;
                }
                let at = rng.below(f.len() as u32) as usize;
                match rng.below(4) {
                    0 => f[at] = rng.byte(),
                    1 => f[at] ^= 1 << rng.below(8),
                    2 => f.truncate(at),
                    _ => f.insert(at, rng.byte()),
                }
            }
            let caps = if rng.chance(50) { 4 } else { 0 };
            if let Ok(info) = s.read(&f, caps) {
                let out = s.write(true);
                let mut t = Song::device();
                let again = t.read(&out, 0);
                let again = again.unwrap_or_else(|e| panic!("seed {seed}: {info:?} then {e:?}"));
                assert_eq!(again.channels, info.channels, "seed {seed}");
                let again_out = t.write(true);
                if unsafe { gt2_saved_subtunes(s.ptr()) } < s.raw().subtunes as u32 {
                    // savesong's corner (0007): one more save, then it holds.
                    let mut u = Song::device();
                    u.read(&again_out, 0).unwrap();
                    assert_eq!(u.write(true), again_out, "seed {seed}");
                } else {
                    assert_eq!(again_out, out, "seed {seed}");
                }
            }
        }
    }
}

#[test]
fn fuzz_reader() {
    fuzz(64, 200);
}

#[test]
#[ignore = "long: cargo test --release -p sidou-tests --test song -- --ignored"]
fn fuzz_reader_long() {
    fuzz(4000, 1000);
}

#[test]
fn a_trailer_settles_goattracker_stereos_guess() {
    // Instrument 1's bytes read as three more orderlists ($FF end marks, restarts 0), so a
    // one-subtune mono song passes all six of GoatTracker Stereo's trial reads (0007).
    let mut spec = SngSpec::new();
    let mut i = [0u8; 25];
    i[..9].copy_from_slice(&[0xff, 0x00, 0x01, 0xff, 0x00, 0x01, 0xff, 0x00, 0x09]);
    spec.instr = vec![i];
    let file = spec.encode();
    assert_eq!(
        determinechannels(&file),
        6,
        "GoatTracker Stereo misreads it"
    );
    let mut s = Song::device();
    // Without a trailer the six-channel reading does not end at the file's end and the
    // three-channel one does, so SIDOU reads it as mono, as GoatTracker 2.77 does.
    assert_eq!(s.read(&file, 4).unwrap().channels, 3);
    // With one, the trailer settles it.
    let mut u = Song::device();
    for (f, v) in i.iter().enumerate() {
        let mut e = gt2_edit::default();
        assert_eq!(
            unsafe { gt2_set_instr(u.ptr_mut(), 1, f as u32, *v, &mut e) },
            GT2_OK
        );
        unsafe { gt2_edit_commit(u.ptr_mut(), &e) };
    }
    assert_eq!(u.write(false), file);
    let with_trailer = u.write(true);
    let mut s = Song::device();
    let info = s.read(&with_trailer, 4).unwrap();
    assert_eq!(info.channels, 3);
    assert_eq!(info.trailer, GT2_TRAILER_READ);
    assert_eq!(s.write(true), with_trailer);
}

// Against GoatTracker's own load and save (gt2-ref, M2c Task 3).

#[test]
fn every_example_canonicalised_round_trips() {
    let mut canonical_as_shipped = Vec::new();
    let examples = gt2_ref::examples();
    for (name, sng) in &examples {
        let canon = gt2_ref::canonical(sng).unwrap();
        if &canon == sng {
            canonical_as_shipped.push(name.clone());
        }
        let mut s = Song::device();
        let info = s
            .read(&canon, 4)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(info.trailer, GT2_TRAILER_NONE);
        assert_eq!(s.write(false), canon, "{name}");
        // And from the file as shipped: SIDOU saves what GoatTracker saves.
        let mut t = Song::device();
        t.read(sng, 4).unwrap();
        assert_eq!(t.write(false), canon, "{name} as shipped");
    }
    eprintln!(
        "{} examples; canonical as shipped: {} ({})",
        examples.len(),
        canonical_as_shipped.len(),
        canonical_as_shipped.join(", ")
    );
}

/// A random song GoatTracker loads, canonical or not: any ident from GTS2, untrimmed
/// counts, trailing unused instruments, rows past a pattern's end, orderlists whose length
/// runs past their $FF.
fn random_loose(rng: &mut Rng) -> Vec<u8> {
    let mut sp = random_canonical(rng, false);
    sp.ident = *[b"GTS2", b"GTS3", b"GTS4", b"GTS5"][rng.below(4) as usize];
    if &sp.ident == b"GTS2" {
        sp.tables.truncate(3);
    }
    for _ in 0..rng.below(3) {
        let mut i = [0u8; 25];
        if rng.chance(50) {
            i[9] = b'X';
        }
        if rng.chance(30) {
            i[7] = rng.byte();
            i[8] = rng.byte();
        }
        sp.instr.push(i);
    }
    for t in sp.tables.iter_mut() {
        for _ in 0..rng.below(3) {
            t.push((0, 0));
        }
    }
    if rng.chance(30) {
        sp.patterns
            .push((vec![EMPTY; 1 + rng.below(20) as usize], [0xff, 0, 0, 0]));
    }
    for p in sp.patterns.iter_mut() {
        if rng.chance(20) {
            p.1 = [0xff, rng.byte() & 0x3f, rng.byte() & 0x0f, rng.byte()];
        }
    }
    let mut f = sp.encode();
    if rng.chance(30) {
        // A pattern with rows after its end row (its length byte past the end mark).
        let mut sp2 = sp.clone();
        let last = sp2.patterns.len() - 1;
        sp2.patterns[last].0.push([0xff, 0, 0, 0]);
        sp2.patterns[last].0.push([0x60, 0, 0, 0]);
        f = sp2.encode();
    }
    f
}

#[test]
fn sidou_saves_what_goattracker_saves() {
    saves_as_goattracker(300);
}

#[test]
#[ignore = "long: cargo test --release -p sidou-tests --test song -- --ignored"]
fn sidou_saves_what_goattracker_saves_long() {
    saves_as_goattracker(20_000);
}

fn saves_as_goattracker(seeds: u32) {
    let mut rng = Rng(0x5eed_9000);
    for seed in 0..seeds {
        let f = random_loose(&mut rng);
        let canon = gt2_ref::canonical(&f).unwrap();
        let mut s = Song::host();
        s.read(&f, 0)
            .unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));
        let ours = s.write(false);
        let at = ours.iter().zip(&canon).position(|(a, b)| a != b);
        assert!(
            ours == canon,
            "seed {seed} ({}): {} vs {} bytes, first difference at {at:?}",
            String::from_utf8_lossy(&f[..4]),
            ours.len(),
            canon.len()
        );
    }
}

/// The reviewer's file: a plain 405-byte mono song (one pattern held, patterns 1 and 2 named
/// and so empty) whose instrument 1 starts `FF 00 01 FF 00 01 FF 00`. Those bytes pass
/// GoatTracker Stereo's three extra trial reads, and the six-channel parse that follows ends
/// at byte 128, nowhere near the file's end. GoatTracker 2.77 opens it as mono.
fn reviewers_mono_file() -> Vec<u8> {
    let mut spec = SngSpec::new();
    let mut i = [0u8; 25];
    i[..8].copy_from_slice(&[0xff, 0x00, 0x01, 0xff, 0x00, 0x01, 0xff, 0x00]);
    spec.instr = vec![i];
    spec.patterns.truncate(1);
    spec.patterns[0].0[0] = [0x60, 1, 0, 0];
    spec.encode()
}

#[test]
fn a_mono_file_that_passes_the_stereo_trial_reads_as_mono() {
    let file = reviewers_mono_file();
    assert_eq!(file.len(), 405);
    assert_eq!(determinechannels(&file), 6, "GoatTracker Stereo's guess");
    let mut s = Song::device();
    let info = s.read(&file, 4).unwrap();
    assert_eq!(info.channels, 3, "{info:?}");
    assert_eq!(info.gt2_bytes as usize, file.len());
    assert_eq!(s.pattern(0).unwrap()[0], [0x60, 1, 0, 0]);
    assert_eq!(s.raw().instr[0][..8], [0xff, 0, 1, 0xff, 0, 1, 0xff, 0]);
    // Saved, the named patterns 1 and 2 are written too, and it still reads as mono.
    let plain = s.write(false);
    assert_eq!(determinechannels(&plain), 6);
    let mut u = Song::device();
    assert_eq!(u.read(&plain, 4).unwrap().channels, 3);
    assert_eq!(u.write(false), plain);
    // A trailer after it: the six-channel parse ends at neither the file's end nor the
    // trailer, the three-channel one at the trailer.
    let t = s.write(true);
    let mut u = Song::device();
    let info = u.read(&t, 4).unwrap();
    assert_eq!((info.channels, info.trailer), (3, GT2_TRAILER_READ));
    // A damaged trailer still ends the three-channel reading better than the six-channel
    // one, which stops at byte 128.
    let mut d = t.clone();
    let last = d.len() - 1;
    d[last] ^= 1;
    let info = u.read(&d, 4).unwrap();
    assert_eq!((info.channels, info.trailer), (3, GT2_TRAILER_DROPPED));
    assert_eq!(u.write(true), t);
}

/// Whatever SIDOU writes without a trailer, it reads back as the same song: mono songs
/// whose instruments or tables look like orderlists to GoatTracker Stereo's trial read
/// included.
#[test]
fn sidou_output_without_a_trailer_reads_back_unless_ambiguous() {
    plain_reads_back(2000);
}

#[test]
#[ignore = "long: cargo test --release -p sidou-tests --test song -- --ignored"]
fn sidou_output_without_a_trailer_reads_back_unless_ambiguous_long() {
    plain_reads_back(200_000);
}

fn plain_reads_back(seeds: u32) {
    let mut rng = Rng(0x5eed_0004);
    let lookalike = [0xff, 0x00, 0x01, 0xff, 0x00, 0x01, 0xff, 0x00];
    let (mut misread, mut ambiguous) = (0, 0);
    for seed in 0..seeds {
        let stereo = seed % 5 == 0;
        let mut spec = random_canonical(&mut rng, stereo);
        if !stereo && seed % 2 == 1 {
            // Make the trial read pass: the first subtune's three extra "orderlists" come
            // from the instrument count and the first instrument's bytes.
            let mut i = [0u8; 25];
            i[..8].copy_from_slice(&lookalike);
            for b in i.iter_mut().skip(8) {
                *b = if rng.chance(50) { 0 } else { rng.byte() };
            }
            i[0] |= 1;
            spec.orders.truncate(1);
            spec.instr.insert(0, i);
            spec.instr.truncate(1);
            for p in spec.patterns.iter_mut() {
                for r in p.0.iter_mut() {
                    r[1] = r[1].min(1);
                }
            }
        }
        let want = if stereo { 6 } else { 3 };
        let file = spec.encode();
        let mut s = Song::device();
        let info = s
            .read(&file, 4)
            .unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));
        if info.channels != want {
            // Only a file that is also the canonical save of a song with the other channel
            // count, which no reader can tell apart, may read otherwise.
            assert_eq!(s.write(false), file, "seed {seed}: {file:02x?}");
            ambiguous += 1;
            continue;
        }
        let plain = s.write(false);
        if determinechannels(&plain) != want as u32 {
            misread += 1;
        }
        let mut t = Song::device();
        let info = t
            .read(&plain, 4)
            .unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));
        assert_eq!(info.channels, want, "seed {seed}");
        assert_eq!(t.write(false), plain, "seed {seed}");
        assert_eq!(Model::of(&t), Model::of(&s), "seed {seed}");
        assert_eq!(s.check_plain(&plain), (0, String::new()), "seed {seed}");
        // With the trailer, always.
        let mut u = Song::device();
        assert_eq!(
            u.read(&s.write(true), 4).unwrap().channels,
            want,
            "seed {seed}"
        );
    }
    eprintln!("{seeds} songs: {misread} GoatTracker Stereo misreads, {ambiguous} ambiguous");
    assert!(misread > 0, "no file GoatTracker Stereo misreads");
    assert!(
        ambiguous * 1000 <= seeds,
        "{ambiguous} ambiguous of {seeds}"
    );
}

/// A file that is the canonical save of a mono song and of a stereo one alike (found by
/// `sidou_output_without_a_trailer_reads_back_unless_ambiguous_long`): both readings end at the same
/// byte, so the trailer they share settles it only through the channel count it records.
#[test]
fn the_trailer_records_the_channel_count() {
    let lookalike = [0xff, 0x00, 0x01, 0xff, 0x00, 0x01, 0xff, 0x00];
    let mut found = 0;
    // The generator's states before three of the ambiguous songs it made.
    for state in [
        0xdf75_5aa4_46c7_9a0d,
        0xe055_6381_75ab_ace3,
        0x0917_3ee8_97e0_f7b5,
    ] {
        let mut rng = Rng(state);
        let mut spec = random_canonical(&mut rng, false);
        let mut i = [0u8; 25];
        i[..8].copy_from_slice(&lookalike);
        for b in i.iter_mut().skip(8) {
            *b = if rng.chance(50) { 0 } else { rng.byte() };
        }
        i[0] |= 1;
        spec.orders.truncate(1);
        spec.instr = vec![i];
        for p in spec.patterns.iter_mut() {
            for r in p.0.iter_mut() {
                r[1] = r[1].min(1);
            }
        }
        let file = spec.encode();
        let mut s = Song::device();
        let Ok(info) = s.read(&file, 4) else { continue };
        if info.channels != 6 || s.write(false) != file {
            continue;
        }
        // Read plain as stereo, and its stereo save is the same file: ambiguous. The mono
        // song (built from the spec by hand) saved with a trailer reads back as mono, and
        // the stereo one as stereo.
        found += 1;
        let mut m = Song::device();
        set_mono_from_spec(&mut m, &spec);
        assert_eq!(m.write(false), file);
        // A plain export of the mono song is caught by its check.
        assert_eq!(
            m.check_plain(&file),
            (1, "SNG: READS BACK AS STEREO".into())
        );
        assert_eq!(s.check_plain(&file), (0, String::new()));
        assert_eq!(m.check_plain(&file[..200]).0, -1);
        for (song, want) in [(&m, 3), (&s, 6)] {
            let t = song.write(true);
            let mut u = Song::device();
            let info = u.read(&t, 4).unwrap();
            assert_eq!((info.channels, info.trailer), (want, GT2_TRAILER_READ));
            assert_eq!(u.write(true), t);
        }
    }
    assert_eq!(found, 3, "the ambiguous files");
}

/// Builds a one-subtune mono spec's song through the edit primitives (the reader would take
/// an ambiguous file as stereo).
fn set_mono_from_spec(s: &mut Song, spec: &SngSpec) {
    let p = s.ptr_mut();
    let commit = |e: &gt2_edit| unsafe { gt2_edit_commit(p, e) };
    let mut e = gt2_edit::default();
    unsafe {
        for (n, (rows, _)) in spec.patterns.iter().enumerate() {
            if n >= 3 {
                let mut got = 0;
                assert_eq!(
                    gt2_pattern_add(p, rows.len() as u32, &mut got, &mut e),
                    GT2_OK
                );
                assert_eq!(got, n as u32);
            } else {
                assert_eq!(
                    gt2_pattern_resize(p, n as u32, rows.len() as u32, &mut e),
                    GT2_OK
                );
            }
            commit(&e);
            for (r, cell) in rows.iter().enumerate() {
                assert_eq!(
                    gt2_set_row(p, n as u32, r as u32, cell.as_ptr(), &mut e),
                    GT2_OK
                );
                commit(&e);
            }
        }
        for (c, (entries, restart)) in spec.orders[0].iter().enumerate() {
            let c = c as u32;
            for (k, v) in entries.iter().enumerate() {
                assert_eq!(gt2_order_insert(p, 0, c, k as u32, *v, &mut e), GT2_OK);
                commit(&e);
            }
            // Drop clearsong's entry, now last.
            assert_eq!(
                gt2_order_delete(p, 0, c, entries.len() as u32, &mut e),
                GT2_OK
            );
            commit(&e);
            assert_eq!(
                gt2_set_order(p, 0, c, entries.len() as u32, *restart, &mut e),
                GT2_OK
            );
            commit(&e);
        }
        for (k, v) in spec.instr[0].iter().enumerate() {
            assert_eq!(gt2_set_instr(p, 1, k as u32, *v, &mut e), GT2_OK);
            commit(&e);
        }
        for (t, rows) in spec.tables.iter().enumerate() {
            for (r, (l, rr)) in rows.iter().enumerate() {
                assert_eq!(gt2_set_table(p, t as u32, r as u32, 0, *l, &mut e), GT2_OK);
                commit(&e);
                assert_eq!(gt2_set_table(p, t as u32, r as u32, 1, *rr, &mut e), GT2_OK);
                commit(&e);
            }
        }
    }
    for k in 0..3 {
        s.set_string(k, &spec.strings[k]);
    }
}

// Compaction.

/// The blocks in the arena (every index entry's), for compaction's step bound.
fn blocks(s: &Song) -> u32 {
    let raw = s.raw();
    (raw.order.iter().flatten().filter(|&&x| x != 0).count()
        + raw.pattern.iter().filter(|&&x| x != 0).count()) as u32
}

/// The free space is one hole: every used unit below every free one.
fn one_hole(s: &Song) -> bool {
    let b = s.bitmap();
    let used = s.raw().used_units as usize;
    b[..used].iter().all(|&u| u) && b[used..].iter().all(|&u| !u)
}

/// Compacts to the end, each step committed at once; the steps taken.
fn compact_all(s: &mut Song) -> u32 {
    let mut steps = 0;
    loop {
        let before = in_use(s);
        let m = Model::of(s);
        let mut e = gt2_edit::default();
        let r = unsafe { gt2_compact_step(s.ptr_mut(), &mut e) };
        assert_ne!(r, -1, "nothing is pending");
        if r == 0 {
            return steps;
        }
        assert_eq!(in_use(s), before, "compaction changed bytes in use");
        unsafe { gt2_edit_commit(s.ptr_mut(), &e) };
        assert_eq!(Model::of(s), m, "compaction changed the song");
        steps += 1;
        assert!(steps <= 100_000, "compaction does not end");
    }
}

/// A review's counterexample: full-size patterns, each followed by a 2-unit hole. The free
/// space is well above the spare but no hole holds a 128-row pattern, and no single move
/// into a hole helps: the old compaction gave up, and every rebuild failed for good.
#[test]
fn compaction_never_dead_ends() {
    let mut s = Song::device();
    unsafe { gt2_song_wipe(s.ptr_mut()) };
    let cap = unsafe { gt2_arena_capacity(s.ptr()) };
    let mut fillers = Vec::new();
    let mut n = 0;
    while (n + 1) * 516 <= cap {
        let mut off4 = 0;
        if unsafe { gt2_arena_alloc(s.ptr_mut(), 516, &mut off4) } != 0 {
            break;
        }
        unsafe { (*s.ptr_mut()).pattern[n as usize] = off4 | 129 << 16 };
        n += 1;
        if unsafe { gt2_arena_alloc(s.ptr_mut(), 8, &mut off4) } == 0 {
            fillers.push(off4);
        }
    }
    for f in fillers {
        unsafe { gt2_arena_release(s.ptr_mut(), f, 8) };
    }
    assert!(n >= 100, "{n} patterns");
    let free = s.raw().arena_units * 4 - unsafe { gt2_arena_used(s.ptr()) };
    assert!(free >= SONG_ARENA_SPARE, "free {free}");
    assert!(unsafe { gt2_arena_largest_hole(s.ptr()) } < 516);
    check_arena(&s);
    let mut e = gt2_edit::default();
    assert_eq!(
        unsafe { gt2_pattern_delete_row(s.ptr_mut(), 0, 0, &mut e) },
        GT2_ERR_FRAGMENTED
    );
    let b = blocks(&s);
    let steps = compact_all(&mut s);
    assert!(steps <= 2 * b, "{steps} steps for {b} blocks");
    assert!(one_hole(&s));
    check_arena(&s);
    for p in 0..n {
        assert_eq!(
            unsafe { gt2_pattern_delete_row(s.ptr_mut(), p, 0, &mut e) },
            GT2_OK,
            "pattern {p}"
        );
        unsafe { gt2_edit_commit(s.ptr_mut(), &e) };
    }
    check_arena(&s);
}

/// Random fragmentation, then compaction to the end: always one hole, within 2 x blocks
/// steps.
#[test]
fn compaction_always_reaches_one_hole() {
    compaction_random(60);
}

#[test]
#[ignore = "long: cargo test --release -p sidou-tests --test song -- --ignored"]
fn compaction_always_reaches_one_hole_long() {
    compaction_random(3000);
}

fn compaction_random(seeds: u64) {
    for seed in 0..seeds {
        let mut rng = Rng(0xc0_0000 + seed);
        let mut s = Song::device();
        s.read(&random_canonical(&mut rng, seed % 4 == 0).encode(), 4)
            .unwrap();
        // Fragment it: patterns added, resized and removed, rebuilds that find no hole
        // skipped.
        // Mostly near-full patterns, so the arena fills and its holes are small.
        for _ in 0..rng.below(1500) {
            let mut e = gt2_edit::default();
            let p = rng.below(208);
            let rows = if rng.chance(70) {
                118 + rng.below(11)
            } else {
                1 + rng.below(128)
            };
            let r = unsafe {
                match rng.below(3) {
                    0 => {
                        let mut got = 0;
                        gt2_pattern_add(s.ptr_mut(), rows, &mut got, &mut e)
                    }
                    1 => gt2_pattern_resize(s.ptr_mut(), p, rows, &mut e),
                    _ => gt2_pattern_remove(s.ptr_mut(), p, &mut e),
                }
            };
            if r == GT2_OK {
                unsafe { gt2_edit_commit(s.ptr_mut(), &e) };
            }
        }
        check_arena(&s);
        let b = blocks(&s);
        let steps = compact_all(&mut s);
        assert!(steps <= 2 * b, "seed {seed}: {steps} steps for {b} blocks");
        assert!(one_hole(&s), "seed {seed}");
        check_arena(&s);
    }
}

// Deferred edits (M2c Task 6): the main loop makes edits and queues them; the audio
// interrupt applies them later, in order, and acknowledges each; the main loop releases
// each after its acknowledgement.

/// What the main loop and the player see: the orderlists and patterns, the instruments and
/// the tables.
#[derive(Clone, Debug, PartialEq)]
struct Whole {
    m: Model,
    instr: Vec<u8>,
    tables: Vec<u8>,
}

impl Whole {
    fn of(s: &Song) -> Whole {
        let r = s.raw();
        Whole {
            m: Model::of(s),
            instr: r.instr.iter().flatten().copied().collect(),
            tables: r
                .ltable
                .iter()
                .chain(r.rtable.iter())
                .flatten()
                .copied()
                .collect(),
        }
    }
}

#[test]
fn deferred_edits_apply_as_made() {
    deferred_edits(60, 600);
}

#[test]
#[ignore = "long: cargo test --release -p sidou-tests --test song -- --ignored"]
fn deferred_edits_long() {
    deferred_edits(2000, 2000);
}

/// The arena bytes the song's orderlists and patterns take, by the model.
fn content_bytes(w: &Whole) -> u32 {
    let orders: usize =
        w.m.orders
            .iter()
            .flatten()
            .map(|o| (o.0.len() + 2).div_ceil(4) * 4)
            .sum();
    let patterns: usize = w.m.patterns.iter().flatten().map(|p| p.len() * 4).sum();
    (orders + patterns) as u32
}

fn deferred_edits(seeds: u64, steps: u32) {
    use std::collections::VecDeque;
    let mut full_seen = 0;
    for seed in 0..seeds {
        let mut rng = Rng(0xde_0000 + seed);
        let mut s = Song::device();
        let file = random_canonical(&mut rng, false).encode();
        s.read(&file, 4).unwrap();
        if seed % 2 == 0 {
            // Near the cap, so rebuilds meet FULL and PENDING for room.
            let mut r = GT2_OK;
            while r == GT2_OK {
                let mut n = 0;
                let rows = 100 + rng.below(29);
                r = rebuild(&mut s, &mut |p, e| unsafe {
                    gt2_pattern_add(p, rows, &mut n, e)
                });
            }
        }
        let cap = unsafe { gt2_arena_capacity(s.ptr()) };
        // The intended song after every edit made so far, and the queue: each edit with the
        // song as it must stand once that edit is applied, and as it stood before.
        let mut want = Whole::of(&s);
        let mut queue: VecDeque<(gt2_edit, Whole, Whole)> = VecDeque::new();
        let mut acked: VecDeque<gt2_edit> = VecDeque::new();
        let (mut made, mut refused, mut cancelled) = (0, 0, 0);
        for step in 0..steps {
            match rng.below(12) {
                // The interrupt applies up to 3 queued edits.
                0 | 1 => {
                    for _ in 0..rng.below(4) {
                        let Some((e, w, _)) = queue.pop_front() else {
                            break;
                        };
                        unsafe { gt2_edit_apply(s.ptr_mut(), &e) };
                        assert_eq!(Whole::of(&s), w, "seed {seed} step {step}: applied");
                        acked.push_back(e);
                    }
                }
                // The main loop releases what is acknowledged.
                2 => {
                    while let Some(e) = acked.pop_front() {
                        unsafe { gt2_edit_release(s.ptr_mut(), &e) };
                    }
                    if queue.is_empty() {
                        check_arena(&s);
                    }
                }
                // A compaction step: refused while an edit holding a slot is pending, else
                // a swap that changes nothing the player sees.
                3 => {
                    let mut e = gt2_edit::default();
                    // Instrument and table cells hold no slot and do not count.
                    let pending = queue
                        .iter()
                        .map(|q| &q.0)
                        .chain(acked.iter())
                        .any(|e| e.slot != GT2_SLOT_NONE as u16);
                    let r = unsafe { gt2_compact_step(s.ptr_mut(), &mut e) };
                    assert_eq!(r == -1, pending, "seed {seed} step {step}: compaction");
                    if r == 1 {
                        queue.push_back((e, want.clone(), want.clone()));
                    }
                }
                // The main loop takes back the last edit it queued, never applied.
                4 => {
                    if let Some((e, _, before)) = queue.pop_back() {
                        unsafe { gt2_edit_cancel(s.ptr_mut(), &e) };
                        want = before;
                        cancelled += 1;
                    }
                }
                // A reload or a clear waits for every pending edit.
                5 if rng.chance(10) => {
                    if !queue.is_empty() || !acked.is_empty() {
                        let live = Whole::of(&s);
                        assert_eq!(s.read(&file, 4).unwrap_err().msg, "SNG: EDITS PENDING");
                        assert_eq!(unsafe { gt2_song_clear(s.ptr_mut()) }, -1);
                        assert_eq!(Whole::of(&s), live, "seed {seed} step {step}");
                    }
                }
                // An edit.
                _ => {
                    let live = in_use(&s);
                    let mut e = gt2_edit::default();
                    let mut w = want.clone();
                    let r = deferred_op(&mut rng, &mut s, &mut w, &mut e);
                    assert_eq!(in_use(&s), live, "seed {seed} step {step}: bytes in use");
                    if r == GT2_OK {
                        made += 1;
                        queue.push_back((e, w.clone(), want.clone()));
                        want = w;
                    } else {
                        refused += 1;
                        if r == GT2_ERR_FULL {
                            // FULL is judged on the song at rest: only when the largest
                            // rebuild would not fit it.
                            full_seen += 1;
                            assert!(content_bytes(&want) + 516 > cap, "seed {seed} step {step}");
                        }
                    }
                }
            }
            // The song's size at rest (every pending edit released) is the model's.
            let raw = s.raw();
            assert_eq!(
                (raw.used_units - raw.pending_old_units) * 4,
                content_bytes(&want),
                "seed {seed} step {step}: the size at rest"
            );
        }
        while let Some((e, w, _)) = queue.pop_front() {
            unsafe { gt2_edit_apply(s.ptr_mut(), &e) };
            assert_eq!(Whole::of(&s), w, "seed {seed}: drained");
            acked.push_back(e);
        }
        while let Some(e) = acked.pop_front() {
            unsafe { gt2_edit_release(s.ptr_mut(), &e) };
        }
        check_arena(&s);
        assert_eq!(Whole::of(&s), want, "seed {seed}");
        let raw = s.raw();
        assert_eq!(
            (raw.pending, raw.pending_loose, raw.pending_old_units),
            (0, 0, 0)
        );
        assert!(
            made > 0 && refused > 0 && cancelled > 0,
            "seed {seed}: {made} made, {refused} refused, {cancelled} cancelled"
        );
    }
    assert!(full_seen > 0, "no FULL met");
}

/// One random primitive on the song, its effect made on the model w if it succeeds.
fn deferred_op(rng: &mut Rng, s: &mut Song, w: &mut Whole, e: &mut gt2_edit) -> gt2_err {
    let held: Vec<u32> = (0..208)
        .filter(|&p| w.m.patterns[p as usize].is_some())
        .collect();
    let p = if held.is_empty() {
        0
    } else {
        held[rng.below(held.len() as u32) as usize]
    };
    let rows = w.m.patterns[p as usize]
        .as_ref()
        .map_or(0, |v| v.len() as u32);
    let d = rng.below(w.m.orders.len() as u32);
    let c = rng.below(3);
    let (entries, restart) = w.m.orders[d as usize][c as usize].clone();
    let n = entries.len() as u32;
    let q = s.ptr_mut();
    unsafe {
        match rng.below(10) {
            0 | 1 => {
                let row = rng.below(rows.max(1));
                let cell = [
                    0x60 + rng.below(0x60) as u8,
                    rng.below(64) as u8,
                    rng.below(16) as u8,
                    rng.byte(),
                ];
                let r = gt2_set_row(q, p, row, cell.as_ptr(), e);
                if r == GT2_OK {
                    w.m.patterns[p as usize].as_mut().unwrap()[row as usize] = cell;
                }
                r
            }
            2 => {
                let (i, f, v) = (1 + rng.below(63), rng.below(25), rng.byte());
                let r = gt2_set_instr(q, i, f, v, e);
                if r == GT2_OK {
                    w.instr[((i - 1) * 25 + f) as usize] = v;
                }
                r
            }
            3 => {
                let (t, row, side, v) = (rng.below(4), rng.below(255), rng.below(2), rng.byte());
                let r = gt2_set_table(q, t, row, side, v, e);
                if r == GT2_OK {
                    w.tables[(side * 1020 + t * 255 + row) as usize] = v;
                }
                r
            }
            4 => {
                let row = rng.below(rows + 1);
                let r = gt2_pattern_insert_row(q, p, row, e);
                if r == GT2_OK {
                    w.m.patterns[p as usize]
                        .as_mut()
                        .unwrap()
                        .insert(row as usize, EMPTY);
                }
                r
            }
            5 => {
                let row = rng.below(rows.max(1));
                let r = gt2_pattern_delete_row(q, p, row, e);
                if r == GT2_OK {
                    w.m.patterns[p as usize]
                        .as_mut()
                        .unwrap()
                        .remove(row as usize);
                }
                r
            }
            6 => {
                let new_rows = 1 + rng.below(128);
                let r = gt2_pattern_resize(q, p, new_rows, e);
                if r == GT2_OK {
                    let v = w.m.patterns[p as usize].as_mut().unwrap();
                    let end = v.pop().unwrap();
                    v.resize(new_rows as usize, EMPTY);
                    v.push(end);
                }
                r
            }
            7 => {
                let mut got = 0;
                let new_rows = 1 + rng.below(128);
                let r = gt2_pattern_add(q, new_rows, &mut got, e);
                if r == GT2_OK {
                    let mut v = vec![EMPTY; new_rows as usize];
                    v.push([0xff, 0, 0, 0]);
                    assert!(w.m.patterns[got as usize].is_none() && !w.m.named(got as u8));
                    w.m.patterns[got as usize] = Some(v);
                } else if rng.chance(50) {
                    // Or remove one.
                    let r = gt2_pattern_remove(q, p, e);
                    if r == GT2_OK {
                        assert!(!w.m.named(p as u8), "removed a named pattern {p}");
                        w.m.patterns[p as usize] = None;
                    }
                    return r;
                }
                r
            }
            8 => {
                let pos = rng.below(n + 1);
                let value = if rng.chance(50) {
                    p as u8
                } else {
                    0xd0 + rng.below(16) as u8
                };
                if rng.chance(30) {
                    // A cell write: an entry, or the restart position.
                    let v = if pos == n {
                        rng.below(n.max(1)) as u8
                    } else {
                        value
                    };
                    let r = gt2_set_order(q, d, c, pos, v, e);
                    if r == GT2_OK {
                        let o = &mut w.m.orders[d as usize][c as usize];
                        if pos == n {
                            o.1 = v;
                        } else {
                            o.0[pos as usize] = v;
                        }
                        assert!(order_valid(&o.0), "an invalid orderlist written");
                        assert!(o
                            .0
                            .iter()
                            .all(|&x| x >= 0xd0 || w.m.patterns[x as usize].is_some()));
                    }
                    return r;
                }
                let r = gt2_order_insert(q, d, c, pos, value, e);
                if r == GT2_OK {
                    let o = &mut w.m.orders[d as usize][c as usize];
                    o.0.insert(pos as usize, value);
                    if pos <= restart as u32 && (restart as u32) < n {
                        o.1 += 1;
                    }
                    assert!(order_valid(&o.0), "an invalid orderlist built");
                    assert!(value >= 0xd0 || w.m.patterns[value as usize].is_some());
                }
                r
            }
            _ => {
                let pos = rng.below(n.max(1));
                let r = gt2_order_delete(q, d, c, pos, e);
                if r == GT2_OK {
                    let o = &mut w.m.orders[d as usize][c as usize];
                    o.0.remove(pos as usize);
                    let mut rs = o.1;
                    if (pos as u8) < rs {
                        rs -= 1;
                    }
                    if o.0.is_empty() {
                        rs = 0;
                    } else if rs as usize >= o.0.len() {
                        rs = o.0.len() as u8 - 1;
                    }
                    o.1 = rs;
                    assert!(order_valid(&o.0), "an invalid orderlist built");
                }
                r
            }
        }
    }
}

/// The deferred contract's refusals (gt2_song.h), one by one.
#[test]
fn pending_edits_are_refused() {
    let mut s = Song::device();
    let p = s.ptr_mut();
    let (mut a, mut b) = (gt2_edit::default(), gt2_edit::default());
    let row = [0x60, 1, 0, 0];
    unsafe {
        // Cell writes to a pattern queue up; its rebuild waits for them.
        assert_eq!(gt2_set_row(p, 0, 0, row.as_ptr(), &mut a), GT2_OK);
        assert_eq!(gt2_set_row(p, 0, 1, row.as_ptr(), &mut b), GT2_OK);
        assert_eq!(gt2_pattern_insert_row(p, 0, 0, &mut e()), GT2_ERR_PENDING);
        gt2_edit_commit(p, &a);
        assert_eq!(gt2_pattern_insert_row(p, 0, 0, &mut e()), GT2_ERR_PENDING);
        gt2_edit_commit(p, &b);
        // (b) One pending swap a pattern; (a) no cell write behind it, which would hold
        // the old copy's offset.
        assert_eq!(gt2_pattern_insert_row(p, 0, 0, &mut a), GT2_OK);
        assert_eq!(gt2_pattern_delete_row(p, 0, 0, &mut e()), GT2_ERR_PENDING);
        assert_eq!(gt2_pattern_resize(p, 0, 9, &mut e()), GT2_ERR_PENDING);
        assert_eq!(
            gt2_set_row(p, 0, 0, row.as_ptr(), &mut e()),
            GT2_ERR_PENDING
        );
        // Another pattern is free to change.
        assert_eq!(gt2_set_row(p, 1, 0, row.as_ptr(), &mut b), GT2_OK);
        gt2_edit_commit(p, &b);
        // (c) No compaction while anything is pending.
        assert_eq!(gt2_compact_step(p, &mut e()), -1);
        // An orderlist edit naming the pattern waits for its swap.
        assert_eq!(gt2_order_insert(p, 0, 1, 0, 0, &mut e()), GT2_ERR_PENDING);
        assert_eq!(gt2_set_order(p, 0, 1, 0, 0, &mut e()), GT2_ERR_PENDING);
        gt2_edit_apply(p, &a);
        assert_eq!(
            gt2_set_row(p, 0, 0, row.as_ptr(), &mut e()),
            GT2_ERR_PENDING
        );
        gt2_edit_release(p, &a);
        // Released, compaction may run again.
        if gt2_compact_step(p, &mut b) == 1 {
            gt2_edit_commit(p, &b);
        } else {
            assert_eq!(gt2_compact_step(p, &mut b), 0);
        }
    }
    // Orderlists: one pending edit each, and a pattern's removal waits for all of them.
    let mut s = Song::device();
    let p = s.ptr_mut();
    unsafe {
        let mut n = 0;
        assert_eq!(gt2_pattern_add(p, 4, &mut n, &mut a), GT2_OK);
        gt2_edit_commit(p, &a);
        assert_eq!(gt2_order_insert(p, 0, 0, 0, n as u8, &mut a), GT2_OK);
        assert_eq!(gt2_order_insert(p, 0, 0, 0, 1, &mut e()), GT2_ERR_PENDING);
        assert_eq!(gt2_set_order(p, 0, 0, 0, 1, &mut e()), GT2_ERR_PENDING);
        assert_eq!(gt2_order_delete(p, 0, 0, 0, &mut e()), GT2_ERR_PENDING);
        assert_eq!(gt2_set_order(p, 0, 1, 0, 2, &mut b), GT2_OK);
        assert_eq!(gt2_pattern_remove(p, n, &mut e()), GT2_ERR_PENDING);
        gt2_edit_commit(p, &a);
        gt2_edit_commit(p, &b);
        assert_eq!(gt2_pattern_remove(p, n, &mut e()), GT2_ERR_IN_USE);
        // A cancelled edit ends its pending too.
        assert_eq!(gt2_order_delete(p, 0, 0, 0, &mut a), GT2_OK);
        gt2_edit_cancel(p, &a);
        assert_eq!(gt2_order_delete(p, 0, 0, 0, &mut a), GT2_OK);
        gt2_edit_commit(p, &a);
        assert_eq!(gt2_pattern_remove(p, n, &mut a), GT2_OK);
        // A pattern being removed is not reused until released.
        gt2_edit_apply(p, &a);
        let mut m = 0;
        assert_eq!(gt2_pattern_add(p, 4, &mut m, &mut b), GT2_OK);
        assert_ne!(m, n);
        gt2_edit_release(p, &a);
        gt2_edit_commit(p, &b);
        // The instruments and tables are never refused.
        for _ in 0..300 {
            assert_eq!(gt2_set_instr(p, 1, 0, 1, &mut a), GT2_OK);
            assert_eq!(gt2_set_table(p, 0, 0, 0, 1, &mut a), GT2_OK);
        }
    }
    assert_eq!(s.raw().pending, 0);
    check_arena(&s);
}

fn e() -> gt2_edit {
    gt2_edit::default()
}

/// OPTS values out of gt2reloc's ranges: what gt2reloc clamps is clamped as it clamps it;
/// what has no meaning there makes the chunk invalid (the defaults stand).
#[test]
fn opts_out_of_range_are_clamped_or_refused() {
    let mut o = gt2_options::default();
    unsafe { gt2_options_default(&mut o) };
    let mut b = [0u8; SONG_OPTS_BYTES as usize];
    unsafe { song_opts_encode(&o, b.as_mut_ptr()) };
    let decode = |b: &[u8]| {
        let mut d = gt2_options::default();
        let r = unsafe { song_opts_decode(b.as_ptr(), b.len() as u32, &mut d) };
        (r, d)
    };
    let (r, d) = decode(&b);
    assert_eq!(r, 0);
    assert_eq!(d.multispeed, 1);
    // Clamped as gt2reloc clamps them: -S above 16, -O and -R above 1, -F below 100, and
    // -V above 1 (any is "always").
    let mut c = b;
    c[3] = 20;
    c[14] = 5;
    c[15] = 9;
    c[5..9].copy_from_slice(&50u32.to_le_bytes());
    c[13] = 7;
    let (r, d) = decode(&c);
    assert_eq!(r, 0);
    assert_eq!(
        (
            d.multispeed,
            d.optimize_pulse,
            d.optimize_realtime,
            d.clock_hz,
            d.finevibrato
        ),
        (16, 1, 1, 0, 2)
    );
    // In range, kept: multispeed 8 (refused only by the device's caps) and 0 (25 Hz).
    for ms in [0u8, 8, 16] {
        let mut c = b;
        c[3] = ms;
        assert_eq!(decode(&c).1.multispeed, ms);
    }
    // No meaning: the NTSC byte above 1, a SID model above 1, an unknown player flag.
    for (at, v) in [(4usize, 2u8), (22, 2), (17, 0x80), (16, 0x80)] {
        let mut c = b;
        c[at] = v;
        assert_eq!(decode(&c).0, -1, "byte {at} = {v:#x}");
    }
}

/// Orderlists GoatTracker loads and plays though they break readme 3.1 (a transpose after a
/// repeat, a last entry that is not a pattern) or hold a mono restart position at or past
/// their entries: SIDOU's reader keeps them as they are, as GoatTracker does, and the song
/// saves to what GoatTracker saves. The C player plays them as player.s does (M2c Task 4);
/// the edit primitives never make one, and an entry write on one is refused.
#[test]
fn orderlists_breaking_the_rules_load_as_goattracker_loads_them() {
    let mut spec = SngSpec::new();
    spec.orders = vec![vec![
        (vec![0xd2, 0xe3, 0], 0),
        (vec![1, 0xe2], 0),
        (vec![2], 7),
    ]];
    let file = spec.encode();
    let mut s = Song::device();
    s.read(&file, 4).unwrap();
    assert_eq!(s.order(0, 0).unwrap(), (vec![0xd2, 0xe3, 0], 0));
    assert_eq!(s.order(0, 1).unwrap(), (vec![1, 0xe2], 0));
    assert_eq!(s.order(0, 2).unwrap(), (vec![2], 7));
    assert_eq!(s.write(false), gt2_ref::canonical(&file).unwrap());
    let mut e = gt2_edit::default();
    assert_eq!(
        unsafe { gt2_set_order(s.ptr_mut(), 0, 0, 2, 1, &mut e) },
        GT2_ERR_ORDER_RULE
    );
    // The restart position is set back in range by an edit that writes it.
    assert_eq!(
        unsafe { gt2_set_order(s.ptr_mut(), 0, 2, 1, 0, &mut e) },
        GT2_OK
    );
    unsafe { gt2_edit_commit(s.ptr_mut(), &e) };
    assert_eq!(s.order(0, 2).unwrap(), (vec![2], 0));
}

/// A review's repro: the song filled to its cap, a row deleted in pattern 3 and left
/// waiting for its swap, then a row deleted in pattern 4. The second makes the song smaller,
/// so it is never FULL: it waits (PENDING) or goes ahead.
#[test]
fn a_waiting_rebuild_never_makes_a_shrinking_one_full() {
    let mut s = Song::device();
    let mut r = GT2_OK;
    while r == GT2_OK {
        let mut n = 0;
        r = rebuild(&mut s, &mut |p, e| unsafe {
            gt2_pattern_add(p, 128, &mut n, e)
        });
    }
    assert_eq!(r, GT2_ERR_FULL);
    // Fill the last bytes with a smaller pattern, so the cap is reached exactly or nearly.
    for rows in (1..128).rev() {
        let mut n = 0;
        let _ = rebuild(&mut s, &mut |p, e| unsafe {
            gt2_pattern_add(p, rows, &mut n, e)
        });
    }
    let p = s.ptr_mut();
    let (mut a, mut b) = (gt2_edit::default(), gt2_edit::default());
    unsafe {
        assert_eq!(gt2_pattern_delete_row(p, 3, 0, &mut a), GT2_OK);
        let r = gt2_pattern_delete_row(p, 4, 0, &mut b);
        assert!(r == GT2_OK || r == GT2_ERR_PENDING, "{r}");
        if r == GT2_OK {
            gt2_edit_commit(p, &b);
        }
        gt2_edit_commit(p, &a);
        // Released, the second goes ahead (after compaction if it must).
    }
    let r = rebuild(&mut s, &mut |p, e| unsafe {
        gt2_pattern_delete_row(p, 4, 0, e)
    });
    assert_eq!(r, GT2_OK);
    check_arena(&s);
}

/// The device's multispeed refusal names the file's value, not the one gt2reloc's clamp
/// makes of it.
#[test]
fn the_multispeed_refusal_shows_the_files_value() {
    let mut s = Song::device();
    let mut o = gt2_options::default();
    unsafe { gt2_options_default(&mut o) };
    o.multispeed = 20;
    assert_eq!(unsafe { song_ext_set_options(s.ptr_mut(), &o) }, 0);
    let file = s.write(true);
    let mut t = Song::device();
    assert_eq!(
        t.read(&file, 4).unwrap_err().msg,
        "SNG: MULTISPEED 20 (MAX 4)"
    );
    let mut h = Song::host();
    h.read(&file, 0).unwrap();
    let mut d = gt2_options::default();
    assert_eq!(unsafe { song_ext_get_options(h.ptr(), &mut d) }, 1);
    assert_eq!(d.multispeed, 16);
}
