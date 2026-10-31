// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::c_void;

use sidou_sys::*;

use crate::{PANEL_ENC_K2, PANEL_ENC_K3};

const RATE: u32 = 44_118;
const SIZE: u32 = REG_RING_SIZE;
const RESET: u8 = LIVE_REG_RESET as u8;
const GATE_RESET: u8 = LIVE_REG_GATE_RESET as u8;
const FILTER_ROUTING: u8 = 0x17;
const UNKNOWN_PSEUDO_REGISTER: u8 = 0x9f;
const THUMP: u8 = (LIVE_REG_CHAR_FIRST + 3) as u8;

fn ring() -> Box<reg_ring> {
    let mut r = Box::<reg_ring>::default();
    unsafe { reg_ring_init(&mut *r) };
    r
}

fn push(r: &mut reg_ring, reg: u8, value: u8) -> bool {
    unsafe { reg_ring_push(r, reg, value) != 0 }
}

fn pop(r: &mut reg_ring) -> Option<(u8, u8)> {
    let mut e = reg_ring_entry::default();
    (unsafe { reg_ring_pop(r, &mut e) } != 0).then_some((e.reg, e.value))
}

fn commit(r: &mut reg_ring) {
    unsafe { reg_ring_commit(r) }
}

fn chips(model: sid_model) -> Box<reg_ring_chips> {
    let mut c = Box::<reg_ring_chips>::default();
    unsafe { reg_ring_chips_init(&mut *c, model, SID_CLOCK_PAL, RATE) };
    c
}

fn render(s: &mut sid, n: usize) -> Vec<i16> {
    let mut out = vec![0i16; n];
    unsafe { sid_render(s, out.as_mut_ptr(), n as u32) };
    out
}

fn thump_after(ticks: u32) -> f32 {
    perceptual_share(ticks * SID_CHARACTER_RAMP) * SID_CHARACTER_THUMP as f32
}

fn perceptual_share(v: u32) -> f32 {
    let a = v as f32 / 127.0;
    let l = SID_CHARACTER_CURVE_LINEAR as f32;
    a * (l + (1.0 - l) * a)
}

#[test]
fn entries_come_out_in_order_and_only_once_committed() {
    let mut r = ring();
    assert!(push(&mut r, 0x01, 0x11));
    assert!(push(&mut r, 0x02, 0x22));
    assert_eq!(pop(&mut r), None, "nothing is visible before the commit");
    assert_eq!(unsafe { reg_ring_used(&*r) }, 2);
    commit(&mut r);
    assert!(push(&mut r, 0x03, 0x33));
    assert_eq!(pop(&mut r), Some((0x01, 0x11)));
    assert_eq!(pop(&mut r), Some((0x02, 0x22)));
    assert_eq!(pop(&mut r), None, "the third is still staged");
    commit(&mut r);
    assert_eq!(pop(&mut r), Some((0x03, 0x33)));
    assert_eq!(unsafe { reg_ring_used(&*r) }, 0);
}

#[test]
fn a_full_ring_refuses_and_keeps_every_entry() {
    let mut r = ring();
    for i in 0..SIZE {
        assert!(push(&mut r, (i % 25) as u8, i as u8), "entry {i}");
    }
    assert!(!push(&mut r, 0x18, 0xff), "the ring holds {SIZE}");
    commit(&mut r);
    assert!(
        !push(&mut r, 0x18, 0xff),
        "committed entries still take room"
    );
    assert_eq!(pop(&mut r), Some((0, 0)));
    assert!(push(&mut r, 0x18, 0xee), "a pop makes room for one");
    assert!(!push(&mut r, 0x18, 0xff));
    commit(&mut r);
    for i in 1..SIZE {
        assert_eq!(pop(&mut r), Some(((i % 25) as u8, i as u8)), "entry {i}");
    }
    assert_eq!(pop(&mut r), Some((0x18, 0xee)));
    assert_eq!(pop(&mut r), None);
}

#[test]
fn the_indexes_wrap_around_u32() {
    let mut r = ring();
    let start = u32::MAX - 3;
    r.head = start;
    r.tail = start;
    r.stage = start;
    for i in 0..10u8 {
        assert!(push(&mut r, i, i));
    }
    commit(&mut r);
    assert_eq!(unsafe { reg_ring_used(&*r) }, 10);
    for i in 0..10u8 {
        assert_eq!(pop(&mut r), Some((i, i)));
    }
    assert_eq!(pop(&mut r), None);
}

#[test]
fn the_drain_applies_registers_to_the_active_chip() {
    let mut r = ring();
    let mut c = chips(SID_MODEL_6581);
    push(&mut r, 0x00, 0x34);
    push(&mut r, 0x01, 0x12);
    commit(&mut r);
    assert_eq!(unsafe { reg_ring_drain(&mut *r, &mut *c) }, 2);
    assert_eq!(c.active[0], 0);
    assert_eq!(c.chip[0].osc[0].freq, 0x1234);
}

#[test]
fn a_reset_entry_never_reaches_sid_write() {
    let mut r = ring();
    let mut c = chips(SID_MODEL_6581);
    push(&mut r, 0x00, 0x34);
    push(&mut r, RESET, SID_MODEL_8580 as u8);
    push(&mut r, UNKNOWN_PSEUDO_REGISTER, 0x99);
    commit(&mut r);
    assert_eq!(unsafe { reg_ring_drain(&mut *r, &mut *c) }, 3);
    assert_eq!(
        c.chip[0].osc[0].freq, 0x34,
        "0x80 and 0x9f left voice 1 alone"
    );
    assert_eq!(c.chip[0].osc[1].freq, 0);
    assert_eq!(c.active[0], 0, "no spare was prepared: no swap");
    assert_eq!(c.lost_resets, 1);
}

#[test]
fn a_reset_entry_swaps_to_the_prepared_spare() {
    let mut r = ring();
    let mut c = chips(SID_MODEL_6581);
    unsafe { sid_init(&mut c.chip[2], SID_MODEL_8580, SID_CLOCK_PAL, RATE) };
    c.spare_model = SID_MODEL_8580 as u8;
    c.spare_ready = 1;
    push(&mut r, RESET, SID_MODEL_8580 as u8);
    push(&mut r, 0x00, 0x56);
    commit(&mut r);
    unsafe { reg_ring_drain(&mut *r, &mut *c) };
    assert_eq!(c.active[0], 2);
    assert_eq!(c.spare_ready, 0);
    assert_eq!(c.lost_resets, 0);
    assert_eq!(c.chip[2].model, SID_MODEL_8580);
    assert_eq!(
        c.chip[2].osc[0].freq, 0x56,
        "writes after the reset go to the new chip"
    );
    assert_eq!(c.chip[0].osc[0].freq, 0);
    assert!(std::ptr::eq(unsafe { reg_ring_chip(&mut *c) }, &c.chip[2]));
}

#[test]
fn a_gate_reset_entry_sets_the_option_on_the_active_chip_only() {
    let mut r = ring();
    let mut c = chips(SID_MODEL_6581);
    unsafe { sid_init(&mut c.chip[2], SID_MODEL_8580, SID_CLOCK_PAL, RATE) };
    c.spare_model = SID_MODEL_8580 as u8;
    c.spare_ready = 1;
    push(&mut r, GATE_RESET, 1);
    push(&mut r, 0x00, 0x12);
    commit(&mut r);
    assert_eq!(unsafe { reg_ring_drain(&mut *r, &mut *c) }, 2);
    assert!(c.chip[0].env.iter().all(|e| e.gate_reset == 1));
    assert_eq!(c.chip[0].osc[0].freq, 0x12, "it never reaches sid_write");
    assert!(c.chip[2].env.iter().all(|e| e.gate_reset == 0));
    push(&mut r, RESET, SID_MODEL_8580 as u8);
    push(&mut r, GATE_RESET, 1);
    commit(&mut r);
    unsafe { reg_ring_drain(&mut *r, &mut *c) };
    assert_eq!(c.active[0], 2);
    assert!(c.chip[2].env.iter().all(|e| e.gate_reset == 1));
    push(&mut r, GATE_RESET, 0);
    commit(&mut r);
    unsafe { reg_ring_drain(&mut *r, &mut *c) };
    assert!(c.chip[2].env.iter().all(|e| e.gate_reset == 0));
}

#[test]
fn a_mute_entry_reaches_the_chip_it_names_and_a_swap_keeps_it() {
    const MUTE: u8 = REG_RING_MUTE as u8;
    let mut r = ring();
    let mut c = chips(SID_MODEL_6581);
    unsafe { sid_init(&mut c.chip[2], SID_MODEL_8580, SID_CLOCK_PAL, RATE) };
    c.spare_model = SID_MODEL_8580 as u8;
    c.spare_ready = 1;
    c.spare_for = 0;
    push(&mut r, FILTER_ROUTING, 0x01);
    push(&mut r, MUTE, 0x05);
    push(&mut r, MUTE | REG_RING_CHIP_B as u8, 0x02);
    commit(&mut r);
    assert_eq!(unsafe { reg_ring_drain(&mut *r, &mut *c) }, 3);
    assert_eq!(c.chip[0].mute, 0x05);
    render(&mut c.chip[0], 0);
    assert_eq!(c.chip[0].voice_path, (0x02 * SID_PATH_DIRECT) as u8);
    assert_eq!(c.chip[1].mute, 0x02, "chip B's entry, chip B's sid");
    assert_eq!(c.chip[0].osc[0].freq, 0, "never a SID write");
    push(&mut r, RESET, SID_MODEL_8580 as u8);
    push(&mut r, FILTER_ROUTING, 0x03);
    commit(&mut r);
    unsafe { reg_ring_drain(&mut *r, &mut *c) };
    assert_eq!(c.active[0], 2);
    assert_eq!(c.chip[2].mute, 0x05);
    render(&mut c.chip[2], 0);
    assert_eq!(c.chip[2].voice_path, (0x02 * SID_PATH_FILTER) as u8);
    push(&mut r, MUTE, 0);
    commit(&mut r);
    unsafe { reg_ring_drain(&mut *r, &mut *c) };
    render(&mut c.chip[2], 0);
    assert_eq!(
        (c.chip[2].mute, c.chip[2].voice_path),
        (0, (0x03 * SID_PATH_FILTER + 0x04 * SID_PATH_DIRECT) as u8)
    );
}

#[test]
fn a_reset_for_another_model_than_the_spare_is_not_taken() {
    let mut r = ring();
    let mut c = chips(SID_MODEL_6581);
    c.spare_model = SID_MODEL_8580 as u8;
    c.spare_ready = 1;
    push(&mut r, RESET, SID_MODEL_6581 as u8);
    commit(&mut r);
    unsafe { reg_ring_drain(&mut *r, &mut *c) };
    assert_eq!((c.active[0], c.lost_resets), (0, 1));
}

struct Device {
    ring: Box<reg_ring>,
    chips: Box<reg_ring_chips>,
    out: Box<reg_ring_out>,
    live: Box<live>,
}

unsafe extern "C" fn drain_now(ctx: *mut c_void) {
    let o = unsafe { &mut *ctx.cast::<reg_ring_out>() };
    unsafe { reg_ring_drain(o.ring, o.chips) };
}

impl Device {
    fn new(model: sid_model) -> Self {
        let mut ring = ring();
        let mut chips = chips(model);
        let mut out = Box::new(reg_ring_out {
            ring: &mut *ring,
            chips: &mut *chips,
            wait: Some(drain_now),
            ..Default::default()
        });
        let o: *mut reg_ring_out = &mut *out;
        out.wait_ctx = o.cast();
        let mut l = Box::<live>::default();
        unsafe { live_init(&mut *l, Some(reg_ring_sink), o.cast(), model) };
        Device {
            ring,
            chips,
            out,
            live: l,
        }
    }

    fn block(&mut self) {
        commit(&mut self.ring);
        unsafe { reg_ring_pass_end(&mut *self.chips) };
        unsafe { reg_ring_drain(&mut *self.ring, &mut *self.chips) };
    }

    fn chip(&mut self) -> &mut sid {
        unsafe { &mut *reg_ring_chip(&mut *self.chips) }
    }
}

struct DirectSink {
    chip: Box<sid>,
    _target: Box<live_sid_target>,
    _dacs: Box<[sid_dac; 2]>,
    live: Box<live>,
}

impl DirectSink {
    fn new(model: sid_model) -> Self {
        let mut chip = Box::<sid>::default();
        let mut dacs = Box::<[sid_dac; 2]>::default();
        let mut target = Box::new(live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: RATE,
            dacs: dacs.as_mut_ptr(),
            ..Default::default()
        });
        let mut l = Box::<live>::default();
        let t: *mut live_sid_target = &mut *target;
        unsafe { live_init(&mut *l, Some(live_sid_sink), t.cast(), model) };
        DirectSink {
            chip,
            _target: target,
            _dacs: dacs,
            live: l,
        }
    }
}

fn play_into_both(dev: &mut Device, direct: &mut DirectSink, f: impl Fn(&mut live)) {
    f(&mut dev.live);
    f(&mut direct.live);
}

fn assert_same_sound(dev: &mut Device, direct: &mut DirectSink) {
    dev.block();
    let a = render(dev.chip(), 8192);
    let b = render(&mut direct.chip, 8192);
    assert!(a.iter().any(|&s| s != 0), "the test sound is silent");
    assert_eq!(a, b, "the ring changed what the chip plays");
}

#[test]
fn live_init_through_the_ring_matches_the_direct_chip() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut dev = Device::new(model);
        let mut direct = DirectSink::new(model);
        assert_eq!(dev.chips.lost_resets, 0);
        play_into_both(&mut dev, &mut direct, |l| unsafe {
            live_note_on(l, 60, 100)
        });
        assert_same_sound(&mut dev, &mut direct);
    }
}

#[test]
fn the_envelope_mode_reaches_the_chip_through_the_ring_and_across_model_changes() {
    let mut dev = Device::new(SID_MODEL_6581);
    let mut direct = DirectSink::new(SID_MODEL_6581);
    let gate_reset = |s: &sid| s.env.map(|e| e.gate_reset);
    dev.block();
    assert_eq!(gate_reset(dev.chip()), [1; 3], "instant by default");
    assert_eq!(gate_reset(&direct.chip), [1; 3]);
    for (mode, want) in [(LIVE_ENVELOPE_AUTHENTIC, 0u8), (LIVE_ENVELOPE_INSTANT, 1)] {
        play_into_both(&mut dev, &mut direct, |l| unsafe {
            live_param_set(l, LIVE_PARAM_ENVELOPE_MODE, mode);
            live_button(l, LIVE_BUTTON_FX);
            live_note_on(l, 60, 100);
        });
        dev.block();
        assert_eq!(gate_reset(dev.chip()), [want; 3], "mode {mode}");
        assert_eq!(gate_reset(&direct.chip), [want; 3], "mode {mode}");
        assert_eq!(dev.chips.lost_resets, 0);
        assert_same_sound(&mut dev, &mut direct);
        play_into_both(&mut dev, &mut direct, |l| unsafe { live_all_notes_off(l) });
    }
}

#[test]
fn the_model_toggle_swaps_chips_and_keeps_the_sound_parameters() {
    let mut dev = Device::new(SID_MODEL_6581);
    let mut direct = DirectSink::new(SID_MODEL_6581);
    dev.block();
    play_into_both(&mut dev, &mut direct, |l| unsafe {
        live_button(l, LIVE_BUTTON_FX)
    });
    assert_eq!(dev.out.reset_waits, 0, "the ring was empty");
    dev.block();
    assert_eq!(dev.chip().model, SID_MODEL_8580);
    assert_eq!(
        dev.chips.active[0], 0,
        "live_init's reset took the spare, sid 2, and the toggle sid 0, chip A's first"
    );

    play_into_both(&mut dev, &mut direct, |l| unsafe {
        live_note_on(l, 64, 100);
        live_knob(l, LIVE_KNOB_CUTOFF, -20);
        live_button(l, LIVE_BUTTON_FX);
    });
    assert!(
        dev.out.reset_waits > 0,
        "the note's writes were still queued"
    );
    dev.block();
    assert_eq!(dev.chip().model, SID_MODEL_6581);
    assert_eq!(dev.chips.lost_resets, 0);
    assert_same_sound(&mut dev, &mut direct);
}

#[test]
fn the_character_reaches_the_chip_through_the_ring_and_across_model_changes() {
    let mut dev = Device::new(SID_MODEL_6581);
    let mut direct = DirectSink::new(SID_MODEL_6581);
    dev.block();
    let default_dac = |model| unsafe { sid_dac_default(i32::from(model == SID_MODEL_6581)) };
    let waits = (dev.out.full_waits, dev.out.reset_waits);
    play_into_both(&mut dev, &mut direct, |l| unsafe {
        live_note_on(l, 60, 100);
        live_param_set(l, LIVE_PARAM_AGE, 90);
        live_param_set(l, LIVE_PARAM_DAC, 127);
        live_param_set(l, LIVE_PARAM_THUMP, 64);
        live_param_set(l, LIVE_PARAM_BLEED, 100);
    });
    const TICKS: u32 = 270;
    let mut tables: Vec<*const sid_dac> = Vec::new();
    let mut playing: Option<(*const sid_dac, Vec<f32>)> = None;
    let ticks = dev.chips.ticks[0];
    for tick in 0..TICKS {
        play_into_both(&mut dev, &mut direct, |l| unsafe { live_tick(l) });
        dev.block();
        let d = dev.chip().dac;
        let wave = unsafe { (*d).wave.to_vec() };
        if let Some((p, w)) = &playing {
            if *p == d {
                assert_eq!(
                    *w, wave,
                    "tick {tick}: the playing chip's DACs changed under it"
                );
            }
        }
        if !tables.contains(&d) {
            tables.push(d);
        }
        playing = Some((d, wave));
        assert_same_sound(&mut dev, &mut direct);
    }
    assert_eq!(
        dev.chips.ticks[0],
        ticks + TICKS,
        "a REG_RING_CHARACTER a tick"
    );
    let s = dev.chip();
    assert_eq!(
        (s.thump, s.bleed, s.filter_t),
        (
            perceptual_share(64) * SID_CHARACTER_THUMP as f32,
            0.5 * perceptual_share(100),
            90.0 / 127.0
        ),
        "the ramp got there"
    );
    assert!(
        tables.len() == 3 && tables[0] == default_dac(SID_MODEL_6581),
        "the DACs went from the default through both character tables: {tables:?}"
    );
    assert_eq!(
        (dev.out.full_waits, dev.out.reset_waits),
        waits,
        "a tick waited"
    );
    assert_eq!(dev.out.character_defers, 0);
    play_into_both(&mut dev, &mut direct, |l| unsafe {
        live_param_set(l, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN);
        live_tick(l);
        live_note_on(l, 64, 100);
    });
    dev.block();
    assert_eq!(dev.chips.lost_resets, 0);
    assert_eq!(dev.chip().filter_curve, 3, "worn's filter row");
    assert_eq!(
        dev.chip().thump,
        perceptual_share(64) * SID_CHARACTER_THUMP as f32,
        "the reset keeps the controls"
    );
    assert_same_sound(&mut dev, &mut direct);
    for (model, curve) in [(SID_MODEL_8580, 0u8), (SID_MODEL_6581, 3)] {
        play_into_both(&mut dev, &mut direct, |l| unsafe {
            live_button(l, LIVE_BUTTON_FX);
            live_note_on(l, 67, 100);
        });
        dev.block();
        assert_eq!(dev.chip().model, model);
        assert_eq!(dev.chip().filter_curve, curve);
        assert!(dev.chip().thump > 0.0, "the controls survive the reset");
        assert_same_sound(&mut dev, &mut direct);
    }
    assert_eq!(dev.live.profile, SID_CHIP_6581_WORN as u8);
    play_into_both(&mut dev, &mut direct, |l| unsafe {
        for p in [
            LIVE_PARAM_AGE,
            LIVE_PARAM_DAC,
            LIVE_PARAM_THUMP,
            LIVE_PARAM_BLEED,
        ] {
            live_param_set(l, p, 0);
        }
        live_param_set(l, LIVE_PARAM_PROFILE, SID_CHIP_6581_R4AR);
        live_tick(l);
        live_note_on(l, 60, 100);
    });
    dev.block();
    assert_eq!(dev.chip().dac, default_dac(SID_MODEL_6581));
    assert_eq!((dev.chip().thump, dev.chip().bleed), (0.0, 0.0));
    assert_same_sound(&mut dev, &mut direct);
}

#[test]
fn a_tick_with_both_stages_queued_waits_its_turn() {
    let mut dev = Device::new(SID_MODEL_6581);
    dev.block();
    let ticks = dev.chips.ticks[0];
    unsafe { live_param_set(&mut *dev.live, LIVE_PARAM_THUMP, 127) };
    for _ in 0..3 {
        unsafe { live_tick(&mut *dev.live) };
    }
    assert_eq!(dev.out.character_defers, 1);
    assert_eq!(dev.chips.stage_pending[0], [1, 1]);
    let step = |n: f32| thump_after(n as u32);
    assert_eq!(
        [dev.chips.stage[0][0].thump, dev.chips.stage[0][1].thump],
        [step(1.0), step(2.0)]
    );
    assert_eq!(dev.out.full_waits + dev.out.reset_waits, 0);
    dev.block();
    assert_eq!(dev.chips.stage_pending[0], [0, 0]);
    assert_eq!(dev.chip().thump, step(2.0), "the later stage last");
    assert_eq!(dev.chips.ticks[0], ticks + 3);
    unsafe { live_tick(&mut *dev.live) };
    dev.block();
    assert_eq!(dev.chip().thump, step(3.0), "the ramp goes on");
}

unsafe extern "C" fn interrupt_once(ctx: *mut c_void) {
    unsafe {
        reg_ring_set_race_hook(None, std::ptr::null_mut());
        drain_now(ctx);
    }
}

#[test]
fn expert_rows_never_build_over_a_rendered_or_queued_lease() {
    let mut dev = Device::new(SID_MODEL_6581);
    dev.block();
    unsafe {
        live_note_on(&mut *dev.live, 60, 100);
        live_param_set(
            &mut *dev.live,
            LIVE_PARAM_EXPERT + SID_EXPERT_FILTER_6581_CURVE,
            90,
        );
    }
    dev.block();
    let active = dev.chips.active[0];
    let o: *mut reg_ring_out = &mut *dev.out;
    for tick in 0..220 {
        dev.out.sliced = 0;
        let rows = dev.chip().filter_rows;
        let before: Vec<_> = rows
            .iter()
            .map(|row| unsafe {
                std::slice::from_raw_parts(row.cutoff, SID_FC_STEPS as usize).to_vec()
            })
            .collect();
        unsafe {
            live_tick(&mut *dev.live);
            reg_ring_commit(&mut *dev.ring);
        }
        if dev.chips.build_pass != 0 {
            assert_ne!(
                dev.out.sliced, 0,
                "tick {tick}: the store must yield to a table slice"
            );
        }
        unsafe {
            reg_ring_pass_end(&mut *dev.chips);
        }
        for (row, values) in rows.iter().zip(&before) {
            assert_eq!(
                unsafe { std::slice::from_raw_parts(row.cutoff, SID_FC_STEPS as usize) },
                values,
                "tick {tick}: live_tick overwrote the currently rendered row"
            );
        }
        unsafe {
            reg_ring_set_race_hook(Some(interrupt_once), o.cast());
            live_tick(&mut *dev.live);
            reg_ring_set_race_hook(None, std::ptr::null_mut());
        }
        dev.block();
        assert_eq!(
            dev.chips.active[0], active,
            "an expert row swap reset the chip"
        );
        assert_eq!(dev.chip().env[0].gate, 1);
    }
    assert_eq!(
        unsafe {
            sid_character_expert_get(
                &dev.chips.ramp[0].now,
                SID_MODEL_6581,
                SID_EXPERT_FILTER_6581_CURVE,
            )
        },
        90
    );
    unsafe { live_param_set(&mut *dev.live, LIVE_PARAM_AGE, 127) };
    for _ in 0..400 {
        unsafe { live_tick(&mut *dev.live) };
        dev.block();
    }
    assert_eq!(
        dev.chips.ramp[0].filter_managed, 0,
        "macro restoration must finish"
    );
    assert_eq!(
        dev.chip().filter_t,
        1.0,
        "AGE must regain its original filter path"
    );
    unsafe { live_param_set(&mut *dev.live, LIVE_PARAM_PROFILE, SID_CHIP_6581_R2) };
    for _ in 0..30 {
        unsafe { live_tick(&mut *dev.live) };
        dev.block();
    }
    assert_eq!(
        dev.chips.lost_resets, 0,
        "a later profile preparation reused leased backing"
    );
}

#[test]
fn a_commit_during_the_choice_of_a_free_table_never_frees_the_one_it_plays() {
    let mut dev = Device::new(SID_MODEL_6581);
    dev.block();
    let dacs = dev.chips.dac.as_ptr();
    let (first, second) = (dacs, unsafe { dacs.add(1) });
    unsafe { live_param_set(&mut *dev.live, LIVE_PARAM_DAC, 127) };
    let mut queued = false;
    for _ in 0..40 {
        unsafe { live_tick(&mut *dev.live) };
        commit(&mut dev.ring);
        let k = (0..2).find(|&k| dev.chips.stage_pending[0][k] != 0);
        if let Some(k) = k {
            if dev.chips.stage[0][k].dac == first && dev.chip().dac == second {
                queued = true;
                break;
            }
        }
        dev.block();
    }
    assert!(
        queued,
        "the third table was never queued while the chip played the second"
    );
    unsafe { reg_ring_pass_end(&mut *dev.chips) };
    let o: *mut reg_ring_out = &mut *dev.out;
    unsafe {
        reg_ring_set_race_hook(Some(interrupt_once), o.cast());
        live_tick(&mut *dev.live);
        reg_ring_set_race_hook(None, std::ptr::null_mut());
    }
    assert_eq!(
        dev.chip().dac,
        first,
        "the interrupt did not commit the stage"
    );
    assert_eq!(
        dev.chips.ramp[0].building, 1,
        "the tick did not start the fourth table"
    );
    assert_eq!(
        dev.chips.ramp[0].job.d.cast_const(),
        second,
        "the fourth table is being built over the one the chip plays"
    );
}

#[test]
fn catch_up_ticks_share_one_character_table_slice_and_make_the_store_yield() {
    for param in [
        LIVE_PARAM_EXPERT + SID_EXPERT_FILTER_6581_CURVE,
        LIVE_PARAM_DAC,
    ] {
        let mut dev = Device::new(SID_MODEL_6581);
        dev.block();
        unsafe {
            live_param_set(&mut *dev.live, param, 127);
            live_tick(&mut *dev.live);
        }
        let progress = |dev: &Device| {
            let r = &dev.chips.ramp[0];
            (r.job.step, r.filter_job.phase, r.filter_job.at)
        };
        let first = progress(&dev);
        assert_ne!(dev.out.sliced, 0, "the store must yield to the table build");
        unsafe { live_tick(&mut *dev.live) };
        assert_eq!(
            progress(&dev),
            first,
            "a catch-up tick ran a second table slice"
        );
        dev.block();
        unsafe { live_tick(&mut *dev.live) };
        assert_ne!(
            progress(&dev),
            first,
            "the next pass did not resume the table build"
        );
    }
}

fn worst_pass(l: &mut live, p: &mut panel, with_model_toggle: bool) {
    unsafe {
        if with_model_toggle {
            live_button(l, LIVE_BUTTON_FX);
        }
        live_slot_next_algorithm(l, u32::from(l.fx_select));
        panel_encoder(p, l, PANEL_ENC_PRESETS, 1);
        live_param_set(l, LIVE_PARAM_VELOCITY_TARGET, LIVE_VELOCITY_CUTOFF);
        live_param_set(l, LIVE_PARAM_PRESSURE_TARGET, LIVE_PRESSURE_PULSE_WIDTH);
        live_param_set(l, LIVE_PARAM_VIBRATO_DEPTH, 127);
        live_poly_pressure(l, 60, 90);
        live_pressure(l, 30);
        live_bend(l, 12000);
        panel_note_keys(p, l, 0, (1 << PANEL_NOTE_KEYS) - 1);
        panel_encoder(p, l, PANEL_ENC_K1, -3);
        panel_encoder(p, l, PANEL_ENC_K2, 2);
        panel_encoder(p, l, PANEL_ENC_K3, 5);
        panel_master(p, l, 300);
        live_tick(l);
        panel_master(p, l, 600);
        live_tick(l);
    }
}

fn panel_box() -> Box<panel> {
    let mut p = Box::<panel>::default();
    unsafe { panel_init(&mut *p) };
    p
}

#[test]
fn the_worst_pass_fits_the_ring_without_waiting() {
    let mut dev = Device::new(SID_MODEL_6581);
    let mut direct = DirectSink::new(SID_MODEL_6581);
    unsafe { live_set_preset(&mut *dev.live, 1) };
    unsafe { live_set_preset(&mut *direct.live, 1) };
    dev.block();
    let (mut pd, mut pr) = (panel_box(), panel_box());

    worst_pass(&mut dev.live, &mut pd, true);
    worst_pass(&mut direct.live, &mut pr, true);
    let used = unsafe { reg_ring_used(&*dev.ring) };
    println!("worst pass: {used} entries");
    assert_eq!(dev.out.full_waits, 0, "the pass waited for room");
    assert!(
        used <= REG_RING_WORST_PASS && REG_RING_WORST_PASS <= SIZE,
        "{used} entries, the documented worst case is {REG_RING_WORST_PASS}"
    );
    assert!(used > 200, "the burst is far from the worst case: {used}");
    assert_eq!(dev.chips.lost_resets, 0);
    assert_eq!(dev.live.preset, 2);
    play_into_both(&mut dev, &mut direct, |l| unsafe {
        live_note_on(l, 57, 100)
    });
    assert_same_sound(&mut dev, &mut direct);
}

#[test]
fn a_burst_past_the_ring_waits_and_drops_nothing() {
    let mut dev = Device::new(SID_MODEL_6581);
    let mut direct = DirectSink::new(SID_MODEL_6581);
    let (mut pd, mut pr) = (panel_box(), panel_box());
    for i in 0..2 * SIZE / 200 {
        worst_pass(&mut dev.live, &mut pd, i == 0);
        worst_pass(&mut direct.live, &mut pr, i == 0);
    }
    assert!(dev.out.full_waits > 0, "the burst never filled the ring");
    assert_eq!(dev.chips.lost_resets, 0);
    assert_eq!(dev.live.model, direct.live.model);
    for n in [48, 55, 60] {
        play_into_both(&mut dev, &mut direct, |l| unsafe {
            live_note_on(l, n, 100)
        });
    }
    assert_same_sound(&mut dev, &mut direct);
}

#[test]
fn a_full_ring_of_gate_writes_ends_with_the_last_gate() {
    let mut dev = Device::new(SID_MODEL_8580);
    let mut direct = DirectSink::new(SID_MODEL_8580);
    for i in 0..SIZE {
        let n = 40 + (i % 30) as u8;
        play_into_both(&mut dev, &mut direct, |l| unsafe {
            live_note_on(l, n, 100);
            live_note_off(l, n);
        });
    }
    assert!(dev.out.full_waits > 0);
    play_into_both(&mut dev, &mut direct, |l| unsafe {
        live_note_on(l, 50, 100)
    });
    assert_same_sound(&mut dev, &mut direct);
}

#[test]
fn the_documented_worst_pass_fits_exactly_without_waiting() {
    let mut dev = Device::new(SID_MODEL_6581);
    let mut direct = DirectSink::new(SID_MODEL_6581);
    dev.block();
    let sink = |reg: u8, value: u8, dev: &mut Device, direct: &mut DirectSink| {
        let o: *mut reg_ring_out = &mut *dev.out;
        unsafe { reg_ring_sink(o.cast(), reg, value) };
        let t = direct.live.sink_ctx;
        unsafe { live_sid_sink(t, reg, value) };
    };
    sink(RESET, SID_MODEL_8580 as u8, &mut dev, &mut direct);
    for i in 1..REG_RING_WORST_PASS {
        sink((i % 25) as u8, (i * 7) as u8, &mut dev, &mut direct);
    }
    assert_eq!(unsafe { reg_ring_used(&*dev.ring) }, REG_RING_WORST_PASS);
    assert_eq!(
        dev.out.full_waits, 0,
        "the documented worst pass waited for room"
    );
    assert_eq!(dev.out.reset_waits, 0);
    dev.block();
    assert_eq!(dev.chips.lost_resets, 0);
    assert_eq!(dev.chip().model, SID_MODEL_8580);
    assert_same_sound(&mut dev, &mut direct);
}

fn character_with_every_control_at(id: u32, v: u8) -> sid_character {
    sid_character {
        profile: (id + 1) as u8,
        age: v,
        dac: v,
        thump: v,
        bleed: v,
        ..Default::default()
    }
}

fn slices_at_zero(model: sid_model, id: u32) -> u32 {
    unsafe { sid_character_prep_slices(model, &character_with_every_control_at(id, 0)) }
}

fn profile_model(id: u32) -> sid_model {
    sid_model::from(unsafe { (*sid_chip_profile_get(id)).model })
}

fn chip_bytes_but_the_dac_pointer(s: &sid) -> Vec<u8> {
    let mut c = Box::new(*s);
    c.dac = std::ptr::null();
    for row in &mut c.filter_rows {
        row.cutoff = std::ptr::null_mut();
        row.res = std::ptr::null_mut();
    }
    for osc in &mut c.osc {
        osc.writeback = std::ptr::null();
    }
    let p: *const sid = &*c;
    unsafe { std::slice::from_raw_parts(p.cast::<u8>(), size_of::<sid>()) }.to_vec()
}

fn dac_eq(a: *const sid_dac, b: *const sid_dac) -> bool {
    let (a, b) = unsafe { (&*a, &*b) };
    a.wave
        .iter()
        .zip(&b.wave)
        .all(|(x, y)| x.to_bits() == y.to_bits())
        && a.env
            .iter()
            .zip(&b.env)
            .all(|(x, y)| x.to_bits() == y.to_bits())
}

#[test]
fn the_staged_preparation_makes_the_one_shot_chip_in_a_fixed_number_of_slices() {
    let mut once = Box::<sid>::default();
    let mut staged = Box::<sid>::default();
    let mut dacs = Box::<[sid_dac; 2]>::default();
    let mut prep = Box::<sid_character_prep>::default();
    let mut counts = Vec::new();
    for id in 0..SID_CHIPS {
        for model in [profile_model(id), 1 - profile_model(id)] {
            for v in [0u8, 1, 64, 127] {
                let c = character_with_every_control_at(id, v);
                unsafe {
                    sid_init(&mut *once, model, SID_CLOCK_PAL, RATE);
                    sid_character_apply(&mut *once, &c, &mut dacs[0]);
                }
                let want = unsafe { sid_character_prep_slices(model, &c) };
                for run in 0..2 {
                    unsafe {
                        sid_init(&mut *staged, 1 - model, SID_CLOCK_PAL, RATE);
                        sid_write(&mut *staged, 0x18, 0x1f);
                        render(&mut staged, 64);
                        sid_character_prep_start(
                            &mut *prep,
                            &mut *staged,
                            model,
                            SID_CLOCK_PAL,
                            RATE,
                            &c,
                            0,
                        );
                    }
                    let mut slices = 0;
                    while unsafe { sid_character_prep_done(&*prep) } == 0 {
                        slices += 1;
                        assert!(slices <= 32, "profile {id} model {model} at {v}: no end");
                        let done = unsafe { sid_character_prep_step(&mut *prep, &mut dacs[1]) };
                        assert_eq!(done != 0, unsafe { sid_character_prep_done(&*prep) } != 0);
                    }
                    let what = format!("profile {id} on model {model} at {v}, run {run}");
                    assert_eq!(slices, want, "{what}: slices");
                    assert_eq!(
                        chip_bytes_but_the_dac_pointer(&staged),
                        chip_bytes_but_the_dac_pointer(&once),
                        "{what}: the chip"
                    );
                    let custom = std::ptr::eq(once.dac, &dacs[0]);
                    if custom {
                        assert!(std::ptr::eq(staged.dac, &dacs[1]), "{what}");
                        assert_eq!(prep.table, staged.dac, "{what}");
                        assert!(dac_eq(staged.dac, once.dac), "{what}: the DAC table");
                    } else {
                        assert_eq!(staged.dac, once.dac, "{what}: the default DACs");
                        assert!(prep.table.is_null(), "{what}");
                    }
                    assert_eq!(
                        unsafe { sid_character_prep_step(&mut *prep, &mut dacs[1]) },
                        1
                    );
                    assert_eq!(
                        chip_bytes_but_the_dac_pointer(&staged),
                        chip_bytes_but_the_dac_pointer(&once),
                        "{what}: after"
                    );
                }
                if model == profile_model(id) {
                    counts.push((id, v, want));
                }
            }
        }
    }
    println!("slices (profile, controls, slices): {counts:?}");
}

#[test]
fn the_staged_preparation_waits_for_a_free_dac_table() {
    let mut s = Box::<sid>::default();
    let mut d = Box::<sid_dac>::default();
    let mut prep = Box::<sid_character_prep>::default();
    let c = character_with_every_control_at(SID_CHIP_6581_WORN, 127);
    unsafe {
        sid_character_prep_start(
            &mut *prep,
            &mut *s,
            SID_MODEL_6581,
            SID_CLOCK_PAL,
            RATE,
            &c,
            0,
        )
    };
    let want = unsafe { sid_character_prep_slices(SID_MODEL_6581, &c) };
    let rows = want - SID_DAC_BUILD_STEPS;
    for _ in 0..rows {
        assert_eq!(
            unsafe { sid_character_prep_step(&mut *prep, std::ptr::null_mut()) },
            0
        );
    }
    for _ in 0..3 {
        assert_eq!(
            unsafe { sid_character_prep_step(&mut *prep, std::ptr::null_mut()) },
            0
        );
        assert!(prep.job.d.is_null(), "the DACs started without a table");
    }
    let mut n = 0;
    while unsafe { sid_character_prep_step(&mut *prep, &mut *d) } == 0 {
        n += 1;
    }
    assert_eq!(n + 1, SID_DAC_BUILD_STEPS);
    assert_eq!(s.dac, &*d as *const sid_dac);
}

fn first_8580_preparation_slices() -> u32 {
    let own = slices_at_zero(SID_MODEL_8580, SID_CHIP_8580_R5);
    own + SID_DAC_BUILD_STEPS
}

fn prepared_device(model: sid_model) -> Device {
    let mut d = Device::new(model);
    let o: *mut reg_ring_out = &mut *d.out;
    unsafe { live_set_preparer(&mut *d.live, Some(reg_ring_prepare_step), o.cast()) };
    d.block();
    d
}

struct Host {
    chip: Box<sid>,
    _spare: Box<sid>,
    target: Box<live_sid_target>,
    _dacs: Box<[sid_dac; 2]>,
    live: Box<live>,
}

impl Host {
    fn new(model: sid_model) -> Self {
        let mut chip = Box::<sid>::default();
        let mut spare = Box::<sid>::default();
        let mut dacs = Box::<[sid_dac; 2]>::default();
        let mut target = Box::new(live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: RATE,
            dacs: dacs.as_mut_ptr(),
            spare: &mut *spare,
            ..Default::default()
        });
        let mut l = Box::<live>::default();
        let t: *mut live_sid_target = &mut *target;
        unsafe {
            live_init(&mut *l, Some(live_sid_sink), t.cast(), model);
            live_set_preparer(&mut *l, Some(live_sid_prepare), t.cast());
        }
        Host {
            chip,
            _spare: spare,
            target,
            _dacs: dacs,
            live: l,
        }
    }
}

fn tick_both(dev: &mut Device, host: &mut Host) -> (Vec<i16>, Vec<i16>) {
    unsafe {
        live_tick(&mut *dev.live);
        live_tick(&mut *host.live);
    }
    dev.block();
    (render(dev.chip(), 128), render(&mut host.chip, 128))
}

#[test]
fn a_profile_step_prepares_the_spare_a_slice_a_tick_and_swaps_at_the_hosts_tick() {
    let mut dev = prepared_device(SID_MODEL_6581);
    let mut host = Host::new(SID_MODEL_6581);
    let resets = (dev.out.unprepared_resets, dev.out.reset_waits);
    assert_eq!(resets.0, 1, "live_init's reset is the one made at once");
    for l in [&mut *dev.live, &mut *host.live] {
        unsafe {
            for p in [
                LIVE_PARAM_AGE,
                LIVE_PARAM_DAC,
                LIVE_PARAM_THUMP,
                LIVE_PARAM_BLEED,
            ] {
                live_param_set(l, p, 127);
            }
            live_note_on(l, 57, 100);
        }
    }
    for _ in 0..270 {
        let (a, b) = tick_both(&mut dev, &mut host);
        assert_eq!(a, b, "the ramp");
    }
    assert_eq!(dev.chips.ramp[0].now.dac, 127, "the DACs' ramp is done");
    let mut slices_per_profile = Vec::new();
    for to in [
        SID_CHIP_6581_WORN,
        SID_CHIP_8580_R5,
        SID_CHIP_6581_R2,
        SID_CHIP_6581_R4AR,
    ] {
        let model = profile_model(to);
        let c = character_with_every_control_at(to, 127);
        let mut want = unsafe { sid_character_prep_slices(model, &c) };
        if model == SID_MODEL_8580 && dev.chips.default_8580_spent == 0 {
            want += SID_DAC_BUILD_STEPS;
        }
        let before = dev.chips.active[0];
        let (slices, was) = (dev.out.slices, dev.chip().filter_curve);
        for l in [&mut *dev.live, &mut *host.live] {
            unsafe { live_param_set(l, LIVE_PARAM_PROFILE, to) };
        }
        let mut swapped_at = None;
        for tick in 1..=want + 2 {
            let (a, b) = tick_both(&mut dev, &mut host);
            assert_eq!(a, b, "to {to}, tick {tick}");
            if dev.chips.active[0] != before && swapped_at.is_none() {
                swapped_at = Some(tick);
            }
            if swapped_at.is_none() {
                assert_eq!(
                    dev.chip().filter_curve,
                    was,
                    "to {to}: the old chip changed"
                );
                assert_eq!(u32::from(dev.live.profile), u32::from(host.live.profile));
            }
        }
        assert_eq!(swapped_at, Some(want), "to {to}: the swap's tick");
        assert_eq!(dev.out.slices - slices, want, "to {to}: a slice a tick");
        assert_eq!(dev.chip().model, model);
        assert_eq!(u32::from(dev.live.profile), to);
        assert_eq!(dev.chips.prep_on, 0);
        slices_per_profile.push((to, want));
    }
    assert_eq!((dev.out.unprepared_resets, dev.out.reset_waits), resets);
    assert_eq!(dev.chips.lost_resets, 0);
    println!("slices at full character: {slices_per_profile:?}");
    assert_same_after_ticks(&mut dev, &mut host);
}

fn assert_same_after_ticks(dev: &mut Device, host: &mut Host) {
    for _ in 0..50 {
        let (a, b) = tick_both(dev, host);
        assert_eq!(a, b);
    }
}

#[test]
fn requests_during_a_preparation_coalesce() {
    let mut dev = prepared_device(SID_MODEL_6581);
    let mut host = Host::new(SID_MODEL_6581);
    let both_live = |dev: &mut Device, host: &mut Host, f: &dyn Fn(&mut live)| {
        f(&mut dev.live);
        f(&mut host.live);
    };
    both_live(&mut dev, &mut host, &|l| unsafe {
        live_note_on(l, 60, 100)
    });
    let worn = slices_at_zero(SID_MODEL_6581, SID_CHIP_6581_WORN);
    let r5 = first_8580_preparation_slices();
    both_live(&mut dev, &mut host, &|l| unsafe {
        live_param_set(l, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN)
    });
    for _ in 0..3 {
        let (a, b) = tick_both(&mut dev, &mut host);
        assert_eq!(a, b);
    }
    assert_eq!(dev.chips.prep_profile, SID_CHIP_6581_WORN as u8);
    both_live(&mut dev, &mut host, &|l| unsafe {
        live_button(l, LIVE_BUTTON_FX)
    });
    assert_eq!(
        dev.live.profile_req, SID_CHIP_8580_R5 as u8,
        "FX: the other model"
    );
    for tick in 1..=r5 {
        assert_eq!(dev.chip().model, SID_MODEL_6581, "tick {tick}");
        let (a, b) = tick_both(&mut dev, &mut host);
        assert_eq!(a, b, "tick {tick}");
    }
    assert_eq!(dev.chip().model, SID_MODEL_8580, "R5 after its own slices");
    both_live(&mut dev, &mut host, &|l| unsafe {
        live_param_set(l, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN)
    });
    let rows = worn - SID_DAC_BUILD_STEPS;
    for tick in 1..=worn {
        if tick == 3 {
            both_live(&mut dev, &mut host, &|l| unsafe {
                live_param_set(l, LIVE_PARAM_THUMP, 127)
            });
        }
        if tick == rows + 2 {
            both_live(&mut dev, &mut host, &|l| unsafe {
                live_param_set(l, LIVE_PARAM_DAC, 30)
            });
        }
        let (a, b) = tick_both(&mut dev, &mut host);
        assert_eq!(a, b, "tick {tick}");
    }
    assert_eq!(
        dev.chip().filter_curve,
        3,
        "WORN after its slices, the controls notwithstanding"
    );
    assert_eq!(dev.chip().thump, 1.0, "the spare took the thump");
    assert_eq!(
        dev.chips.ramp[0].now.dac, 0,
        "and the DACs as they were at the rows' end"
    );
    assert_same_after_ticks(&mut dev, &mut host);
    assert_eq!(
        dev.chips.ramp[0].now.dac, 30,
        "the ramp took the DACs the rest of the way"
    );
    let active = dev.chips.active[0];
    both_live(&mut dev, &mut host, &|l| unsafe {
        live_param_set(l, LIVE_PARAM_PROFILE, SID_CHIP_6581_R2);
        live_param_set(l, LIVE_PARAM_DAC, 90);
    });
    for _ in 0..2 {
        let (a, b) = tick_both(&mut dev, &mut host);
        assert_eq!(a, b);
    }
    assert_eq!(dev.chips.prep_on, 1);
    both_live(&mut dev, &mut host, &|l| unsafe {
        live_param_set(l, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN)
    });
    let (a, b) = tick_both(&mut dev, &mut host);
    assert_eq!(a, b);
    assert_eq!((dev.chips.prep_on, dev.live.preparing), (0, 0), "dropped");
    assert_eq!(dev.chips.active[0], active, "no reset");
    assert_same_after_ticks(&mut dev, &mut host);
    assert!(
        dev.chips.ramp[0].now.dac > 30,
        "the ramp builds its DACs again"
    );
    assert_eq!(dev.chips.lost_resets, 0);
    assert_eq!(dev.out.unprepared_resets, 1);
}

#[test]
fn a_preparation_never_starts_on_a_spare_still_queued_to_swap() {
    let mut dev = prepared_device(SID_MODEL_6581);
    let r5 = first_8580_preparation_slices();
    unsafe { live_param_set(&mut *dev.live, LIVE_PARAM_PROFILE, SID_CHIP_8580_R5) };
    for _ in 0..r5 - 1 {
        unsafe { live_tick(&mut *dev.live) };
        dev.block();
    }
    let playing = dev.chips.active[0];
    unsafe {
        live_tick(&mut *dev.live);
        assert_eq!(dev.chips.spare_ready, 1);
        live_param_set(&mut *dev.live, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN);
        let slices = dev.out.slices;
        live_tick(&mut *dev.live);
        assert_eq!(
            dev.out.slices, slices,
            "a slice ran on the chip still rendering"
        );
        assert_eq!(dev.chips.prep_on, 0);
    }
    assert_eq!(dev.chips.active[0], playing);
    dev.block();
    assert_ne!(dev.chips.active[0], playing, "the swap");
    assert_eq!(dev.chip().model, SID_MODEL_8580);
    let worn = slices_at_zero(SID_MODEL_6581, SID_CHIP_6581_WORN);
    for _ in 0..worn {
        unsafe { live_tick(&mut *dev.live) };
        dev.block();
    }
    assert_eq!(dev.chip().filter_curve, 3);
    assert_eq!(dev.chips.lost_resets, 0);
    assert_eq!(dev.out.unprepared_resets, 1);
}

struct Two {
    ring: Box<reg_ring>,
    chips: Box<reg_ring_chips>,
    outs: [Box<reg_ring_out>; 2],
    lives: [Box<live>; 2],
}

impl Two {
    fn new() -> Self {
        let mut ring = ring();
        let mut chips = chips(SID_MODEL_6581);
        let mut out = |chip: u8| {
            let mut o = Box::new(reg_ring_out {
                ring: &mut *ring,
                chips: &mut *chips,
                chip,
                wait: Some(drain_now),
                ..Default::default()
            });
            let p: *mut reg_ring_out = &mut *o;
            o.wait_ctx = p.cast();
            o
        };
        let mut outs = [out(0), out(1)];
        let lives = [0, 1].map(|x| {
            let mut l = Box::<live>::default();
            let o: *mut reg_ring_out = &mut *outs[x];
            unsafe {
                live_init(&mut *l, Some(reg_ring_sink), o.cast(), SID_MODEL_6581);
                live_set_preparer(&mut *l, Some(reg_ring_prepare_step), o.cast());
            }
            l
        });
        let mut t = Two {
            ring,
            chips,
            outs,
            lives,
        };
        t.block();
        t
    }

    fn block(&mut self) {
        commit(&mut self.ring);
        unsafe {
            reg_ring_pass_end(&mut *self.chips);
            reg_ring_drain(&mut *self.ring, &mut *self.chips);
        }
    }

    fn chip(&mut self, x: u32) -> &mut sid {
        unsafe { &mut *reg_ring_chip_of(&mut *self.chips, x) }
    }

    fn tick(&mut self) {
        for l in &mut self.lives {
            unsafe { live_tick(&mut **l) };
        }
        self.block();
    }

    fn sink(&mut self, x: usize, reg: u8, value: u8) {
        let o: *mut reg_ring_out = &mut *self.outs[x];
        unsafe { reg_ring_sink(o.cast(), reg, value) };
    }

    fn queued(&self) -> Vec<(u8, u8)> {
        let r = &*self.ring;
        (r.tail..r.stage)
            .map(|i| {
                let e = r.e[(i & (SIZE - 1)) as usize];
                (e.reg, e.value)
            })
            .collect()
    }
}

#[test]
fn every_entry_reaches_the_chip_it_names() {
    let mut t = Two::new();
    let slot = (LIVE_REG_SLOT_BASE | 1 << 3 | 2) as u8;
    let profile = SID_CHIP_6581_WORN;
    for x in 0..2usize {
        let bit = if x == 1 { REG_RING_CHIP_B as u8 } else { 0 };
        let other = 1 - x as u32;
        let before = chip_bytes_but_the_dac_pointer(t.chip(other));
        t.sink(x, 0x05, 0x9a);
        t.sink(x, 0x18, 0x0c);
        t.sink(x, GATE_RESET, 0);
        t.sink(x, slot, 77);
        t.sink(x, THUMP, 127);
        t.sink(x, LIVE_REG_CHAR_TICK as u8, 0);
        assert_eq!(
            t.queued(),
            [
                (0x05 | bit, 0x9a),
                (0x18 | bit, 0x0c),
                (GATE_RESET | bit, 0),
                (slot, 77),
                (REG_RING_CHARACTER as u8 | bit, 0),
            ],
            "chip {x}'s entries"
        );
        t.block();
        let s = t.chip(x as u32);
        assert_eq!(s.env[0].attack, 0x9);
        assert_eq!(s.mode_vol, 0x0c);
        assert!(s.env.iter().all(|e| e.gate_reset == 0));
        assert_eq!(s.thump, thump_after(1), "the tick's stage");
        assert_eq!(t.chips.ticks[x], 1);
        assert_eq!(t.chips.ticks[1 - x], u32::from(x == 1), "the other's ticks");
        assert_eq!(
            chip_bytes_but_the_dac_pointer(t.chip(other)),
            before,
            "chip {other} changed"
        );
        let was = t.chips.active[x];
        let o: *mut reg_ring_out = &mut *t.outs[x];
        let mut n = 0;
        while unsafe { reg_ring_prepare_step(o.cast(), profile as u8) } == 0 {
            unsafe { reg_ring_pass_end(&mut *t.chips) };
            n += 1;
            assert!(n < 32);
        }
        t.sink(x, LIVE_REG_CHAR_FIRST as u8, profile as u8);
        t.sink(x, RESET, SID_MODEL_6581 as u8);
        assert_eq!(t.queued(), [(RESET | bit, SID_MODEL_6581 as u8)]);
        t.block();
        assert_ne!(t.chips.active[x], was, "chip {x} swapped");
        assert_eq!(t.chip(x as u32).filter_curve, 3);
        assert_eq!(
            chip_bytes_but_the_dac_pointer(t.chip(other)),
            before,
            "chip {other} changed at the reset"
        );
    }
    assert_eq!(t.chips.lost_resets, 0);
    assert_eq!(
        t.outs.iter().map(|o| o.unprepared_resets).sum::<u32>(),
        2,
        "live_init's"
    );
}

struct Order {
    chips: *mut reg_ring_chips,
    seen: Vec<(u8, u8, u8)>,
}

unsafe extern "C" fn record_order(ctx: *mut c_void, reg: u8, _value: u8) {
    let o = unsafe { &mut *ctx.cast::<Order>() };
    let c = o.chips;
    let vol = |x: u32| unsafe { (*reg_ring_chip_of(c, x)).mode_vol };
    o.seen.push((reg, vol(0), vol(1)));
}

#[test]
fn writes_to_both_chips_in_one_pass_apply_in_order() {
    let mut t = Two::new();
    let mut order = Box::new(Order {
        chips: &mut *t.chips,
        seen: Vec::new(),
    });
    let o: *mut Order = &mut *order;
    t.chips.pseudo = Some(record_order);
    t.chips.pseudo_ctx = o.cast();
    let slot = |k: u32| (LIVE_REG_SLOT_BASE | k) as u8;
    let b_was = t.chip(1).mode_vol;
    t.sink(0, 0x18, 1);
    t.sink(1, slot(0), 0);
    t.sink(1, 0x18, 2);
    t.sink(0, slot(1), 0);
    t.sink(0, 0x18, 3);
    t.sink(1, slot(2), 0);
    t.sink(1, 0x18, 4);
    t.sink(1, 0x18, 5);
    t.sink(0, slot(3), 0);
    t.block();
    assert_eq!(
        order.seen,
        [
            (slot(0), 1, b_was),
            (slot(1), 1, 2),
            (slot(2), 3, 2),
            (slot(3), 3, 5)
        ],
        "(slot, A's $18, B's $18) as each slot write was applied"
    );
}

fn chip_b_reference() -> Host {
    let mut h = Host::new(SID_MODEL_6581);
    unsafe { reg_ring_power_on(&mut *h.chip) };
    h
}

#[test]
fn a_reset_of_a_leaves_b_untouched_bit_for_bit() {
    let mut t = Two::new();
    let mut b = chip_b_reference();
    for n in [48, 55, 60] {
        unsafe {
            live_note_on(&mut *t.lives[1], n, 100);
            live_note_on(&mut *b.live, n, 100);
            live_note_on(&mut *t.lives[0], n + 12, 100);
        }
    }
    t.block();
    let (a_was, b_was) = (t.chips.active[0], t.chips.active[1]);
    let mut swaps = 0;
    for (i, profile) in [SID_CHIP_6581_WORN, SID_CHIP_8580_R5, SID_CHIP_6581_R4AR]
        .into_iter()
        .enumerate()
    {
        unsafe { live_param_set(&mut *t.lives[0], LIVE_PARAM_PROFILE, profile) };
        for tick in 0..14 {
            t.tick();
            unsafe { live_tick(&mut *b.live) };
            let got = render(t.chip(1), 128);
            let want = render(&mut b.chip, 128);
            assert_eq!(got, want, "step {i}, tick {tick}: chip B's sound");
            assert_eq!(
                chip_bytes_but_the_dac_pointer(t.chip(1)),
                chip_bytes_but_the_dac_pointer(&b.chip),
                "step {i}, tick {tick}"
            );
        }
        assert_eq!(u32::from(t.lives[0].profile), profile);
        swaps += 1;
    }
    assert_eq!(swaps, 3);
    assert_ne!(t.chips.active[0], a_was);
    assert_eq!(t.chips.active[1], b_was, "chip B never swapped");
    assert_eq!(t.chips.lost_resets, 0);
}

#[test]
fn the_shared_spare_serialises_two_requests() {
    let mut t = Two::new();
    let worn = slices_at_zero(SID_MODEL_6581, SID_CHIP_6581_WORN);
    let r5 = first_8580_preparation_slices();
    unsafe {
        live_param_set(&mut *t.lives[0], LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN);
        live_param_set(&mut *t.lives[1], LIVE_PARAM_PROFILE, SID_CHIP_8580_R5);
    }
    let mut a_at = None;
    let mut b_at = None;
    for tick in 1..=worn + r5 + 3 {
        t.tick();
        if a_at.is_none() && t.chip(0).filter_curve == 3 {
            a_at = Some(tick);
        }
        if b_at.is_none() && t.chip(1).model == SID_MODEL_8580 {
            b_at = Some(tick);
        }
        if a_at.is_none() {
            assert_eq!(
                t.outs[1].slices, 0,
                "tick {tick}: B's slice during A's preparation"
            );
            assert_eq!(t.chips.prep_chip, 0);
        }
    }
    assert_eq!(a_at, Some(worn), "A after its own slices");
    assert_eq!(b_at, Some(worn + r5), "B from the tick after A's swap");
    assert_eq!((t.outs[0].slices, t.outs[1].slices), (worn, r5));
    assert_eq!(u32::from(t.lives[1].profile), SID_CHIP_8580_R5);
    assert_eq!(t.chips.lost_resets, 0);
    let sids: Vec<u8> = t.chips.active.to_vec();
    assert!(sids[0] != sids[1], "two chips on one sid: {sids:?}");
}

#[test]
fn the_shared_build_table_serialises_two_ramps() {
    let mut t = Two::new();
    for l in &mut t.lives {
        unsafe { live_param_set(&mut **l, LIVE_PARAM_DAC, 127) };
    }
    let mut both_waited = false;
    let mut playing: [Option<(*const sid_dac, Vec<f32>)>; 2] = [None, None];
    let mut tick = 0;
    while (0..2).any(|x| t.chips.ramp[x].now.dac != 127) {
        tick += 1;
        assert!(tick < 1000, "the ramps never ended");
        t.tick();
        let building = [t.chips.ramp[0].building, t.chips.ramp[1].building];
        assert!(building != [1, 1], "tick {tick}: both ramps build at once");
        if building.contains(&1) {
            let waiting = (0..2).any(|x| building[x] == 0 && t.chips.ramp[x].now.dac != 127);
            both_waited |= waiting;
        }
        for x in 0..2u32 {
            let d = t.chip(x).dac;
            let wave = unsafe { (*d).wave.to_vec() };
            if let Some((p, w)) = &playing[x as usize] {
                if *p == d {
                    assert_eq!(*w, wave, "tick {tick}: chip {x}'s DACs changed under it");
                }
            }
            playing[x as usize] = Some((d, wave));
        }
    }
    assert!(both_waited, "one ramp never waited for the other's");
    println!("both DAC ramps done in {tick} ticks");
    assert!(tick > 172, "{tick} ticks: the ramps did not take turns");
    for x in 0..2u32 {
        let d = t.chip(x).dac;
        assert!(
            t.chips.dac.iter().any(|t| std::ptr::eq(t, d)),
            "chip {x} plays a character table"
        );
    }
    let (a, b) = (t.chip(0).dac, t.chip(1).dac);
    assert_ne!(a, b, "the chips share a table");
    assert_eq!(t.chips.lost_resets, 0);
}

#[test]
fn the_worst_pass_for_two_chips_fits_without_waiting() {
    let mut t = Two::new();
    unsafe {
        live_set_preparer(&mut *t.lives[1], None, std::ptr::null_mut());
        live_set_preset(&mut *t.lives[1], 1);
    }
    t.block();
    let mut p = panel_box();
    worst_pass(&mut t.lives[1], &mut p, true);
    for r in 0..25u8 {
        t.sink(0, r, r.wrapping_mul(9));
    }
    t.sink(0, GATE_RESET, 1);
    for _ in 0..2 {
        t.sink(0, LIVE_REG_CHAR_TICK as u8, 0);
    }
    let used = unsafe { reg_ring_used(&*t.ring) };
    println!("two chips' worst pass: {used} entries");
    assert_eq!(
        t.outs.iter().map(|o| o.full_waits).sum::<u32>(),
        0,
        "a wait for room"
    );
    assert!(
        used <= REG_RING_WORST_PASS,
        "{used} entries, over {REG_RING_WORST_PASS}"
    );
    assert!(
        used > 200 + REG_RING_SONG_PASS,
        "{used}: far from the worst case"
    );
    assert_eq!(
        REG_RING_WORST_PASS,
        REG_RING_LIVE_PASS + REG_RING_SONG_PASS + REG_RING_MOVE_PASS
    );
    t.block();
    assert_eq!(t.chips.lost_resets, 0);
    assert_eq!(
        t.chip(1).model,
        SID_MODEL_8580,
        "the instrument's FX took chip B"
    );
    assert_eq!(t.chip(0).model, SID_MODEL_6581);
}

#[test]
fn every_ring_test_passes_alone() {
    use std::process::Command;
    let exe = std::env::current_exe().expect("the test binary");
    let list = Command::new(&exe)
        .args(["--list", "--format", "terse", "reg_ring::"])
        .output()
        .expect("list the tests");
    let me = "reg_ring::every_ring_test_passes_alone";
    let names: Vec<String> = String::from_utf8_lossy(&list.stdout)
        .lines()
        .filter_map(|l| l.strip_suffix(": test"))
        .filter(|n| n.starts_with("reg_ring::") && *n != me)
        .map(str::to_owned)
        .collect();
    assert!(names.len() > 20, "found only {names:?}");
    let failed: Vec<&String> = names
        .iter()
        .filter(|n| {
            let out = Command::new(&exe)
                .args(["--exact", n.as_str(), "--test-threads=1"])
                .output()
                .expect("run a test");
            !(out.status.success() && String::from_utf8_lossy(&out.stdout).contains("1 passed"))
        })
        .collect();
    assert!(failed.is_empty(), "failed alone: {failed:#?}");
}

#[test]
fn the_pass_that_takes_the_spare_starts_no_dac_table() {
    let mut dev = prepared_device(SID_MODEL_6581);
    let mut host = Host::new(SID_MODEL_6581);
    let worn = slices_at_zero(SID_MODEL_6581, SID_CHIP_6581_WORN);
    for l in [&mut *dev.live, &mut *host.live] {
        unsafe { live_param_set(l, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN) };
    }
    let rows = worn - SID_DAC_BUILD_STEPS;
    let mut swapped = None;
    for tick in 1..=worn + 1 {
        if tick == rows + 1 {
            for l in [&mut *dev.live, &mut *host.live] {
                unsafe { live_param_set(l, LIVE_PARAM_DAC, 127) };
            }
        }
        let active = dev.chips.active[0];
        let (a, b) = tick_both(&mut dev, &mut host);
        assert_eq!(a, b, "tick {tick}");
        if swapped.is_none() && dev.chips.active[0] != active {
            swapped = Some(tick);
            assert_eq!(
                dev.chips.ramp[0].building, 0,
                "a table started in the swap's pass"
            );
            assert_eq!(host.target.ramp.building, 0, "the host started one");
        } else if swapped.is_some() {
            assert_eq!(dev.chips.ramp[0].building, 1, "the tick after starts it");
        }
    }
    assert_eq!(swapped, Some(worn));
}

#[test]
fn a_catch_up_pass_runs_one_slice() {
    let mut dev = prepared_device(SID_MODEL_6581);
    unsafe { live_param_set(&mut *dev.live, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN) };
    for pass in 1..=3 {
        unsafe {
            live_tick(&mut *dev.live);
            live_tick(&mut *dev.live);
        }
        dev.block();
        assert_eq!(dev.out.slices, pass, "pass {pass}");
    }
}

#[test]
fn the_other_chips_preparation_waits_for_the_keys_ramp_within_the_bound() {
    let mut t = Two::new();
    unsafe { live_param_set(&mut *t.lives[0], LIVE_PARAM_DAC, 127) };
    t.tick();
    assert_eq!(t.chips.ramp[0].building, 1, "A's ramp builds a table");
    unsafe { live_param_set(&mut *t.lives[1], LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN) };
    let mut started = None;
    for tick in 1..=600u32 {
        t.tick();
        if t.chips.prep_on == 1 && started.is_none() {
            started = Some(tick);
        }
        if started.is_none() {
            assert_eq!(t.chips.prep_wait[1], 1, "tick {tick}: B waits");
        }
    }
    let started = started.expect("B's preparation never started");
    println!("B's preparation started after {started} ticks");
    assert!(
        (REG_RING_YIELD_TICKS..=REG_RING_YIELD_TICKS + SID_DAC_BUILD_STEPS + 1).contains(&started),
        "after {started} ticks"
    );
    assert_eq!(t.chip(1).filter_curve, 3, "B got WORN");
    assert_eq!(t.chips.ramp[0].now.dac, 127, "A's ramp got there");
}

#[test]
fn the_keys_request_preempts_the_others_preparation() {
    let mut t = Two::new();
    let worn = slices_at_zero(SID_MODEL_6581, SID_CHIP_6581_WORN);
    let r2 = slices_at_zero(SID_MODEL_6581, SID_CHIP_6581_R2);
    unsafe { live_param_set(&mut *t.lives[1], LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN) };
    for _ in 0..3 {
        t.tick();
    }
    assert_eq!((t.chips.prep_on, t.chips.prep_chip), (1, 1), "B prepares");
    unsafe { live_param_set(&mut *t.lives[0], LIVE_PARAM_PROFILE, SID_CHIP_6581_R2) };
    let curve_r2 = unsafe { (*sid_chip_profile_get(SID_CHIP_6581_R2)).curve };
    let mut a_at = None;
    for tick in 1..=r2 + worn + 4 {
        t.tick();
        if a_at.is_none() && u32::from(t.lives[0].profile) == SID_CHIP_6581_R2 {
            a_at = Some(tick);
        }
    }
    assert_eq!(a_at, Some(r2), "A was held up");
    assert_eq!(t.chip(0).filter_curve, curve_r2);
    assert_eq!(
        u32::from(t.lives[1].profile),
        SID_CHIP_6581_WORN,
        "B never got WORN"
    );
    assert_eq!(t.chips.lost_resets, 0);
}

#[test]
fn the_keys_chip_plays_as_if_alone() {
    let mut t = Two::new();
    let mut lone = prepared_device(SID_MODEL_6581);
    let a = |t: &mut Two, lone: &mut Device, p: u32, v: u32| unsafe {
        live_param_set(&mut *t.lives[0], p, v);
        live_param_set(&mut *lone.live, p, v);
    };
    for n in [57, 64] {
        unsafe {
            live_note_on(&mut *t.lives[0], n, 100);
            live_note_on(&mut *lone.live, n, 100);
        }
    }
    let mut sound_t = Vec::new();
    let mut sound_lone = Vec::new();
    for tick in 0..700u32 {
        match tick {
            5 => unsafe { live_param_set(&mut *t.lives[1], LIVE_PARAM_DAC, 127) },
            9 => a(&mut t, &mut lone, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN),
            12 => unsafe { live_param_set(&mut *t.lives[1], LIVE_PARAM_PROFILE, SID_CHIP_8580_R5) },
            40 => a(&mut t, &mut lone, LIVE_PARAM_DAC, 90),
            44 => unsafe { live_param_set(&mut *t.lives[1], LIVE_PARAM_THUMP, 127) },
            300 => a(&mut t, &mut lone, LIVE_PARAM_AGE, 127),
            310 => unsafe {
                live_param_set(&mut *t.lives[1], LIVE_PARAM_PROFILE, SID_CHIP_6581_R2)
            },
            320 => a(&mut t, &mut lone, LIVE_PARAM_PROFILE, SID_CHIP_6581_R4AR),
            _ => {}
        }
        t.tick();
        unsafe { live_tick(&mut *lone.live) };
        lone.block();
        assert_eq!(
            chip_bytes_but_the_dac_pointer(t.chip(0)),
            chip_bytes_but_the_dac_pointer(lone.chip()),
            "tick {tick}"
        );
        sound_t.extend(render(t.chip(0), 64));
        sound_lone.extend(render(lone.chip(), 64));
    }
    assert_eq!(sound_t, sound_lone);
    assert_eq!(
        u32::from(t.lives[1].profile),
        SID_CHIP_6581_R2,
        "B caught up"
    );
    assert_eq!(t.chips.ramp[1].now.thump, 127);
    assert_eq!(t.chips.lost_resets, 0);
}

#[test]
fn no_pass_runs_two_builds() {
    let mut t = Two::new();
    let job =
        |c: &reg_ring_chips, x: usize| (c.ramp[x].building, c.ramp[x].job.d, c.ramp[x].job.step);
    let mut most = 0;
    for tick in 0..900u32 {
        let v = tick * 37 % 128;
        match tick % 60 {
            0 => unsafe { live_param_set(&mut *t.lives[0], LIVE_PARAM_DAC, v) },
            7 => unsafe { live_param_set(&mut *t.lives[1], LIVE_PARAM_DAC, 127 - v) },
            19 => unsafe {
                live_param_set(&mut *t.lives[1], LIVE_PARAM_PROFILE, tick / 60 % SID_CHIPS)
            },
            31 => unsafe {
                live_param_set(
                    &mut *t.lives[0],
                    LIVE_PARAM_PROFILE,
                    (tick / 60 + 2) % SID_CHIPS,
                )
            },
            _ => {}
        }
        let before = [job(&t.chips, 0), job(&t.chips, 1)];
        let slices = t.outs[0].slices + t.outs[1].slices;
        t.tick();
        let after = [job(&t.chips, 0), job(&t.chips, 1)];
        let steps: u32 = (0..2)
            .map(|x| {
                let moved = (before[x].1, before[x].2) != (after[x].1, after[x].2);
                u32::from(moved && (before[x].0 == 1 || after[x].0 == 1))
            })
            .sum();
        let work = steps + t.outs[0].slices + t.outs[1].slices - slices;
        most = most.max(work);
        assert!(work <= 1, "tick {tick}: {work} builds in one pass");
    }
    assert_eq!(most, 1);
    assert_eq!(t.chips.lost_resets, 0);
}

#[test]
fn chip_bs_unknown_pseudo_registers_are_dropped() {
    let mut t = Two::new();
    let mut order = Box::new(Order {
        chips: &mut *t.chips,
        seen: Vec::new(),
    });
    let o: *mut Order = &mut *order;
    t.chips.pseudo = Some(record_order);
    t.chips.pseudo_ctx = o.cast();
    for reg in 0xC2..=0xC7u8 {
        assert!(unsafe { reg_ring_push(&mut *t.ring, reg, 1) } != 0);
    }
    assert!(unsafe { reg_ring_push(&mut *t.ring, LIVE_REG_SLOT_BASE as u8, 1) } != 0);
    t.block();
    let regs: Vec<u8> = order.seen.iter().map(|e| e.0).collect();
    assert_eq!(regs, [LIVE_REG_SLOT_BASE as u8]);
}

#[test]
fn a_dac_sweep_on_the_keys_chip_does_not_starve_the_others_preparation() {
    let mut t = Two::new();
    unsafe { live_param_set(&mut *t.lives[1], LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN) };
    let mut done = None;
    for tick in 0..400u32 {
        if tick % 180 == 0 {
            let v = if tick / 180 % 2 == 0 { 127 } else { 0 };
            unsafe { live_param_set(&mut *t.lives[0], LIVE_PARAM_DAC, v) };
        }
        t.tick();
        if done.is_none() && u32::from(t.lives[1].profile) == SID_CHIP_6581_WORN {
            done = Some(tick);
        }
    }
    let done = done.expect("chip B's preparation never ran under the sweep");
    println!("chip B's preparation done after {done} ticks under the sweep");
    assert!(
        done <= REG_RING_YIELD_TICKS + 4 + 11 + 8,
        "after {done} ticks"
    );
    assert_eq!(t.chips.lost_resets, 0);
}
