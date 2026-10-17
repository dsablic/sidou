// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;
use sidou_tests::song::*;
use std::ffi::c_void;

struct Input<'a> {
    bytes: &'a [u8],
    step_bytes: usize,
    calls: usize,
}
unsafe extern "C" fn read(ctx: *mut c_void, at: u32, buf: *mut u8, len: u32) -> u32 {
    let input = unsafe { &mut *ctx.cast::<Input<'_>>() };
    let at = at as usize;
    let n = input
        .bytes
        .len()
        .saturating_sub(at)
        .min(len as usize)
        .min(37);
    if n != 0 {
        unsafe { std::ptr::copy_nonoverlapping(input.bytes.as_ptr().add(at), buf, n) };
    }
    input.step_bytes += n;
    input.calls += 1;
    n as u32
}
struct Output {
    bytes: Vec<u8>,
    calls: usize,
}
unsafe extern "C" fn write(ctx: *mut c_void, buf: *const u8, len: u32) -> u32 {
    let out = unsafe { &mut *ctx.cast::<Output>() };
    out.calls += 1;
    if out.calls % 3 == 0 {
        return 0;
    }
    let n = len.min(17);
    out.bytes
        .extend_from_slice(unsafe { std::slice::from_raw_parts(buf, n as usize) });
    n
}
fn load(song: &mut Song, bytes: &[u8]) -> i32 {
    let mut w = gt2_stream::default();
    let mut input = Input {
        bytes,
        step_bytes: 0,
        calls: 0,
    };
    let caps = gt2_caps { max_multispeed: 4 };
    unsafe {
        gt2_stream_read_begin(
            &mut w,
            song.ptr_mut(),
            Some(read),
            (&mut input as *mut Input<'_>).cast(),
            &caps,
        )
    };
    for _ in 0..20000 {
        input.step_bytes = 0;
        let status = unsafe { gt2_stream_read_step(&mut w) };
        assert!(
            input.step_bytes <= 520,
            "unbounded read: {}",
            input.step_bytes
        );
        if status != 0 {
            return status;
        }
    }
    panic!("reader did not terminate")
}
fn save(song: &Song, trailer: bool) -> Vec<u8> {
    let mut w = gt2_stream::default();
    let mut out = Output {
        bytes: Vec::new(),
        calls: 0,
    };
    unsafe { gt2_stream_write_begin(&mut w, song.ptr(), i32::from(trailer)) };
    let mut buf = [0; 256];
    for _ in 0..100000 {
        let status = unsafe { gt2_stream_write_step(&mut w, buf.as_mut_ptr(), buf.len() as u32) };
        assert!(status >= 0);
        if status == 0 {
            return out.bytes;
        }
        let mut at = 0;
        while at < status as usize {
            at += unsafe {
                write(
                    (&mut out as *mut Output).cast(),
                    buf.as_ptr().add(at),
                    status as u32 - at as u32,
                )
            } as usize;
        }
    }
    panic!("writer did not terminate")
}

#[test]
fn streamed_canonical_songs_match_synchronous_with_backpressure() {
    let mut rng = Rng(0x57ea_0001);
    for seed in 0..80 {
        let file = random_canonical(&mut rng, seed % 7 == 0).encode();
        let mut sync = Song::device();
        sync.read(&file, 4).unwrap();
        let mut streamed = Song::device();
        assert_eq!(load(&mut streamed, &file), 1, "seed {seed}");
        assert_eq!(streamed.write(true), sync.write(true), "seed {seed}");
        assert_eq!(save(&sync, true), sync.write(true), "seed {seed}");
        assert_eq!(save(&sync, false), sync.write(false), "seed {seed}");
        let extended = sync.write(true);
        assert_eq!(load(&mut streamed, &extended), 1);
        assert_eq!(streamed.write(true), extended);
    }
}

#[test]
fn truncated_streams_fail_and_bad_trailers_are_dropped() {
    let song = Song::device();
    let file = song.write(true);
    for end in [0, 3, 100, 110, 150] {
        assert_eq!(load(&mut Song::device(), &file[..end]), -1);
    }
    let mut damaged = file.clone();
    *damaged.last_mut().unwrap() ^= 1;
    let mut streamed = Song::device();
    assert_eq!(load(&mut streamed, &damaged), 1);
    assert_eq!(streamed.write(false), song.write(false));
    let w = gt2_stream::default();
    assert!(!unsafe { gt2_stream_info(&w) }.is_null());
}

#[test]
fn every_bundled_example_matches_the_synchronous_reader() {
    for (name, file) in gt2_ref::examples() {
        let mut sync = Song::host();
        let result = sync.read(&file, 4);
        let mut streamed = Song::host();
        let status = load(&mut streamed, &file);
        assert_eq!(status == 1, result.is_ok(), "{name}: {result:?}");
        if result.is_ok() {
            assert_eq!(streamed.write(true), sync.write(true), "{name}");
            assert_eq!(save(&sync, true), sync.write(true), "{name}");
        }
    }
}

#[test]
fn legacy_versions_convert_and_trim_like_the_synchronous_reader() {
    let mut rng = Rng(0x01de_1234);
    for seed in 0..160 {
        let mut spec = random_canonical(&mut rng, seed % 11 == 0);
        spec.ident = *[b"GTS2", b"GTS3", b"GTS4", b"GTS5"][seed % 4];
        if spec.ident == *b"GTS2" {
            spec.tables.truncate(3);
        }
        let file = spec.encode();
        let mut sync = Song::device();
        let result = sync.read(&file, 4);
        let mut streamed = Song::device();
        assert_eq!(
            load(&mut streamed, &file) == 1,
            result.is_ok(),
            "seed {seed}"
        );
        if result.is_ok() {
            assert_eq!(streamed.write(true), sync.write(true), "seed {seed}");
        }
    }
}

#[test]
fn trailer_chunks_crc_and_multispeed_caps_match() {
    let mut sync = Song::device();
    let mut opts = gt2_options::default();
    unsafe { gt2_options_default(&mut opts) };
    opts.multispeed = 3;
    assert_eq!(unsafe { song_ext_set_options(sync.ptr_mut(), &opts) }, 0);
    let unknown = vec![0x71; 3000];
    assert_eq!(
        unsafe {
            song_ext_set(
                sync.ptr_mut(),
                tag(b"TEST"),
                unknown.as_ptr(),
                unknown.len() as u32,
            )
        },
        0
    );
    let lock = song_lock {
        pattern: 0,
        row: 0,
        param: 0,
        value: 123,
    };
    assert_eq!(unsafe { song_ext_set_locks(sync.ptr_mut(), &lock, 1) }, 0);
    let file = sync.write(true);
    assert_eq!(save(&sync, true), file);
    for damage in [None, Some(4), Some(file.len() - 1)] {
        let mut input = file.clone();
        if let Some(at) = damage {
            input[at] ^= 1;
        }
        let mut expected = Song::device();
        expected.read(&input, 4).unwrap();
        let mut actual = Song::device();
        assert_eq!(load(&mut actual, &input), 1);
        assert_eq!(actual.write(true), expected.write(true));
    }
    opts.multispeed = 8;
    assert_eq!(unsafe { song_ext_set_options(sync.ptr_mut(), &opts) }, 0);
    assert_eq!(load(&mut Song::device(), &sync.write(true)), -1);
    let mut broken = sync.write(true);
    *broken.last_mut().unwrap() ^= 1;
    assert_eq!(load(&mut Song::device(), &broken), 1);
}

#[test]
fn false_stereo_guess_and_missing_patterns_follow_the_ranked_parse() {
    let mut spec = SngSpec::new();
    let mut instr = [0; 25];
    instr[..8].copy_from_slice(&[255, 0, 1, 255, 0, 1, 255, 0]);
    spec.instr = vec![instr];
    spec.patterns.truncate(1);
    spec.patterns[0].0[0] = [0x60, 1, 0, 0];
    let file = spec.encode();
    assert_eq!(determinechannels(&file), 6);
    let mut sync = Song::device();
    assert_eq!(sync.read(&file, 4).unwrap().channels, 3);
    let extended = sync.write(true);
    for input in [&file, &extended] {
        let mut streamed = Song::device();
        assert_eq!(load(&mut streamed, input), 1);
        assert_eq!(streamed.write(true), extended);
    }
}

#[test]
fn corrupt_inputs_never_turn_a_refusal_into_a_success() {
    let file = SngSpec::new().encode();
    let mut rng = Rng(0xbadd_1978);
    for _ in 0..500 {
        let mut damaged = file.clone();
        let at = rng.below(damaged.len() as u32) as usize;
        damaged[at] ^= (rng.below(255) + 1) as u8;
        let mut sync = Song::device();
        let result = sync.read(&damaged, 4);
        let mut streamed = Song::device();
        assert_eq!(
            load(&mut streamed, &damaged) == 1,
            result.is_ok(),
            "at {at}"
        );
        if result.is_ok() {
            assert_eq!(streamed.write(true), sync.write(true), "at {at}");
        }
    }
}

#[test]
fn writer_supports_one_byte_output_buffers() {
    let song = Song::device();
    let mut w = gt2_stream::default();
    unsafe { gt2_stream_write_begin(&mut w, song.ptr(), 1) };
    let mut out = Vec::new();
    let mut byte = 0;
    for _ in 0..2000 {
        let status = unsafe { gt2_stream_write_step(&mut w, &mut byte, 1) };
        assert!(status >= 0);
        if status == 0 {
            break;
        }
        out.push(byte);
    }
    assert_eq!(out, song.write(true));
}

#[test]
fn largest_patterns_stream_on_the_host_and_refuse_the_device_region() {
    let mut spec = SngSpec::new();
    spec.patterns = (0..GT2_MAX_PATTERNS)
        .map(|_| (vec![[0x60, 0, 0, 0]; GT2_MAX_ROWS as usize], [255, 0, 0, 0]))
        .collect();
    let file = spec.encode();
    let mut sync = Song::host();
    sync.read(&file, 4).unwrap();
    let mut streamed = Song::host();
    assert_eq!(load(&mut streamed, &file), 1);
    assert_eq!(streamed.write(true), sync.write(true));
    assert_eq!(save(&streamed, true), sync.write(true));
    assert_eq!(load(&mut Song::device(), &file), -1);
}
