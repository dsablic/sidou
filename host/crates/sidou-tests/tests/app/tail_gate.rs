// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::RELEASE_TO_ZERO_MS;

const BLOCK: usize = TAIL_BLOCK as usize;
const RATE: usize = 44100;
const HELD_SAMPLES: usize = RATE / 10;
const HALF_SECOND: usize = RATE / 2 / BLOCK * BLOCK;
const FADE_BLOCKS: usize = 1;
const SLACK_BLOCKS: usize = 1;

fn chip(model: sid_model) -> Box<sid> {
    let mut s = Box::<sid>::default();
    unsafe { sid_init(&mut *s, model, SID_CLOCK_PAL, RATE as u32) };
    s
}

fn write(s: &mut sid, reg: u8, value: u8) {
    unsafe { sid_write(s, reg, value) }
}

fn released_chord(model: sid_model, release: u8, ctrl1: u8, ctrl3: u8) -> Box<sid> {
    let mut s = chip(model);
    for (r, v) in [
        (0x18, 0x0f),
        (0x00, 0x45),
        (0x01, 0x1d),
        (0x05, 0x00),
        (0x06, 0xf0 | release),
        (0x04, ctrl1),
        (0x0e, 0x00),
        (0x0f, 0x2a),
        (0x10, 0x00),
        (0x11, 0x08),
        (0x13, 0x00),
        (0x14, 0xf0 | release),
        (0x12, ctrl3),
    ] {
        write(&mut s, r, v);
    }
    let mut out = vec![0i16; HELD_SAMPLES];
    unsafe { sid_render(&mut *s, out.as_mut_ptr(), HELD_SAMPLES as u32) };
    write(&mut s, 0x04, ctrl1 & 0xfe);
    write(&mut s, 0x12, ctrl3 & 0xfe);
    s
}

fn render_in_blocks(s: &mut sid, gate: Option<&mut tail_gate>, n: usize) -> Vec<i16> {
    let mut out = vec![0i16; n];
    let mut gate = gate;
    for b in out.chunks_mut(BLOCK) {
        assert_eq!(b.len(), BLOCK);
        unsafe {
            sid_render(s, b.as_mut_ptr(), BLOCK as u32);
            if let Some(g) = gate.as_deref_mut() {
                tail_gate_run(g, s, std::ptr::null(), b.as_mut_ptr());
            }
        }
    }
    out
}

fn silent(s: &sid) -> bool {
    s.env.iter().all(|e| e.counter == 0 && e.gate == 0)
}

const SETTLE_SAMPLES: usize = (TAIL_HOLD_BLOCKS as usize + FADE_BLOCKS + SLACK_BLOCKS) * BLOCK;

#[test]
fn the_tail_after_notes_reaches_exact_silence() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        for release in [0u8, 4, 8] {
            let mut s = released_chord(model, release, 0x21, 0x41);
            let mut gate = tail_gate::default();
            let deadline =
                RELEASE_TO_ZERO_MS[release as usize] as usize * RATE / 1000 + SETTLE_SAMPLES;
            let blocks = deadline.div_ceil(BLOCK) * BLOCK;
            let out = render_in_blocks(&mut s, Some(&mut gate), blocks + 344 * BLOCK);
            assert!(silent(&s));
            let last = out.iter().rposition(|&x| x != 0);
            assert!(
                last.is_none_or(|l| l < deadline),
                "model {model} release {release}: {} at sample {last:?} of {deadline}",
                last.map_or(0, |l| out[l])
            );
        }
    }
}

#[test]
fn the_leak_fades_out_without_a_click() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut gated = released_chord(model, 0, 0x11, 0x11);
        let mut kept = released_chord(model, 0, 0x11, 0x11);
        let mut gate = tail_gate::default();
        let n = HALF_SECOND;
        let a = render_in_blocks(&mut gated, Some(&mut gate), n);
        let b = render_in_blocks(&mut kept, None, n);
        let d: Vec<i32> = a
            .iter()
            .zip(&b)
            .map(|(&x, &y)| i32::from(x) - i32::from(y))
            .collect();
        let step = d.windows(2).map(|w| (w[1] - w[0]).abs()).max().unwrap();
        let reach = d.iter().map(|x| x.abs()).max().unwrap();
        println!("model {model}: the gate's effect reaches {reach} LSB, steps {step}");
        assert!(
            reach >= 20,
            "model {model}: the leak was {reach} LSB, a test of nothing"
        );
        assert!(
            step <= 8,
            "model {model}: the gate stepped {step} LSB (leak {reach})"
        );
    }
}

#[test]
fn a_chip_with_bleed_keeps_its_leak() {
    let mut s = released_chord(SID_MODEL_6581, 0, 0x21, 0x41);
    s.bleed = 0.25;
    let mut gate = tail_gate::default();
    render_in_blocks(&mut s, Some(&mut gate), HALF_SECOND);
    assert!(silent(&s));
    let tail = render_in_blocks(&mut s, Some(&mut gate), 4096);
    assert!(
        tail.iter().any(|&v| v != 0),
        "the bleed's leak is a deliberate effect"
    );
}

#[test]
fn a_note_after_the_silence_is_the_chips_own_output() {
    let mut gated = released_chord(SID_MODEL_6581, 0, 0x21, 0x41);
    let mut kept = released_chord(SID_MODEL_6581, 0, 0x21, 0x41);
    let mut gate = tail_gate::default();
    let n = HALF_SECOND;
    render_in_blocks(&mut gated, Some(&mut gate), n);
    render_in_blocks(&mut kept, None, n);
    assert_eq!(gate.n, TAIL_HOLD_BLOCKS + 2, "the gate is shut");
    write(&mut gated, 0x04, 0x21);
    write(&mut kept, 0x04, 0x21);
    let a = render_in_blocks(&mut gated, Some(&mut gate), 4096);
    let b = render_in_blocks(&mut kept, None, 4096);
    assert_eq!(a, b);
    assert_eq!(gate.n, 0);
}

#[test]
fn notes_within_a_phrase_play_untouched() {
    let mut gated = released_chord(SID_MODEL_6581, 0, 0x21, 0x41);
    let mut kept = released_chord(SID_MODEL_6581, 0, 0x21, 0x41);
    let mut gate = tail_gate::default();
    let gap = (TAIL_HOLD_BLOCKS as usize - 2) * BLOCK;
    let a1 = render_in_blocks(&mut gated, Some(&mut gate), gap);
    let b1 = render_in_blocks(&mut kept, None, gap);
    assert_eq!(a1, b1);
    write(&mut gated, 0x04, 0x21);
    write(&mut kept, 0x04, 0x21);
    assert_eq!(
        render_in_blocks(&mut gated, Some(&mut gate), 4096),
        render_in_blocks(&mut kept, None, 4096)
    );
}
