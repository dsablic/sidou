// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

const RING: u32 = SCOPE_RING;
const D: usize = SCOPE_DECIMATE as usize;
const RATE: u32 = 44_118;

fn new_scope() -> Box<scope> {
    let mut sc = Box::<scope>::default();
    unsafe { scope_init(&mut *sc) };
    sc
}

fn produce_point(sc: &mut scope, k: u32) {
    let slot = (sc.head % RING) as usize;
    sc.point[slot] = scope_point {
        wave: [k as u8, (k >> 8) as u8, 7],
        phase: [0, 0, 0],
    };
    sc.head = sc.head.wrapping_add(1);
}

fn read(sc: &scope, since: u32, n: u32) -> Vec<u32> {
    let mut out = vec![scope_point::default(); n as usize];
    let got = unsafe { scope_read(sc, since, out.as_mut_ptr(), n) };
    out[..got as usize]
        .iter()
        .map(|p| u32::from(p.wave[0]) | u32::from(p.wave[1]) << 8)
        .collect()
}

#[test]
fn points_come_out_oldest_first_and_only_the_newest() {
    let mut sc = new_scope();
    assert_eq!(read(&sc, 0, 10), [] as [u32; 0], "nothing written");
    for k in 0..10 {
        produce_point(&mut sc, k);
    }
    assert_eq!(read(&sc, 0, 10), (0..10).collect::<Vec<_>>());
    assert_eq!(read(&sc, 0, 4), [6, 7, 8, 9], "the newest four");
    assert_eq!(
        read(&sc, 0, 50),
        (0..10).collect::<Vec<_>>(),
        "no more than written"
    );
    assert_eq!(read(&sc, 7, 50), [7, 8, 9], "only those since a head");
}

#[test]
fn the_ring_wraps_and_the_oldest_points_are_dropped() {
    let mut sc = new_scope();
    for k in 0..1000 {
        produce_point(&mut sc, k);
    }
    let got = read(&sc, 0, RING);
    assert_eq!(
        got.len() as u32,
        SCOPE_READ_MAX,
        "a copy keeps a margin from the producer"
    );
    let want: Vec<u32> = (1000 - SCOPE_READ_MAX..1000).collect();
    assert_eq!(got, want, "in order across the wrap");
    sc.head = u32::MAX - 5;
    for k in 0..20 {
        produce_point(&mut sc, k);
    }
    assert_eq!(read(&sc, u32::MAX - 5, 30), (0..20).collect::<Vec<_>>());
}

#[test]
fn a_copy_the_producer_came_round_to_is_not_intact() {
    let intact = |oldest: u32, head: u32| unsafe { scope_intact(oldest, head) } != 0;
    assert!(intact(100, 100));
    assert!(
        intact(100, 100 + RING - 1),
        "the producer writes one behind the copy"
    );
    assert!(
        !intact(100, 100 + RING),
        "the producer wrote over the oldest point"
    );
    assert!(!intact(100, 100 + 5 * RING));
    assert!(intact(u32::MAX - 3, 10), "across the head's wrap");
}

fn chip() -> Box<sid> {
    let mut s = Box::<sid>::default();
    unsafe {
        sid_init(&mut *s, SID_MODEL_6581, SID_CLOCK_PAL, RATE);
        for (v, (freq, ctrl)) in [(0x1d45u16, 0x21u8), (0x0e00, 0x41), (0x2000, 0x81)]
            .into_iter()
            .enumerate()
        {
            let b = 7 * v as u8;
            sid_write(&mut *s, b, freq as u8);
            sid_write(&mut *s, b + 1, (freq >> 8) as u8);
            sid_write(&mut *s, b + 3, 0x08);
            sid_write(&mut *s, b + 5, 0x09);
            sid_write(&mut *s, b + 6, 0xa6);
            sid_write(&mut *s, b + 4, ctrl);
        }
        sid_write(&mut *s, 0x18, 0x0f);
    }
    s
}

#[test]
fn the_tap_takes_a_point_every_decimate_samples_and_leaves_the_audio_alone() {
    const BLOCK: usize = 128;
    let mut plain = chip();
    let mut tapped = chip();
    let mut probe = chip();
    let mut sc = new_scope();
    sc.on = 1;
    let mut want_points = Vec::new();
    let mut a = vec![0i16; BLOCK];
    let mut b = vec![0i16; BLOCK];
    let mut c = vec![0i16; D];
    for block in 0..40 {
        unsafe {
            sid_render(&mut *plain, a.as_mut_ptr(), BLOCK as u32);
            scope_render(&mut *sc, &mut *tapped, b.as_mut_ptr(), BLOCK as u32);
        }
        assert_eq!(a, b, "block {block}: the tap changed the audio");
        for _ in 0..BLOCK / D {
            unsafe { sid_render(&mut *probe, c.as_mut_ptr(), D as u32) };
            want_points.push(scope_point {
                wave: std::array::from_fn(|v| (probe.osc[v].out >> 4) as u8),
                phase: std::array::from_fn(|v| (probe.osc[v].acc >> 16) as u8),
            });
        }
        for v in 0..3 {
            assert_eq!(
                sc.env[v], tapped.env[v].env3,
                "block {block}: voice {v}'s ENV3"
            );
            assert_eq!(u32::from(sc.state[v]), tapped.env[v].state);
            assert_eq!(sc.sustain[v], tapped.env[v].sustain);
        }
    }
    assert_eq!(sc.head as usize, want_points.len());
    let n = SCOPE_READ_MAX as usize;
    let mut got = vec![scope_point::default(); n];
    let copied = unsafe { scope_read(&*sc, 0, got.as_mut_ptr(), n as u32) };
    assert_eq!(copied as usize, n);
    let tail = &want_points[want_points.len() - n..];
    assert!(
        got.iter()
            .zip(tail)
            .all(|(g, w)| g.wave == w.wave && g.phase == w.phase),
        "the points are not the voices every {D} samples"
    );
    assert!(sc.env.iter().all(|&e| e > 0));
    assert!(got.windows(2).any(|w| w[0].wave[0] != w[1].wave[0]));

    sc.on = 0;
    let head = sc.head;
    unsafe {
        sid_render(&mut *plain, a.as_mut_ptr(), BLOCK as u32);
        scope_render(&mut *sc, &mut *tapped, b.as_mut_ptr(), BLOCK as u32);
    }
    assert_eq!(a, b);
    assert_eq!(sc.head, head, "points taken while off");
}

#[test]
fn the_peaks_rise_with_the_strided_frames_and_clear_when_taken() {
    let mut sc = new_scope();
    let stride = SCOPE_PEAK_STRIDE as usize;
    let mut lr = stereo_block();
    lr[left(stride)] = -1200;
    lr[right(3 * stride)] = 900;
    let between_strides = stride + 1;
    lr[left(between_strides)] = 30_000;
    unsafe { scope_peaks(&mut *sc, lr.as_ptr(), BLOCK_FRAMES) };
    let mut out = [0u16; 2];
    unsafe { scope_take_peaks(&mut *sc, out.as_mut_ptr()) };
    assert_eq!(out, [0, 0], "off: nothing");
    sc.on = 1;
    unsafe { scope_peaks(&mut *sc, lr.as_ptr(), BLOCK_FRAMES) };
    lr[left(stride)] = 100;
    unsafe { scope_peaks(&mut *sc, lr.as_ptr(), BLOCK_FRAMES) };
    unsafe { scope_take_peaks(&mut *sc, out.as_mut_ptr()) };
    assert_eq!(out, [1200, 900], "the largest since the last take");
    unsafe { scope_take_peaks(&mut *sc, out.as_mut_ptr()) };
    assert_eq!(out, [0, 0], "cleared");
    lr[0] = i16::MIN;
    unsafe { scope_peaks(&mut *sc, lr.as_ptr(), BLOCK_FRAMES) };
    unsafe { scope_take_peaks(&mut *sc, out.as_mut_ptr()) };
    assert_eq!(out[0], 32768, "the most negative sample");
}

const BLOCK_FRAMES: u32 = 128;

fn stereo_block() -> Vec<i16> {
    vec![0i16; 2 * BLOCK_FRAMES as usize]
}

fn left(frame: usize) -> usize {
    2 * frame
}

fn right(frame: usize) -> usize {
    2 * frame + 1
}

unsafe extern "C" fn block_once(ctx: *mut std::ffi::c_void) {
    unsafe {
        scope_set_race_hook(None, std::ptr::null_mut());
        let sc = ctx.cast::<scope>();
        let stride = SCOPE_PEAK_STRIDE as usize;
        let mut lr = stereo_block();
        lr[left(stride)] = 2000;
        scope_peaks(sc, lr.as_ptr(), BLOCK_FRAMES);
    }
}

#[test]
fn a_peak_raised_during_a_take_comes_out_of_this_take_or_the_next() {
    let mut sc = new_scope();
    sc.on = 1;
    let stride = SCOPE_PEAK_STRIDE as usize;
    let mut lr = stereo_block();
    lr[left(stride)] = 500;
    unsafe { scope_peaks(&mut *sc, lr.as_ptr(), BLOCK_FRAMES) };
    let mut first = [0u16; 2];
    let mut second = [0u16; 2];
    let p: *mut scope = &mut *sc;
    unsafe {
        scope_set_race_hook(Some(block_once), p.cast());
        scope_take_peaks(p, first.as_mut_ptr());
        scope_set_race_hook(None, std::ptr::null_mut());
        scope_take_peaks(p, second.as_mut_ptr());
    }
    assert!(
        first[0] == 2000 || second[0] == 2000,
        "the interrupt's block was lost: {first:?} then {second:?}"
    );
    assert!(first[0] >= 500, "the block before the take: {first:?}");
}

fn trace(points: &[scope_point], voice: u32, by: u32, cycles: u32) -> Vec<u8> {
    let mut out = vec![0u8; SCOPE_W as usize];
    unsafe {
        scope_trace(
            points.as_ptr(),
            points.len() as u32,
            voice,
            by,
            cycles,
            out.as_mut_ptr(),
        )
    };
    out
}

fn point(wave: u8, phase: u8) -> scope_point {
    scope_point {
        wave: [wave, 0, 0],
        phase: [phase, 0, 255 - phase],
    }
}

#[test]
fn the_trace_folds_the_points_by_phase() {
    let w = SCOPE_W as usize;
    assert!(
        trace(&[], 0, 0, 2).iter().all(|&y| y == 128),
        "no points: flat"
    );
    assert!(
        trace(&[point(40, 77)], 0, 0, 2).iter().all(|&y| y == 40),
        "one point: its value everywhere"
    );
    let mut pts: Vec<scope_point> = (0..64u32)
        .map(|k| point((k * 4) as u8, (k * 4) as u8))
        .collect();
    pts.reverse();
    pts.swap(3, 40);
    for cycles in [1u32, 2, 4] {
        let t = trace(&pts, 0, 0, cycles);
        let per = w / cycles as usize;
        for (x, &y) in t.iter().enumerate() {
            let phase = (x % per) * 256 / per;
            let past_the_last_point = phase > 248;
            if past_the_last_point {
                continue;
            }
            let want = phase as i32;
            assert!(
                (i32::from(y) - want).abs() <= 8,
                "{cycles} cycles, x {x}: {y} for phase {phase}"
            );
        }
    }
    let t = trace(&pts, 0, 2, 1);
    for x in [w / 8, w / 2, 3 * w / 4] {
        let want = 255 - (x * 256 / w) as i32;
        assert!(
            (i32::from(t[x]) - want).abs() <= 8,
            "by voice 2, x {x}: {} for {want}",
            t[x]
        );
    }
    let t = trace(&[point(10, 128), point(200, 128), point(201, 129)], 0, 0, 1);
    assert_eq!(t[w / 2], 137, "points in one column: their mean");
}

#[test]
fn columns_between_points_take_a_line_round_the_period() {
    let w = SCOPE_W as usize;
    let t = trace(&[point(0, 0), point(160, 128)], 0, 0, 1);
    assert_eq!(t[0], 0);
    assert_eq!(t[w / 2], 160);
    assert_eq!(t[w / 4], 80, "half way up");
    assert_eq!(t[3 * w / 4], 80, "half way down, across the wrap");
    assert!(t
        .windows(2)
        .all(|p| (i32::from(p[0]) - i32::from(p[1])).abs() <= 2));
}

#[test]
fn the_meter_is_logarithmic() {
    let m = |a: u32| unsafe { scope_meter(a) };
    assert_eq!(m(32768), SCOPE_METER_MAX);
    assert_eq!(m(40000), SCOPE_METER_MAX, "clamped");
    assert_eq!(m(16384), SCOPE_METER_MAX - 8, "-6 dB, a cell lower");
    assert_eq!(m(64), 0, "-54 dB");
    assert_eq!(m(0), 0);
    let mut last = 0;
    for a in 1..=32768 {
        let v = m(a);
        assert!(v >= last, "falls at {a}");
        last = v;
    }
}
