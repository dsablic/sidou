// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! MASTER as a digital gain on the final output (`app/master_gain.c`): the gain's endpoints,
//! unity untouched, the ramp, silence staying silent, and what the move from the chip's
//! volume register costs live play.

use sidou_sys::*;

fn gain(pot: u32) -> u32 {
    unsafe { master_gain_of(pot, LIVE_MASTER_MAX) }
}

fn apply(lr: &[i16], from: u32, to: u32) -> Vec<i16> {
    assert_eq!(lr.len(), 2 * MASTER_BLOCK as usize);
    let mut out = lr.to_vec();
    unsafe { master_gain_apply(out.as_mut_ptr(), MASTER_BLOCK_LOG2, from, to) };
    out
}

fn block(f: impl Fn(usize) -> i32) -> Vec<i16> {
    (0..2 * MASTER_BLOCK as usize)
        .map(|i| f(i) as i16)
        .collect()
}

#[test]
fn the_gain_spans_zero_to_unity_linearly() {
    assert_eq!(gain(0), 0);
    assert_eq!(gain(LIVE_MASTER_MAX), MASTER_UNITY);
    assert_eq!(gain(LIVE_MASTER_MAX + 500), MASTER_UNITY, "clamped");
    let mut last = 0;
    for p in 1..=LIVE_MASTER_MAX {
        let g = gain(p);
        assert!(g >= last, "monotonic at {p}");
        last = g;
        let ideal = f64::from(p) * f64::from(MASTER_UNITY) / f64::from(LIVE_MASTER_MAX);
        assert!(
            (f64::from(g) - ideal).abs() <= 0.5,
            "{p}: {g} against {ideal}"
        );
    }
}

/// Full MASTER leaves the output alone, bit for bit: the stage does not run.
#[test]
fn unity_changes_nothing() {
    let x = block(|i| (i as i32 * 509 % 65536) - 32768);
    assert_eq!(apply(&x, MASTER_UNITY, MASTER_UNITY), x);
}

#[test]
fn a_constant_gain_scales_every_sample_rounded_to_nearest() {
    let x = block(|i| (i as i32 * 509 % 65536) - 32768);
    for pot in [0, 1, 100, 256, 511, 900, 1022] {
        let g = gain(pot);
        let got = apply(&x, g, g);
        for (i, (&a, &b)) in x.iter().zip(&got).enumerate() {
            let ideal = f64::from(a) * f64::from(g) / f64::from(MASTER_UNITY);
            assert!(
                (f64::from(b) - ideal).abs() <= 0.5,
                "pot {pot}, sample {i}: {a} -> {b}, ideal {ideal}"
            );
        }
    }
    assert!(
        apply(&x, 0, 0).iter().all(|&v| v == 0),
        "MASTER 0 is silence"
    );
}

/// 0 stays 0 at every gain, so a silent tail stays exactly silent.
#[test]
fn silence_stays_silent() {
    let z = block(|_| 0);
    for (from, to) in [
        (0, MASTER_UNITY),
        (MASTER_UNITY, 1),
        (12345, 12345),
        (7, 31000),
    ] {
        assert_eq!(apply(&z, from, to), z, "{from} -> {to}");
    }
}

/// A change ramps over the block, sample i at from + (to - from) (i + 1) / 128, the block ending
/// on `to`: a DC input shows the ramp, and its step is the one gain step per sample.
#[test]
fn a_change_ramps_linearly_over_the_block() {
    let x = block(|_| 20000);
    let (from, to) = (gain(1023), gain(256));
    let got = apply(&x, from, to);
    for f in 0..MASTER_BLOCK as usize {
        let g = i64::from(from) + (((i64::from(to) - i64::from(from)) * (f as i64 + 1)) >> 7);
        let ideal = 20000.0 * g as f64 / f64::from(MASTER_UNITY);
        for c in 0..2 {
            assert!(
                (f64::from(got[2 * f + c]) - ideal).abs() <= 1.0,
                "frame {f}: {} against {ideal}",
                got[2 * f + c]
            );
        }
    }
    let last = got[2 * 127];
    assert!(
        (i32::from(last) - 20000 * 256 / 1023).abs() <= 1,
        "ends on the new gain"
    );
    let step = got
        .chunks(2)
        .map(|c| i32::from(c[0]))
        .collect::<Vec<_>>()
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .max()
        .unwrap();
    assert!(step <= 1 + 20000 * (gain(1023) - gain(256)) as i32 / 128 / MASTER_UNITY as i32 + 1);
}

fn chip() -> Box<sid> {
    let mut s = Box::<sid>::default();
    unsafe { sid_init(&mut *s, SID_MODEL_6581, SID_CLOCK_PAL, 44100) };
    s
}

/// A held chord (a saw, a pulse and a triangle) at chip volume `volume`, rendered in blocks of
/// 128 for 0.5 s: the mono samples as the firmware's blocks have them.
fn chord(volume: u8) -> Vec<i16> {
    let mut s = chip();
    let w = |s: &mut sid, r: u8, v: u8| unsafe { sid_write(s, r, v) };
    for (r, v) in [
        (0x00, 0x45),
        (0x01, 0x1d),
        (0x05, 0x00),
        (0x06, 0xf0),
        (0x04, 0x21),
        (0x07, 0x00),
        (0x08, 0x16),
        (0x0a, 0x00),
        (0x0b, 0x0c),
        (0x0c, 0x00),
        (0x0d, 0xf0),
        (0x0b, 0x41),
        (0x0e, 0x00),
        (0x0f, 0x2a),
        (0x13, 0x00),
        (0x14, 0xf0),
        (0x12, 0x11),
        (0x18, volume),
    ] {
        w(&mut s, r, v);
    }
    let mut out = vec![0i16; 22050];
    for c in out.chunks_mut(128) {
        unsafe { sid_render(&mut *s, c.as_mut_ptr(), c.len() as u32) };
    }
    out
}

/// What the move from the volume register to a digital gain does to live play. Before, MASTER
/// was the register's 4 bits, round(pot * 15 / 1023) of 15; now it is pot / 1023, continuous. The
/// chip's volume is a linear scale of its output (the thump, which would add a DC, is 0 by
/// default), so the two outputs differ by the gain's quantisation only: at most half a register
/// step, 1/30 of full scale, of the loudest sample, plus the rounding of the integer gain stage
/// (a LSB or two). At full MASTER the register is 15 as before and the stage is not there: the
/// output is identical, bit for bit.
#[test]
fn live_play_sounds_as_before_to_within_the_volume_registers_quantisation() {
    let full = chord(0x0f);
    let peak = full.iter().map(|&v| i32::from(v).abs()).max().unwrap();
    assert!(peak > 3000, "a chord that sounds: {peak}");
    let tolerance = peak / 30 + 2;
    let mut worst_seen = 0;
    for pot in (0..=LIVE_MASTER_MAX).step_by(7).chain([LIVE_MASTER_MAX]) {
        let volume = ((pot * 15 + LIVE_MASTER_MAX / 2) / LIVE_MASTER_MAX) as u8;
        let before = chord(volume);
        let g = gain(pot);
        let mut after = Vec::with_capacity(full.len());
        for c in full.chunks(128).filter(|c| c.len() == 128) {
            let lr: Vec<i16> = c.iter().flat_map(|&v| [v, v]).collect();
            let scaled = apply(&lr, g, g);
            after.extend(scaled.chunks(2).map(|f| f[0]));
        }
        for (i, (&a, &b)) in after.iter().zip(&before).enumerate() {
            let d = (i32::from(a) - i32::from(b)).abs();
            worst_seen = worst_seen.max(d);
            assert!(
                d <= tolerance,
                "pot {pot} (register {volume}), sample {i}: {a} against {b}, tolerance {tolerance}"
            );
        }
        if pot == LIVE_MASTER_MAX {
            assert_eq!(volume, 15);
            assert_eq!(
                after,
                before[..after.len()],
                "full MASTER is the chip's output"
            );
        }
    }
    println!("worst difference {worst_seen} LSB of a {peak} peak (tolerance {tolerance})");
}

/// MASTER's gain scales a song's own volume writes: a fade to volume 8 reaches half of what
/// the register fade alone gives, at any MASTER: the two compose multiplicatively.
#[test]
fn a_songs_volume_writes_compose_with_master() {
    let (loud, soft) = (chord(0x0f), chord(0x08));
    let peak = |v: &[i16]| v.iter().map(|&x| i32::from(x).abs()).max().unwrap();
    let g = gain(256);
    let scale = |v: &[i16]| -> Vec<i16> {
        let mut out = Vec::new();
        for c in v.chunks(128).filter(|c| c.len() == 128) {
            let lr: Vec<i16> = c.iter().flat_map(|&s| [s, s]).collect();
            out.extend(apply(&lr, g, g).chunks(2).map(|f| f[0]));
        }
        out
    };
    let ratio_chip = f64::from(peak(&soft)) / f64::from(peak(&loud));
    let ratio_master = f64::from(peak(&scale(&soft))) / f64::from(peak(&scale(&loud)));
    assert!(
        (ratio_chip - 8.0 / 15.0).abs() < 0.01,
        "the register is linear: {ratio_chip}"
    );
    assert!(
        (ratio_master - ratio_chip).abs() < 0.01,
        "MASTER keeps the song's fade relative: {ratio_master} against {ratio_chip}"
    );
    let scaled_peak = f64::from(peak(&scale(&soft)));
    assert!(
        (scaled_peak / f64::from(peak(&soft)) - f64::from(g) / f64::from(MASTER_UNITY)).abs()
            < 0.01
    );
}
