// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;

fn new_sid(model: sid_model) -> Box<sid> {
    let mut s = Box::<sid>::default();
    unsafe { sid_init(&mut *s, model, SID_CLOCK_PAL, 44100) };
    s
}

fn write(s: &mut sid, reg: u8, value: u8) {
    unsafe { sid_write(s, reg, value) }
}

fn render(s: &mut sid, n: u32) -> Vec<i16> {
    let mut out = vec![0i16; n as usize];
    unsafe { sid_render(s, out.as_mut_ptr(), n) };
    out
}

fn peak(v: &[i16]) -> i32 {
    v.iter().map(|&x| i32::from(x).abs()).max().unwrap_or(0)
}

fn saw_a4(s: &mut sid) {
    write(s, 0x00, 0x45); // 440 Hz at PAL: 0x1d45
    write(s, 0x01, 0x1d);
    write(s, 0x05, 0x00);
    write(s, 0x06, 0xf0);
    write(s, 0x18, 0x0f);
    write(s, 0x04, 0x21);
}

#[test]
fn cycles_per_sample_is_pal_clock_over_sample_rate() {
    let s = new_sid(SID_MODEL_8580);
    assert_eq!(
        s.cycles_per_sample_q16,
        ((u64::from(SID_CLOCK_PAL) << 16) / 44100) as u32
    );
}

#[test]
fn an_8580_is_silent_after_init() {
    let mut s = new_sid(SID_MODEL_8580);
    assert_eq!(peak(&render(&mut s, 4410)), 0);
}

#[test]
fn gated_saw_produces_unclipped_sound() {
    let mut s = new_sid(SID_MODEL_8580);
    saw_a4(&mut s);
    let out = render(&mut s, 4410);
    assert!(peak(&out) > 1000);
    assert!(peak(&out) < 32767);
}

#[test]
fn volume_write_on_a_6581_clicks_then_settles() {
    let mut s = new_sid(SID_MODEL_6581);
    write(&mut s, 0x18, 0x0f);
    let click = render(&mut s, 10);
    assert!(peak(&click) > 1000);
    render(&mut s, 44100);
    assert!(peak(&render(&mut s, 100)) < 50);
}

#[test]
fn voice_3_off_mutes_an_unfiltered_voice_3() {
    let mut s = new_sid(SID_MODEL_8580);
    write(&mut s, 0x0e, 0x45);
    write(&mut s, 0x0f, 0x1d);
    write(&mut s, 0x14, 0xf0);
    write(&mut s, 0x18, 0x8f);
    write(&mut s, 0x12, 0x21);
    assert_eq!(peak(&render(&mut s, 4410)), 0);
}

#[test]
fn low_pass_filter_removes_top_end() {
    let mut a = new_sid(SID_MODEL_8580);
    let mut b = new_sid(SID_MODEL_8580);
    saw_a4(&mut a);
    saw_a4(&mut b);
    write(&mut b, 0x16, 0x10); // fc ~ 128 -> 800 Hz
    write(&mut b, 0x17, 0x01); // voice 1 through the filter
    write(&mut b, 0x18, 0x1f); // low-pass, volume 15
    let dry = render(&mut a, 4410);
    let wet = render(&mut b, 4410);
    let hf_energy = |v: &[i16]| -> f64 {
        v.windows(2)
            .map(|p| {
                let d = f64::from(p[1]) - f64::from(p[0]);
                d * d
            })
            .sum()
    };
    assert!(hf_energy(&wet) < hf_energy(&dry) * 0.1);
}

fn assert_dc_blocker_never_subnormal(s: &mut sid, seconds: u32) {
    let mut block = vec![0i16; 4410];
    for i in 0..seconds * 10 {
        unsafe { sid_render(s, block.as_mut_ptr(), block.len() as u32) };
        assert!(!s.dc_x1.is_subnormal(), "block {i}");
        assert!(!s.dc_y1.is_subnormal(), "block {i}");
    }
}

#[test]
fn dc_blocker_on_a_6581_volume_step_never_goes_subnormal() {
    let mut s = new_sid(SID_MODEL_6581);
    write(&mut s, 0x18, 0x0f);
    assert_dc_blocker_never_subnormal(&mut s, 20);
}

#[test]
fn dc_blocker_after_an_8580_note_never_goes_subnormal() {
    let mut s = new_sid(SID_MODEL_8580);
    saw_a4(&mut s);
    render(&mut s, 4410);
    write(&mut s, 0x04, 0x20);
    assert_dc_blocker_never_subnormal(&mut s, 20);
}

#[test]
fn voice_3_off_keeps_a_filtered_voice_3() {
    let mut s = new_sid(SID_MODEL_8580);
    write(&mut s, 0x0e, 0x45);
    write(&mut s, 0x0f, 0x1d);
    write(&mut s, 0x14, 0xf0);
    write(&mut s, 0x16, 0xff);
    write(&mut s, 0x17, 0x04);
    write(&mut s, 0x18, 0x9f);
    write(&mut s, 0x12, 0x21);
    assert!(peak(&render(&mut s, 4410)) > 1000);
}

#[test]
fn cutoff_low_bits_come_from_register_15() {
    let mut a = new_sid(SID_MODEL_8580);
    let mut b = new_sid(SID_MODEL_8580);
    for s in [&mut a, &mut b] {
        saw_a4(s);
        write(s, 0x16, 0x02);
        write(s, 0x17, 0x01);
        write(s, 0x18, 0x1f);
    }
    write(&mut b, 0x15, 0x07);
    assert_eq!(a.fc, 0x010);
    assert_eq!(b.fc, 0x017);
    assert_ne!(render(&mut a, 4410), render(&mut b, 4410));
}
