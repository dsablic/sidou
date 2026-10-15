// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! MIDI learn (`app/midi_learn.c`): the state machine (arm, knob, CC, cancel, timeout, forget,
//! clear), the bindings over the map (midi_map_cc), through MIDI input (midi_rx) and out
//! (midi_out), and the panel's knob-to-parameter mapping it shares with the panel.

use sidou_sys::*;

use crate::Rig;

const TRS: u8 = MIDI_SRC_TRS as u8;
const NO_CC: u8 = MIDI_MAP_NO_CC as u8;
const ROOM: u32 = 64;
const TICK: usize = MIDI_OUT_TICK_PACKETS as usize;

fn learn() -> Box<midi_learn> {
    let mut ml = Box::<midi_learn>::default();
    unsafe { midi_learn_init(&mut *ml) };
    ml
}

fn row(param: u32) -> &'static midi_map_entry {
    let e = unsafe { midi_map_for_param(param) };
    assert!(!e.is_null());
    unsafe { &*e }
}

fn routed(ml: &midi_learn, cc: u32) -> Option<u32> {
    let e = unsafe { midi_learn_for_cc(ml, cc) };
    (!e.is_null()).then(|| u32::from(unsafe { &*e }.param))
}

fn cc_of(ml: &midi_learn, param: u32) -> u8 {
    unsafe { midi_learn_cc_of(ml, param) }
}

fn take(ml: &mut midi_learn) -> midi_learn_result {
    unsafe { midi_learn_take(ml) }
}

/// Arm, pick param with a knob, then the CC through midi_learn_route.
fn learn_cc(ml: &mut midi_learn, param: u32, cc: u8) {
    unsafe {
        midi_learn_button(ml, 0);
        assert_eq!(midi_learn_knob(ml, param, 0), 1);
        let e = midi_learn_route(ml, cc);
        assert!(!e.is_null());
        assert_eq!(u32::from((*e).param), param);
    }
    assert_eq!(take(ml), MIDI_LEARN_LEARNED);
}

// --- the state machine ---

#[test]
fn at_power_on_nothing_is_bound_and_learn_is_idle() {
    let ml = learn();
    assert_eq!(u32::from(ml.state), MIDI_LEARN_IDLE);
    assert_eq!(ml.count, 0);
    for cc in 0..128 {
        assert_eq!(routed(&ml, cc), None);
    }
    for p in 0..LIVE_PARAM_COUNT {
        assert_eq!(cc_of(&ml, p), NO_CC);
    }
}

#[test]
fn the_learnable_ccs_are_0_to_119_but_data_entry_and_parameter_selection() {
    let learnable: Vec<u32> = (0..=255)
        .filter(|&cc| unsafe { midi_learn_learnable(cc) } != 0)
        .collect();
    let want: Vec<u32> = (0..120)
        .filter(|cc| ![6, 38, 96, 97, 98, 99, 100, 101].contains(cc))
        .collect();
    assert_eq!(learnable, want);
}

#[test]
fn every_parameter_with_a_map_row_can_be_learned() {
    for p in 0..LIVE_PARAM_COUNT + 2 {
        let has_row = !unsafe { midi_map_for_param(p) }.is_null();
        assert_eq!(unsafe { midi_learn_param(p) } != 0, has_row, "{p}");
    }
    assert_eq!(unsafe { midi_learn_param(MIDI_MAP_RESERVED) }, 0);
}

#[test]
fn arm_then_a_knob_then_a_cc_binds_the_cc_to_the_knobs_parameter() {
    let mut ml = learn();
    unsafe { midi_learn_button(&mut *ml, 100) };
    assert_eq!(u32::from(ml.state), MIDI_LEARN_KNOB);
    // A CC before a knob binds nothing.
    assert!(unsafe { midi_learn_route(&mut *ml, 21) }.is_null());
    assert_eq!(u32::from(ml.state), MIDI_LEARN_KNOB);
    assert_eq!(
        unsafe { midi_learn_knob(&mut *ml, LIVE_PARAM_CUTOFF, 200) },
        1
    );
    assert_eq!(u32::from(ml.state), MIDI_LEARN_CC);
    assert_eq!(u32::from(ml.param), LIVE_PARAM_CUTOFF);
    // Data entry and the channel mode messages do not bind: it still waits.
    for cc in [6, 38, 96, 97, 98, 99, 100, 101, 120, 121, 123, 127] {
        assert!(unsafe { midi_learn_route(&mut *ml, cc) }.is_null(), "{cc}");
        assert_eq!(u32::from(ml.state), MIDI_LEARN_CC);
    }
    let e = unsafe { midi_learn_route(&mut *ml, 21) };
    assert_eq!(e, row(LIVE_PARAM_CUTOFF) as *const _);
    assert_eq!(u32::from(ml.state), MIDI_LEARN_IDLE);
    assert_eq!((ml.result_cc, ml.count), (21, 1));
    assert_eq!(take(&mut ml), MIDI_LEARN_LEARNED);
    assert_eq!(take(&mut ml), MIDI_LEARN_NOTHING, "taken once");
    assert_eq!(routed(&ml, 21), Some(LIVE_PARAM_CUTOFF));
    assert_eq!(cc_of(&ml, LIVE_PARAM_CUTOFF), 21);
    // Idle: the next CC is only routed, nothing more binds.
    assert!(unsafe { midi_learn_route(&mut *ml, 22) }.is_null());
    assert_eq!(ml.count, 1);
}

#[test]
fn a_knob_while_idle_or_without_a_parameter_is_not_taken() {
    let mut ml = learn();
    assert_eq!(
        unsafe { midi_learn_knob(&mut *ml, LIVE_PARAM_CUTOFF, 0) },
        0
    );
    assert_eq!(u32::from(ml.state), MIDI_LEARN_IDLE);
    unsafe { midi_learn_button(&mut *ml, 0) };
    // K4 (the preset) has no parameter: the panel says LIVE_PARAM_COUNT.
    assert_eq!(unsafe { midi_learn_knob(&mut *ml, LIVE_PARAM_COUNT, 0) }, 0);
    assert_eq!(u32::from(ml.state), MIDI_LEARN_KNOB);
    // Another knob after the first picks again.
    unsafe { midi_learn_knob(&mut *ml, LIVE_PARAM_CUTOFF, 0) };
    unsafe { midi_learn_knob(&mut *ml, LIVE_PARAM_RESONANCE, 0) };
    assert_eq!(u32::from(ml.param), LIVE_PARAM_RESONANCE);
}

#[test]
fn home_cancels_at_every_step_and_binds_nothing() {
    for knob in [false, true] {
        let mut ml = learn();
        unsafe { midi_learn_button(&mut *ml, 0) };
        if knob {
            unsafe { midi_learn_knob(&mut *ml, LIVE_PARAM_CUTOFF, 0) };
        }
        unsafe { midi_learn_cancel(&mut *ml) };
        assert_eq!(u32::from(ml.state), MIDI_LEARN_IDLE);
        assert_eq!(take(&mut ml), MIDI_LEARN_CANCELLED);
        assert!(unsafe { midi_learn_route(&mut *ml, 21) }.is_null());
        assert_eq!(ml.count, 0);
    }
    let mut ml = learn();
    unsafe { midi_learn_cancel(&mut *ml) };
    assert_eq!(take(&mut ml), MIDI_LEARN_NOTHING, "nothing to cancel");
}

#[test]
fn ten_seconds_without_a_step_time_out_and_each_step_restarts_the_time() {
    let t = MIDI_LEARN_TIMEOUT_MS;
    let mut ml = learn();
    let start = 0xffff_f000u32; // across the clock's wrap
    unsafe { midi_learn_button(&mut *ml, start) };
    assert_eq!(
        unsafe { midi_learn_tick(&mut *ml, start.wrapping_add(t - 1)) },
        0
    );
    unsafe { midi_learn_knob(&mut *ml, LIVE_PARAM_CUTOFF, start.wrapping_add(t - 1)) };
    assert_eq!(
        unsafe { midi_learn_tick(&mut *ml, start.wrapping_add(2 * t - 2)) },
        0
    );
    assert_eq!(u32::from(ml.state), MIDI_LEARN_CC);
    assert_eq!(
        unsafe { midi_learn_tick(&mut *ml, start.wrapping_add(2 * t - 1)) },
        1
    );
    assert_eq!(u32::from(ml.state), MIDI_LEARN_IDLE);
    assert_eq!(take(&mut ml), MIDI_LEARN_TIMED_OUT);
    assert!(unsafe { midi_learn_route(&mut *ml, 21) }.is_null());
    assert_eq!(
        unsafe { midi_learn_tick(&mut *ml, 0) },
        0,
        "idle never times out"
    );
}

#[test]
fn the_button_three_times_clears_every_binding() {
    let mut ml = learn();
    learn_cc(&mut ml, LIVE_PARAM_CUTOFF, 21);
    learn_cc(&mut ml, LIVE_PARAM_RESONANCE, 22);
    unsafe { midi_learn_button(&mut *ml, 0) };
    unsafe { midi_learn_button(&mut *ml, 0) };
    assert_eq!(u32::from(ml.state), MIDI_LEARN_CONFIRM);
    assert_eq!(ml.count, 2, "asking clears nothing");
    unsafe { midi_learn_button(&mut *ml, 0) };
    assert_eq!(u32::from(ml.state), MIDI_LEARN_IDLE);
    assert_eq!(take(&mut ml), MIDI_LEARN_CLEARED);
    assert_eq!(ml.count, 0);
    assert_eq!((routed(&ml, 21), routed(&ml, 22)), (None, None));
    assert_eq!(cc_of(&ml, LIVE_PARAM_CUTOFF), NO_CC);
}

#[test]
fn home_or_a_knob_after_the_question_clears_nothing() {
    let mut ml = learn();
    learn_cc(&mut ml, LIVE_PARAM_CUTOFF, 21);
    unsafe { midi_learn_button(&mut *ml, 0) };
    unsafe { midi_learn_button(&mut *ml, 0) };
    unsafe { midi_learn_cancel(&mut *ml) };
    assert_eq!(take(&mut ml), MIDI_LEARN_CANCELLED);
    assert_eq!(ml.count, 1);
    unsafe { midi_learn_button(&mut *ml, 0) };
    unsafe { midi_learn_button(&mut *ml, 0) };
    assert_eq!(
        unsafe { midi_learn_knob(&mut *ml, LIVE_PARAM_RESONANCE, 0) },
        1
    );
    assert_eq!(u32::from(ml.state), MIDI_LEARN_CC);
    assert_eq!(ml.count, 1);
}

#[test]
fn the_button_instead_of_a_cc_forgets_the_parameters_binding() {
    let mut ml = learn();
    learn_cc(&mut ml, LIVE_PARAM_CUTOFF, 21);
    learn_cc(&mut ml, LIVE_PARAM_RESONANCE, 22);
    unsafe { midi_learn_button(&mut *ml, 0) };
    unsafe { midi_learn_knob(&mut *ml, LIVE_PARAM_CUTOFF, 0) };
    unsafe { midi_learn_button(&mut *ml, 0) };
    assert_eq!(u32::from(ml.state), MIDI_LEARN_IDLE);
    assert_eq!((take(&mut ml), ml.result_cc), (MIDI_LEARN_FORGOT, 21));
    assert_eq!(routed(&ml, 21), None);
    assert_eq!(
        routed(&ml, 22),
        Some(LIVE_PARAM_RESONANCE),
        "the other stays"
    );
    assert_eq!(ml.count, 1);
    // A parameter with nothing bound: forgot nothing.
    unsafe { midi_learn_button(&mut *ml, 0) };
    unsafe { midi_learn_knob(&mut *ml, LIVE_PARAM_CUTOFF, 0) };
    unsafe { midi_learn_button(&mut *ml, 0) };
    assert_eq!((take(&mut ml), ml.result_cc), (MIDI_LEARN_FORGOT, NO_CC));
}

#[test]
fn one_cc_per_parameter_and_one_parameter_per_cc() {
    let mut ml = learn();
    learn_cc(&mut ml, LIVE_PARAM_CUTOFF, 21);
    // The cutoff again, on another CC: it moves.
    learn_cc(&mut ml, LIVE_PARAM_CUTOFF, 22);
    assert_eq!(routed(&ml, 21), None);
    assert_eq!(routed(&ml, 22), Some(LIVE_PARAM_CUTOFF));
    assert_eq!(ml.count, 1);
    // CC 22 for the resonance: the CC moves, the cutoff has none.
    learn_cc(&mut ml, LIVE_PARAM_RESONANCE, 22);
    assert_eq!(routed(&ml, 22), Some(LIVE_PARAM_RESONANCE));
    assert_eq!(cc_of(&ml, LIVE_PARAM_CUTOFF), NO_CC);
    assert_eq!(ml.count, 1);
    // The same binding again changes nothing.
    learn_cc(&mut ml, LIVE_PARAM_RESONANCE, 22);
    assert_eq!(ml.count, 1);
}

#[test]
fn every_parameter_can_hold_a_binding_at_once() {
    let mut ml = learn();
    let params: Vec<u32> = (0..LIVE_PARAM_COUNT)
        .filter(|&p| unsafe { midi_learn_param(p) } != 0)
        .collect();
    let ccs: Vec<u32> = (0..128)
        .filter(|&c| unsafe { midi_learn_learnable(c) } != 0)
        .collect();
    for (&p, &c) in params.iter().zip(&ccs) {
        assert_eq!(unsafe { midi_learn_bind(&mut *ml, c, p) }, 1);
    }
    assert_eq!(usize::from(ml.count), params.len());
    for (&p, &c) in params.iter().zip(&ccs) {
        assert_eq!(routed(&ml, c), Some(p));
        assert_eq!(u32::from(cc_of(&ml, p)), c);
    }
    assert_eq!(
        unsafe { midi_learn_bind(&mut *ml, 6, LIVE_PARAM_CUTOFF) },
        0
    );
    assert_eq!(
        unsafe { midi_learn_bind(&mut *ml, 21, LIVE_PARAM_COUNT) },
        0
    );
    assert_eq!(
        unsafe { midi_learn_bind(&mut *ml, 200, LIVE_PARAM_CUTOFF) },
        0
    );
}

// --- through the map and MIDI input ---

/// A live instrument fed through a `midi_rx` and watched by MIDI out, with learn wired in as
/// the device does (platform/fm1/midi.c midi_init).
struct Learned {
    rig: Rig,
    rx: Box<midi_rx>,
    out: Box<midi_out>,
    ml: Box<midi_learn>,
}

impl Learned {
    fn new() -> Self {
        let mut rig = Rig::new(SID_MODEL_6581);
        let mut rx = Box::<midi_rx>::default();
        let mut out = Box::<midi_out>::default();
        let mut ml = learn();
        unsafe {
            midi_rx_init(&mut *rx, &mut *rig.live, 0);
            midi_out_init(&mut *out, &*rig.live, 0, 0);
            midi_rx_watch(&mut *rx, &mut *out);
            midi_rx_learn(&mut *rx, &mut *ml);
            midi_out_learn(&mut *out, &*ml);
        }
        Learned { rig, rx, out, ml }
    }

    fn entry(&mut self, source: u8, data: u32, epoch: u8) {
        let e = midi_ring_entry {
            stamp: 0,
            data,
            source,
            epoch,
        };
        unsafe { midi_rx_entry(&mut *self.rx, &e) };
    }

    /// TRS bytes in one drain pass.
    fn trs(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.entry(TRS, u32::from(b), self.rx.epoch);
        }
        unsafe { midi_rx_settle(&mut *self.rx) };
    }

    fn usb_cc(&mut self, channel: u8, cc: u8, value: u8) {
        let p = u32::from_le_bytes([0xB, 0xB0 | channel, cc, value]);
        self.entry(MIDI_SRC_USB as u8, p, self.rx.epoch);
        unsafe { midi_rx_settle(&mut *self.rx) };
    }

    fn get(&self, param: u32) -> u32 {
        unsafe { live_param_get(&*self.rig.live, param) }
    }

    fn set(&mut self, param: u32, value: u32) {
        unsafe { live_param_set(&mut *self.rig.live, param, value) }
    }

    fn tick(&mut self) -> Vec<u32> {
        let mut buf = [0u32; TICK];
        let n = unsafe { midi_out_tick(&mut *self.out, &*self.rig.live, ROOM, buf.as_mut_ptr()) };
        buf[..n as usize].to_vec()
    }

    /// The gesture with the CC from TRS (on channel 5: learn is omni).
    fn learn(&mut self, param: u32, cc: u8, value: u8) {
        unsafe {
            midi_learn_button(&mut *self.ml, 0);
            midi_learn_knob(&mut *self.ml, param, 0);
        }
        self.trs(&[0xB4, cc, value]);
        assert_eq!(take(&mut self.ml), MIDI_LEARN_LEARNED);
    }
}

fn from_cc(param: u32, value: u8) -> u32 {
    unsafe { midi_map_from_cc(row(param), value) }
}

fn cc_packet(cc: u8, value: u8) -> u32 {
    u32::from_le_bytes([0x0B | (MIDI_OUT_CABLE as u8) << 4, 0xB0, cc, value])
}

#[test]
fn the_cc_that_teaches_also_sets_and_then_the_parameter_follows_it_on_every_channel() {
    let mut m = Learned::new();
    m.learn(LIVE_PARAM_CUTOFF, 21, 100);
    assert_eq!(m.get(LIVE_PARAM_CUTOFF), from_cc(LIVE_PARAM_CUTOFF, 100));
    for (ch, v) in [(0u8, 0u8), (15, 127), (9, 64)] {
        m.trs(&[0xB0 | ch, 21, v]);
        assert_eq!(m.get(LIVE_PARAM_CUTOFF), from_cc(LIVE_PARAM_CUTOFF, v));
    }
    m.usb_cc(3, 21, 10);
    assert_eq!(m.get(LIVE_PARAM_CUTOFF), from_cc(LIVE_PARAM_CUTOFF, 10));
    // The map's own CC still works.
    m.trs(&[0xB0, LIVE_CC_CUTOFF as u8, 90]);
    assert_eq!(m.get(LIVE_PARAM_CUTOFF), from_cc(LIVE_PARAM_CUTOFF, 90));
}

#[test]
fn a_learned_cc_takes_the_parameters_curve_and_range() {
    let mut m = Learned::new();
    m.learn(LIVE_PARAM_RESONANCE, 21, 0);
    for v in [0u8, 7, 8, 63, 64, 127] {
        m.trs(&[0xB0, 21, v]);
        assert_eq!(m.get(LIVE_PARAM_RESONANCE), u32::from(v) * 16 / 128, "{v}");
    }
    m.learn(LIVE_PARAM_SLOT + LIVE_SLOT_FIELDS + LIVE_SLOT_MIX, 22, 0);
    m.trs(&[0xB0, 22, 99]);
    assert_eq!(m.rig.live.slot[1][LIVE_SLOT_MIX as usize], 99);
}

#[test]
fn a_learned_cc_that_was_another_parameters_wins_and_gives_it_back_when_forgotten() {
    let mut m = Learned::new();
    let cutoff = m.get(LIVE_PARAM_CUTOFF);
    m.learn(LIVE_PARAM_RESONANCE, LIVE_CC_CUTOFF as u8, 127);
    assert_eq!(m.get(LIVE_PARAM_RESONANCE), 15);
    m.trs(&[0xB0, LIVE_CC_CUTOFF as u8, 0]);
    assert_eq!(m.get(LIVE_PARAM_RESONANCE), 0);
    assert_eq!(
        m.get(LIVE_PARAM_CUTOFF),
        cutoff,
        "CC 74 is the resonance's now"
    );
    // The cutoff's NRPN still reaches it.
    m.trs(&[0xB0, 99, 0, 98, 0, 6, 3, 38, 0]);
    assert_eq!(m.get(LIVE_PARAM_CUTOFF), 3 << 7);
    // Forgotten: CC 74 is the cutoff's again.
    unsafe { midi_learn_forget(&mut *m.ml, LIVE_PARAM_RESONANCE) };
    m.trs(&[0xB0, LIVE_CC_CUTOFF as u8, 127]);
    assert_eq!(m.get(LIVE_PARAM_CUTOFF), LIVE_CUTOFF_MAX);
}

#[test]
fn a_learned_cc_1_wins_over_the_mod_wheel() {
    let mut m = Learned::new();
    m.set(LIVE_PARAM_MOD_TARGET, LIVE_MOD_VIBRATO);
    m.learn(LIVE_PARAM_CUTOFF, 1, 0);
    m.trs(&[0xB0, 1, 127]);
    assert_eq!(m.get(LIVE_PARAM_CUTOFF), LIVE_CUTOFF_MAX);
    assert_eq!(m.rig.live.mod_wheel, 0, "the wheel did not move");
}

#[test]
fn nrpn_data_entry_and_channel_mode_are_never_learned() {
    let mut m = Learned::new();
    unsafe {
        midi_learn_button(&mut *m.ml, 0);
        midi_learn_knob(&mut *m.ml, LIVE_PARAM_CUTOFF, 0);
    }
    // An NRPN for the resonance, all of it, while learn waits.
    m.trs(&[0xB0, 99, 0, 98, 1, 6, 0, 38, 9, 96, 0, 0xB0, 121, 0, 123, 0]);
    assert_eq!(m.get(LIVE_PARAM_RESONANCE), 10);
    assert_eq!(u32::from(m.ml.state), MIDI_LEARN_CC, "still waiting");
    assert_eq!(m.ml.count, 0);
}

#[test]
fn bindings_survive_a_gap_in_the_input_and_a_program_change() {
    let mut m = Learned::new();
    m.learn(LIVE_PARAM_CUTOFF, 21, 0);
    m.entry(TRS, 0xB0, 1); // a later epoch: the parsers restart
    assert_eq!(m.rx.restarts, 1);
    m.trs(&[0xC0, 1, 0xB0, 21, 127]);
    assert_eq!(m.rig.live.preset, 1);
    assert_eq!(m.get(LIVE_PARAM_CUTOFF), LIVE_CUTOFF_MAX);
}

// --- MIDI out ---

#[test]
fn a_panel_change_to_a_learned_parameter_goes_out_on_the_learned_cc() {
    let mut m = Learned::new();
    m.learn(LIVE_PARAM_CUTOFF, 21, 64);
    assert_eq!(m.tick(), [], "the CC that taught is MIDI's: not echoed");
    m.set(LIVE_PARAM_CUTOFF, 2047);
    assert_eq!(m.tick(), [cc_packet(21, 127)]);
    // The others still go on the map's CCs.
    m.set(LIVE_PARAM_RESONANCE, 3);
    assert_eq!(
        m.tick(),
        [cc_packet(LIVE_CC_RESONANCE as u8, unsafe {
            midi_map_to_cc(row(LIVE_PARAM_RESONANCE), 3)
        })]
    );
}

#[test]
fn a_parameter_whose_cc_was_learned_by_another_goes_out_as_its_nrpn() {
    let mut m = Learned::new();
    m.learn(LIVE_PARAM_RESONANCE, LIVE_CC_CUTOFF as u8, 0);
    m.tick();
    m.set(LIVE_PARAM_CUTOFF, 1000);
    let nrpn = row(LIVE_PARAM_CUTOFF).nrpn;
    assert_eq!(
        m.tick(),
        [
            cc_packet(99, (nrpn >> 7) as u8),
            cc_packet(98, (nrpn & 0x7f) as u8),
            cc_packet(6, (1000 >> 7) as u8),
            cc_packet(38, (1000 & 0x7f) as u8)
        ]
    );
    m.set(LIVE_PARAM_RESONANCE, 15);
    assert_eq!(m.tick(), [cc_packet(LIVE_CC_CUTOFF as u8, 127)]);
}

#[test]
fn a_learned_cc_from_midi_is_never_echoed_even_on_cc_1() {
    let mut m = Learned::new();
    m.set(LIVE_PARAM_MOD_TARGET, LIVE_MOD_VIBRATO);
    m.tick();
    m.learn(LIVE_PARAM_CUTOFF, 1, 5);
    m.learn(LIVE_PARAM_RESONANCE, 21, 5);
    for v in [10u8, 50, 127] {
        m.trs(&[0xB0, 1, v, 21, v]);
        assert_eq!(m.get(LIVE_PARAM_CUTOFF), from_cc(LIVE_PARAM_CUTOFF, v));
        assert_eq!(m.tick(), [], "{v} echoed");
    }
    // A panel change in the same pass's look still goes.
    m.set(LIVE_PARAM_CUTOFF, 0);
    assert_eq!(m.tick(), [cc_packet(1, 0)]);
}

// --- the panel's knobs ---

fn knob_param(l: &live, held: u32, enc: u32) -> u32 {
    unsafe { panel_knob_param(l, held, enc) }
}

#[test]
fn the_knobs_parameters_are_the_ones_the_panel_turns() {
    let edit = 1 << PANEL_BTN_EDIT;
    for select in 0..LIVE_SLOTS {
        for held in [0, edit] {
            for enc in 0..PANEL_ENCODERS {
                let mut rig = Rig::new(SID_MODEL_6581);
                let mut p = Box::<panel>::default();
                unsafe { panel_init(&mut *p) };
                rig.live.fx_select = select as u8;
                unsafe { panel_buttons(&mut *p, &mut *rig.live, 0, held) };
                let param = knob_param(&rig.live, held, enc);
                let before: Vec<u32> = (0..LIVE_PARAM_COUNT)
                    .map(|q| unsafe { live_param_get(&*rig.live, q) })
                    .collect();
                let preset = rig.live.preset;
                unsafe { panel_encoder(&mut *p, &mut *rig.live, enc, 1) };
                let changed: Vec<u32> = (0..LIVE_PARAM_COUNT)
                    .filter(|&q| unsafe { live_param_get(&*rig.live, q) } != before[q as usize])
                    .collect();
                if param == LIVE_PARAM_COUNT {
                    // K4 without EDIT is the preset; SELECT, ALGORITHM and PRESETS nothing.
                    let k4 = held == 0 && enc == PANEL_ENC_K4;
                    assert_eq!(rig.live.preset != preset, k4, "enc {enc}");
                    if !k4 {
                        assert!(changed.is_empty(), "enc {enc} held {held}: {changed:?}");
                    }
                } else {
                    assert_eq!(changed, [param], "enc {enc} held {held} slot {select}");
                }
            }
        }
    }
    let l = Rig::new(SID_MODEL_6581);
    assert_eq!(knob_param(&l.live, 0, PANEL_ENC_K1), LIVE_PARAM_CUTOFF);
    assert_eq!(
        knob_param(&l.live, 0, PANEL_ENC_K1 + 1),
        LIVE_PARAM_RESONANCE
    );
    assert_eq!(
        knob_param(&l.live, 0, PANEL_ENC_K1 + 2),
        LIVE_PARAM_PULSE_WIDTH
    );
    assert_eq!(
        knob_param(&l.live, edit, PANEL_ENC_K1),
        LIVE_PARAM_SLOT + LIVE_SLOT_AMOUNT
    );
}
