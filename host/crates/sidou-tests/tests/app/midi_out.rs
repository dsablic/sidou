// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! MIDI out (`app/midi_out.c`): the outgoing stream of parameter changes, its packets, rate
//! limit, deduplication and feedback suppression.

use sidou_sys::*;

use crate::Rig;

const TRS: u8 = MIDI_SRC_TRS as u8;
const USB: u8 = MIDI_SRC_USB as u8;
const ROOM: u32 = 64;
const TICK: usize = MIDI_OUT_TICK_PACKETS as usize;

/// A live instrument with MIDI in (a `midi_rx`) and MIDI out, wired as the device does
/// (midi_rx_watch).
struct Out {
    rig: Rig,
    rx: Box<midi_rx>,
    out: Box<midi_out>,
}

impl Out {
    fn new() -> Self {
        Self::with(0, 0)
    }

    fn with(channel: u8, hires: u8) -> Self {
        let mut rig = Rig::new(SID_MODEL_6581);
        let mut rx = Box::<midi_rx>::default();
        let mut out = Box::<midi_out>::default();
        unsafe {
            midi_rx_init(&mut *rx, &mut *rig.live, 0);
            midi_out_init(&mut *out, &*rig.live, channel, hires);
            midi_rx_watch(&mut *rx, &mut *out);
        }
        Out { rig, rx, out }
    }

    fn entry(&mut self, source: u8, data: u32) {
        let e = midi_ring_entry {
            stamp: 0,
            data,
            source,
            epoch: 0,
        };
        unsafe { midi_rx_entry(&mut *self.rx, &e) };
    }

    /// The end of a drain pass (midi_rx_settle), as the device's midi_drain ends one.
    fn settle(&mut self) {
        unsafe { midi_rx_settle(&mut *self.rx) };
    }

    /// TRS bytes in one drain pass.
    fn trs(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.entry(TRS, u32::from(b));
        }
        self.settle();
    }

    /// USB-MIDI packets in one drain pass.
    fn usb(&mut self, packets: &[u32]) {
        for &p in packets {
            self.entry(USB, p);
        }
        self.settle();
    }

    fn usb_cc(&mut self, cc: u8, value: u8) {
        self.usb(&[u32::from_le_bytes([0xB, 0xB0, cc, value])]);
    }

    fn set(&mut self, param: u32, value: u32) {
        unsafe { live_param_set(&mut *self.rig.live, param, value) }
    }

    fn get(&self, param: u32) -> u32 {
        unsafe { live_param_get(&*self.rig.live, param) }
    }

    fn tick_room(&mut self, room: u32) -> Vec<u32> {
        let mut buf = [0u32; TICK];
        let n = unsafe { midi_out_tick(&mut *self.out, &*self.rig.live, room, buf.as_mut_ptr()) };
        assert!(
            n as usize <= TICK && n <= room,
            "{n} packets for room {room}"
        );
        buf[..n as usize].to_vec()
    }

    fn tick(&mut self) -> Vec<u32> {
        self.tick_room(ROOM)
    }

    /// Ticks until nothing more comes (at most 100), every packet in order.
    fn drain(&mut self) -> Vec<u32> {
        let mut all = Vec::new();
        for _ in 0..100 {
            let p = self.tick();
            if p.is_empty() {
                return all;
            }
            all.extend(p);
        }
        panic!("the stream never ends");
    }
}

fn cc_packet(channel: u8, cc: u8, value: u8) -> u32 {
    u32::from_le_bytes([
        0x0B | (MIDI_OUT_CABLE as u8) << 4,
        0xB0 | channel,
        cc,
        value,
    ])
}

fn nrpn_packets(channel: u8, nrpn: u16, value: u32) -> Vec<u32> {
    vec![
        cc_packet(channel, 99, (nrpn >> 7) as u8),
        cc_packet(channel, 98, (nrpn & 0x7f) as u8),
        cc_packet(channel, 6, (value >> 7) as u8),
        cc_packet(channel, 38, (value & 0x7f) as u8),
    ]
}

fn row(param: u32) -> &'static midi_map_entry {
    let e = unsafe { midi_map_for_param(param) };
    assert!(!e.is_null());
    unsafe { &*e }
}

fn to_cc(param: u32, value: u32) -> u8 {
    unsafe { midi_map_to_cc(row(param), value) }
}

/// The MIDI bytes of a stream of packets (midi_usb_bytes), which also checks each CIN.
fn bytes(packets: &[u32]) -> Vec<u8> {
    let mut v = Vec::new();
    for &p in packets {
        let mut b = [0u8; 3];
        let n = unsafe { midi_usb_bytes(p, b.as_mut_ptr()) };
        assert!(n > 0, "packet {p:08x} carries nothing");
        v.extend_from_slice(&b[..n as usize]);
    }
    v
}

#[test]
fn nothing_is_sent_at_rest() {
    let mut o = Out::new();
    for _ in 0..10 {
        assert_eq!(o.tick(), []);
    }
    o.rig.tick();
    assert_eq!(o.tick(), [], "live_tick changes no parameter");
    assert_eq!(o.out.packets, 0);
}

#[test]
fn a_knob_move_is_its_cc_once_a_tick_with_the_latest_value() {
    let mut o = Out::new();
    o.rig.knob(LIVE_KNOB_CUTOFF, 4);
    o.rig.knob(LIVE_KNOB_CUTOFF, 4);
    o.rig.knob(LIVE_KNOB_CUTOFF, -1);
    let c = o.get(LIVE_PARAM_CUTOFF);
    assert_eq!(
        u32::from(c),
        u32::from(crate::cutoff_after(SID_MODEL_6581, 1200, 7))
    );
    assert!(c > 1200);
    assert_eq!(
        o.tick(),
        [cc_packet(
            0,
            LIVE_CC_CUTOFF as u8,
            to_cc(LIVE_PARAM_CUTOFF, c)
        )]
    );
    assert_eq!(o.tick(), [], "sent once");
    o.rig.knob(LIVE_KNOB_RESONANCE, 1);
    o.rig.pulse(1);
    let pw = o.get(LIVE_PARAM_PULSE_WIDTH);
    let res = o.get(LIVE_PARAM_RESONANCE);
    // Round robin from where the last tick left off; both in the one tick.
    let mut got = o.tick();
    got.sort_unstable();
    let mut want = vec![
        cc_packet(0, LIVE_CC_RESONANCE as u8, to_cc(LIVE_PARAM_RESONANCE, res)),
        cc_packet(
            0,
            LIVE_CC_PULSE_WIDTH as u8,
            to_cc(LIVE_PARAM_PULSE_WIDTH, pw),
        ),
    ];
    want.sort_unstable();
    assert_eq!(got, want);
    assert_eq!(o.out.packets, 3);
}

#[test]
fn the_packets_are_usb_midi_event_packets() {
    let mut o = Out::with(9, 0);
    o.rig.knob(LIVE_KNOB_RESONANCE, 1);
    let p = o.tick();
    assert_eq!(p.len(), 1);
    let b = p[0].to_le_bytes();
    assert_eq!(b[0] & 0x0f, 0x0B, "CIN: control change");
    assert_eq!(u32::from(b[0] >> 4), MIDI_OUT_CABLE);
    assert_eq!(b[1], 0xB9, "status: CC on channel 10");
    assert_eq!(u32::from(b[2]), LIVE_CC_RESONANCE);
    assert_eq!(b[3], to_cc(LIVE_PARAM_RESONANCE, 7));
    // A preset from the panel: a program change, CIN 0xC, with one data byte.
    o.rig.preset_step(1);
    let p = o.tick();
    assert_eq!(p, [u32::from_le_bytes([0x0C, 0xC9, 1, 0])]);
    assert_eq!(bytes(&p), [0xC9, 1]);
}

#[test]
fn the_channel_is_masked_to_4_bits() {
    let mut o = Out::with(16 + 3, 0);
    o.rig.knob(LIVE_KNOB_RESONANCE, 1);
    assert_eq!(o.tick()[0].to_le_bytes()[1], 0xB3);
}

#[test]
fn every_parameter_reaches_a_second_instrument_through_its_stream() {
    for hires in [0u8, 1] {
        let mut o = Out::with(0, hires);
        let mut far = Out::new();
        for p in 0..LIVE_PARAM_COUNT {
            let max = unsafe { live_param_max(p) };
            // Away from the default, to a value the CC curve keeps (max for switches).
            let v = if max <= 15 { max } else { max * 2 / 3 };
            o.set(p, v);
        }
        // Read back after them all: the profile (worn 6581) sets the model back to the 6581.
        let want: Vec<u32> = (0..LIVE_PARAM_COUNT).map(|p| o.get(p)).collect();
        let stream = o.drain();
        far.usb(&stream);
        for p in 0..LIVE_PARAM_COUNT {
            let r = row(p);
            // The pulse width goes as its NRPN too: the wheel drives the cutoff by now.
            let exact = hires == 1
                || r.max < 128
                || r.cc == MIDI_MAP_NO_CC as u8
                || p == LIVE_PARAM_PULSE_WIDTH;
            let got = far.get(p);
            if exact {
                assert_eq!(got, want[p as usize], "parameter {p} (hires {hires})");
            } else {
                let back = unsafe { midi_map_from_cc(r, to_cc(p, want[p as usize])) };
                assert_eq!(got, back, "parameter {p} through its CC");
            }
        }
        // ... and the far side's own output stays silent: it heard it all as MIDI.
        assert_eq!(far.drain(), []);
    }
}

#[test]
fn high_resolution_parameters_go_as_nrpns_only_when_asked() {
    let mut o = Out::with(0, 1);
    o.set(LIVE_PARAM_CUTOFF, 1234);
    o.set(LIVE_PARAM_RESONANCE, 3);
    let mut got = o.drain();
    let res = cc_packet(0, LIVE_CC_RESONANCE as u8, to_cc(LIVE_PARAM_RESONANCE, 3));
    assert!(got.contains(&res), "a small parameter stays a CC");
    got.retain(|&p| p != res);
    assert_eq!(got, nrpn_packets(0, row(LIVE_PARAM_CUTOFF).nrpn, 1234));
    // Every packet carries its status: USB-MIDI has no running status.
    assert_eq!(
        bytes(&got),
        [0xB0, 99, 0, 0xB0, 98, 0, 0xB0, 6, 9, 0xB0, 38, 82]
    );
    // The default: the plain CC.
    let mut o = Out::new();
    o.set(LIVE_PARAM_CUTOFF, 1234);
    assert_eq!(
        o.drain(),
        [cc_packet(0, 74, to_cc(LIVE_PARAM_CUTOFF, 1234))]
    );
}

#[test]
fn parameters_without_a_cc_go_as_nrpns() {
    let mut o = Out::new();
    o.set(LIVE_PARAM_VELOCITY_TARGET, LIVE_VELOCITY_SUSTAIN);
    assert_eq!(
        o.drain(),
        nrpn_packets(
            0,
            row(LIVE_PARAM_VELOCITY_TARGET).nrpn,
            LIVE_VELOCITY_SUSTAIN
        )
    );
}

#[test]
fn the_pulse_width_leaves_cc_1_to_the_wheel_when_the_wheel_drives_another_target() {
    let mut o = Out::new();
    o.trs(&[0xB0, 99, 6, 98, 2, 6, 0, 38, LIVE_MOD_VIBRATO as u8]);
    assert_eq!(o.get(LIVE_PARAM_MOD_TARGET), LIVE_MOD_VIBRATO);
    o.rig.pulse(3);
    let pw = o.get(LIVE_PARAM_PULSE_WIDTH);
    assert_eq!(
        o.drain(),
        nrpn_packets(0, row(LIVE_PARAM_PULSE_WIDTH).nrpn, pw)
    );
}

#[test]
fn a_change_within_one_cc_step_or_back_again_sends_nothing() {
    let mut o = Out::new();
    // 4096 / 128 = 32 pulse-width values a CC step: one detent of 32 moves it one step,
    // a single unit does not.
    let pw = o.get(LIVE_PARAM_PULSE_WIDTH);
    o.set(LIVE_PARAM_PULSE_WIDTH, pw + 1);
    assert_eq!(
        to_cc(LIVE_PARAM_PULSE_WIDTH, pw + 1),
        to_cc(LIVE_PARAM_PULSE_WIDTH, pw)
    );
    assert_eq!(o.tick(), []);
    o.rig.knob(LIVE_KNOB_CUTOFF, 5);
    o.rig.knob(LIVE_KNOB_CUTOFF, -5);
    assert_eq!(o.tick(), [], "there and back within a tick");
    // The same CC value sent twice in a row is never sent twice.
    o.rig.knob(LIVE_KNOB_RESONANCE, 1);
    assert_eq!(o.tick().len(), 1);
    o.rig.knob(LIVE_KNOB_RESONANCE, 1);
    o.rig.knob(LIVE_KNOB_RESONANCE, -1);
    assert_eq!(o.tick(), []);
}

#[test]
fn a_change_from_midi_is_never_echoed_on_either_port() {
    let mut o = Out::new();
    o.trs(&[0xC0, 2]); // a program change
    o.trs(&[0xB0, 74, 10]);
    o.usb_cc(71, 100);
    o.trs(&[0xB0, 99, 0, 98, 0, 6, 5, 38, 1]); // the cutoff by NRPN
    o.trs(&[0xB0, 25, 127]); // the chip model
    assert_eq!(o.get(LIVE_PARAM_CUTOFF), 5 << 7 | 1);
    assert_eq!(o.get(LIVE_PARAM_MODEL), 1);
    assert_eq!(o.drain(), []);
    o.rig.tick();
    assert_eq!(
        o.drain(),
        [],
        "the tick applies the model asked for: no change to send"
    );
}

#[test]
fn midi_after_a_panel_change_of_the_same_parameter_wins_and_is_not_echoed() {
    let mut o = Out::new();
    o.rig.knob(LIVE_KNOB_CUTOFF, 10);
    o.rig.knob(LIVE_KNOB_RESONANCE, 1);
    o.trs(&[0xB0, 74, 3]);
    let res = o.get(LIVE_PARAM_RESONANCE);
    assert_eq!(
        o.tick(),
        [cc_packet(0, 71, to_cc(LIVE_PARAM_RESONANCE, res))],
        "the resonance (the panel's) goes, the cutoff (last set by MIDI) does not"
    );
    // A panel move after the MIDI one is sent again.
    o.rig.knob(LIVE_KNOB_CUTOFF, 1);
    let c = o.get(LIVE_PARAM_CUTOFF);
    assert_eq!(o.tick(), [cc_packet(0, 74, to_cc(LIVE_PARAM_CUTOFF, c))]);
}

#[test]
fn a_panel_change_back_to_the_value_midi_set_sends_nothing() {
    let mut o = Out::new();
    o.trs(&[0xB0, 71, 127]);
    o.rig.knob(LIVE_KNOB_RESONANCE, -1);
    o.rig.knob(LIVE_KNOB_RESONANCE, 1);
    assert_eq!(o.tick(), []);
}

#[test]
fn the_fx_button_sends_the_chip_model() {
    // And the profile, which follows the model: the 8580 R5, then back to the 6581 R4AR.
    let profile = |p: u32| cc_packet(0, 85, to_cc(LIVE_PARAM_PROFILE, p));
    let mut o = Out::new();
    o.rig.button(LIVE_BUTTON_FX);
    assert_eq!(o.tick(), [cc_packet(0, 25, 127), profile(SID_CHIP_8580_R5)]);
    o.rig.button(LIVE_BUTTON_FX);
    assert_eq!(o.tick(), [cc_packet(0, 25, 0), profile(SID_CHIP_6581_R4AR)]);
}

#[test]
fn a_panel_preset_is_a_program_change_without_the_parameters_it_loads() {
    let mut o = Out::new();
    o.rig.knob(LIVE_KNOB_CUTOFF, 3); // pending, then replaced by the preset's
    o.rig.preset_step(1);
    assert_eq!(o.tick(), [u32::from_le_bytes([0x0C, 0xC0, 1, 0])]);
    assert_eq!(o.tick(), []);
    // A knob moved in the same tick as the preset goes out after the program change: that
    // carries the preset's factory settings, and the knob's value differs from them.
    o.rig.preset_step(1);
    o.rig.knob(LIVE_KNOB_CUTOFF, 3);
    let c = o.get(LIVE_PARAM_CUTOFF);
    assert_eq!(
        o.tick(),
        [
            u32::from_le_bytes([0x0C, 0xC0, 2, 0]),
            cc_packet(0, 74, to_cc(LIVE_PARAM_CUTOFF, c))
        ]
    );
    o.rig.knob(LIVE_KNOB_CUTOFF, 1);
    let c = o.get(LIVE_PARAM_CUTOFF);
    assert_eq!(o.tick(), [cc_packet(0, 74, to_cc(LIVE_PARAM_CUTOFF, c))]);
    // A knob seen after the preset (MIDI input came between) goes in the same tick, after
    // the program change.
    o.rig.preset_step(-1);
    unsafe { midi_out_local(&mut *o.out, &*o.rig.live) };
    o.rig.knob(LIVE_KNOB_CUTOFF, 1);
    let c = o.get(LIVE_PARAM_CUTOFF);
    assert_eq!(
        o.tick(),
        [
            u32::from_le_bytes([0x0C, 0xC0, 1, 0]),
            cc_packet(0, 74, to_cc(LIVE_PARAM_CUTOFF, c))
        ]
    );
    // The last of several preset moves within a tick wins (SAW LEAD brings back its first edit).
    o.rig.preset_step(1);
    o.rig.preset_step(-3);
    let c = o.get(LIVE_PARAM_CUTOFF);
    assert_eq!(
        o.tick(),
        [
            u32::from_le_bytes([0x0C, 0xC0, 0, 0]),
            cc_packet(0, 74, to_cc(LIVE_PARAM_CUTOFF, c))
        ]
    );
}

#[test]
fn the_rate_is_bounded_and_nothing_starves() {
    let mut o = Out::with(0, 1);
    for p in 0..LIVE_PARAM_COUNT {
        let max = unsafe { live_param_max(p) };
        o.set(p, max);
    }
    let mut ticks = 0;
    let mut seen = std::collections::BTreeMap::new();
    loop {
        let t = o.tick();
        if t.is_empty() {
            break;
        }
        ticks += 1;
        assert!(t.len() <= TICK);
        // Whole messages only: an NRPN never splits across ticks.
        let b = bytes(&t);
        let ccs: Vec<(u8, u8)> = b.chunks(3).map(|m| (m[1], m[2])).collect();
        for (i, &(cc, _)) in ccs.iter().enumerate() {
            if cc == 99 {
                assert_eq!(ccs[i + 1].0, 98);
                assert_eq!(ccs[i + 2].0, 6);
                assert_eq!(ccs[i + 3].0, 38);
            }
        }
        for (cc, v) in ccs {
            *seen.entry(cc).or_insert(0) += 1;
            let _ = v;
        }
        // Meanwhile the knob keeps moving: its latest value goes, once a tick at most.
        o.rig.knob(LIVE_KNOB_RESONANCE, -1);
        assert!(ticks < 20);
    }
    // Every parameter but the volume and the filter routing (at their maximum already) and the
    // chip model (the worn 6581 profile, set after it, takes the 6581 back): the cutoff, the
    // pulse width (the wheel now drives the cutoff), the bend range and the three targets as
    // NRPNs, the other 33 as CCs: 24 + 33 = 57 packets, 16 a tick.
    assert!(ticks >= 4, "{ticks} ticks");
    assert!(o.out.deferred >= 3);
    for cc in [
        20u8, 21, 22, 23, 71, 72, 73, 75, 76, 77, 78, 79, 85, 86, 87, 89, 90, 102, 116,
    ] {
        assert!(seen.contains_key(&cc), "CC {cc} never went out");
    }
    assert_eq!(o.get(LIVE_PARAM_RESONANCE), 15 - ticks.min(15));
}

#[test]
fn a_message_waits_for_room_and_goes_whole() {
    let mut o = Out::with(0, 1);
    o.set(LIVE_PARAM_CUTOFF, 99);
    assert_eq!(o.tick_room(3), [], "an NRPN needs 4");
    assert_eq!(o.out.deferred, 1);
    o.set(LIVE_PARAM_CUTOFF, 100);
    assert_eq!(
        o.tick_room(4),
        nrpn_packets(0, row(LIVE_PARAM_CUTOFF).nrpn, 100),
        "the latest value"
    );
    o.rig.knob(LIVE_KNOB_RESONANCE, 1);
    assert_eq!(o.tick_room(0), []);
    assert_eq!(o.tick_room(1).len(), 1);
}

#[test]
fn without_a_host_pending_changes_are_dropped() {
    let mut o = Out::new();
    o.rig.knob(LIVE_KNOB_CUTOFF, 1);
    o.rig.knob(LIVE_KNOB_RESONANCE, 1);
    o.rig.preset_step(1);
    unsafe { midi_out_drop(&mut *o.out, &*o.rig.live) };
    assert_eq!(
        o.out.dropped, 1,
        "the program change (it replaced the knobs' changes)"
    );
    assert_eq!(o.tick(), []);
    o.rig.knob(LIVE_KNOB_CUTOFF, 1);
    unsafe { midi_out_drop(&mut *o.out, &*o.rig.live) };
    assert_eq!(o.out.dropped, 2);
    assert_eq!(o.tick(), []);
    // What came after is sent as usual.
    o.rig.knob(LIVE_KNOB_CUTOFF, 1);
    assert_eq!(o.tick().len(), 1);
}

#[test]
fn the_effect_slots_go_out_on_their_ccs() {
    let mut o = Out::new();
    unsafe { live_slot_next_algorithm(&mut *o.rig.live, 2) };
    unsafe { live_slot_turn(&mut *o.rig.live, 2, LIVE_SLOT_MIX, 8) };
    let mut got = o.drain();
    got.sort_unstable();
    let fx = LIVE_PARAM_SLOT + 2 * LIVE_SLOT_FIELDS;
    let mut want = vec![
        cc_packet(0, 112, to_cc(fx, 1)),
        cc_packet(0, 113, to_cc(fx + LIVE_SLOT_AMOUNT, 48)),
        cc_packet(0, 114, to_cc(fx + LIVE_SLOT_A, 64)),
        cc_packet(0, 115, to_cc(fx + LIVE_SLOT_B, 48)),
        cc_packet(0, 116, to_cc(fx + LIVE_SLOT_MIX, 72)),
    ];
    want.sort_unstable();
    assert_eq!(got, want);
}

#[test]
fn notes_bends_and_pressure_between_leave_a_panel_change_the_panels() {
    let mut o = Out::new();
    o.rig.knob(LIVE_KNOB_CUTOFF, 2);
    o.trs(&[0x90, 60, 100, 0xE0, 0, 0x50, 0xD0, 99, 0x80, 60, 0]);
    o.trs(&[0xB0, 71, 127]); // a CC on another parameter: the cutoff was the panel's
    let c = o.get(LIVE_PARAM_CUTOFF);
    assert_eq!(o.tick(), [cc_packet(0, 74, to_cc(LIVE_PARAM_CUTOFF, c))]);
}

#[test]
fn without_a_watch_midi_in_changes_count_as_local() {
    let mut o = Out::new();
    unsafe { midi_rx_watch(&mut *o.rx, std::ptr::null_mut()) };
    o.trs(&[0xB0, 71, 127]);
    assert_eq!(o.tick(), [cc_packet(0, 71, 127)]);
}

#[test]
fn a_panel_change_the_preset_does_not_load_survives_a_program_change_from_midi() {
    let mut o = Out::new();
    o.set(LIVE_PARAM_VOLUME, 500);
    o.rig.button(LIVE_BUTTON_FX);
    // A program change over USB before the tick: it loads neither the volume nor the model.
    o.usb(&[u32::from_le_bytes([0x0C, 0xC0, 2, 0])]);
    assert_eq!(o.rig.live.preset, 2);
    let mut got = o.tick();
    got.sort_unstable();
    let mut want = vec![
        cc_packet(0, 7, to_cc(LIVE_PARAM_VOLUME, 500)),
        cc_packet(0, 25, 127),
        cc_packet(0, 85, to_cc(LIVE_PARAM_PROFILE, SID_CHIP_8580_R5)),
    ];
    want.sort_unstable();
    assert_eq!(
        got, want,
        "the program change is MIDI's, the panel's changes go"
    );
}

#[test]
fn a_deferred_panel_change_the_preset_does_not_load_goes_after_the_program_change() {
    let mut o = Out::new();
    o.set(LIVE_PARAM_VOLUME, 500);
    unsafe { live_slot_turn(&mut *o.rig.live, 1, LIVE_SLOT_MIX, 8) };
    assert_eq!(o.tick_room(0), [], "no room: pending");
    o.rig.preset_step(1);
    let got = o.tick();
    assert_eq!(got[0], u32::from_le_bytes([0x0C, 0xC0, 1, 0]));
    let mut rest = got[1..].to_vec();
    rest.sort_unstable();
    let slot = LIVE_PARAM_SLOT + LIVE_SLOT_FIELDS + LIVE_SLOT_MIX;
    let mut want = vec![
        cc_packet(0, 7, to_cc(LIVE_PARAM_VOLUME, 500)),
        cc_packet(0, row(slot).cc, to_cc(slot, o.get(slot))),
    ];
    want.sort_unstable();
    assert_eq!(
        rest, want,
        "kept by the preset, so still the panel's to send"
    );
}

#[test]
fn the_mod_target_goes_before_a_pulse_width_that_depends_on_it() {
    let mut o = Out::new();
    o.trs(&[0xB0, 99, 6, 98, 2, 6, 0, 38, LIVE_MOD_VIBRATO as u8]);
    // The panel puts the wheel back on the pulse width and moves it in the same tick: the
    // receiver must hear the target first, or it takes CC 1 as the vibrato wheel.
    o.set(LIVE_PARAM_MOD_TARGET, LIVE_MOD_PULSE_WIDTH);
    o.rig.pulse(3);
    let pw = o.get(LIVE_PARAM_PULSE_WIDTH);
    let mut want = nrpn_packets(0, row(LIVE_PARAM_MOD_TARGET).nrpn, LIVE_MOD_PULSE_WIDTH);
    want.push(cc_packet(0, 1, to_cc(LIVE_PARAM_PULSE_WIDTH, pw)));
    assert_eq!(o.tick(), want);
    // No room for the target's NRPN: nothing goes before it.
    o.trs(&[0xB0, 99, 6, 98, 2, 6, 0, 38, LIVE_MOD_VIBRATO as u8]);
    o.set(LIVE_PARAM_MOD_TARGET, LIVE_MOD_PULSE_WIDTH);
    o.rig.pulse(3);
    let pw = o.get(LIVE_PARAM_PULSE_WIDTH);
    assert_eq!(o.tick_room(3), []);
    let mut want = nrpn_packets(0, row(LIVE_PARAM_MOD_TARGET).nrpn, LIVE_MOD_PULSE_WIDTH);
    want.push(cc_packet(0, 1, to_cc(LIVE_PARAM_PULSE_WIDTH, pw)));
    assert_eq!(o.tick(), want);
}

#[test]
fn nrpn_data_entry_alone_is_midi_input_and_not_echoed() {
    let mut o = Out::new();
    o.trs(&[0xB0, 99, 0, 98, 0]); // the selection: it changes no parameter
    o.trs(&[0xB0, 6, 5, 38, 1]);
    assert_eq!(o.get(LIVE_PARAM_CUTOFF), 5 << 7 | 1);
    o.trs(&[0xB0, 96, 0]);
    o.trs(&[0xB0, 97, 0]);
    o.trs(&[0xB0, 97, 0]);
    assert_eq!(o.get(LIVE_PARAM_CUTOFF), 5 << 7);
    // The wheel on the pulse width is its CC 1.
    o.trs(&[0xB0, 1, 99]);
    assert_eq!(o.get(LIVE_PARAM_PULSE_WIDTH), unsafe {
        midi_map_from_cc(row(LIVE_PARAM_PULSE_WIDTH), 99)
    });
    assert_eq!(o.drain(), []);
}

#[test]
fn a_burst_in_one_pass_is_all_midi_and_the_panel_change_before_it_still_goes() {
    let mut o = Out::new();
    o.set(LIVE_PARAM_VOLUME, 500); // the preset below keeps it
                                   // One pass: notes and selection first (no look), then edits of other parameters, the
                                   // wheel on another target, a preset and edits after it.
    let mut pass = vec![
        u32::from_le_bytes([0x09, 0x90, 60, 100]),
        u32::from_le_bytes([0x0B, 0xB0, 99, 6]),
        u32::from_le_bytes([0x0B, 0xB0, 98, 2]),
        u32::from_le_bytes([0x0B, 0xB0, 6, 0]),
        u32::from_le_bytes([0x0B, 0xB0, 38, LIVE_MOD_VIBRATO as u8]),
        u32::from_le_bytes([0x0B, 0xB0, 1, 64]),
    ];
    for v in 0..16u8 {
        pass.push(u32::from_le_bytes([0x0B, 0xB0, 74, v * 8]));
    }
    pass.push(u32::from_le_bytes([0x0C, 0xC0, 3, 0]));
    pass.push(u32::from_le_bytes([0x0B, 0xB0, 25, 127]));
    pass.push(u32::from_le_bytes([0x0B, 0xB0, 73, 127]));
    o.usb(&pass);
    assert_eq!(o.rig.live.preset, 3);
    assert_eq!(o.get(LIVE_PARAM_MODEL), 1);
    assert_eq!(
        o.drain(),
        [cc_packet(0, 7, to_cc(LIVE_PARAM_VOLUME, 500))],
        "only the panel's volume"
    );
}

/// The character's parameters (M1c Task 3) go out on their map rows' CCs, like the others.
#[test]
fn the_character_goes_out_on_its_ccs() {
    let mut o = Out::new();
    o.set(LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN);
    o.set(LIVE_PARAM_AGE, 100);
    o.set(LIVE_PARAM_DAC, 127);
    o.set(LIVE_PARAM_THUMP, 64);
    o.set(LIVE_PARAM_BLEED, 1);
    let mut got = o.tick();
    got.sort_unstable();
    let mut want = vec![
        cc_packet(0, 85, to_cc(LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN)),
        cc_packet(0, 86, 100),
        cc_packet(0, 87, 127),
        cc_packet(0, 89, 64),
        cc_packet(0, 90, 1),
    ];
    want.sort_unstable();
    assert_eq!(got, want);
}

fn pc_packet(channel: u8, program: u8) -> u32 {
    u32::from_le_bytes([
        0x0C | (MIDI_OUT_CABLE as u8) << 4,
        0xC0 | channel,
        program,
        0,
    ])
}

/// M2e Task 4: a song from the SONG menu goes out as bank 1 and its program; the next preset
/// from the panel selects bank 0 again first, and a preset after it is its program change
/// alone, as in M1.
#[test]
fn a_song_goes_out_in_bank_1_and_a_preset_after_it_selects_bank_0_again() {
    let mut o = Out::with(2, 0);
    unsafe { midi_out_song(&mut *o.out, 5) };
    assert_eq!(
        o.tick(),
        [cc_packet(2, 0, 0), cc_packet(2, 32, 1), pc_packet(2, 5)]
    );
    o.rig.preset_step(1);
    assert_eq!(
        o.tick(),
        [cc_packet(2, 0, 0), cc_packet(2, 32, 0), pc_packet(2, 1)]
    );
    o.rig.preset_step(1);
    assert_eq!(o.tick(), [pc_packet(2, 2)]);
    // Without room for the whole message, it waits.
    unsafe { midi_out_song(&mut *o.out, 9) };
    assert!(o.tick_room(2).is_empty());
    assert_eq!(o.tick_room(3).len(), 3);
    // No host: dropped.
    unsafe {
        midi_out_song(&mut *o.out, 1);
        midi_out_drop(&mut *o.out, &*o.rig.live);
    }
    assert!(o.tick().is_empty());
}

#[test]
fn a_recall_sends_the_program_and_the_edits() {
    let mut o = Out::new();
    // Edit SAW LEAD, and let the edits go out.
    o.rig.knob(LIVE_KNOB_CUTOFF, 3);
    o.rig.knob(LIVE_KNOB_RESONANCE, 2);
    let (c, r) = (o.get(LIVE_PARAM_CUTOFF), o.get(LIVE_PARAM_RESONANCE));
    assert_eq!(
        o.tick(),
        [
            cc_packet(0, 74, to_cc(LIVE_PARAM_CUTOFF, c)),
            cc_packet(0, 71, to_cc(LIVE_PARAM_RESONANCE, r))
        ]
    );
    // PRESETS to PWM BASS: a program change alone (its factory settings).
    o.rig.preset_step(1);
    assert_eq!(o.tick(), [u32::from_le_bytes([0x0C, 0xC0, 1, 0])]);
    assert_eq!(o.tick(), []);
    // And back: the program change carries SAW LEAD's factory settings, and the edits that came
    // back follow it as CCs.
    o.rig.preset_step(-1);
    assert_eq!(
        (o.get(LIVE_PARAM_CUTOFF), o.get(LIVE_PARAM_RESONANCE)),
        (c, r)
    );
    assert_eq!(
        o.tick(),
        [
            u32::from_le_bytes([0x0C, 0xC0, 0, 0]),
            cc_packet(0, 74, to_cc(LIVE_PARAM_CUTOFF, c)),
            cc_packet(0, 71, to_cc(LIVE_PARAM_RESONANCE, r))
        ]
    );
    assert_eq!(o.tick(), [], "and nothing more");
    // A program change from MIDI is the factory's: nothing follows it.
    o.usb(&[u32::from_le_bytes([0xC, 0xC0, 1, 0])]);
    assert_eq!(o.get(LIVE_PARAM_CUTOFF), 700);
    assert_eq!(o.tick(), []);
}
