// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The effect slots in the live instrument (`app/fx_live.c`, live's slot settings, the
//! panel's EDIT layer and the slot CCs).

use std::ffi::c_void;

use sidou_sys::*;

use crate::Rig;

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
    // CHORUS starts from its defaults (live_fx_defaults); the detents add to them.
    let a = unsafe { live_fx_defaults[1][SID_MOD_CHORUS as usize] }[1];
    assert_eq!(rig.live.slot[1][LIVE_SLOT_A as usize], a);
    unsafe { live_slot_turn(&mut *rig.live, 1, LIVE_SLOT_A, 3) };
    assert_eq!(
        rig.live.slot[1][LIVE_SLOT_A as usize],
        a + (3 * LIVE_SLOT_DETENT) as u8
    );
    unsafe { live_slot_turn(&mut *rig.live, 1, LIVE_SLOT_A, 100) };
    assert_eq!(rig.live.slot[1][LIVE_SLOT_A as usize], LIVE_SLOT_MAX as u8);
    // The algorithm is not a knob.
    unsafe { live_slot_turn(&mut *rig.live, 1, LIVE_SLOT_ALGORITHM, 3) };
    assert_eq!(rig.live.slot[1][0], SID_MOD_CHORUS as u8);
}

#[test]
fn slot_ccs_map_slot_and_field() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.clear();
    rig.cc(103, 64); // Distortion Amount
    rig.cc(102, 127); // Distortion algorithm: the last
    rig.cc(112, 80); // FX algorithm: 80 * 5 / 128 = 3, the light reverb
    rig.cc(116, 127); // FX Mix
    rig.cc(110, 33); // Modulation B
    assert_eq!(
        rig.log(),
        &[
            (slot_reg(0, LIVE_SLOT_AMOUNT), 64),
            (slot_reg(0, LIVE_SLOT_ALGORITHM), (SID_DIST_COUNT - 1) as u8),
            (
                slot_reg(2, LIVE_SLOT_ALGORITHM),
                SID_FXSLOT_REVERB_LIGHT as u8
            ),
            (slot_reg(2, LIVE_SLOT_MIX), 127),
            (slot_reg(1, LIVE_SLOT_B), 33),
        ]
    );
    // Every algorithm of every slot is reachable by CC.
    for slot in 0..LIVE_SLOTS {
        let count = unsafe { live_slot_algorithms(slot) };
        let reached: std::collections::BTreeSet<u8> = (0..128u8)
            .map(|v| {
                rig.cc(LIVE_CC_SLOT_FIRST + slot * LIVE_SLOT_FIELDS, v);
                rig.live.slot[slot as usize][0]
            })
            .collect();
        assert_eq!(reached.len() as u32, count, "slot {slot}");
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
    // SCL twice: the FX slot; ENV: its first algorithm.
    pn.buttons(1 << PANEL_BTN_SCL, 0);
    pn.buttons(1 << PANEL_BTN_SCL, 0);
    pn.buttons(1 << PANEL_BTN_ENV, 0);
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
    let d = LIVE_SLOT_DETENT as u8;
    let f = unsafe { live_fx_defaults[2][SID_FXSLOT_DELAY as usize] };
    assert_eq!(
        &s[1..],
        &[f[0] + d, f[1] + 2 * d, f[2] + 3 * d, f[3] + 4 * d],
        "Amount, A, B, Mix from DELAY's defaults"
    );
    assert_eq!(pn.rig.live.cutoff, cutoff, "K1 did not move the cutoff");
    // Let go of EDIT: K1 is the cutoff again.
    pn.buttons(0, 0);
    pn.enc(PANEL_ENC_K1, -1);
    assert_ne!(pn.rig.live.cutoff, cutoff);
    assert_eq!(pn.rig.live.slot[2][1], f[0] + d);
    // SCL wraps back to Distortion, which is off: EDIT dark.
    pn.buttons(1 << PANEL_BTN_SCL, 0);
    assert_eq!(pn.rig.live.fx_select, 0);
    assert_eq!(pn.leds().buttons & edit, 0);
}

fn slots_box() -> (Box<sid_slots>, Box<sid_slot_pool>) {
    let mut pool = Box::<sid_slot_pool>::default();
    let mut s = Box::<sid_slots>::default();
    unsafe { sid_slots_init(&mut *s, 44100, 1, &mut *pool) };
    (s, pool)
}

#[test]
fn fx_live_applies_slot_registers_and_nothing_else() {
    let (mut s, _pool) = slots_box();
    let mut f = Box::<fx_live>::default();
    unsafe { fx_live_init(&mut *f, &mut *s) };
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
    // Not slot registers: a SID register, the reset, a field past Mix, a slot past FX.
    for reg in [0x11u8, LIVE_REG_RESET as u8, slot_reg(0, 5), slot_reg(3, 0)] {
        assert_eq!(apply(&mut f, reg, 1), 0, "reg {reg:#x}");
    }
}

/// What the drain delivered, in order: a pseudo-register apply, or a SID write seen as a
/// change in the active chip's cutoff or resonance.
#[derive(Debug, PartialEq)]
enum Delivered {
    Slot(u8, u8),
    Cutoff,
    Resonance,
}

/// Wraps fx_live_pseudo: before each apply, records the SID writes that landed since the
/// last one (the drain has no SID-write hook, so they show as chip state changes).
struct Recorder {
    fx: *mut fx_live,
    chips: *const reg_ring_chips,
    fc: u32,
    res: u8,
    seen: Vec<Delivered>,
}

impl Recorder {
    fn catch_up(&mut self) {
        let c = unsafe { &*self.chips };
        let chip = &c.chip[usize::from(c.active[0])];
        if chip.fc != self.fc {
            self.fc = chip.fc;
            self.seen.push(Delivered::Cutoff);
        }
        if chip.res != self.res {
            self.res = chip.res;
            self.seen.push(Delivered::Resonance);
        }
    }
}

unsafe extern "C" fn record_pseudo(ctx: *mut c_void, reg: u8, value: u8) {
    let r = unsafe { &mut *ctx.cast::<Recorder>() };
    r.catch_up();
    r.seen.push(Delivered::Slot(reg, value));
    unsafe { fx_live_pseudo(r.fx.cast(), reg, value) };
}

/// The device's path: live -> reg_ring_sink -> ring -> drain -> pseudo handler -> slots.
/// Slot settings and SID writes interleaved in one pass come out of the drain in the order
/// live sent them.
#[test]
fn slot_settings_cross_the_register_ring_in_order() {
    let (mut s, _pool) = slots_box();
    let mut f = Box::<fx_live>::default();
    unsafe { fx_live_init(&mut *f, &mut *s) };
    let mut ring = Box::<reg_ring>::default();
    let chips: *mut reg_ring_chips = Box::into_raw(Box::<reg_ring_chips>::default());
    unsafe {
        reg_ring_init(&mut *ring);
        reg_ring_chips_init(chips, SID_MODEL_6581, SID_CLOCK_PAL, 44100);
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
    let (fc, res) = unsafe {
        let c = &*chips;
        let chip = &c.chip[usize::from(c.active[0])];
        (chip.fc, chip.res)
    };
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
    rec.catch_up();
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
    unsafe { sid_init(&mut *chip, SID_MODEL_6581, SID_CLOCK_PAL, 44100) };
    let before = format!("{:?}", chip.osc[0]);
    let mut t = live_sid_target {
        chip: &mut *chip,
        clock_hz: SID_CLOCK_PAL,
        sample_rate: 44100,
        ..Default::default()
    };
    // 0xA1 would alias to register 0x01 (voice 1's frequency high byte).
    unsafe {
        live_sid_sink(
            (&mut t as *mut live_sid_target).cast(),
            slot_reg(0, 1),
            0xff,
        )
    };
    assert_eq!(format!("{:?}", chip.osc[0]), before);
}

fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
}

/// What a slot adds to a 220 Hz saw at the algorithm's defaults: the RMS of wet minus dry
/// over the last 0.4 s, both channels, against the dry RMS.
fn added_by(slot: u32, alg: u32) -> f64 {
    let n = 22050usize;
    let saw: Vec<i16> = (0..n)
        .map(|i| (((i as f64 * 220.0 / 44100.0).fract() * 2.0 - 1.0) * 16384.0) as i16)
        .collect();
    let d = unsafe { live_fx_defaults[slot as usize][alg as usize] };
    let run = |p: sid_slot_params| {
        let (mut s, _pool) = slots_box();
        unsafe { sid_slots_set(&mut *s, slot, &p) };
        let mut out = Vec::new();
        for b in saw.chunks(128) {
            let mut o = vec![0i16; 2 * b.len()];
            unsafe { sid_slots_process(&mut *s, b.as_ptr(), o.as_mut_ptr(), b.len() as u32) };
            out.extend(o);
        }
        out
    };
    let f = |v: u8| f32::from(v) / 127.0;
    let wet = run(sid_slot_params {
        algorithm: alg,
        amount: f(d[0]),
        a: f(d[1]),
        b: f(d[2]),
        mix: f(d[3]),
    });
    let dry = run(sid_slot_params {
        algorithm: 0,
        amount: 0.0,
        a: 0.0,
        b: 0.0,
        mix: 0.0,
    });
    let from = 2 * (n - 17640);
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
            let r = added_by(slot, alg);
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
