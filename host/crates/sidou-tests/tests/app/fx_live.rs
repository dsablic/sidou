// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::c_void;

use sidou_sys::*;

use crate::{rms, Rig};

const fn slot_reg(slot: u32, field: u32) -> u8 {
    (LIVE_REG_SLOT_BASE | slot << 3 | field) as u8
}

#[test]
fn slot_settings_are_clamped_and_sent_once_per_change() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.clear();
    unsafe {
        live_slot_set(&mut *rig.live, 1, LIVE_SLOT_AMOUNT, 200);
        live_slot_set(&mut *rig.live, 1, LIVE_SLOT_AMOUNT, 127);
        live_slot_set(&mut *rig.live, 2, LIVE_SLOT_ALGORITHM, 99);
        live_slot_set(&mut *rig.live, 0, LIVE_SLOT_MIX, -5);
        live_slot_set(&mut *rig.live, 3, LIVE_SLOT_MIX, 10);
    }
    assert_eq!(
        rig.log(),
        &[
            (slot_reg(1, LIVE_SLOT_AMOUNT), 127),
            (
                slot_reg(2, LIVE_SLOT_ALGORITHM),
                (SID_FXSLOT_COUNT - 1) as u8
            )
        ],
        "clamped, sent only on change, nothing for a slot that does not exist"
    );
    assert_eq!(rig.live.slot[1][LIVE_SLOT_AMOUNT as usize], 127);
}

#[test]
fn algorithms_cycle_and_controls_turn_by_detents() {
    let mut rig = Rig::new(SID_MODEL_6581);
    for _ in 0..SID_MOD_COUNT {
        unsafe { live_slot_next_algorithm(&mut *rig.live, 1) };
    }
    assert_eq!(rig.live.slot[1][0], 0, "wraps from the last to NONE");
    unsafe { live_slot_next_algorithm(&mut *rig.live, 1) };
    assert_eq!(rig.live.slot[1][0], SID_MOD_CHORUS as u8);
    let a = unsafe { live_fx_defaults[1][SID_MOD_CHORUS as usize] }[1];
    assert_eq!(
        rig.live.slot[1][LIVE_SLOT_A as usize], a,
        "CHORUS starts from its defaults"
    );
    unsafe { live_slot_turn(&mut *rig.live, 1, LIVE_SLOT_A, 3) };
    let w = unsafe { live_walk_slot(1, SID_MOD_CHORUS, LIVE_SLOT_A) };
    assert_eq!(
        u32::from(rig.live.slot[1][LIVE_SLOT_A as usize]),
        unsafe { live_walk_step(w, u32::from(a), 3) },
        "three detents along the rate's walk"
    );
    unsafe { live_slot_turn(&mut *rig.live, 1, LIVE_SLOT_A, 100) };
    assert_eq!(rig.live.slot[1][LIVE_SLOT_A as usize], LIVE_SLOT_MAX as u8);
    unsafe { live_slot_turn(&mut *rig.live, 1, LIVE_SLOT_ALGORITHM, 3) };
    assert_eq!(
        rig.live.slot[1][0], SID_MOD_CHORUS as u8,
        "the algorithm is not a knob"
    );
}

#[test]
fn slot_ccs_map_slot_and_field() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.clear();
    let (dist_amount, dist_algorithm, fx_algorithm, fx_mix, modulation_b) =
        (103, 102, 112, 116, 110);
    rig.cc(dist_amount, 64);
    rig.cc(dist_algorithm, 127);
    rig.cc(fx_algorithm, 80);
    rig.cc(fx_mix, 127);
    rig.cc(modulation_b, 33);
    assert_eq!(
        rig.log(),
        &[
            (slot_reg(0, LIVE_SLOT_AMOUNT), 66),
            (slot_reg(0, LIVE_SLOT_ALGORITHM), (SID_DIST_COUNT - 1) as u8),
            (
                slot_reg(2, LIVE_SLOT_ALGORITHM),
                SID_FXSLOT_REVERB_LIGHT as u8
            ),
            (slot_reg(2, LIVE_SLOT_MIX), 127),
            (slot_reg(1, LIVE_SLOT_B), 32),
        ],
        "CC values land on the controls' walks"
    );
    for slot in 0..LIVE_SLOTS {
        let count = unsafe { live_slot_algorithms(slot) };
        let reached: std::collections::BTreeSet<u8> = (0..128u8)
            .map(|v| {
                rig.cc(LIVE_CC_SLOT_FIRST + slot * LIVE_SLOT_FIELDS, v);
                rig.live.slot[slot as usize][0]
            })
            .collect();
        assert_eq!(
            reached.len() as u32,
            count,
            "slot {slot}: every algorithm is reachable by CC"
        );
    }
}

struct Panel {
    rig: Rig,
    p: Box<panel>,
}

impl Panel {
    fn new() -> Self {
        let mut p = Box::<panel>::default();
        unsafe { panel_init(&mut *p) };
        let mut rig = Rig::new(SID_MODEL_6581);
        rig.clear();
        Panel { rig, p }
    }

    fn buttons(&mut self, pressed: u32, held: u32) {
        unsafe { panel_buttons(&mut *self.p, &mut *self.rig.live, pressed, held) }
    }

    fn enc(&mut self, enc: u32, steps: i32) {
        unsafe { panel_encoder(&mut *self.p, &mut *self.rig.live, enc, steps) }
    }

    fn leds(&self) -> panel_leds {
        unsafe { panel_get_leds(&*self.p, &*self.rig.live, std::ptr::null()) }
    }
}

#[test]
fn edit_held_turns_k1_k4_into_the_selected_slots_controls() {
    let mut pn = Panel::new();
    let edit = 1 << PANEL_BTN_EDIT;
    pn.buttons(1 << PANEL_BTN_SCL, 0);
    pn.buttons(1 << PANEL_BTN_SCL, 0);
    unsafe { live_slot_next_algorithm(&mut *pn.rig.live, 2) };
    assert_eq!(pn.rig.live.fx_select, 2);
    assert_eq!(pn.rig.live.slot[2][0], SID_FXSLOT_DELAY as u8);
    assert!(
        pn.leds().buttons & edit != 0,
        "EDIT lit: the selected slot has an algorithm"
    );
    let cutoff = pn.rig.live.cutoff;
    pn.buttons(0, edit);
    for (i, enc) in (PANEL_ENC_K1..=PANEL_ENC_K4).enumerate() {
        pn.enc(enc, i as i32 + 1);
    }
    let s = pn.rig.live.slot[2];
    let f = unsafe { live_fx_defaults[2][SID_FXSLOT_DELAY as usize] };
    let after = |k: u32| unsafe {
        live_walk_step(
            live_walk_slot(2, SID_FXSLOT_DELAY, LIVE_SLOT_AMOUNT + k),
            u32::from(f[k as usize]),
            k as i32 + 1,
        ) as u8
    };
    assert_eq!(
        &s[1..],
        &[after(0), after(1), after(2), after(3)],
        "Amount, A, B, Mix from DELAY's defaults, along their walks"
    );
    assert_eq!(pn.rig.live.cutoff, cutoff, "K1 did not move the cutoff");
    pn.buttons(0, 0);
    pn.enc(PANEL_ENC_K1, -1);
    assert_ne!(
        pn.rig.live.cutoff, cutoff,
        "EDIT let go: K1 is the cutoff again"
    );
    assert_eq!(pn.rig.live.slot[2][1], after(0));
    pn.buttons(1 << PANEL_BTN_SCL, 0);
    assert_eq!(pn.rig.live.fx_select, 0, "SCL wraps back to Distortion");
    assert_eq!(pn.leds().buttons & edit, 0, "Distortion is off: EDIT dark");
}

const SAMPLE_RATE: u32 = 44100;
const HELD_TAIL_SAMPLES: usize = 17640;

fn slots_box() -> (Box<sid_slots>, Box<sid_slot_pool>) {
    let mut pool = Box::<sid_slot_pool>::default();
    let mut s = Box::<sid_slots>::default();
    unsafe { sid_slots_init(&mut *s, SAMPLE_RATE, 1, &mut *pool) };
    (s, pool)
}

fn fx_live_box() -> (Box<sid_slots>, Box<sid_slot_pool>, Box<fx_live>) {
    let (mut s, pool) = slots_box();
    let mut f = Box::<fx_live>::default();
    unsafe { fx_live_init(&mut *f, &mut *s) };
    (s, pool, f)
}

fn process_slots(s: &mut sid_slots, input: &[i16]) -> Vec<i16> {
    let mut out = Vec::with_capacity(2 * input.len());
    for b in input.chunks(128) {
        let mut o = vec![0i16; 2 * b.len()];
        unsafe { sid_slots_process(s, b.as_ptr(), o.as_mut_ptr(), b.len() as u32) };
        out.extend(o);
    }
    out
}

fn unit(v: u8) -> f32 {
    f32::from(v) / 127.0
}

#[test]
fn fx_live_applies_slot_registers_and_nothing_else() {
    let (s, _pool, mut f) = fx_live_box();
    let apply = |f: &mut fx_live, reg: u8, v: u8| unsafe { fx_live_apply(f, reg, v) };
    assert_eq!(
        apply(
            &mut f,
            slot_reg(1, LIVE_SLOT_ALGORITHM),
            SID_MOD_FLANGER as u8
        ),
        1
    );
    assert_eq!(apply(&mut f, slot_reg(1, LIVE_SLOT_A), 127), 1);
    assert_eq!(apply(&mut f, slot_reg(1, LIVE_SLOT_MIX), 64), 1);
    let t = s.slot[1].target;
    assert_eq!(t.algorithm, SID_MOD_FLANGER);
    assert_eq!(t.a, 1.0);
    assert!((t.mix - 64.0 / 127.0).abs() < 1e-6);
    let (sid_register, field_past_mix, slot_past_fx) = (0x11u8, slot_reg(0, 5), slot_reg(3, 0));
    for reg in [
        sid_register,
        LIVE_REG_RESET as u8,
        field_past_mix,
        slot_past_fx,
    ] {
        assert_eq!(apply(&mut f, reg, 1), 0, "reg {reg:#x}");
    }
}

#[derive(Debug, PartialEq)]
enum Delivered {
    Slot(u8, u8),
    Cutoff,
    Resonance,
}

fn active_fc_res(chips: *const reg_ring_chips) -> (u32, u8) {
    let c = unsafe { &*chips };
    let chip = &c.chip[usize::from(c.active[0])];
    (chip.fc, chip.res)
}

struct Recorder {
    fx: *mut fx_live,
    chips: *const reg_ring_chips,
    fc: u32,
    res: u8,
    seen: Vec<Delivered>,
}

impl Recorder {
    fn record_sid_writes_as_chip_changes(&mut self) {
        let (fc, res) = active_fc_res(self.chips);
        if fc != self.fc {
            self.fc = fc;
            self.seen.push(Delivered::Cutoff);
        }
        if res != self.res {
            self.res = res;
            self.seen.push(Delivered::Resonance);
        }
    }
}

unsafe extern "C" fn record_pseudo(ctx: *mut c_void, reg: u8, value: u8) {
    let r = unsafe { &mut *ctx.cast::<Recorder>() };
    r.record_sid_writes_as_chip_changes();
    r.seen.push(Delivered::Slot(reg, value));
    unsafe { fx_live_pseudo(r.fx.cast(), reg, value) };
}

#[test]
fn slot_settings_and_sid_writes_leave_the_register_ring_in_the_order_live_sent_them() {
    let (s, _pool, mut f) = fx_live_box();
    let mut ring = Box::<reg_ring>::default();
    let chips: *mut reg_ring_chips = Box::into_raw(Box::<reg_ring_chips>::default());
    unsafe {
        reg_ring_init(&mut *ring);
        reg_ring_chips_init(chips, SID_MODEL_6581, SID_CLOCK_PAL, SAMPLE_RATE);
    }
    unsafe extern "C" fn drain_now(ctx: *mut c_void) {
        let o = unsafe { &mut *ctx.cast::<reg_ring_out>() };
        unsafe { reg_ring_drain(o.ring, o.chips) };
    }
    let mut out = Box::new(reg_ring_out {
        ring: &mut *ring,
        chips,
        wait: Some(drain_now),
        wait_ctx: std::ptr::null_mut(),
        ..Default::default()
    });
    let out_ptr: *mut reg_ring_out = &mut *out;
    out.wait_ctx = out_ptr.cast();
    let mut l = Box::<live>::default();
    unsafe {
        live_init(&mut *l, Some(reg_ring_sink), out_ptr.cast(), SID_MODEL_6581);
        reg_ring_commit(&mut *ring);
        reg_ring_drain(&mut *ring, chips);
    }
    let (fc, res) = active_fc_res(chips);
    let mut rec = Box::new(Recorder {
        fx: &mut *f,
        chips,
        fc,
        res,
        seen: Vec::new(),
    });
    unsafe {
        (*chips).pseudo = Some(record_pseudo);
        (*chips).pseudo_ctx = (&mut *rec as *mut Recorder).cast();
    }

    let lush = SID_FXSLOT_REVERB_LUSH as u8;
    unsafe {
        live_slot_set(&mut *l, 2, LIVE_SLOT_ALGORITHM, i32::from(lush));
        live_cc(&mut *l, LIVE_CC_CUTOFF as u8, 127);
        live_slot_set(&mut *l, 2, LIVE_SLOT_MIX, 127);
        live_cc(&mut *l, LIVE_CC_RESONANCE as u8, 127);
        live_slot_set(&mut *l, 2, LIVE_SLOT_AMOUNT, 64);
    }
    assert!(rec.seen.is_empty(), "nothing before the drain");
    assert_eq!(s.slot[2].target.algorithm, 0, "nothing before the drain");
    unsafe {
        reg_ring_commit(&mut *ring);
        reg_ring_drain(&mut *ring, chips);
    }
    rec.record_sid_writes_as_chip_changes();
    assert_eq!(
        rec.seen,
        [
            Delivered::Slot(slot_reg(2, LIVE_SLOT_ALGORITHM), lush),
            Delivered::Cutoff,
            Delivered::Slot(slot_reg(2, LIVE_SLOT_MIX), 127),
            Delivered::Resonance,
            Delivered::Slot(slot_reg(2, LIVE_SLOT_AMOUNT), 64),
        ],
        "delivered in the order sent"
    );
    assert_eq!(s.slot[2].target.algorithm, SID_FXSLOT_REVERB_LUSH);
    assert_eq!(s.slot[2].target.mix, 1.0);
    assert_eq!(unsafe { (*chips).lost_resets }, 0);
    drop(out);
    drop(unsafe { Box::from_raw(chips) });
}

#[test]
fn the_host_sink_never_writes_a_slot_register_to_the_chip() {
    let mut chip = Box::<sid>::default();
    unsafe { sid_init(&mut *chip, SID_MODEL_6581, SID_CLOCK_PAL, SAMPLE_RATE) };
    let before = format!("{:?}", chip.osc[0]);
    let mut t = live_sid_target {
        chip: &mut *chip,
        clock_hz: SID_CLOCK_PAL,
        sample_rate: SAMPLE_RATE,
        ..Default::default()
    };
    let aliases_voice_1_frequency_high = slot_reg(0, 1);
    unsafe {
        live_sid_sink(
            (&mut t as *mut live_sid_target).cast(),
            aliases_voice_1_frequency_high,
            0xff,
        )
    };
    assert_eq!(format!("{:?}", chip.osc[0]), before);
}

fn wet_minus_dry_rms_over_dry_on_a_220_hz_saw(slot: u32, alg: u32) -> f64 {
    let n = 22050usize;
    let saw: Vec<i16> = (0..n)
        .map(|i| {
            (((i as f64 * 220.0 / f64::from(SAMPLE_RATE)).fract() * 2.0 - 1.0) * 16384.0) as i16
        })
        .collect();
    let d = unsafe { live_fx_defaults[slot as usize][alg as usize] };
    let run = |p: sid_slot_params| {
        let (mut s, _pool) = slots_box();
        unsafe { sid_slots_set(&mut *s, slot, &p) };
        process_slots(&mut s, &saw)
    };
    let wet = run(sid_slot_params {
        algorithm: alg,
        amount: unit(d[0]),
        a: unit(d[1]),
        b: unit(d[2]),
        mix: unit(d[3]),
    });
    let dry = run(sid_slot_params {
        algorithm: 0,
        amount: 0.0,
        a: 0.0,
        b: 0.0,
        mix: 0.0,
    });
    let from = 2 * (n - HELD_TAIL_SAMPLES);
    let diff: Vec<f64> = (from..2 * n)
        .map(|i| f64::from(wet[i]) - f64::from(dry[i]))
        .collect();
    let dry: Vec<f64> = dry[from..].iter().map(|&v| f64::from(v)).collect();
    rms(&diff) / rms(&dry)
}

#[test]
fn every_default_is_audible() {
    for slot in 0..LIVE_SLOTS {
        for alg in 1..unsafe { live_slot_algorithms(slot) } {
            let r = wet_minus_dry_rms_over_dry_on_a_220_hz_saw(slot, alg);
            assert!(
                r >= 0.1,
                "slot {slot} algorithm {alg}: adds {:.1} dB of the dry",
                20.0 * r.log10()
            );
        }
        assert_eq!(
            unsafe { live_fx_defaults[slot as usize][0] },
            [0; 4],
            "OFF's row is zeros"
        );
    }
}

fn saw_lead_a4_as_the_device_renders_it() -> Vec<i16> {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.note_on(69);
    let mut chip = Box::<sid>::default();
    unsafe { sid_init(&mut *chip, SID_MODEL_6581, SID_CLOCK_PAL, SAMPLE_RATE) };
    for &(r, v) in rig.log() {
        if r <= 0x18 {
            unsafe { sid_write(&mut *chip, r, v) };
        }
    }
    let mut out = vec![0i16; 26460];
    for c in out.chunks_mut(128) {
        unsafe { sid_render(&mut *chip, c.as_mut_ptr(), c.len() as u32) };
    }
    out
}

fn distorted_at_full_mix_summed_held_tail(alg: u32, amount: u8, input: &[i16]) -> Vec<f64> {
    let d = unsafe { live_fx_defaults[0][alg as usize] };
    let (mut s, _pool) = slots_box();
    let p = sid_slot_params {
        algorithm: alg,
        amount: unit(amount),
        a: unit(d[1]),
        b: unit(d[2]),
        mix: 1.0,
    };
    unsafe { sid_slots_set(&mut *s, 0, &p) };
    let out: Vec<f64> = process_slots(&mut s, input)
        .chunks(2)
        .map(|lr| f64::from(lr[0]) + f64::from(lr[1]))
        .collect();
    out[out.len() - HELD_TAIL_SAMPLES..].to_vec()
}

const SILENCE_IS_AS_DIFFERENT_AS_CAN_BE_DB: f64 = 0.0;

fn shape_change_db_with_level_taken_out(a: &[f64], b: &[f64]) -> f64 {
    let (ra, rb) = (rms(a), rms(b));
    if ra == 0.0 || rb == 0.0 {
        return SILENCE_IS_AS_DIFFERENT_AS_CAN_BE_DB;
    }
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| x / ra - y / rb).collect();
    20.0 * rms(&d).log10()
}

#[test]
fn every_distortion_amount_changes_the_shape_not_the_level_of_a_saw_lead_note() {
    let note = saw_lead_a4_as_the_device_renders_it();
    let held = &note[note.len() - HELD_TAIL_SAMPLES..];
    let peak = held.iter().map(|&v| i32::from(v).abs()).max().unwrap();
    let level: Vec<f64> = held.iter().map(|&v| f64::from(v)).collect();
    println!(
        "SAW LEAD A4 at the slot's input: rms {:.1} dBFS, peak {:.1} dBFS",
        20.0 * (rms(&level) / 32768.0).log10(),
        20.0 * (f64::from(peak) / 32768.0).log10()
    );
    let mut weak = Vec::new();
    for alg in 1..unsafe { live_slot_algorithms(0) } {
        let lo = distorted_at_full_mix_summed_held_tail(alg, 0, &note);
        let hi = distorted_at_full_mix_summed_held_tail(alg, 127, &note);
        let shape = shape_change_db_with_level_taken_out(&lo, &hi);
        let level = 20.0 * (rms(&hi).max(1e-9) / rms(&lo).max(1e-9)).log10();
        println!(
            "DIST {alg}: Amount 0 -> 127 changes the shape {shape:.1} dB, the level {level:+.1} dB"
        );
        if shape < -15.0 || rms(&hi) < 0.1 * rms(&lo) || level.abs() > 6.0 {
            weak.push(format!("{alg}: shape {shape:.1} dB, level {level:+.1} dB"));
        }
    }
    assert!(weak.is_empty(), "{weak:#?}");
}
