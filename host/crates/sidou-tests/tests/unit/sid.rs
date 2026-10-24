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
    // The same chip with voice 3 playing and silent: with voice 3 off, they must not
    // differ. (Silent voices are not exactly 0: the DACs leak, as reSIDfp's do.)
    let setup = |play: bool, mode_vol: u8| {
        let mut s = new_sid(SID_MODEL_8580);
        write(&mut s, 0x0e, 0x45);
        write(&mut s, 0x0f, 0x1d);
        write(&mut s, 0x14, 0xf0);
        write(&mut s, 0x18, mode_vol);
        if play {
            write(&mut s, 0x12, 0x21);
        }
        render(&mut s, 4410)
    };
    assert_eq!(setup(true, 0x8f), setup(false, 0x8f));
    assert_ne!(setup(true, 0x0f), setup(false, 0x0f), "voice 3 on is heard");
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
    s.vf.osc3_held = 0;
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
        let (mut co, mut ro) = (clocked.osc[v], rendered.osc[v]);
        unsafe {
            sid_noise_settle(&mut co);
            sid_noise_settle(&mut ro);
        }
        assert_eq!(co.sr, ro.sr, "voice {v}");
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

#[test]
fn render_folds_deferred_shift_counts_by_the_noise_period() {
    let mut s = new_sid(SID_MODEL_8580);
    write(&mut s, 0x00, 0xff);
    write(&mut s, 0x01, 0xff);
    write(&mut s, 0x04, 0x21); // saw: the noise register's shifts are deferred
    render(&mut s, 64);
    assert_eq!(s.osc[0].defer, 1);
    s.osc[0].owed = 0x8000_0123;
    let mut unfolded = s.osc[0];
    render(&mut s, 0);
    assert!(s.osc[0].owed < 0x8000_0000);
    let mut folded = s.osc[0];
    unsafe {
        sid_noise_settle(&mut unfolded);
        sid_noise_settle(&mut folded);
    }
    assert_eq!(folded.sr, unfolded.sr);
}

/// The loudest the chip gets: three voices in phase at maximum resonance, filtered in every
/// mode and unfiltered, swept over the cutoff, for pulse and sawtooth at two pitches.
/// Returns the peak magnitude at the given volume and the case it came from. The volume
/// scales the output after the filter, so the peak at volume 7 times 15/7 is volume 15's
/// before the int16 clamp.
fn worst_case_peak(model: sid_model, volume: u8) -> (i32, String) {
    let mut worst = (0, String::new());
    for (wave, control) in [("pulse", 0x41u8), ("saw", 0x21)] {
        for (lo, hi) in [(0x00u8, 0x08u8), (0x45, 0x1d)] {
            for (mode, mode_bits, routing) in [
                ("unfiltered", 0x00u8, 0x00u8),
                ("LP", 0x10, 0x07),
                ("BP", 0x20, 0x07),
                ("HP", 0x40, 0x07),
                ("LP+BP+HP", 0x70, 0x07),
            ] {
                for fc_hi in [0x00u8, 0x08, 0x20, 0x40, 0x80, 0xc0, 0xff] {
                    let mut s = new_sid(model);
                    for v in 0..3u8 {
                        let base = 7 * v;
                        write(&mut s, base, lo);
                        write(&mut s, base + 1, hi);
                        write(&mut s, base + 2, 0x00);
                        write(&mut s, base + 3, 0x08);
                        write(&mut s, base + 5, 0x00);
                        write(&mut s, base + 6, 0xf0);
                    }
                    write(&mut s, 0x15, 0x07);
                    write(&mut s, 0x16, fc_hi);
                    write(&mut s, 0x17, 0xf0 | routing);
                    write(&mut s, 0x18, mode_bits | volume);
                    for v in 0..3u8 {
                        write(&mut s, 7 * v + 4, control);
                    }
                    let p = peak(&render(&mut s, 22050));
                    if p > worst.0 {
                        let at = format!("{wave} at {hi:02x}{lo:02x}, {mode}, FC hi {fc_hi:#04x}");
                        worst = (p, at);
                    }
                }
            }
        }
    }
    worst
}

/* The output's worst case, at reSIDfp's level (LEVEL_6581/LEVEL_8580 in sid.c): three
 * voices at volume 15 and maximum resonance. The 6581 stays below full scale (measured
 * 0.4 dB of headroom; 0.53 dB with the fitted filter tables of M1b Task 5). The 8580
 * clips, by design: its peak before the int16 clamp is about 1.0 dB over full scale (1.07
 * since Task 5; an 8580 pulse at 0x1d45 through the high pass at FC hi 0x08),
 * where reSIDfp's own output saturates too. The bounds hold those numbers. */
#[test]
fn three_voices_at_maximum_resonance_headroom() {
    for (name, model, max_db) in [
        ("6581", SID_MODEL_6581, -0.2),
        ("8580", SID_MODEL_8580, 1.2),
    ] {
        let (p, at) = worst_case_peak(model, 7);
        let peak15 = f64::from(p) * 15.0 / 7.0;
        let db = 20.0 * (peak15 / 32767.0).log10();
        println!("{name}: worst peak at volume 15 before the clamp {peak15:.0} ({at}): {db:+.2} dB re full scale");
        assert!(
            db < max_db,
            "{name}: {db:+.2} dB re full scale ({at}), over {max_db:+.1}"
        );
    }
    // reSIDfp saturates in the 8580's worst case too.
    use sidou_tests::render::{render_residfp, RegWrite};
    let r = |reg, value| RegWrite {
        frame: 0,
        reg,
        value,
    };
    let mut writes = Vec::new();
    for v in 0..3u8 {
        let b = 7 * v;
        writes.extend([
            r(b, 0x45),
            r(b + 1, 0x1d),
            r(b + 3, 0x08),
            r(b + 5, 0x00),
            r(b + 6, 0xf0),
        ]);
    }
    writes.extend([r(0x15, 0x07), r(0x16, 0x08), r(0x17, 0xf7), r(0x18, 0x4f)]);
    writes.extend((0..3u8).map(|v| r(7 * v + 4, 0x41)));
    let reference = render_residfp(SID_MODEL_8580, &writes, 25, false);
    let peak = reference.iter().fold(0.0f32, |m, &v| m.max(v.abs())) * 32768.0;
    assert!(peak >= 32700.0, "reSIDfp's 8580 peaks at {peak:.0} here");
}

/// Voice 3 as SAW LEAD plays it (attack 0, decay 9, sustain 10, release 6), held, then
/// released to 0: the ADSR delay's setting for the next gate.
fn released_voice_3(gate_reset: i32) -> Box<sid> {
    let mut s = new_sid(SID_MODEL_6581);
    unsafe { sid_set_gate_reset(&mut *s, gate_reset) };
    write(&mut s, 0x13, 0x09);
    write(&mut s, 0x14, 0xa6);
    write(&mut s, 0x12, 0x21);
    clock(&mut s, 100_000);
    write(&mut s, 0x12, 0x20);
    clock(&mut s, 4_000_000);
    assert_eq!(read(&mut s, 0x1c), 0);
    s
}

/// Cycles from the gate until ENV3 reads nonzero, at most `limit`.
fn env3_rises_after(s: &mut sid, limit: u32) -> Option<u32> {
    write(s, 0x12, 0x21);
    (1..=limit).find(|_| {
        clock(s, 1);
        read(s, 0x1c) != 0
    })
}

#[test]
fn sid_init_leaves_the_gate_reset_off_on_every_voice() {
    let mut s = new_sid(SID_MODEL_8580);
    assert!(s.env.iter().all(|e| e.gate_reset == 0));
    unsafe { sid_set_gate_reset(&mut *s, 1) };
    assert!(s.env.iter().all(|e| e.gate_reset == 1));
    unsafe { sid_set_gate_reset(&mut *s, 0) };
    assert!(s.env.iter().all(|e| e.gate_reset == 0));
    unsafe { sid_set_gate_reset(&mut *s, 7) }; // any nonzero is on
    assert!(s.env.iter().all(|e| e.gate_reset == 1));
    unsafe { sid_init(&mut *s, SID_MODEL_8580, SID_CLOCK_PAL, 44100) };
    assert!(
        s.env.iter().all(|e| e.gate_reset == 0),
        "sid_init clears it"
    );
}

#[test]
fn with_the_gate_reset_env3_rises_within_an_attack_period_at_every_phase() {
    // Release 6's period is 267 cycles: a gate at each of its phases. ENV3 shows the step
    // one cycle after the counter takes it: 9 + 3 + 1 cycles at most.
    let released = released_voice_3(1);
    for phase in 0..267 {
        let mut s = Box::new(*released);
        clock(&mut s, phase);
        let rise = env3_rises_after(&mut s, 100).expect("ENV3 never rose");
        assert!(rise <= 13, "phase {phase}: {rise} cycles");
    }
}

#[test]
fn without_the_gate_reset_most_phases_wait_for_the_lfsr() {
    // The ADSR delay, as reSIDfp has it: 8 of the 267 phases start at once.
    let released = released_voice_3(0);
    let waits: Vec<u32> = (0..267)
        .map(|phase| {
            let mut s = Box::new(*released);
            clock(&mut s, phase);
            env3_rises_after(&mut s, 40_000).expect("ENV3 never rose")
        })
        .collect();
    assert_eq!(waits.iter().filter(|&&w| w <= 13).count(), 8, "{waits:?}");
    assert!(waits.iter().all(|&w| w <= 13 || w > 32_000), "{waits:?}");
}

/// Voice 3 as SAW LEAD plays it (attack 0, decay 9, sustain 10, release 6), held into
/// sustain: ENV3 at 0xaa, the rate LFSR running at the decay's period (977 cycles).
fn held_voice_3(gate_reset: i32) -> Box<sid> {
    let mut s = new_sid(SID_MODEL_6581);
    unsafe { sid_set_gate_reset(&mut *s, gate_reset) };
    write(&mut s, 0x13, 0x09);
    write(&mut s, 0x14, 0xa6);
    write(&mut s, 0x12, 0x21);
    clock(&mut s, 200_000);
    assert_eq!(read(&mut s, 0x1c), 0xaa);
    s
}

/// Cycles from the gate's falling edge until ENV3 reads below 0xaa, at most `limit`.
fn env3_falls_after(s: &mut sid, limit: u32) -> Option<u32> {
    write(s, 0x12, 0x20);
    (1..=limit).find(|_| {
        clock(s, 1);
        read(s, 0x1c) < 0xaa
    })
}

#[test]
fn with_the_gate_reset_env3_falls_within_a_release_period_at_every_phase() {
    // A note-off at each phase of the decay's period. ENV3 shows the step one cycle after
    // the counter takes it: release 6's 267 + 3 + 1 cycles at most.
    let held = held_voice_3(1);
    for phase in 0..977 {
        let mut s = Box::new(*held);
        clock(&mut s, phase);
        let fall = env3_falls_after(&mut s, 1000).expect("ENV3 never fell");
        assert!(fall <= 271, "phase {phase}: {fall} cycles");
    }
}

#[test]
fn without_the_gate_reset_most_note_offs_wait_for_the_lfsr() {
    // The ADSR delay on release, as reSIDfp has it: the 267 phases that find the LFSR at or
    // before release 6's position start at once, the 710 past it wait for it to come round
    // (every 7th phase here: 38 of 140 at once).
    let held = held_voice_3(0);
    let waits: Vec<u32> = (0..977)
        .step_by(7)
        .map(|phase| {
            let mut s = Box::new(*held);
            clock(&mut s, phase);
            env3_falls_after(&mut s, 40_000).expect("ENV3 never fell")
        })
        .collect();
    assert_eq!(waits.iter().filter(|&&w| w <= 271).count(), 38, "{waits:?}");
    assert!(waits.iter().all(|&w| w <= 271 || w > 32_000), "{waits:?}");
}

/// What a tap saw: the oscillators' accumulators each time it ran.
unsafe extern "C" fn record_tap(ctx: *mut std::ffi::c_void, s: *const sid) {
    let seen = unsafe { &mut *ctx.cast::<Vec<[u32; 3]>>() };
    let o = unsafe { &(*s).osc };
    seen.push([o[0].acc, o[1].acc, o[2].acc]);
}

/// sid_render_tap renders exactly sid_render's samples, on both models, and runs the tap
/// after every `every` samples and after the last, short chunk, with the voices as those
/// samples left them.
#[test]
fn a_tapped_render_is_the_plain_render_with_the_voices_at_each_chunk() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let (mut a, mut b, mut c) = (new_sid(model), new_sid(model), new_sid(model));
        for s in [&mut a, &mut b, &mut c] {
            saw_a4(s);
            write(s, 0x17, 0xf1);
            write(s, 0x16, 0x40);
            write(s, 0x18, 0x1f);
        }
        let mut seen: Vec<[u32; 3]> = Vec::new();
        let mut expect: Vec<[u32; 3]> = Vec::new();
        for frames in [128u32, 40, 128] {
            let plain = render(&mut a, frames);
            let mut tapped = vec![0i16; frames as usize];
            unsafe {
                sid_render_tap(
                    &mut *b,
                    tapped.as_mut_ptr(),
                    frames,
                    16,
                    Some(record_tap),
                    (&raw mut seen).cast(),
                );
            }
            assert_eq!(plain, tapped, "model {model}, {frames} frames");
            let mut k = 0;
            while k < frames {
                let n = (frames - k).min(16);
                render(&mut c, n);
                expect.push([c.osc[0].acc, c.osc[1].acc, c.osc[2].acc]);
                k += n;
            }
        }
        assert_eq!(seen.len(), 8 + 3 + 8);
        assert_eq!(seen, expect, "model {model}");
        assert!(
            peak(&render(&mut b, 128)) > 1000,
            "silent: the test would be vacuous"
        );
    }
}

/// sid_set_filter_nonlinearity scales the 6581's non-linearity (sid_nonlin): 1 is the
/// default sid_init sets (reSIDfp's), 0 a linear filter; the references stay. A profile
/// (M1c) can blend it while the chip plays: each call writes one float at a time.
#[test]
fn the_6581_non_linearity_is_a_per_chip_amount() {
    let mut s = new_sid(SID_MODEL_6581);
    let full = s.nl;
    assert!(full.level > 0.0 && full.load > 0.0 && full.floor > 0.0);
    unsafe { sid_set_filter_nonlinearity(&mut *s, 0.5) };
    assert_eq!(s.nl.level, full.level * 0.5);
    assert_eq!(s.nl.load, full.load * 0.5);
    assert_eq!(s.nl.floor, full.floor * 0.5);
    assert_eq!(s.nl.level_ref, full.level_ref);
    assert_eq!(s.nl.load_ref, full.load_ref);
    unsafe { sid_set_filter_nonlinearity(&mut *s, 0.0) };
    assert_eq!((s.nl.level, s.nl.load, s.nl.floor), (0.0, 0.0, 0.0));
    let other = new_sid(SID_MODEL_6581);
    assert_eq!(other.nl.level, full.level, "per chip");
}

/// The 8580 stays linear: the 6581's non-linearity, whatever its amount, does not change a
/// sample of it.
#[test]
fn the_8580_ignores_the_6581_non_linearity() {
    let play = |amount: f32| {
        let mut s = new_sid(SID_MODEL_8580);
        unsafe { sid_set_filter_nonlinearity(&mut *s, amount) };
        saw_a4(&mut s);
        write(&mut s, 0x17, 0xf1);
        write(&mut s, 0x16, 0x10);
        write(&mut s, 0x18, 0x1f);
        render(&mut s, 8820)
    };
    assert_eq!(play(0.0), play(1.0));
    assert_eq!(play(1.0), play(3.0));
}

/// Correlation of a filtered voice with the same voice unfiltered, at lag 0.
fn polarity(model: sid_model, mode: u8, fc_hi: u8) -> f64 {
    let run = |filtered: bool| {
        let mut s = new_sid(model);
        write(&mut s, 0x00, 0xff);
        write(&mut s, 0x01, 0xff);
        write(&mut s, 0x05, 0x00);
        write(&mut s, 0x06, 0x40);
        write(&mut s, 0x16, fc_hi);
        write(&mut s, 0x17, if filtered { 0x01 } else { 0x00 });
        write(&mut s, 0x18, mode | 0x0f);
        write(&mut s, 0x04, 0x81);
        render(&mut s, 22050)[4410..]
            .iter()
            .map(|&v| f64::from(v))
            .collect::<Vec<_>>()
    };
    let (d, f) = (run(false), run(true));
    let md = d.iter().sum::<f64>() / d.len() as f64;
    let mf = f.iter().sum::<f64>() / f.len() as f64;
    let c: f64 = d.iter().zip(&f).map(|(a, b)| (a - md) * (b - mf)).sum();
    let nd = d.iter().map(|a| (a - md).powi(2)).sum::<f64>().sqrt();
    let nf = f.iter().map(|b| (b - mf).powi(2)).sum::<f64>().sqrt();
    c / nd / nf
}

/// As in reSIDfp, the 6581's low-pass and high-pass come out inverted against the
/// unfiltered voices and its band-pass does not (the summer and the integrators are
/// inverting op-amps); a mixed mode such as LP+BP depends on it.
#[test]
fn the_6581_low_and_high_pass_are_inverted() {
    assert!(polarity(SID_MODEL_6581, 0x10, 0xe0) < -0.5);
    assert!(polarity(SID_MODEL_6581, 0x20, 0x30) > 0.3);
    assert!(polarity(SID_MODEL_6581, 0x40, 0x10) < -0.5);
    // The 8580 is unchanged (M1b): all three in phase.
    assert!(polarity(SID_MODEL_8580, 0x10, 0xe0) > 0.5);
    assert!(polarity(SID_MODEL_8580, 0x40, 0x10) > 0.5);
}

/// sid_set_filter_nonlinearity clamps its amount to [0, 2] and takes NaN as 0: below -0.5
/// the filter runs to inf and NaN at low cutoffs (and NaN to int16 is undefined), and from
/// about 3 the band-pass saturates at FC 0.
#[test]
fn the_6581_non_linearity_amount_is_clamped_to_0_to_2() {
    let nl = |amount: f32| {
        let mut s = new_sid(SID_MODEL_6581);
        unsafe { sid_set_filter_nonlinearity(&mut *s, amount) };
        let n = s.nl;
        (n.level, n.level_off, n.load, n.floor)
    };
    assert_eq!(nl(5.0), nl(2.0));
    assert_eq!(nl(f32::INFINITY), nl(2.0));
    assert_eq!(nl(-0.6), nl(0.0));
    assert_eq!(nl(-1.0), nl(0.0));
    assert_eq!(nl(f32::NEG_INFINITY), nl(0.0));
    assert_eq!(nl(f32::NAN), nl(0.0));
    assert_eq!(nl(0.0), (0.0, 0.0, 0.0, 0.0));
    assert_ne!(nl(2.0), nl(1.0));
    assert_eq!(nl(2.0).0, 2.0 * nl(1.0).0);
}

/// Across the amount's range, three loud voices into the filter at full resonance stay
/// finite and bounded, at every mode and at the cutoff's ends and middle: no state runs
/// away and the output never sits at full scale.
#[test]
fn the_6581_filter_is_stable_across_the_non_linearity_range() {
    for step in 0..=8 {
        let amount = step as f32 * 0.25;
        for fc in [0u16, 64, 300, 1024, 2047] {
            for mode in [0x1fu8, 0x2f, 0x4f] {
                let mut s = new_sid(SID_MODEL_6581);
                unsafe { sid_set_filter_nonlinearity(&mut *s, amount) };
                for (v, (freq, wave)) in [(0x1d45u16, 0x21u8), (0x0b00, 0x81), (0x2b00, 0x41)]
                    .into_iter()
                    .enumerate()
                {
                    let b = 7 * v as u8;
                    write(&mut s, b, (freq & 0xff) as u8);
                    write(&mut s, b + 1, (freq >> 8) as u8);
                    write(&mut s, b + 2, 0x00);
                    write(&mut s, b + 3, 0x08);
                    write(&mut s, b + 5, 0x00);
                    write(&mut s, b + 6, 0xf0);
                    write(&mut s, b + 4, wave);
                }
                write(&mut s, 0x15, (fc & 7) as u8);
                write(&mut s, 0x16, (fc >> 3) as u8);
                write(&mut s, 0x17, 0xf7);
                write(&mut s, 0x18, mode);
                let out = render(&mut s, 44_100 / 4);
                let f = s.svf;
                for x in [f.ic1, f.ic2, f.lp, f.bp, f.hp, f.bpn, s.out_lp] {
                    assert!(
                        x.is_finite() && x.abs() < 64.0,
                        "amount {amount}, FC {fc}, mode {mode:#x}: the state is {x}"
                    );
                }
                let pinned = out
                    .iter()
                    .filter(|&&x| x == i16::MAX || x == i16::MIN)
                    .count();
                assert!(
                    pinned < out.len() / 100,
                    "amount {amount}, FC {fc}, mode {mode:#x}: {pinned} samples at full scale"
                );
            }
        }
    }
}

/// The 6581's band-pass mix is over the damping (sid_svf_tick_6581's bpn), so it follows
/// every write that sets the damping: a $15/$16 write after new tables (sid_set_filter_curve)
/// takes the new damping, and the mix with it.
#[test]
fn the_6581_band_pass_mix_follows_the_damping_at_every_filter_write() {
    let mut s = new_sid(SID_MODEL_6581);
    write(&mut s, 0x17, 0xf1);
    write(&mut s, 0x18, 0x2f);
    crate::settle(&mut s);
    assert_eq!(s.mix_bpn, 0.8125 / s.svf.k);
    for reg in [0x15u8, 0x16] {
        s.res_k.fill(1.5 + f32::from(reg - 0x15)); // as a new curve's tables would
        write(&mut s, reg, 0x40);
        crate::settle(&mut s);
        assert_eq!(s.svf.k, 1.5 + f32::from(reg - 0x15));
        assert_eq!(s.mix_bpn, 0.8125 / s.svf.k, "after ${reg:02x}");
    }
}

/// The 6581's lowest cutoff (sid_nonlin's floor) is the chip's: it follows the cutoff table
/// at FC 0 as a filter curve setting or a blend moves it (the darker settings reach lower, as
/// reSIDfp's do), and an amount above 1 does not raise it. At the default curve and amounts up
/// to 1 it is what M1b fitted, bit for bit.
#[test]
fn the_6581_lowest_cutoff_follows_the_filter_table() {
    let floor = |curve: u32, amount: f32| {
        let mut s = new_sid(SID_MODEL_6581);
        unsafe {
            sid_set_filter_curve(&mut *s, curve);
            sid_set_filter_nonlinearity(&mut *s, amount);
        }
        (s.nl.floor, s.cutoff_g[0])
    };
    let (f0, g0) = floor(0, 1.0);
    assert_eq!(
        f0,
        new_sid(SID_MODEL_6581).nl.floor,
        "the default, unchanged"
    );
    assert_eq!(floor(0, 0.5).0, 0.5 * f0);
    assert_eq!(
        floor(0, 2.0).0,
        f0,
        "a stronger non-linearity keeps the chip's floor"
    );
    let last = unsafe { sid_filter_curves(SID_MODEL_6581) } - 1;
    let (f, g) = floor(last, 1.0);
    assert!(g < g0 && f < f0, "a darker curve setting: {f} against {f0}");
    assert!(
        (f / f0 - g / g0).abs() < 1e-6,
        "in proportion to the table at FC 0"
    );
}
