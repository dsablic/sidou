// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

const FS: u32 = 44118;
const BLOCK: usize = 128;

fn chip(thump: u8) -> (Box<sid>, Box<sid_dac>) {
    let mut s = Box::<sid>::default();
    let mut dac = Box::<sid_dac>::default();
    let c = sid_character {
        profile: (SID_CHIP_6581_R4AR + 1) as u8,
        thump,
        ..Default::default()
    };
    unsafe {
        sid_init(&mut *s, SID_MODEL_6581, SID_CLOCK_PAL, FS);
        sid_character_apply(&mut *s, &c, &mut *dac);
    }
    (s, dac)
}

fn write(s: &mut sid, regs: &[(u8, u8)]) {
    for &(r, v) in regs {
        unsafe { sid_write(s, r, v) };
    }
}

fn render(s: &mut sid, out: &mut [i16]) {
    unsafe { sid_render(s, out.as_mut_ptr(), out.len() as u32) };
}

fn on_a_rail(v: i32) -> bool {
    v >= 32767 || v <= -32768
}

#[test]
fn idle_it_touches_nothing() {
    let lr: Vec<i16> = (0..2 * BLOCK)
        .map(|i| (i as i16).wrapping_mul(517) ^ 0x5a5a)
        .collect();
    let mut t = thump_out::default();
    let mut out = lr.clone();
    unsafe { thump_out_apply(&mut t, out.as_mut_ptr(), BLOCK as u32) };
    assert_eq!(out, lr);
    let (mut s, _dac) = chip(0);
    write(
        &mut s,
        &[(0x18, 0x0f), (0x05, 0x00), (0x06, 0xf0), (0x04, 0x41)],
    );
    let mut mono = vec![0i16; BLOCK];
    render(&mut s, &mut mono);
    write(&mut s, &[(0x18, 0x00), (0x04, 0x40), (0x04, 0x41)]);
    unsafe {
        thump_out_take(&mut t, 0, &mut *s);
        thump_out_apply(&mut t, out.as_mut_ptr(), BLOCK as u32);
    }
    assert_eq!(out, lr);
}

#[test]
fn a_step_decays_as_the_dc_blocker_plays_it_from_the_sample_it_was_written() {
    for late in [0usize, 50] {
        let (mut s, _dac) = chip(127);
        let mut mono = vec![0i16; BLOCK];
        write(&mut s, &[(0x18, 0x0f), (0x05, 0x00), (0x06, 0xf0)]);
        render(&mut s, &mut mono);
        unsafe { thump_out_drop(&mut *s) };
        let mut t = thump_out::default();
        render(&mut s, &mut mono[..late]);
        write(&mut s, &[(0x04, 0x01)]);
        render(&mut s, &mut mono[late..]);
        let step = s.thump_pending + s.thump_late;
        assert!(
            step > 1.5,
            "a note's thump at 100 %: {step} voice DAC units"
        );
        let (gain, r) = (s.output_gain, s.dc_r);
        let mut x = step * gain;
        let mut silent = 0;
        for block in 0..60 {
            let mut lr = vec![0i16; 2 * BLOCK];
            unsafe {
                thump_out_take(&mut t, 0, &mut *s);
                thump_out_apply(&mut t, lr.as_mut_ptr(), BLOCK as u32);
            }
            for f in 0..BLOCK {
                let want = if block == 0 && f < late {
                    0
                } else {
                    let v = (x + if x >= 0.0 { 0.5 } else { -0.5 }) as i16;
                    x *= r;
                    v
                };
                assert_eq!(
                    (lr[2 * f], lr[2 * f + 1]),
                    (want, want),
                    "late {late} block {block} sample {f}"
                );
            }
            if t.level[0] == 0.0 {
                silent = block;
                break;
            }
        }
        assert!(silent > 20, "idle after {silent} blocks");
    }
}

#[test]
fn the_thump_fits_into_the_room_the_output_leaves() {
    let one = |s: i16, x: f32| -> i16 {
        let mut t = thump_out {
            level: [x, 0.0],
            r: [1.0, 1.0],
            ..Default::default()
        };
        let mut lr = [s, s];
        unsafe { thump_out_apply(&mut t, lr.as_mut_ptr(), 1) };
        assert_eq!(lr[0], lr[1]);
        lr[0]
    };
    for sign in [1i32, -1] {
        for s in [0i32, 12000, 30000, 32000, 32765] {
            let room = THUMP_OUT_CEILING as i32 - s;
            let mut last = s;
            for k in 0..200 {
                let x = k as f32 * 400.0;
                let o = one((sign * s) as i16, sign as f32 * x) as i32 * sign;
                if x <= room as f32 / 2.0 {
                    assert_eq!(o, s + x as i32, "s {s} x {x}: untouched");
                }
                assert!(
                    o >= last && o <= THUMP_OUT_CEILING as i32,
                    "s {s} x {x}: {o} after {last}"
                );
                last = o;
            }
        }
        assert_eq!(
            one((sign * 20000) as i16, -sign as f32 * 1000.0),
            (sign * 19000) as i16
        );
    }
    assert_eq!(one(32767, 5000.0), 32767);
    assert_eq!(one(-32768, -5000.0), -32768);
    assert_eq!(one(-32768, 5000.0), -27768);
}

#[derive(Debug, Default, PartialEq)]
struct Rails {
    chips: Vec<usize>,
    mix: Vec<usize>,
    slots: Vec<usize>,
    out: Vec<usize>,
}

const CHORDS_BLOCKS: usize = 460;
const CHORD_EVERY_BLOCKS: usize = 28;
const RELEASE_AFTER_BLOCKS: usize = 21;
const SONG_CALL_SAMPLE: usize = BLOCK / 2;

fn chords(thump: u8, fx: Option<(u32, sid_slot_params)>) -> (Rails, Vec<i16>, Vec<i16>) {
    let (mut a, _da) = chip(thump);
    let (mut b, _db) = chip(thump);
    let setup = |f: [u16; 3]| -> Vec<(u8, u8)> {
        let mut regs = vec![(0x15, 0x00), (0x16, 0x40), (0x17, 0x07), (0x18, 0x1f)];
        for (v, hz) in f.iter().enumerate() {
            let base = (v * 7) as u8;
            regs.extend([
                (base, *hz as u8),
                (base + 1, (*hz >> 8) as u8),
                (base + 2, 0x00),
                (base + 3, 0x08),
                (base + 5, 0x00),
                (base + 6, 0xf0),
            ]);
        }
        regs
    };
    let gates = |on: u8| -> Vec<(u8, u8)> { (0..3).map(|v| (v * 7 + 4, 0x40 | on)).collect() };
    let chord = [0x1167, 0x15ed, 0x1a13];
    write(&mut a, &setup(chord));
    write(&mut b, &setup(chord));
    let mut pool = Box::<sid_slot_pool>::default();
    let mut slots = Box::<sid_slots>::default();
    unsafe { sid_slots_init(&mut *slots, FS, 0x51D0, &mut *pool) };
    if let Some((slot, p)) = fx {
        unsafe { sid_slots_set(&mut *slots, slot, &p) };
    }
    let mut t = thump_out::default();
    let (mut ma, mut mb) = (vec![0i16; BLOCK], vec![0i16; BLOCK]);
    let mut rails = Rails::default();
    let mut out = Vec::new();
    let mut chips = Vec::new();
    for block in 0..CHORDS_BLOCKS {
        let start = block % CHORD_EVERY_BLOCKS == 0;
        if block % CHORD_EVERY_BLOCKS == RELEASE_AFTER_BLOCKS {
            write(&mut a, &gates(0));
            write(&mut b, &gates(0));
        }
        if start {
            write(&mut b, &gates(1));
            render(&mut a, &mut ma[..SONG_CALL_SAMPLE]);
            write(&mut a, &gates(1));
            render(&mut a, &mut ma[SONG_CALL_SAMPLE..]);
        } else {
            render(&mut a, &mut ma);
        }
        render(&mut b, &mut mb);
        let mut lr = vec![0i16; 2 * BLOCK];
        for i in 0..BLOCK {
            let n = block * BLOCK + i;
            let (x, y) = (i32::from(ma[i]), i32::from(mb[i]));
            if on_a_rail(x) || on_a_rail(y) {
                rails.chips.push(n);
            }
            if on_a_rail(x + y) {
                rails.mix.push(n);
            }
            chips.push(ma[i]);
            chips.push(mb[i]);
        }
        let mut over = vec![0i16; BLOCK];
        unsafe {
            chip_mix(
                ma.as_mut_ptr(),
                mb.as_ptr(),
                over.as_mut_ptr(),
                BLOCK as u32,
                CHIP_MIX_FULL,
            );
            sid_slots_process_wide(
                &mut *slots,
                ma.as_ptr(),
                over.as_ptr(),
                lr.as_mut_ptr(),
                BLOCK as u32,
            );
        }
        for (i, &v) in lr.iter().enumerate() {
            if on_a_rail(i32::from(v)) {
                rails.slots.push(2 * block * BLOCK + i);
            }
        }
        unsafe {
            thump_out_take(&mut t, 0, &mut *a);
            thump_out_take(&mut t, 1, &mut *b);
            thump_out_apply(&mut t, lr.as_mut_ptr(), BLOCK as u32);
        }
        for (i, &v) in lr.iter().enumerate() {
            if on_a_rail(i32::from(v)) {
                rails.out.push(2 * block * BLOCK + i);
            }
        }
        out.extend(lr);
    }
    (rails, out, chips)
}

fn effects() -> Vec<(&'static str, u32, [f32; 3])> {
    vec![
        ("tape", SID_SLOT_DIST, [1.0, 0.0, 0.0]),
        ("overdrive", SID_SLOT_DIST, [0.0, 1.0, 1.0]),
        ("fuzz", SID_SLOT_DIST, [0.0, 0.0, 1.0]),
        ("crush", SID_SLOT_DIST, [1.0, 0.0, 0.0]),
        ("lofi", SID_SLOT_DIST, [0.0, 1.0, 1.0]),
        ("chorus", SID_SLOT_MOD, [0.0, 0.0, 1.0]),
        ("flanger", SID_SLOT_MOD, [1.0, 0.0, 1.0]),
        ("phaser", SID_SLOT_MOD, [1.0, 0.0, 1.0]),
        ("tremolo", SID_SLOT_MOD, [1.0, 0.0, 0.0]),
        ("autopan", SID_SLOT_MOD, [1.0, 0.0, 0.0]),
        ("wow", SID_SLOT_MOD, [1.0, 1.0, 0.0]),
        ("delay", SID_SLOT_FX, [1.0, 0.5, 0.0]),
        ("ping-pong", SID_SLOT_FX, [1.0, 0.5, 0.0]),
        ("reverb", SID_SLOT_FX, [1.0, 0.0, 0.0]),
        ("lush reverb", SID_SLOT_FX, [0.0, 0.0, 0.0]),
    ]
}

fn algorithm(name: &str) -> u32 {
    match name {
        "tape" => SID_DIST_TAPE,
        "overdrive" => SID_DIST_OVERDRIVE,
        "fuzz" => SID_DIST_FUZZ,
        "crush" => SID_DIST_CRUSH,
        "lofi" => SID_DIST_LOFI,
        "chorus" => SID_MOD_CHORUS,
        "flanger" => SID_MOD_FLANGER,
        "phaser" => SID_MOD_PHASER,
        "tremolo" => SID_MOD_TREMOLO,
        "autopan" => SID_MOD_AUTOPAN,
        "wow" => SID_MOD_WOW,
        "delay" => SID_FXSLOT_DELAY,
        "ping-pong" => SID_FXSLOT_PINGPONG,
        "reverb" => SID_FXSLOT_REVERB_LIGHT,
        _ => SID_FXSLOT_REVERB_LUSH,
    }
}

fn peak(x: &[i16]) -> i32 {
    x.iter().map(|&v| i32::from(v).abs()).max().unwrap_or(0)
}

#[test]
fn no_thump_clips_two_full_scale_chords_through_any_effect() {
    let (plain_rails, plain, plain_chips) = chords(0, None);
    assert_eq!(plain_rails, Rails::default(), "the chords alone are clean");
    let full = peak(&plain);
    assert!(
        full > SID_FX_LIMIT_KNEE as i32,
        "the chords reach {full}: past the limiter's knee"
    );
    let (rails, thumped, chips) = chords(127, None);
    assert_eq!(rails, Rails::default(), "THUMP 100 %, no effect");
    assert_eq!(chips, plain_chips, "the chips carry no thump of their own");
    let most = plain
        .iter()
        .zip(&thumped)
        .map(|(&x, &y)| (i32::from(y) - i32::from(x)).abs())
        .max()
        .unwrap();
    println!("the chords peak at {full}; the thump moves them by up to {most}");
    assert!(most > 3000, "the thump is there: {most}");
    let mut worst: Option<(&str, i32)> = None;
    for (name, slot, [amount, a, b]) in effects() {
        let p = sid_slot_params {
            algorithm: algorithm(name),
            amount,
            a,
            b,
            mix: 1.0,
        };
        let (plain_rails, plain, _) = chords(0, Some((slot, p)));
        let (rails, _, _) = chords(127, Some((slot, p)));
        println!(
            "{name}: peak {} ({:.2} dB), rails without the thump {}, with it {}",
            peak(&plain),
            20.0 * (f64::from(peak(&plain)) / f64::from(full)).log10(),
            plain_rails.out.len(),
            rails.out.len()
        );
        assert!(
            rails.chips == plain_rails.chips
                && rails.mix == plain_rails.mix
                && rails.slots == plain_rails.slots,
            "{name}: the thump reached the chips, the mix or the slots"
        );
        let added = rails
            .out
            .iter()
            .filter(|i| plain_rails.out.binary_search(i).is_err())
            .count();
        assert_eq!(added, 0, "{name}: the thump put {added} samples on a rail");
        if plain_rails == Rails::default() && worst.is_none_or(|w| peak(&plain) > w.1) {
            worst = Some((name, peak(&plain)));
        }
    }
    let (name, loudest) = worst.expect("an effect the chords alone do not clip");
    println!("the worst effect: {name}, peak {loudest}");
    assert!(
        loudest > full,
        "the worst effect raises the level: {loudest}"
    );
}
