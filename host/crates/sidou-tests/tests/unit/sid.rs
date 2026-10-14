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
    // Let the power-on envelopes of the silent voices 1 and 2 release to 0 first: with no
    // waveform selected, an 8580 voice outputs its DAC's zero level times the envelope.
    clock(&mut s, 10_000);
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

fn read(s: &mut sid, reg: u8) -> u8 {
    unsafe { sid_read(s, reg) }
}

/// A voice 3 control write, then forget the OSC3 latched before it: these tests poke
/// the accumulators directly instead of running cycles.
fn write_control(s: &mut sid, value: u8) {
    write(s, 0x12, value);
    s.osc3_held = 0;
}

fn clock(s: &mut sid, cycles: u32) {
    unsafe { sid_clock(s, cycles) }
}

#[test]
fn envelopes_and_accumulators_power_up_like_the_chip() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let s = new_sid(model);
        for v in 0..3 {
            assert_eq!(s.env[v].counter, 0xaa, "voice {v}");
            assert_eq!(s.env[v].counter_enabled, 1, "voice {v}");
            assert_eq!(s.env[v].state, SID_ENV_RELEASE, "voice {v}");
            assert_eq!(s.osc[v].acc, 0x555555, "voice {v}");
        }
    }
}

#[test]
fn power_on_envelopes_release_to_zero_and_freeze() {
    let mut s = new_sid(SID_MODEL_8580);
    // Release rate 0 from 0xaa: well under 10,000 cycles (reSIDfp: ~7 ms).
    clock(&mut s, 10_000);
    for v in 0..3 {
        assert_eq!(s.env[v].counter, 0, "voice {v}");
        assert_eq!(s.env[v].counter_enabled, 0, "voice {v}");
    }
}

#[test]
fn env3_reads_the_voice_3_envelope_counter_one_cycle_late() {
    let mut s = new_sid(SID_MODEL_8580);
    // reSIDfp latches ENV3 at the start of each cycle; nothing is latched before the first.
    assert_eq!(read(&mut s, 0x1c), 0x00);
    clock(&mut s, 1);
    assert_eq!(read(&mut s, 0x1c), 0xaa);
    clock(&mut s, 2); // the first power-on release step lands in the third cycle
    assert_eq!(s.env[2].counter, 0xa9);
    assert_eq!(read(&mut s, 0x1c), 0xaa);
    clock(&mut s, 1);
    assert_eq!(read(&mut s, 0x1c), 0xa9);
}

#[test]
fn osc3_reads_the_top_8_bits_of_the_voice_3_waveform() {
    let mut s = new_sid(SID_MODEL_6581);
    write_control(&mut s, 0x20); // saw
    s.osc[2].acc = 0xabcdef;
    assert_eq!(read(&mut s, 0x1b), 0xab);
    write_control(&mut s, 0x10); // triangle: the MSB folds the ramp
    s.osc[2].acc = 0x400000;
    assert_eq!(read(&mut s, 0x1b), 0x80);
    s.osc[2].acc = 0xc00000;
    assert_eq!(read(&mut s, 0x1b), 0x7f);
    write_control(&mut s, 0x00); // no waveform
    assert_eq!(read(&mut s, 0x1b), 0x00);
}

#[test]
fn osc3_follows_voice_3_ring_modulation_by_voice_2() {
    let mut s = new_sid(SID_MODEL_6581);
    write_control(&mut s, 0x14); // triangle, ring modulated
    s.osc[2].acc = 0x400000;
    s.osc[1].acc = 0x800000;
    assert_eq!(read(&mut s, 0x1b), 0x80);
    s.osc[1].acc = 0x000000; // voice 2's MSB low: voice 3's MSB is inverted
    assert_eq!(read(&mut s, 0x1b), 0x7f);
}

#[test]
fn osc3_reads_the_value_latched_before_a_write_until_the_next_cycle() {
    let mut s = new_sid(SID_MODEL_6581);
    write(&mut s, 0x12, 0x20); // saw
    clock(&mut s, 1);
    s.osc[2].acc = 0xabc000;
    clock(&mut s, 1);
    let saw = read(&mut s, 0x1b);
    write(&mut s, 0x12, 0x40); // pulse
    assert_eq!(read(&mut s, 0x1b), saw);
    write(&mut s, 0x0e, 0x12); // further writes keep the first latch
    assert_eq!(read(&mut s, 0x1b), saw);
    clock(&mut s, 1);
    assert_ne!(read(&mut s, 0x1b), saw);
}

#[test]
fn paddles_read_0xff() {
    let mut s = new_sid(SID_MODEL_8580);
    assert_eq!(read(&mut s, 0x19), 0xff);
    assert_eq!(read(&mut s, 0x1a), 0xff);
}

#[test]
fn write_only_registers_read_the_last_bus_value() {
    let mut s = new_sid(SID_MODEL_8580);
    assert_eq!(read(&mut s, 0x00), 0x00);
    write(&mut s, 0x05, 0x3c);
    assert_eq!(read(&mut s, 0x00), 0x3c);
    assert_eq!(read(&mut s, 0x18), 0x3c);
    write(&mut s, 0x18, 0x0f);
    assert_eq!(read(&mut s, 0x05), 0x0f);
    // A read of a readable register drives the bus too, as in reSIDfp.
    clock(&mut s, 1);
    assert_eq!(read(&mut s, 0x1c), 0xaa);
    assert_eq!(read(&mut s, 0x1d), 0xaa);
    assert_eq!(read(&mut s, 0x19), 0xff);
    assert_eq!(read(&mut s, 0x1f), 0xff);
}

#[test]
fn register_numbers_wrap_at_0x20() {
    let mut s = new_sid(SID_MODEL_8580);
    s.env[2].env3 = 0x42;
    assert_eq!(read(&mut s, 0x3c), 0x42);
    assert_eq!(read(&mut s, 0xf9), 0xff);
}

#[test]
fn clock_advances_an_exact_number_of_cycles() {
    let mut s = new_sid(SID_MODEL_8580);
    write(&mut s, 0x0e, 0x00);
    write(&mut s, 0x0f, 0x10); // voice 3 freq 0x1000
    clock(&mut s, 1);
    assert_eq!(s.osc[2].acc, 0x556555);
    clock(&mut s, 99_999);
    assert_eq!(s.osc[2].acc, (0x555555 + 100_000 * 0x1000) & 0xffffff);
    assert_eq!(s.osc[0].acc, 0x555555);
}

#[test]
fn clock_leaves_the_chip_where_render_would() {
    let setup = |s: &mut sid| {
        write(s, 0x0e, 0x45);
        write(s, 0x0f, 0x1d);
        write(s, 0x07, 0x77);
        write(s, 0x08, 0x2a);
        write(s, 0x13, 0x26);
        write(s, 0x14, 0x84);
        write(s, 0x12, 0x81); // voice 3 noise, gated
        write(s, 0x0b, 0x41);
    };
    let mut rendered = new_sid(SID_MODEL_6581);
    let mut clocked = new_sid(SID_MODEL_6581);
    setup(&mut rendered);
    setup(&mut clocked);
    const SAMPLES: u32 = 4410;
    render(&mut rendered, SAMPLES);
    let cycles = (u64::from(SAMPLES) * u64::from(rendered.cycles_per_sample_q16)) >> 16;
    clock(&mut clocked, u32::try_from(cycles).unwrap());
    for v in 0..3 {
        assert_eq!(clocked.osc[v].acc, rendered.osc[v].acc, "voice {v}");
        assert_eq!(clocked.osc[v].sr, rendered.osc[v].sr, "voice {v}");
        assert_eq!(clocked.env[v].counter, rendered.env[v].counter, "voice {v}");
        let (mut a, mut b) = (clocked.env[v], rendered.env[v]);
        unsafe {
            sid_env_settle(&mut a);
            sid_env_settle(&mut b);
        }
        assert_eq!(a.lfsr_pos, b.lfsr_pos, "voice {v}");
        assert_eq!(a.exp_counter, b.exp_counter, "voice {v}");
    }
    assert_eq!(read(&mut clocked, 0x1b), read(&mut rendered, 0x1b));
    assert_eq!(read(&mut clocked, 0x1c), read(&mut rendered, 0x1c));
}

#[test]
fn combined_profile_selects_tables_and_falls_back_to_average() {
    let mut s = new_sid(SID_MODEL_6581);
    let average = s.osc[0].pulldown;
    for (profile, differs) in [
        (SID_PROFILE_WEAK as i32, true),
        (SID_PROFILE_STRONG as i32, true),
    ] {
        unsafe { sid_set_combined_profile(&mut *s, profile) };
        assert_eq!(s.osc[0].pulldown != average, differs, "profile {profile}");
        assert_eq!(s.osc[2].pulldown, s.osc[0].pulldown);
    }
    for profile in [SID_PROFILE_AVERAGE as i32, -1, 3, i32::MAX] {
        unsafe { sid_set_combined_profile(&mut *s, profile) };
        assert_eq!(s.osc[0].pulldown, average, "profile {profile}");
    }
}

/// The 8580's OSC3 a cycle after switching into a triangle: from power-on its latch
/// holds 0x555 (reSIDfp's initial value); after a waveform without triangle or saw, the
/// last triangle/saw value.
#[test]
fn osc3_on_the_8580_uses_the_latched_triangle_saw_for_one_cycle() {
    let mut s = new_sid(SID_MODEL_8580);
    clock(&mut s, 100);
    write(&mut s, 0x12, 0x10);
    clock(&mut s, 1);
    assert_eq!(read(&mut s, 0x1b), 0x55);
    // Saw for a while, then pulse (no triangle or saw), then triangle again: the latch
    // holds the saw's last value through the pulse.
    write(&mut s, 0x0e, 0x00);
    write(&mut s, 0x0f, 0x10);
    write(&mut s, 0x12, 0x20);
    clock(&mut s, 1000);
    let saw_ramp = (s.osc[2].acc >> 12) & 0xfff;
    write(&mut s, 0x12, 0x40);
    clock(&mut s, 500);
    write(&mut s, 0x12, 0x10);
    clock(&mut s, 1);
    assert_eq!(u32::from(read(&mut s, 0x1b)), saw_ramp >> 4);
}
