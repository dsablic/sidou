// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The main-loop to audio-interrupt register ring (`app/reg_ring.c`): the very drain and
//! sink the device runs, with the interrupt played by the wait callback or by the test.

use std::ffi::c_void;

use sidou_sys::*;

const RATE: u32 = 44_118;
const SIZE: u32 = REG_RING_SIZE;
const RESET: u8 = LIVE_REG_RESET as u8;
const GATE_RESET: u8 = LIVE_REG_GATE_RESET as u8;

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

// --- the ring ---

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

// --- the drain ---

#[test]
fn the_drain_applies_registers_to_the_active_chip() {
    let mut r = ring();
    let mut c = chips(SID_MODEL_6581);
    push(&mut r, 0x00, 0x34);
    push(&mut r, 0x01, 0x12);
    commit(&mut r);
    assert_eq!(unsafe { reg_ring_drain(&mut *r, &mut *c) }, 2);
    assert_eq!(c.active, 0);
    assert_eq!(c.chip[0].osc[0].freq, 0x1234);
}

#[test]
fn a_reset_entry_never_reaches_sid_write() {
    // sid_write masks the register to 5 bits: 0x80 would land on 0x00, the low frequency.
    let mut r = ring();
    let mut c = chips(SID_MODEL_6581);
    push(&mut r, 0x00, 0x34);
    push(&mut r, RESET, SID_MODEL_8580 as u8);
    push(&mut r, 0x9f, 0x99); // an unknown pseudo-register is ignored as well
    commit(&mut r);
    assert_eq!(unsafe { reg_ring_drain(&mut *r, &mut *c) }, 3);
    assert_eq!(
        c.chip[0].osc[0].freq, 0x34,
        "0x80 and 0x9f left voice 1 alone"
    );
    assert_eq!(c.chip[0].osc[1].freq, 0);
    assert_eq!(c.active, 0, "no spare was prepared: no swap");
    assert_eq!(c.lost_resets, 1);
}

#[test]
fn a_reset_entry_swaps_to_the_prepared_spare() {
    let mut r = ring();
    let mut c = chips(SID_MODEL_6581);
    unsafe { sid_init(&mut c.chip[1], SID_MODEL_8580, SID_CLOCK_PAL, RATE) };
    c.spare_model = SID_MODEL_8580 as u8;
    c.spare_ready = 1;
    push(&mut r, RESET, SID_MODEL_8580 as u8);
    push(&mut r, 0x00, 0x56);
    commit(&mut r);
    unsafe { reg_ring_drain(&mut *r, &mut *c) };
    assert_eq!(c.active, 1);
    assert_eq!(c.spare_ready, 0);
    assert_eq!(c.lost_resets, 0);
    assert_eq!(c.chip[1].model, SID_MODEL_8580);
    assert_eq!(
        c.chip[1].osc[0].freq, 0x56,
        "writes after the reset go to the new chip"
    );
    assert_eq!(c.chip[0].osc[0].freq, 0);
    assert!(std::ptr::eq(unsafe { reg_ring_chip(&mut *c) }, &c.chip[1]));
}

#[test]
fn a_gate_reset_entry_sets_the_option_on_the_active_chip_only() {
    let mut r = ring();
    let mut c = chips(SID_MODEL_6581);
    unsafe { sid_init(&mut c.chip[1], SID_MODEL_8580, SID_CLOCK_PAL, RATE) };
    c.spare_model = SID_MODEL_8580 as u8;
    c.spare_ready = 1;
    push(&mut r, GATE_RESET, 1);
    push(&mut r, 0x00, 0x12);
    commit(&mut r);
    assert_eq!(unsafe { reg_ring_drain(&mut *r, &mut *c) }, 2);
    assert!(c.chip[0].env.iter().all(|e| e.gate_reset == 1));
    assert_eq!(c.chip[0].osc[0].freq, 0x12, "it never reaches sid_write");
    assert!(c.chip[1].env.iter().all(|e| e.gate_reset == 0));
    // A reset swaps to a chip sid_init left authentic; the entry after it sets the new one.
    push(&mut r, RESET, SID_MODEL_8580 as u8);
    push(&mut r, GATE_RESET, 1);
    commit(&mut r);
    unsafe { reg_ring_drain(&mut *r, &mut *c) };
    assert_eq!(c.active, 1);
    assert!(c.chip[1].env.iter().all(|e| e.gate_reset == 1));
    push(&mut r, GATE_RESET, 0);
    commit(&mut r);
    unsafe { reg_ring_drain(&mut *r, &mut *c) };
    assert!(c.chip[1].env.iter().all(|e| e.gate_reset == 0));
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
    assert_eq!((c.active, c.lost_resets), (0, 1));
}

// --- the sink, against live writing straight into a chip ---

/// live writing through the ring into two chips; the wait callback is the audio interrupt.
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
        // The device's chips start zeroed and initialised by live_init's reset.
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

    /// The interrupt at a block boundary.
    fn block(&mut self) {
        commit(&mut self.ring);
        unsafe { reg_ring_drain(&mut *self.ring, &mut *self.chips) };
    }

    fn chip(&mut self) -> &mut sid {
        unsafe { &mut *reg_ring_chip(&mut *self.chips) }
    }
}

/// The reference: live_sid_sink applies each write at once.
struct Direct {
    chip: Box<sid>,
    _target: Box<live_sid_target>,
    _dacs: Box<[sid_dac; 2]>,
    live: Box<live>,
}

impl Direct {
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
        Direct {
            chip,
            _target: target,
            _dacs: dacs,
            live: l,
        }
    }
}

/// Plays the same calls into both.
fn both(dev: &mut Device, direct: &mut Direct, f: impl Fn(&mut live)) {
    f(&mut dev.live);
    f(&mut direct.live);
}

fn assert_same_sound(dev: &mut Device, direct: &mut Direct) {
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
        let mut direct = Direct::new(model);
        assert_eq!(dev.chips.lost_resets, 0);
        both(&mut dev, &mut direct, |l| unsafe {
            live_note_on(l, 60, 100)
        });
        assert_same_sound(&mut dev, &mut direct);
    }
}

#[test]
fn the_envelope_mode_reaches_the_chip_through_the_ring_and_across_model_changes() {
    let mut dev = Device::new(SID_MODEL_6581);
    let mut direct = Direct::new(SID_MODEL_6581);
    let gate_reset = |s: &sid| s.env.map(|e| e.gate_reset);
    dev.block();
    assert_eq!(gate_reset(dev.chip()), [1; 3], "instant by default");
    assert_eq!(gate_reset(&direct.chip), [1; 3]);
    for (mode, want) in [(LIVE_ENVELOPE_AUTHENTIC, 0u8), (LIVE_ENVELOPE_INSTANT, 1)] {
        both(&mut dev, &mut direct, |l| unsafe {
            live_param_set(l, LIVE_PARAM_ENVELOPE_MODE, mode);
            live_button(l, LIVE_BUTTON_FX);
            live_note_on(l, 60, 100);
        });
        dev.block();
        assert_eq!(gate_reset(dev.chip()), [want; 3], "mode {mode}");
        assert_eq!(gate_reset(&direct.chip), [want; 3], "mode {mode}");
        assert_eq!(dev.chips.lost_resets, 0);
        assert_same_sound(&mut dev, &mut direct);
        both(&mut dev, &mut direct, |l| unsafe { live_all_notes_off(l) });
    }
}

#[test]
fn the_model_toggle_swaps_chips_and_keeps_the_sound_parameters() {
    let mut dev = Device::new(SID_MODEL_6581);
    let mut direct = Direct::new(SID_MODEL_6581);
    dev.block();
    both(&mut dev, &mut direct, |l| unsafe {
        live_button(l, LIVE_BUTTON_FX)
    });
    assert_eq!(dev.out.reset_waits, 0, "the ring was empty");
    dev.block();
    assert_eq!(dev.chip().model, SID_MODEL_8580);
    assert_eq!(
        dev.chips.active, 0,
        "live_init's reset took chip 1, the toggle chip 0"
    );

    // With writes still queued the sink waits for them before it touches the spare.
    both(&mut dev, &mut direct, |l| unsafe {
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

/// The character (live.h, LIVE_REG_CHAR): its controls ramp the playing chip a tick at a time
/// (sid_character_ramp_tick), a profile resets it, and every reset keeps them, through the
/// ring exactly as live_sid_sink makes them at once, tick for tick. Nothing waits for the
/// interrupt, and no table a chip plays from changes under it.
#[test]
fn the_character_reaches_the_chip_through_the_ring_and_across_model_changes() {
    let mut dev = Device::new(SID_MODEL_6581);
    let mut direct = Direct::new(SID_MODEL_6581);
    dev.block();
    let default_dac = |model| unsafe { sid_dac_default(i32::from(model == SID_MODEL_6581)) };
    let waits = (dev.out.full_waits, dev.out.reset_waits);
    // The controls, a note held, then a tick a block until the ramp is done: 32 ticks for
    // age, thump and bleed, 16 for the DACs (four tables, each over SID_DAC_BUILD_STEPS).
    both(&mut dev, &mut direct, |l| unsafe {
        live_note_on(l, 60, 100);
        live_param_set(l, LIVE_PARAM_AGE, 90);
        live_param_set(l, LIVE_PARAM_DAC, 127);
        live_param_set(l, LIVE_PARAM_THUMP, 64);
        live_param_set(l, LIVE_PARAM_BLEED, 100);
    });
    let mut tables: Vec<*const sid_dac> = Vec::new();
    let mut playing: Option<(*const sid_dac, Vec<f32>)> = None;
    let ticks = dev.chips.ticks;
    for tick in 0..40 {
        both(&mut dev, &mut direct, |l| unsafe { live_tick(l) });
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
    assert_eq!(dev.chips.ticks, ticks + 40, "a REG_RING_CHARACTER a tick");
    let s = dev.chip();
    assert_eq!(
        (s.thump, s.bleed, s.filter_t),
        (64.0 / 127.0, 0.5 * 100.0 / 127.0, 90.0 / 127.0),
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
    // A profile is a reset: the spare gets it and the controls before the swap, at once.
    both(&mut dev, &mut direct, |l| unsafe {
        live_param_set(l, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN);
        live_tick(l);
        live_note_on(l, 64, 100);
    });
    dev.block();
    assert_eq!(dev.chips.lost_resets, 0);
    assert_eq!(dev.chip().filter_curve, 3, "worn's filter row");
    assert_eq!(
        dev.chip().thump,
        64.0 / 127.0,
        "the reset keeps the controls"
    );
    assert_same_sound(&mut dev, &mut direct);
    // To the 8580 and back: the 8580 R5 profile, then worn again, the controls kept.
    for (model, curve) in [(SID_MODEL_8580, 0u8), (SID_MODEL_6581, 3)] {
        both(&mut dev, &mut direct, |l| unsafe {
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
    // Back to zero: the profile's own DACs and output stage.
    both(&mut dev, &mut direct, |l| unsafe {
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

/// Two ticks in one pass (the main loop catching up) queue both stages; a third before the
/// interrupt took either finds neither free and waits a tick (character_defers) rather than
/// write a queued stage or wait for the interrupt. The drain commits them in order.
#[test]
fn a_tick_with_both_stages_queued_waits_its_turn() {
    let mut dev = Device::new(SID_MODEL_6581);
    dev.block();
    let ticks = dev.chips.ticks;
    unsafe { live_param_set(&mut *dev.live, LIVE_PARAM_THUMP, 127) };
    for _ in 0..3 {
        unsafe { live_tick(&mut *dev.live) };
    }
    assert_eq!(dev.out.character_defers, 1);
    assert_eq!(dev.chips.stage_pending, [1, 1]);
    let step = |n: f32| n * SID_CHARACTER_RAMP as f32 / 127.0;
    assert_eq!(
        [dev.chips.stage[0].thump, dev.chips.stage[1].thump],
        [step(1.0), step(2.0)]
    );
    assert_eq!(dev.out.full_waits + dev.out.reset_waits, 0);
    dev.block();
    assert_eq!(dev.chips.stage_pending, [0, 0]);
    assert_eq!(dev.chip().thump, step(2.0), "the later stage last");
    assert_eq!(dev.chips.ticks, ticks + 3);
    unsafe { live_tick(&mut *dev.live) };
    dev.block();
    assert_eq!(dev.chip().thump, step(3.0), "the ramp goes on");
}

/// One main-loop pass at its worst (reg_ring.c, REG_RING_WORST_PASS), in the order the
/// device's input code calls live: buttons, note keys, encoders, then each tick catch-up
/// after a MASTER reading (platform/fm1/main.c).
/// Without `fx` the pass skips the model toggle, whose sink waits for an empty ring.
fn worst_pass(l: &mut live, p: &mut panel, fx: bool) {
    unsafe {
        if fx {
            panel_buttons(p, l, 1 << PANEL_BTN_FX, 0);
        }
        // the selected effect slot's next algorithm (EDIT not held: the knobs stay knobs)
        panel_buttons(p, l, 1 << PANEL_BTN_ENV, 0);
        // the sync lead: every note-on also sets the sync voice's frequency
        panel_encoder(p, l, PANEL_ENC_K4, 1);
        // Expression MIDI set earlier (here after K4, so the preset does not reset it): the
        // velocity lowers the cutoff, aftertouch raises the pulse width and starts again at 0
        // with each note, the vibrato and the bend move every frequency at the ticks.
        live_param_set(l, LIVE_PARAM_VELOCITY_TARGET, LIVE_VELOCITY_CUTOFF);
        live_param_set(l, LIVE_PARAM_PRESSURE_TARGET, LIVE_PRESSURE_PULSE_WIDTH);
        live_param_set(l, LIVE_PARAM_VIBRATO_DEPTH, 127);
        live_poly_pressure(l, 60, 90);
        live_pressure(l, 30);
        live_bend(l, 12000);
        // every key pressed and let go within the pass, each retriggering voice 1
        panel_note_keys(p, l, 0, (1 << PANEL_NOTE_KEYS) - 1);
        panel_encoder(p, l, PANEL_ENC_K1, -3);
        panel_encoder(p, l, PANEL_ENC_K1 + 1, 2);
        panel_encoder(p, l, PANEL_ENC_K1 + 2, 5);
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
    let mut direct = Direct::new(SID_MODEL_6581);
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
    both(&mut dev, &mut direct, |l| unsafe {
        live_note_on(l, 57, 100)
    });
    assert_same_sound(&mut dev, &mut direct);
}

#[test]
fn a_burst_past_the_ring_waits_and_drops_nothing() {
    let mut dev = Device::new(SID_MODEL_6581);
    let mut direct = Direct::new(SID_MODEL_6581);
    let (mut pd, mut pr) = (panel_box(), panel_box());
    // Worst passes with no block in between until they fill the ring: over 200 entries each
    // (the test above), so twice the ring's size in passes is more than enough. Only the
    // first toggles the model (a toggle waits for an empty ring anyway).
    for i in 0..2 * SIZE / 200 {
        worst_pass(&mut dev.live, &mut pd, i == 0);
        worst_pass(&mut direct.live, &mut pr, i == 0);
    }
    assert!(dev.out.full_waits > 0, "the burst never filled the ring");
    assert_eq!(dev.chips.lost_resets, 0);
    assert_eq!(dev.live.model, direct.live.model);
    // Hold a chord so the comparison hears the gates.
    for n in [48, 55, 60] {
        both(&mut dev, &mut direct, |l| unsafe {
            live_note_on(l, n, 100)
        });
    }
    assert_same_sound(&mut dev, &mut direct);
}

#[test]
fn a_full_ring_of_gate_writes_ends_with_the_last_gate() {
    // Without waiting, the write past the ring's size would be lost; with it, the final gate
    // state holds.
    let mut dev = Device::new(SID_MODEL_8580);
    let mut direct = Direct::new(SID_MODEL_8580);
    for i in 0..SIZE {
        let n = 40 + (i % 30) as u8;
        both(&mut dev, &mut direct, |l| unsafe {
            live_note_on(l, n, 100);
            live_note_off(l, n);
        });
    }
    assert!(dev.out.full_waits > 0);
    both(&mut dev, &mut direct, |l| unsafe {
        live_note_on(l, 50, 100)
    });
    assert_same_sound(&mut dev, &mut direct);
}

#[test]
fn the_documented_worst_pass_fits_exactly_without_waiting() {
    // REG_RING_WORST_PASS entries in one pass, as the sink sees them: a model change (the
    // reset waits for an empty ring) and every register after it, then single writes.
    let mut dev = Device::new(SID_MODEL_6581);
    let mut direct = Direct::new(SID_MODEL_6581);
    dev.block();
    let sink = |reg: u8, value: u8, dev: &mut Device, direct: &mut Direct| {
        let o: *mut reg_ring_out = &mut *dev.out;
        unsafe { reg_ring_sink(o.cast(), reg, value) };
        let t = direct.live.sink_ctx;
        unsafe { live_sid_sink(t, reg, value) };
    };
    sink(RESET, SID_MODEL_8580 as u8, &mut dev, &mut direct);
    for i in 1..REG_RING_WORST_PASS {
        // cycle through every SID register with changing values
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
