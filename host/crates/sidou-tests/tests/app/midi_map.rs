// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The CC/NRPN map (`app/midi_map.c`): the table, its round trips, the NRPN receiver, the
//! live parameters it drives, and the published reference generated from it
//! (docs/reference/midi.md; SIDOU_MIDI_DOC_UPDATE=1 rewrites it).

use std::collections::BTreeSet;
use std::ffi::CStr;
use std::path::PathBuf;

use sidou_sys::*;

use crate::Rig;

const TRS: u8 = MIDI_SRC_TRS as u8;
const USB: u8 = MIDI_SRC_USB as u8;
/// The chip model's CC (docs/reference/midi.md).
const CC_MODEL: u8 = 25;

/// One row of the table, read through FFI.
#[derive(Debug, Clone)]
struct Row {
    name: String,
    group: String,
    values: String,
    param: u8,
    cc: u8,
    nrpn: u16,
    max: u16,
    def: u16,
    curve: u32,
}

impl Row {
    fn reserved(&self) -> bool {
        u32::from(self.param) == MIDI_MAP_RESERVED
    }
    fn has_cc(&self) -> bool {
        u32::from(self.cc) != MIDI_MAP_NO_CC
    }
    fn has_nrpn(&self) -> bool {
        u32::from(self.nrpn) != MIDI_MAP_NO_NRPN
    }
    fn steps(&self) -> bool {
        self.curve == MIDI_CURVE_STEPS
    }
}

fn text(p: *const std::ffi::c_char) -> String {
    assert!(!p.is_null());
    unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_owned()
}

fn entry(i: u32) -> &'static midi_map_entry {
    let e = unsafe { midi_map_row(i) };
    assert!(!e.is_null(), "row {i}");
    unsafe { &*e }
}

fn rows() -> Vec<Row> {
    assert!(unsafe { midi_map_row(MIDI_MAP_ROWS) }.is_null());
    (0..MIDI_MAP_ROWS)
        .map(|i| {
            let e = entry(i);
            Row {
                name: text(e.name),
                group: text(e.group),
                values: text(e.values),
                param: e.param,
                cc: e.cc,
                nrpn: e.nrpn,
                max: e.max,
                def: e.def,
                curve: u32::from(e.curve),
            }
        })
        .collect()
}

fn row_for(param: u32) -> Row {
    rows()
        .into_iter()
        .find(|r| u32::from(r.param) == param)
        .unwrap_or_else(|| panic!("no row for parameter {param}"))
}

fn ptr_for(param: u32) -> *const midi_map_entry {
    let e = unsafe { midi_map_for_param(param) };
    assert!(!e.is_null(), "parameter {param}");
    e
}

/// The curves as documented, computed independently of the C.
fn expect_from_cc(r: &Row, v: u8) -> u32 {
    let max = u32::from(r.max);
    if r.steps() {
        u32::from(v) * (max + 1) / 128
    } else {
        (f64::from(v) * f64::from(max) / 127.0).round() as u32
    }
}

fn nrpn(msb: u8, lsb: u8) -> u16 {
    u16::from(msb) << 7 | u16::from(lsb)
}

fn get(rig: &Rig, param: u32) -> u32 {
    unsafe { live_param_get(&*rig.live, param) }
}

fn set(rig: &mut Rig, param: u32, value: u32) {
    unsafe { live_param_set(&mut *rig.live, param, value) }
}

/// Every USER slot holds a profile (slot n: the built-in profile n % 5), so the profile
/// parameter's values above the built-in ones apply (M2a Task 6; an empty slot does nothing).
fn fill_users(rig: &mut Rig) {
    let l = &mut *rig.live;
    l.user_valid = 0xff;
    for n in 0..LIVE_USER_PROFILES as usize {
        l.user_base[n] = (n % SID_CHIPS as usize) as u8;
        l.user_ctl[n] = [n as u8, 2 * n as u8, 3 * n as u8, 4 * n as u8];
    }
}

/// What live_param_get gives after `value` was set on `param`: itself, but for the profile
/// parameter's USER slots, which set their base profile (the slot is the parameter's, not the
/// profile's).
fn expected(rig: &Rig, param: u32, value: u32) -> u32 {
    if param == LIVE_PARAM_PROFILE && value >= SID_CHIPS {
        u32::from(rig.live.user_base[(value - SID_CHIPS) as usize])
    } else {
        value
    }
}

// --- the table ---

#[test]
fn every_parameter_has_exactly_one_row() {
    let rows = rows();
    for p in 0..LIVE_PARAM_COUNT {
        let n = rows.iter().filter(|r| u32::from(r.param) == p).count();
        assert_eq!(n, 1, "parameter {p}");
        let r = row_for(p);
        assert_eq!(unsafe { midi_map_for_param(p) }, entry(index_of(&r)));
    }
    assert!(unsafe { midi_map_for_param(LIVE_PARAM_COUNT) }.is_null());
    for r in &rows {
        assert!(
            r.reserved() || u32::from(r.param) < LIVE_PARAM_COUNT,
            "{}: parameter {}",
            r.name,
            r.param
        );
    }
}

fn index_of(r: &Row) -> u32 {
    rows()
        .iter()
        .position(|x| x.name == r.name && x.group == r.group)
        .unwrap() as u32
}

#[test]
fn ccs_and_nrpns_are_unique_and_every_row_has_one() {
    let rows = rows();
    let mut ccs = BTreeSet::new();
    let mut nrpns = BTreeSet::new();
    let mut names = BTreeSet::new();
    for r in &rows {
        assert!(r.has_cc() || r.has_nrpn(), "{} has neither", r.name);
        assert!(
            names.insert((r.group.clone(), r.name.clone())),
            "{}",
            r.name
        );
        if r.has_cc() {
            assert!(r.cc < 128, "{}: CC {}", r.name, r.cc);
            assert!(ccs.insert(r.cc), "CC {} twice", r.cc);
        }
        if r.has_nrpn() {
            assert!(
                r.nrpn < 0x3fff,
                "{}: NRPN {} (127/127 is null)",
                r.name,
                r.nrpn
            );
            assert!(nrpns.insert(r.nrpn), "NRPN {} twice", r.nrpn);
        }
    }
}

#[test]
fn ccs_keep_clear_of_the_numbers_midi_reserves() {
    // Bank select, data entry, the pedals, high-resolution velocity, increment/decrement,
    // NRPN/RPN selection, channel mode.
    let reserved: Vec<u8> = [0, 32, 6, 38, 88]
        .into_iter()
        .chain(64..=69)
        .chain(96..=101)
        .chain(120..=127)
        .collect();
    for r in rows().iter().filter(|r| r.has_cc()) {
        assert!(!reserved.contains(&r.cc), "{} on CC {}", r.name, r.cc);
    }
}

#[test]
fn m1a_and_m1c_numbers_are_kept() {
    let cc = |p: u32| row_for(p).cc as u32;
    assert_eq!(cc(LIVE_PARAM_CUTOFF), 74);
    assert_eq!(cc(LIVE_PARAM_RESONANCE), 71);
    assert_eq!(cc(LIVE_PARAM_PULSE_WIDTH), 1);
    assert_eq!(cc(LIVE_PARAM_VOLUME), 7);
    assert_eq!(cc(LIVE_PARAM_CUTOFF), LIVE_CC_CUTOFF);
    assert_eq!(cc(LIVE_PARAM_RESONANCE), LIVE_CC_RESONANCE);
    assert_eq!(cc(LIVE_PARAM_PULSE_WIDTH), LIVE_CC_PULSE_WIDTH);
    assert_eq!(cc(LIVE_PARAM_VOLUME), LIVE_CC_VOLUME);
    for slot in 0..LIVE_SLOTS {
        for field in 0..LIVE_SLOT_FIELDS {
            let p = LIVE_PARAM_SLOT + slot * LIVE_SLOT_FIELDS + field;
            assert_eq!(cc(p), 102 + slot * 5 + field, "slot {slot} field {field}");
        }
    }
    assert_eq!(LIVE_CC_SLOT_FIRST, 102);
    assert_eq!(LIVE_CC_SLOT_LAST, 116);
}

#[test]
fn full_resolution_cutoff_and_pulse_width_have_nrpns() {
    for (p, max) in [
        (LIVE_PARAM_CUTOFF, LIVE_CUTOFF_MAX),
        (LIVE_PARAM_PULSE_WIDTH, LIVE_PULSE_WIDTH_MAX),
    ] {
        let r = row_for(p);
        assert!(r.has_nrpn(), "{}", r.name);
        assert_eq!(u32::from(r.max), max, "{}", r.name);
    }
}

#[test]
fn ranges_match_the_instrument() {
    for r in rows().iter().filter(|r| !r.reserved()) {
        let p = u32::from(r.param);
        assert_eq!(u32::from(r.max), unsafe { live_param_max(p) }, "{}", r.name);
        assert!(r.max > 0, "{}", r.name);
        assert!(r.def <= r.max, "{}: default {} > {}", r.name, r.def, r.max);
        if r.steps() {
            assert!(r.max < 128, "{}: too many steps for a CC", r.name);
        } else {
            assert_eq!(r.curve, MIDI_CURVE_LINEAR, "{}", r.name);
            assert!(
                r.max >= 127,
                "{}: a linear curve needs >= 128 values",
                r.name
            );
        }
    }
    assert_eq!(unsafe { live_param_max(LIVE_PARAM_COUNT) }, 0);
}

#[test]
fn defaults_are_the_power_on_values() {
    let rig = Rig::new(SID_MODEL_6581);
    for r in rows().iter().filter(|r| !r.reserved()) {
        assert_eq!(
            get(&rig, u32::from(r.param)),
            u32::from(r.def),
            "{}",
            r.name
        );
    }
}

/// The Character rows, reserved until M1c Task 3, are the character's parameters, on the
/// numbers they kept: CC 85, 86, 87, 89, 90 and NRPN 2/0..4.
#[test]
fn the_character_rows_are_the_character_parameters() {
    let character: Vec<Row> = rows()
        .into_iter()
        .filter(|r| r.group == "Character")
        .collect();
    let got: Vec<(&str, u32, u8, u16)> = character
        .iter()
        .map(|r| (r.name.as_str(), u32::from(r.param), r.cc, r.nrpn))
        .collect();
    let nrpn = |lsb: u16| (2 << 7) | lsb;
    assert_eq!(
        got,
        [
            ("Profile", LIVE_PARAM_PROFILE, 85, nrpn(0)),
            ("Age/heat", LIVE_PARAM_AGE, 86, nrpn(1)),
            ("DAC sloppiness", LIVE_PARAM_DAC, 87, nrpn(2)),
            ("Output thump", LIVE_PARAM_THUMP, 89, nrpn(3)),
            ("Filter bleed", LIVE_PARAM_BLEED, 90, nrpn(4)),
        ]
    );
    assert!(
        rows().iter().all(|r| !r.reserved()),
        "no row is reserved now"
    );
}

#[test]
fn lookups_find_each_row_by_cc_and_by_nrpn() {
    for (i, r) in rows().iter().enumerate() {
        let e = entry(i as u32) as *const midi_map_entry;
        if r.has_cc() {
            assert_eq!(unsafe { midi_map_for_cc(r.cc) }, e, "{}", r.name);
        }
        if r.has_nrpn() {
            assert_eq!(unsafe { midi_map_for_nrpn(r.nrpn) }, e, "{}", r.name);
        }
    }
    let used: BTreeSet<u8> = rows().iter().map(|r| r.cc).collect();
    for cc in (0..=255u8).filter(|c| !used.contains(c)) {
        assert!(unsafe { midi_map_for_cc(cc) }.is_null(), "CC {cc}");
    }
    assert!(unsafe { midi_map_for_nrpn(0x3fff) }.is_null());
    assert!(unsafe { midi_map_for_nrpn(0xffff) }.is_null());
}

// --- round trips ---

#[test]
fn each_entry_round_trips_through_its_cc() {
    for r in rows().iter().filter(|r| r.has_cc() && !r.reserved()) {
        let e = ptr_for(u32::from(r.param));
        let mut rig = Rig::new(SID_MODEL_6581);
        fill_users(&mut rig);
        for v in 0..128u8 {
            let want = expect_from_cc(r, v);
            assert_eq!(unsafe { midi_map_from_cc(e, v) }, want, "{} CC {v}", r.name);
            rig.cc(u32::from(r.cc), v);
            assert_eq!(
                get(&rig, u32::from(r.param)),
                expected(&rig, u32::from(r.param), want),
                "{} CC {v}",
                r.name
            );
            if !r.steps() {
                assert_eq!(
                    unsafe { midi_map_to_cc(e, want) },
                    v,
                    "{}: CC {v} -> {want} -> CC",
                    r.name
                );
            }
        }
        // And from the parameter's side: a value sent out as a CC comes back as itself
        // (steps), or as the nearest value a CC can carry (linear).
        for p in 0..=u32::from(r.max) {
            let c = unsafe { midi_map_to_cc(e, p) };
            assert!(c < 128, "{} {p}", r.name);
            let back = expect_from_cc(r, c);
            if r.steps() {
                assert_eq!(back, p, "{}: {p} -> CC {c} -> {back}", r.name);
            } else {
                let step = f64::from(r.max) / 127.0;
                assert!(
                    (f64::from(back) - f64::from(p)).abs() <= step / 2.0 + 0.5,
                    "{}: {p} -> CC {c} -> {back}",
                    r.name
                );
            }
        }
        assert_eq!(unsafe { midi_map_to_cc(e, 0) }, 0, "{}", r.name);
        assert_eq!(
            unsafe { midi_map_to_cc(e, u32::from(r.max)) },
            127,
            "{}",
            r.name
        );
        assert_eq!(
            unsafe { midi_map_to_cc(e, u32::from(r.max) + 1000) },
            127,
            "{}: out of range",
            r.name
        );
    }
}

#[test]
fn each_entry_round_trips_through_its_nrpn_at_full_resolution() {
    for r in rows().iter().filter(|r| r.has_nrpn() && !r.reserved()) {
        let mut m = Midi::new();
        fill_users(&mut m.rig);
        let (msb, lsb) = ((r.nrpn >> 7) as u8, (r.nrpn & 0x7f) as u8);
        m.trs(&[0xB0, 99, msb, 98, lsb]);
        let param = u32::from(r.param);
        for p in (0..=r.max).rev() {
            m.trs(&[6, (p >> 7) as u8, 38, (p & 0x7f) as u8]);
            assert_eq!(
                get(&m.rig, param),
                expected(&m.rig, param, u32::from(p)),
                "{}",
                r.name
            );
        }
        // Past the range: clamped.
        m.trs(&[6, 127, 38, 127]);
        assert_eq!(
            get(&m.rig, param),
            expected(&m.rig, param, u32::from(r.max)),
            "{}",
            r.name
        );
    }
}

/// The character's settings live sends before every reset (live.h): the profile and its four
/// controls.
const CHARACTER: usize = LIVE_CHAR_FIELDS as usize;

// --- the live parameters ---

#[test]
fn parameters_set_clamp_and_write_only_on_a_change() {
    let mut rig = Rig::new(SID_MODEL_6581);
    fill_users(&mut rig);
    for p in 0..LIVE_PARAM_COUNT {
        let max = unsafe { live_param_max(p) };
        set(&mut rig, p, max + 1000);
        assert_eq!(get(&rig, p), expected(&rig, p, max), "parameter {p} clamps");
        rig.clear();
        set(&mut rig, p, max);
        assert!(rig.log().is_empty(), "parameter {p}: {:?}", rig.log());
        set(&mut rig, p, 0);
        assert_eq!(get(&rig, p), 0, "parameter {p}");
    }
    rig.clear();
    set(&mut rig, LIVE_PARAM_COUNT, 1);
    assert!(rig.log().is_empty());
}

#[test]
fn the_sid_parameters_reach_their_registers() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.note_on(60);
    let v = 0u8; // the voice it took
    assert!(rig.gated(v));
    set(&mut rig, LIVE_PARAM_WAVEFORM, 4 | 1); // pulse + triangle
    set(&mut rig, LIVE_PARAM_RING, 1);
    set(&mut rig, LIVE_PARAM_SYNC, 1);
    for voice in 0..3 {
        assert_eq!(
            rig.control(voice) & 0xfe,
            0x50 | 0x04 | 0x02,
            "voice {voice}"
        );
    }
    assert!(rig.gated(v), "the gate is kept");
    assert!(!rig.gated(1));
    set(&mut rig, LIVE_PARAM_ATTACK, 0xc);
    set(&mut rig, LIVE_PARAM_DECAY, 0x3);
    set(&mut rig, LIVE_PARAM_SUSTAIN, 0xe);
    set(&mut rig, LIVE_PARAM_RELEASE, 0x1);
    for voice in 0..3 {
        assert_eq!(rig.reg(voice * 7 + 5), 0xc3);
        assert_eq!(rig.reg(voice * 7 + 6), 0xe1);
    }
    let vol = rig.reg(0x18) & 0x0f;
    set(&mut rig, LIVE_PARAM_FILTER_MODE, 2 | 4); // band-pass + high-pass
    assert_eq!(rig.reg(0x18), 0x60 | vol);
    let res = rig.reg(0x17) & 0xf0;
    set(&mut rig, LIVE_PARAM_FILTER_VOICES, 2);
    assert_eq!(rig.reg(0x17), res | 2);

    // A program change loads the preset's own again.
    rig.set_preset(0);
    let p = crate::preset(0);
    assert_eq!(rig.control(0), p.control);
    assert_eq!(rig.reg(5), p.attack_decay);
    assert_eq!(rig.reg(6), p.sustain_release);
    assert_eq!(rig.reg(0x17) & 7, p.filter_voices);
    assert_eq!(rig.reg(0x18) & 0x70, p.filter_mode);
}

#[test]
fn the_sid_parameters_survive_a_model_change() {
    let mut rig = Rig::new(SID_MODEL_6581);
    set(&mut rig, LIVE_PARAM_WAVEFORM, 8);
    set(&mut rig, LIVE_PARAM_ATTACK, 7);
    rig.clear();
    set(&mut rig, LIVE_PARAM_MODEL, 1);
    rig.tick();
    assert_eq!(rig.live.model, SID_MODEL_8580);
    assert_eq!(
        rig.log()[CHARACTER],
        (LIVE_REG_RESET as u8, SID_MODEL_8580 as u8)
    );
    assert_eq!(rig.log()[CHARACTER + 1], (LIVE_REG_GATE_RESET as u8, 1));
    assert_eq!(
        rig.log().len(),
        CHARACTER + 27,
        "the character, a reset, the envelope mode and every register"
    );
    assert_eq!(rig.control(0), 0x80);
    assert_eq!(rig.reg(5) >> 4, 7);
    set(&mut rig, LIVE_PARAM_MODEL, 0);
    rig.tick();
    assert_eq!(rig.live.model, SID_MODEL_6581);
    assert_eq!(get(&rig, LIVE_PARAM_WAVEFORM), 8);
}

#[test]
fn the_chip_model_changes_at_most_once_a_tick_and_the_last_request_wins() {
    let mut m = Midi::new();
    m.rig.clear();
    // 101 toggles at MIDI rate within one tick, ending on the 8580.
    m.trs(&[0xB0]);
    for i in 0..101 {
        m.trs(&[CC_MODEL, if i % 2 == 0 { 64 } else { 63 }]);
    }
    assert!(
        m.rig.log().is_empty(),
        "applied before the tick: {:?}",
        m.rig.log()
    );
    assert_eq!(
        m.rig.live.model, SID_MODEL_6581,
        "the chip is the one applied"
    );
    assert_eq!(get(&m.rig, LIVE_PARAM_MODEL), 1, "the request reads back");
    m.rig.tick();
    assert_eq!(resets(&m.rig), 1);
    assert_eq!(
        m.rig.log().len(),
        CHARACTER + 27,
        "the character, a reset, the envelope mode and every register"
    );
    assert_eq!(m.rig.live.model, SID_MODEL_8580);
    m.rig.clear();
    m.rig.tick();
    assert!(m.rig.log().is_empty(), "applied twice: {:?}", m.rig.log());

    // Toggles that end on the model already applied change nothing at all.
    for i in 0..100 {
        m.trs(&[CC_MODEL, if i % 2 == 0 { 63 } else { 64 }]);
    }
    m.rig.tick();
    assert!(m.rig.log().is_empty(), "writes: {:?}", m.rig.log());
    assert_eq!(m.rig.live.model, SID_MODEL_8580);
}

#[test]
fn the_fx_button_toggles_the_chip_at_once_over_a_pending_request() {
    let mut m = Midi::new();
    m.trs(&[0xB0, CC_MODEL, 127]); // 8580 asked for, not applied yet
    m.rig.clear();
    m.rig.button(LIVE_BUTTON_FX);
    assert_eq!(
        m.rig.live.model, SID_MODEL_8580,
        "the button flips the chip heard"
    );
    assert_eq!(
        m.rig.log()[CHARACTER],
        (LIVE_REG_RESET as u8, SID_MODEL_8580 as u8)
    );
    assert_eq!(get(&m.rig, LIVE_PARAM_MODEL), 1);
    m.rig.clear();
    m.rig.tick();
    assert!(m.rig.log().is_empty(), "applied twice: {:?}", m.rig.log());
}

#[test]
fn the_sync_lead_keeps_its_silent_voice_3_whatever_the_waveform() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.set_preset(2);
    rig.note_on(60);
    set(&mut rig, LIVE_PARAM_WAVEFORM, 4);
    assert_eq!(rig.control(0), 0x40 | 0x02 | 1);
    assert_eq!(rig.control(1), 0);
    assert_eq!(rig.control(2), 0);
}

// --- the NRPN receiver ---

/// A live instrument (writing into a recorder) fed through a `midi_rx`.
struct Midi {
    rig: Rig,
    rx: Box<midi_rx>,
    epoch: u8,
}

impl Midi {
    fn new() -> Self {
        Self::with_model(SID_MODEL_6581)
    }

    fn with_model(model: sid_model) -> Self {
        let mut rig = Rig::new(model);
        let mut rx = Box::<midi_rx>::default();
        unsafe { midi_rx_init(&mut *rx, &mut *rig.live, 0) };
        Midi { rig, rx, epoch: 0 }
    }

    fn entry(&mut self, source: u8, data: u32) {
        let e = midi_ring_entry {
            stamp: 0,
            data,
            source,
            epoch: self.epoch,
        };
        unsafe { midi_rx_entry(&mut *self.rx, &e) };
    }

    fn trs(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.entry(TRS, u32::from(b));
        }
    }

    fn usb_cc(&mut self, channel: u8, cc: u8, value: u8) {
        self.entry(USB, u32::from_le_bytes([0xB, 0xB0 | channel, cc, value]));
    }

    fn cutoff(&self) -> u16 {
        self.rig.live.cutoff
    }
}

const CUTOFF_NRPN: (u8, u8) = (0, 0);

fn select_cutoff(m: &mut Midi, status: u8) {
    m.trs(&[status, 99, CUTOFF_NRPN.0, 98, CUTOFF_NRPN.1]);
}

#[test]
fn the_cutoff_nrpn_is_where_the_reference_says() {
    assert_eq!(
        row_for(LIVE_PARAM_CUTOFF).nrpn,
        nrpn(CUTOFF_NRPN.0, CUTOFF_NRPN.1)
    );
}

#[test]
fn data_entry_msb_then_lsb_sets_11_bits() {
    let mut m = Midi::new();
    select_cutoff(&mut m, 0xB0);
    m.trs(&[6, 9]);
    assert_eq!(m.cutoff(), 9 << 7, "an MSB alone has an LSB of 0");
    m.trs(&[38, 82]);
    assert_eq!(m.cutoff(), 1234);
    assert_eq!(
        u16::from(m.rig.reg(0x15) & 7) | u16::from(m.rig.reg(0x16)) << 3,
        1234,
        "the registers carry all 11 bits"
    );
}

#[test]
fn lsb_only_updates_keep_the_msb() {
    let mut m = Midi::new();
    select_cutoff(&mut m, 0xB0);
    m.trs(&[6, 9, 38, 82]);
    m.trs(&[38, 0]);
    assert_eq!(m.cutoff(), 9 << 7);
    m.trs(&[38, 127]);
    assert_eq!(m.cutoff(), 9 << 7 | 127);
    // The MSB of the last data entry, even when it was clamped.
    m.trs(&[6, 127]);
    assert_eq!(u32::from(m.cutoff()), LIVE_CUTOFF_MAX);
    m.trs(&[38, 0]);
    assert_eq!(u32::from(m.cutoff()), LIVE_CUTOFF_MAX);
}

#[test]
fn an_lsb_alone_after_selecting_fine_tunes_the_current_value() {
    let mut m = Midi::new();
    // The preset's 1200 = 9 * 128 + 48.
    assert_eq!(m.cutoff(), 1200);
    select_cutoff(&mut m, 0xB0);
    m.trs(&[38, 50]);
    assert_eq!(m.cutoff(), 9 << 7 | 50);
}

#[test]
fn increment_and_decrement_step_by_one_and_clamp() {
    let mut m = Midi::new();
    select_cutoff(&mut m, 0xB0);
    m.trs(&[6, 9, 38, 82]);
    m.trs(&[96, 0]);
    assert_eq!(m.cutoff(), 1235);
    m.trs(&[97, 0, 97, 5]);
    assert_eq!(m.cutoff(), 1233, "the data byte is not a step count");
    m.trs(&[38, 0]);
    assert_eq!(
        m.cutoff(),
        1152,
        "an LSB after a step: the current value's MSB"
    );
    m.trs(&[6, 0, 97, 0]);
    assert_eq!(m.cutoff(), 0, "decrement stops at 0");
    m.trs(&[6, 127, 96, 0]);
    assert_eq!(
        u32::from(m.cutoff()),
        LIVE_CUTOFF_MAX,
        "increment stops at the top"
    );
    // A 4-bit parameter steps by one too.
    let r = row_for(LIVE_PARAM_RESONANCE);
    m.trs(&[99, (r.nrpn >> 7) as u8, 98, (r.nrpn & 0x7f) as u8]);
    let before = m.rig.live.resonance;
    m.trs(&[96, 0]);
    assert_eq!(m.rig.live.resonance, before + 1);
}

#[test]
fn the_null_nrpn_deselects() {
    let mut m = Midi::new();
    select_cutoff(&mut m, 0xB0);
    m.trs(&[6, 5]);
    assert_eq!(m.cutoff(), 5 << 7);
    m.trs(&[99, 127, 98, 127]);
    m.rig.clear();
    m.trs(&[6, 9, 38, 1, 96, 0, 97, 0]);
    assert_eq!(m.cutoff(), 5 << 7);
    assert!(m.rig.log().is_empty(), "writes: {:?}", m.rig.log());
    // Half of it is not null: MSB 127 with LSB 0 is a parameter number like any other.
    m.trs(&[98, 0]);
    m.trs(&[6, 9]);
    assert_eq!(m.cutoff(), 5 << 7, "NRPN 127/0 is not mapped");
    m.trs(&[99, 0, 6, 9]);
    assert_eq!(m.cutoff(), 9 << 7, "MSB 0 again: the cutoff");
}

#[test]
fn an_rpn_takes_data_entry_away_from_the_nrpn() {
    let mut m = Midi::new();
    select_cutoff(&mut m, 0xB0);
    m.trs(&[6, 5]);
    // RPN 0/0, pitch-bend sensitivity: the bend range, not the cutoff.
    m.trs(&[101, 0, 100, 0]);
    m.rig.clear();
    m.trs(&[6, 12, 38, 0, 96, 0, 97, 0]);
    assert!(m.rig.log().is_empty(), "writes: {:?}", m.rig.log());
    assert_eq!(m.cutoff(), 5 << 7);
    // Selecting the NRPN's MSB again brings it back, with the LSB it had.
    m.trs(&[99, 0, 6, 7]);
    assert_eq!(m.cutoff(), 7 << 7);
    // The null RPN does not select the NRPN either.
    m.trs(&[101, 127, 100, 127, 6, 3]);
    assert_eq!(m.cutoff(), 7 << 7);
}

fn bend_range(m: &Midi) -> u32 {
    get(&m.rig, LIVE_PARAM_BEND_RANGE)
}

#[test]
fn rpn_0_sets_the_bend_range_in_semitones_at_the_msb() {
    let mut m = Midi::new();
    m.trs(&[0xB0, 101, 0, 100, 0, 6, 12]);
    assert_eq!(bend_range(&m), 12, "CC 6 is the semitones");
    m.trs(&[38, 50]);
    assert_eq!(bend_range(&m), 12, "CC 38, the cents, is ignored");
    m.trs(&[96, 0, 96, 0]);
    assert_eq!(bend_range(&m), 14);
    m.trs(&[97, 0]);
    assert_eq!(bend_range(&m), 13);
    m.trs(&[6, 100]);
    assert_eq!(bend_range(&m), LIVE_BEND_RANGE_MAX, "clamped");
    m.trs(&[6, 0, 97, 0]);
    assert_eq!(bend_range(&m), 0);
    // The range is the expression row's NRPN too, its value at the LSB.
    let r = row_for(LIVE_PARAM_BEND_RANGE);
    m.trs(&[
        99,
        (r.nrpn >> 7) as u8,
        98,
        (r.nrpn & 0x7f) as u8,
        6,
        0,
        38,
        7,
    ]);
    assert_eq!(bend_range(&m), 7);
}

/// NRPN 6/0, the bend range's row.
fn nrpn_bend_range(m: &mut Midi, semitones: u8) {
    let r = row_for(LIVE_PARAM_BEND_RANGE);
    m.trs(&[
        0xB0,
        99,
        (r.nrpn >> 7) as u8,
        98,
        (r.nrpn & 0x7f) as u8,
        6,
        0,
        38,
        semitones,
    ]);
}

#[test]
fn rpn_0_keeps_the_bend_range_across_program_changes() {
    // RP-015: RPN values are the channel's state, so a DAW's pitch-bend sensitivity, sent
    // once before its program change, holds.
    let mut m = Midi::new();
    m.trs(&[0xB0, 101, 0, 100, 0, 6, 12]);
    m.trs(&[0xC0, 1]);
    assert_eq!(bend_range(&m), 12, "the program change kept RPN 0's range");
    m.trs(&[0xC0, 0]);
    assert_eq!(bend_range(&m), 12, "and the next");
    // Neither reset all controllers nor a gap in the input resets it.
    m.trs(&[0xB0, 121, 0]);
    m.epoch = m.epoch.wrapping_add(1);
    m.trs(&[0xC0, 2]);
    assert_eq!(bend_range(&m), 12);
    // It bends the notes of the new preset.
    m.trs(&[0x90, 60, 100, 0xE0, 0, 0]);
    m.rig.tick();
    assert_eq!(
        m.rig.live.voice[0].freq_out,
        ((u32::from(unsafe { live_freq_at(60, -8192 * 12) }) * 704) >> 8) as u16,
        "the sync lead's voice 1, 12 semitones down"
    );
    // The increment and decrement are RPN 0's data entry too.
    m.trs(&[0xB0, 101, 0, 100, 0, 97, 0, 0xC0, 0]);
    assert_eq!(bend_range(&m), 11);
}

#[test]
fn the_bend_range_nrpn_edits_the_preset_and_the_last_of_rpn_and_nrpn_wins() {
    let mut m = Midi::new();
    // NRPN 6/0 alone is a preset edit, like every NRPN: a program change discards it.
    nrpn_bend_range(&mut m, 7);
    assert_eq!(bend_range(&m), 7);
    m.trs(&[0xC0, 1]);
    assert_eq!(bend_range(&m), LIVE_BEND_RANGE_DEFAULT);
    // RPN 0 then NRPN 6/0: the NRPN wins, and as an edit it ends RPN 0's hold.
    m.trs(&[0xB0, 101, 0, 100, 0, 6, 12]);
    nrpn_bend_range(&mut m, 5);
    assert_eq!(bend_range(&m), 5);
    m.trs(&[0xC0, 0]);
    assert_eq!(
        bend_range(&m),
        LIVE_BEND_RANGE_DEFAULT,
        "back to the preset's"
    );
    // NRPN 6/0 then RPN 0: the RPN wins and holds again.
    nrpn_bend_range(&mut m, 5);
    m.trs(&[0xB0, 101, 0, 100, 0, 6, 9]);
    assert_eq!(bend_range(&m), 9);
    m.trs(&[0xC0, 1]);
    assert_eq!(bend_range(&m), 9);
    // Power-on forgets it.
    let m = Midi::new();
    assert_eq!(bend_range(&m), LIVE_BEND_RANGE_DEFAULT);
}

#[test]
fn other_rpns_and_half_selected_ones_change_nothing() {
    let mut m = Midi::new();
    // fine tuning (0/1), coarse tuning (0/2), and RPN MSB 0 with the LSB still null
    for sel in [[0u8, 1], [0, 2], [1, 0]] {
        m.trs(&[0xB0, 101, sel[0], 100, sel[1], 6, 9, 38, 9, 96, 0]);
    }
    m.trs(&[121, 0, 101, 0, 6, 9]);
    assert_eq!(bend_range(&m), LIVE_BEND_RANGE_DEFAULT);
}

#[test]
fn nrpn_state_is_per_channel() {
    let mut m = Midi::new();
    let res = row_for(LIVE_PARAM_RESONANCE).nrpn;
    m.trs(&[0xB0, 99, 0, 98, 0]); // channel 1: cutoff
    m.trs(&[0xB3, 99, (res >> 7) as u8, 98, (res & 0x7f) as u8]); // channel 4: resonance
    m.trs(&[0xB0, 6, 3]);
    m.trs(&[0xB3, 6, 0, 38, 2]);
    assert_eq!(m.cutoff(), 3 << 7);
    assert_eq!(m.rig.live.resonance, 2);
    m.rig.clear();
    m.trs(&[0xB7, 6, 1, 38, 1, 96, 0]); // channel 8 has nothing selected
    assert!(m.rig.log().is_empty(), "writes: {:?}", m.rig.log());
}

#[test]
fn nrpn_state_is_per_input() {
    let mut m = Midi::new();
    select_cutoff(&mut m, 0xB0);
    m.usb_cc(0, 6, 1); // USB on the same channel selected nothing
    assert_eq!(m.cutoff(), 1200);
    m.usb_cc(0, 99, 0);
    m.usb_cc(0, 98, 0);
    m.usb_cc(0, 6, 1);
    assert_eq!(m.cutoff(), 1 << 7);
    m.trs(&[6, 2]);
    assert_eq!(m.cutoff(), 2 << 7);
}

#[test]
fn reset_all_controllers_deselects() {
    let mut m = Midi::new();
    select_cutoff(&mut m, 0xB0);
    m.trs(&[121, 0]);
    m.trs(&[6, 1]);
    assert_eq!(m.cutoff(), 1200);
}

#[test]
fn a_gap_in_the_input_deselects() {
    let mut m = Midi::new();
    select_cutoff(&mut m, 0xB0);
    m.epoch = 1; // input lost here: the selection may have changed in it
    m.trs(&[0xB0, 6, 1]);
    assert_eq!(m.rx.restarts, 1);
    assert_eq!(m.cutoff(), 1200);
}

#[test]
fn unmapped_nrpns_and_bad_channels_change_nothing() {
    let mut m = Midi::new();
    m.rig.clear();
    let used: BTreeSet<u16> = rows().iter().map(|r| r.nrpn).collect();
    for msb in 0..128u8 {
        for lsb in [0u8, 5, 31, 64, 127] {
            if !used.contains(&nrpn(msb, lsb)) {
                m.trs(&[0xB0, 99, msb, 98, lsb, 6, 1, 38, 1, 96, 0]);
            }
        }
    }
    assert!(m.rig.log().is_empty(), "writes: {:?}", m.rig.log());
    let mut port = Box::<midi_map_port>::default();
    unsafe {
        midi_map_port_init(&mut *port, &mut *m.rig.live);
        midi_map_cc(&mut *port, 16, 99, 0);
        midi_map_cc(&mut *port, 16, 98, 0);
        midi_map_cc(&mut *port, 16, 6, 1);
        midi_map_cc(&mut *port, 0, 6, 200);
    }
    assert!(m.rig.log().is_empty(), "writes: {:?}", m.rig.log());
}

#[test]
fn the_pulse_width_nrpn_sets_12_bits() {
    let mut m = Midi::new();
    let r = row_for(LIVE_PARAM_PULSE_WIDTH);
    m.trs(&[0xB0, 99, (r.nrpn >> 7) as u8, 98, (r.nrpn & 0x7f) as u8]);
    m.trs(&[6, 31, 38, 127]); // 4095
    assert_eq!(m.rig.voice_pw(0), 4095);
    m.trs(&[6, 1, 38, 1]);
    assert_eq!(m.rig.voice_pw(2), 129);
}

fn select(m: &mut Midi, param: u32) {
    let r = row_for(param);
    m.trs(&[0xB0, 99, (r.nrpn >> 7) as u8, 98, (r.nrpn & 0x7f) as u8]);
}

fn resets(rig: &Rig) -> usize {
    rig.log()
        .iter()
        .filter(|w| w.0 == LIVE_REG_RESET as u8)
        .count()
}

#[test]
fn a_small_parameter_takes_the_value_at_the_lsb_without_passing_through_0() {
    // A standard 14-bit sender: 99, 98, then 6 = 0 and 38 = 1 for the value 1.
    let mut m = Midi::with_model(SID_MODEL_8580);
    select(&mut m, LIVE_PARAM_MODEL);
    m.rig.clear();
    m.trs(&[6, 0]);
    m.rig.tick();
    m.trs(&[38, 1]);
    m.rig.tick();
    assert_eq!(m.rig.live.model, SID_MODEL_8580);
    assert_eq!(get(&m.rig, LIVE_PARAM_MODEL), 1);
    assert_eq!(resets(&m.rig), 0, "writes: {:?}", m.rig.log());
    assert!(m.rig.log().is_empty(), "writes: {:?}", m.rig.log());
}

#[test]
fn the_waveform_by_nrpn_never_passes_through_silence() {
    let mut m = Midi::new();
    select(&mut m, LIVE_PARAM_WAVEFORM);
    m.rig.clear();
    for (cc, value) in [(6, 0), (38, 4), (6, 0), (38, 1), (6, 0), (38, 8)] {
        m.trs(&[cc, value]);
        assert_ne!(
            get(&m.rig, LIVE_PARAM_WAVEFORM),
            0,
            "after CC {cc} = {value}"
        );
    }
    assert_eq!(get(&m.rig, LIVE_PARAM_WAVEFORM), 8);
    for &(reg, value) in m.rig.log() {
        if reg % 7 == 4 && reg < 21 {
            assert_ne!(value & 0xf0, 0, "control {reg} written silent");
        }
    }
}

#[test]
fn an_msb_alone_leaves_a_small_parameter_alone() {
    let mut m = Midi::new();
    select(&mut m, LIVE_PARAM_RESONANCE);
    m.rig.clear();
    m.trs(&[6, 0, 6, 1, 6, 127]);
    assert!(m.rig.log().is_empty(), "writes: {:?}", m.rig.log());
    // The stored MSB still counts for the LSB that completes it.
    m.trs(&[6, 0, 38, 9]);
    assert_eq!(m.rig.live.resonance, 9);
    m.trs(&[6, 1, 38, 0]);
    assert_eq!(
        u32::from(m.rig.live.resonance),
        LIVE_RESONANCE_MAX,
        "128 clamps"
    );
}

#[test]
fn an_nrpn_reaches_a_slot_field() {
    let mut m = Midi::new();
    let amount = LIVE_PARAM_SLOT + LIVE_SLOT_FIELDS + LIVE_SLOT_AMOUNT; // Modulation's
    set(&mut m.rig, amount, 50);
    select(&mut m, amount);
    m.rig.clear();
    m.trs(&[6, 0, 38, 100]);
    assert_eq!(m.rig.live.slot[1][LIVE_SLOT_AMOUNT as usize], 100);
    let reg = (LIVE_REG_SLOT_BASE | 1 << 3 | LIVE_SLOT_AMOUNT) as u8;
    assert_eq!(m.rig.log(), &[(reg, 100)], "one write, no 0 on the way");
}

#[test]
fn an_nrpn_reaches_the_chip_model() {
    let mut m = Midi::new();
    select(&mut m, LIVE_PARAM_MODEL);
    m.rig.clear();
    m.trs(&[6, 0, 38, 1]);
    m.rig.tick();
    assert_eq!(m.rig.live.model, SID_MODEL_8580);
    assert_eq!(resets(&m.rig), 1);
    assert_eq!(
        m.rig.log()[CHARACTER],
        (LIVE_REG_RESET as u8, SID_MODEL_8580 as u8)
    );
}

#[test]
fn the_envelope_mode_is_cc_26_and_nrpn_1_10() {
    // Next to the chip model (CC 25, NRPN 1/5) in the SID's block: like it, a setting of the
    // chip rather than of the preset. CC 26 is in 20..31, which MIDI leaves undefined.
    let r = row_for(LIVE_PARAM_ENVELOPE_MODE);
    assert_eq!((r.cc, r.nrpn), (26, nrpn(1, 10)));
    assert_eq!((r.max, r.def), (1, LIVE_ENVELOPE_INSTANT as u16));
    assert!(r.steps());
    let mut m = Midi::new();
    m.rig.clear();
    m.usb_cc(3, 26, 64); // steps: on from 64
    assert_eq!(
        get(&m.rig, LIVE_PARAM_ENVELOPE_MODE),
        LIVE_ENVELOPE_AUTHENTIC
    );
    assert_eq!(m.rig.log(), &[(LIVE_REG_GATE_RESET as u8, 0)]);
    m.usb_cc(3, 26, 63);
    assert_eq!(get(&m.rig, LIVE_PARAM_ENVELOPE_MODE), LIVE_ENVELOPE_INSTANT);
    select(&mut m, LIVE_PARAM_ENVELOPE_MODE);
    m.rig.clear();
    m.trs(&[6, 0, 38, 1]);
    assert_eq!(
        get(&m.rig, LIVE_PARAM_ENVELOPE_MODE),
        LIVE_ENVELOPE_AUTHENTIC
    );
    assert_eq!(
        m.rig.log(),
        &[(LIVE_REG_GATE_RESET as u8, 0)],
        "no 0 on the way"
    );
}

// --- the published reference ---

// The generated file's own licence header; reuse must not read it as this file's.
// REUSE-IgnoreStart
const HEADER: &str = "<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->\n\
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->\n";
// REUSE-IgnoreEnd

fn intro() -> String {
    format!(
        "\
<!-- Generated from app/midi_map.c by host/crates/sidou-tests/tests/app/midi_map.rs; do not \
edit. SIDOU_MIDI_DOC_UPDATE=1 regenerates it. -->
# MIDI: the CC and NRPN map

Every parameter of the live instrument has an NRPN (14-bit, full resolution). All except
the bend range, three expression targets and expert layer also have a fixed CC (7-bit).
MIDI learn can bind a CC to any expert knob. The map uses the same numbers on every channel, on TRS and USB. Notes
play on every channel (omni) by default; MIDI's mode (Modes, below) can give the instrument
one channel and chip A's voices channels of their own. Program change 0..{last} selects the
preset (in bank 0, the power-on bank: Program change and banks, below). A program change
loads the preset's cutoff, resonance, pulse width, SID settings (waveform, ring, sync,
the envelope, filter routing and mode) and expression settings (bend range, the targets,
the vibrato) over any edits, even when it selects the preset already playing; volume, the
chip model and character, the envelope mode, the effect slots, the controllers (bend, wheel, aftertouch) and
a bend range set by RPN 0 are kept.

**How the numbers were chosen.** CCs that MIDI gives a meaning are kept for it:
General MIDI's where they fit (1 modulation wheel: the pulse width by default, see
Expression; 7 volume; 71 resonance; 74 cutoff, i.e. brightness; 72, 73 and 75 release,
attack and decay time; 76, 77 and 78 vibrato rate, depth and delay) and left alone where
they do not (0 and 32 bank select, 6 and 38 data entry, 64..69 the pedals, 88
high-resolution velocity, 96..101 increment, decrement, NRPN and RPN, 120..127 channel
mode). The rest is in blocks MIDI leaves undefined: the SID in 20..26 and 79, the
character layer in 85..87 and 89..90, the effect slots in 102..116. NRPNs are grouped by
MSB: 0 the performance controls, 1 the SID, 2 the character layer, 3..5 the effect slots,
6 expression.

CC 71..78 are absolute here, like every CC of the map: the value sets the parameter
across its range. General MIDI 2 makes its sound controllers relative instead, moving the
patch's value up or down around 64; SIDOU does not.

## Receiving values

- **CC, linear curve:** 0..127 spans the whole range, value * max / 127 rounded, so 0 and
  127 are the ends.
- **CC, steps curve:** 0..127 is divided evenly among the parameter's values,
  value * (max + 1) / 128: a switch is off below 64 and on from 64.
- **NRPN:** CC 99 and 98 select the parameter (MSB, LSB). The data is the parameter's own
  value, MSB * 128 + LSB, clamped to its range: CC 6 sets the MSB (the LSB taken as 0),
  CC 38 the LSB (with the MSB of the last CC 6, or, if none came since the selection or
  the last step, the current value's). CC 96 and 97 step the value by one (their data byte
  is ignored).
- **NRPN, parameters up to 0..127** (the 0..15 envelope rates, sustain, resonance and
  waveform, the switches and modes, the effect slots): the value is all in the LSB, with
  an MSB of 0, e.g. 6 = 0, 38 = 9 for 9. CC 6 only stores the MSB and CC 38 applies the
  value, so a sender's 6 = 0, 38 = v goes straight to v and never through 0 (which would
  reset the chip for the chip model, or silence the waveform). A controller that sends
  CC 6 alone (\"7-bit NRPN\") changes nothing here: use CC 38, or the parameter's plain
  CC.
- **Null and RPN:** NRPN 127/127 and RPN 127/127 select nothing. An RPN (CC 101, 100)
  takes data entry away from the NRPN until CC 99 or 98 comes again. RPN 0/0 (pitch-bend
  sensitivity) sets the bend range: CC 6 the semitones, at once (0..24), CC 96 and 97 a
  semitone up or down; CC 38 (the cents) is ignored. Other RPNs do nothing. CC 121 (reset
  all controllers) and a gap in the input select nothing.
- **Bend range, RPN 0 and NRPN 6/0:** RPN 0 is the channel's setting, as RP-015 has it, so
  program changes keep it until power-off: a sequencer may send it once, before its
  program change. NRPN 6/0 is the preset's, like every NRPN: a program change replaces it
  with the new preset's. Of the two the last one received wins, and NRPN 6/0 ends RPN 0's
  hold: after it, the next program change loads the preset's range again. Neither CC 121
  nor a gap in the input changes the range.
- **Chip model:** a change resets the chip, so it waits for the next 10 ms tick and takes
  the last value received. The new chip is prepared in the background a slice a tick, 70 to
  110 ms (the first 8580 after power-on about 150 ms), while the old one plays on, and swaps
  in when it is whole: a stream of changes never holds the instrument up, and the last one
  received is the chip you get. A stream faster than that (a change every 70 ms or less)
  restarts the preparation each time, so the chip swaps only once it stops. The FX button
  asks for a change the same way.
- **Two chips:** everything here (notes, CCs, NRPNs, expression, learn) plays the
  instrument, so the chip the keys play: chip A, or chip B while a song plays on chip A (the
  KEYS setting AUTO; with KEYS A, chip A always). The chip-level CCs (25 model, 85 profile,
  86-90 the character) set the keys' setup, which both chips hold: the other chip follows it
  in the background. On chip A's own channels (Modes, below) they set chip A's setup.
- **Character:** the profile (CC 85) is a chip and a model: a change resets the chip as a
  model change does, and the model follows it. The model (CC 25, the FX button) selects
  the 8580 R5 profile, or goes back to the last 6581 profile. Age/heat, DAC sloppiness,
  output thump and filter bleed change the playing chip without a reset; they too go from
  the next tick, the last value received, and glide there so a jump does not click (age
  and thump 0 to 127 in 0.32 s, the bleed in 1.27 s, the DACs in 1.72 s). The profiles, their sources and
   what each control does: docs/decisions/0006-chip-profiles.md.
- **Expert character (NRPN 2/5..18):** fourteen independent controls, each a raw value
  0..127 mapped to the physical units in the table. Send CC 99 = 2, CC 98 = the LSB,
  CC 6 = 0, then CC 38 = the value. They never reset the chip or interrupt a held note;
  table builds run in slices and their stages swap at an audio block boundary.
  A macro move releases only its related overrides (AGE: curves/range/non-linearity;
  DAC: kink/leakage; THUMP and BLEED: their namesakes). Other experts stay edited.
  Selecting a built-in profile/model releases all overrides; USER profiles recall theirs.
  Outgoing MIDI sends the model/profile/macros before explicit expert overrides, and does
  not emit derived expert NRPNs for a macro move: those would freeze the receiving macro's path.
  USER schema 2 and settings schema 4 keep the override state, not a quantized copy of defaults.
  The 6581/8580 curves apply to their named model; input bias, non-linearity and the
  external low-pass are 6581-only. Inapplicable knobs are grey and say N/A on EXPERT and
  in the value popup. Combined strength's AVERAGE band is labelled MED on the panel.
- **Envelope mode:** 0 INSTANT (the default) restarts each voice's envelope rate counter at
  each gate edge, so a note's attack, and its release at the note-off, take their first
  step within one rate period and 3 chip cycles (12 cycles at attack 0). 1 AUTHENTIC is
  the chip's ADSR delay bug, as reSIDfp has it: after a slower rate (a release before the
  attack, a decay before the release), most notes and note-offs wait up to 33 ms. A change
  counts from the next gate edge, so a sounding note's release follows it.
  [controls.md](controls.md) has the why.
- The selection is kept per channel and per input (TRS, USB).
- CC 120 and 123 release every note.

## Expression

The expression settings are per preset (NRPN group 6, the vibrato also on CC 76..78); the
controllers (bend, wheel, aftertouch) are one set for every channel, like the notes.

- **Pitch bend:** all 14 bits, +-the bend range (2 semitones on every preset; RPN 0 or
  NRPN 6/0 change it, see above). At +-2 one bend step is 0.024 cents. The SID's frequency register
  is coarser than that at every pitch (one step is 0.39 cents at C4, 1.6 cents at C2), so
  the bend moves the pitch one register step at a time, with no larger jumps. The
  frequency between the notes is computed with integers from generated tables, within 0.6
  of a register step of the exact value. Every sounding voice bends, released ones too, and
  a note starts at the current bend. The sync lead's two oscillators take the same bend
  and vibrato, so their ratio, and the timbre, holds, up to the top of voice 1's register
  (from note 90, or lower bent up: [controls.md](controls.md), Presets).
- **Control rate:** bend, aftertouch, the wheel (on a target other than the pulse width)
  and the vibrato apply at the instrument's 100 Hz tick: up to 10 ms after the message,
  twice a C64 player's frame rate. A flood of messages costs one update per tick.
- **Vibrato:** a triangle per voice, from 0 at each note-on once its delay is over. Rate
  CC 76 (0.1 Hz steps, up to 12.7 Hz), depth CC 77 (127 is +-99 cents), delay CC 78
  (10 ms steps). The wheel and aftertouch add to the depth where they are assigned to the
  vibrato, up to 127. Updated at 100 Hz, a triangle of +-d cents at f Hz moves 4 * d * f /
  100 cents a tick: 6 cents at +-25 cents and 6 Hz, 24 cents at the full depth. Above
  about 8 Hz a cycle has fewer than 12 steps and the vibrato starts to sound stepped.
- **Modulation wheel (CC 1):** by default it sets the pulse width across its range, as
  M1a did and the map's row says. The mod wheel target (NRPN 6/2) moves it to the vibrato
  depth (it adds) or to the cutoff (it opens the filter above the cutoff that is set, 8
  steps per value). A new target starts the wheel at 0.
- **Aftertouch:** channel and polyphonic. A voice's pressure is the larger of the
  channel's and its note's, and a note-on starts its own at 0. The target (NRPN 6/3): off
  (the default), the vibrato depth (it adds), the pulse width (it raises the voice's, 16
  steps per value) or the cutoff (it opens the filter 8 steps per value of the most
  pressed voice: the SID has one filter).
- **Velocity:** the SID has no amplifier per voice, so velocity sets one of three things,
  per preset (NRPN 6/1): off (the default, full level), the sustain level (scaled by
  velocity / 127, written before the gate; the attack still peaks at the full level), the
  cutoff (8 steps lower per velocity below 127, by the last note: one filter) or the pulse
  width (the voice's pulse narrowed by velocity / 127; heard on the pulse waveform only).
  127 always sounds as off does. The panel's keys play velocity 127, so they sound as off
  does whatever the target.
- **Reset:** CC 121 centres the bend and sets the wheel and aftertouch to 0 (the pulse
  width is a parameter, not the wheel's position, and is kept). A gap in the input does
  the same, so a lost return to the centre cannot leave the pitch bent.

## Output

SIDOU sends its parameter changes over USB (the USB-MIDI IN endpoint, cable 0), so a
DAW can record the panel as automation and play it back. Nothing goes out of the TRS
jack, which is MIDI IN only.

- **What:** a parameter that changes on the panel (the knobs, MASTER, the FX button,
  the effect-slot controls) goes out as the map's CC: the value as the CC curve gives it,
  round(value * 127 / max). A preset from the panel (K4) goes out as a program change,
  and the parameters it loads do not (the program change carries them).
- **High resolution:** an option, off by default, sends the parameters of more than
  128 values (cutoff, pulse width, volume) as their NRPNs instead, at full resolution.
  A parameter with no CC (the bend range, the expression targets) always goes as its
  NRPN, and so does the pulse width while the mod wheel drives another target (CC 1 is
  then the wheel). A change of the mod target goes before the tick's other parameters
  (after a program change), so CC 1 is never read with the old target. An NRPN is {nrpn} CCs, without running status: 99 and 98 its
  number, 6 and 38 the value's MSB and LSB (value / 128, value % 128), which is the
  order the input above takes; no null NRPN follows.
- **Channel:** 1 by default. Until M2 stores settings, the channel and the
  high-resolution option are build options (CMake `SIDOU_MIDI_OUT_CHANNEL`, 1..16, and
  `SIDOU_MIDI_OUT_NRPN`). The input follows MIDI's mode (Modes, below).
- **No echo:** a MIDI-only change, from either port, is never sent back:
  the sender has it, and a DAW recording with MIDI thru on would otherwise loop. If
  MIDI changes a parameter after the panel did, and before the panel's change went out,
  MIDI's wins and nothing is sent. So TRS input is not passed on to USB: SIDOU is no
  MIDI thru, and only the panel's changes go out.
  If an earlier local profile/macro is still pending when a remote expert edit arrives,
  that expert is restored after the local message so the local release cannot erase it.
- **Rate:** at the instrument's {hz} Hz tick, at most one message per parameter (its
  latest value) and {tick} packets in all a tick. A value that sends as the one last
  sent (or received), such as a pulse-width move within one CC step, sends nothing.
  What does not fit waits for the next tick, still as one latest value: nothing queues
  up. With no host (no cable, or the computer asleep) the changes are dropped, so a host
  that comes later gets none stale.
  A cold expert snapshot or mixed macro/expert restoration uses half the packet limit
  for that tick, keeping input handling inside the same instruction budget. Later ticks
  continue with the latest values; no obsolete intermediate edits are queued.
- **Limit:** a parameter the preset loads (not the volume, the chip model, the effect
  slots or a bend range set by RPN 0) that the panel also changes in the same tick as the
  preset counts as loaded by it and is not sent.

",
        last = LIVE_PRESET_COUNT - 1,
        nrpn = MIDI_OUT_NRPN_PACKETS,
        hz = LIVE_TICK_HZ,
        tick = MIDI_OUT_TICK_PACKETS
    )
}

/// The CCs below the channel mode messages (120..127) that MIDI learn leaves to the NRPN and
/// RPN receiver, from app/midi_learn.c.
fn unlearnable() -> Vec<u32> {
    (0..120)
        .filter(|&cc| unsafe { midi_learn_learnable(cc) } == 0)
        .collect()
}

fn list(v: &[u32]) -> String {
    let s: Vec<String> = v.iter().map(u32::to_string).collect();
    match s.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => s.join(""),
    }
}

fn learn_section() -> String {
    format!(
        "\
## MIDI learn

Any knob's parameter can also follow a CC of your choice: hold SAVE and turn the knob (EDIT held
for the effect-slot layer), then send the CC ([controls.md](controls.md), MIDI learn, has
the gesture, how to cancel and how to clear). What is learned:

- **Which CCs:** 0..119, except {unlearnable}, which carry bank select (0 and 32, see
  Program change and banks) and NRPN and RPN data entry and selection; 120..127 are
  channel mode messages. A CC from either port (TRS or USB) and on
  any channel: like the map, a learned CC is omni.
- **Added, and it wins:** the parameter's own CC and NRPN still work. If the learned CC was
  another parameter's, it now moves the learned parameter only; the other keeps its NRPN,
  and gets its CC back when the binding is forgotten. CC 1 learned for a parameter no longer
  moves the modulation wheel.
- **One each way:** a parameter has at most one learned CC and a CC at most one parameter;
  learning again moves the binding. The gesture reaches the {knobs} parameters on the knobs
  (the cutoff, resonance and pulse width, CHIP's Age, DAC, Thump and Bleed, and each
  effect slot's Amount, A, B and Mix), so
  there are at most {knobs} bindings.
- **Values:** through the parameter's curve and range in the map below, as its own CC.
- **Output:** a panel change to a learned parameter goes out on the learned CC, so a DAW
  records what the controller sends and plays it back onto the same parameter. A parameter
  whose own CC another parameter learned goes out as its NRPN. A learned CC from MIDI is
  never echoed, as any other.
- **Kept:** across program changes and power cycles, saved with the settings five seconds
  after the last change, or at once with SAVE. An armed learn with no step for {timeout} s is
  cancelled.

",
        unlearnable = list(&unlearnable()),
        knobs = 3 + (LIVE_CHAR_FIELDS - 1) + LIVE_SLOTS * (LIVE_SLOT_FIELDS - 1),
        timeout = MIDI_LEARN_TIMEOUT_MS / 1000,
    )
}

/// MIDI's modes (app/midi_route.c, M2e Task 3): the table from the router's own text.
fn modes_section() -> String {
    let mut s = String::from(
        "\
## Modes

MIDI's mode says which channel plays what. It is set on the MIDI page (HOME, MIDI:
[controls.md](controls.md)) and saved with a song in its `MIDI` chunk
([song-format.md](song-format.md)); the page's own setting stays in RAM until the settings
persist. N is the page's CHANNEL, 1..16, and N + 1.. wrap round from 16 to 1.

| Mode | What plays |
|---|---|
",
    );
    for m in 0..MIDI_MODE_COUNT {
        s.push_str(&format!(
            "| {} | {} |\n",
            text(unsafe { midi_route_mode_name(m) }),
            text(unsafe { midi_route_mode_doc(m) })
        ));
    }
    s.push_str(&format!(
        "\
\n- **Chip A is the player's** in MULTI and both WHOLE modes, with or without a song: the
  instrument (the panel's keys, and its MIDI channel) plays chip B, with the keys' setup, as
  while a song plays.
- **Chip A's channels:** notes 12..107 (GoatTracker's C-0..B-7), without velocity (its
  instruments have none). Program change n (0..{instr}) picks GoatTracker instrument n + 1
  for the channel's next note. Pitch bend bends the channel's voice by the channel's own
  range, {range} semitones until RPN 0 sets it, at the 100 Hz tick. CC 74 (cutoff) and 71
  (resonance) lock chip A's filter at the value, over the song's, until CC 121 or a change
  of mode. CC 25, 26, 85, 86, 87, 89 and 90 set chip A's model, envelope mode, profile and
  character. CC 120 and 123 release the channel's voices; CC 121 also centres its bend.
  Nothing else (the map, MIDI learn) reaches them: those are the instrument's channel's.
- **A jam note takes its voice from the song** while it sounds (the song's writes to it
  are held back) and gives it back at its release.
- **The options:** LEGATO (MULTI, CHORD) moves to the next note without a new attack while
  one is held. CHORD plays the note and its two intervals (CHORD 2 and CHORD 3 on the page,
  +{c2} and +{c3} by default: a major triad). ARP cycles the held notes, in the order they
  were pressed, on voice 1, a step every ARP RATE frames ({arp} by default), or every 16th of
  the MIDI clock ({clocks} clocks) with the rate at CLOCK.
- **Not yet on the device:** chip A's channels are routed and tracked, and sound once song
  control (M2c) takes jam releases, voices and bends.

",
        instr = MIDI_ROUTE_INSTR_MAX - 1,
        range = LIVE_BEND_RANGE_DEFAULT,
        c2 = MIDI_ROUTE_CHORD_DEFAULT_2,
        c3 = MIDI_ROUTE_CHORD_DEFAULT_3,
        arp = MIDI_ROUTE_ARP_DEFAULT,
        clocks = MIDI_ROUTE_ARP_CLOCKS,
    ));
    s
}

/// Program change and bank select (app/midi_route.c, M2e Task 4): the banks from the
/// router's own text.
fn banks_section() -> String {
    let mut s = String::from(
        "\
## Program change and banks

CC 0 (bank select MSB) and CC 32 (LSB) choose a channel's bank, MSB * 128 + LSB, and keep it
until they change: bank 1 is CC 0 = 0, CC 32 = 1. A program change then means what the
channel's bank says. Bank 0, the power-on bank, is M1's meaning on the instrument's channel.

| Bank | Program n selects |
|---|---|
",
    );
    for b in 0..MIDI_ROUTE_BANKS {
        s.push_str(&format!(
            "| {b} | {} |\n",
            text(unsafe { midi_route_bank_doc(b) })
        ));
    }
    s.push_str(
        "\
\n- Any other bank, or a program past a bank's range, does nothing. The bank is kept per
  channel for both ports together, and across a gap in the input.
- **Songs:** a load takes a few passes; a second request before it is done replaces the
  first. Until the device keeps songs (M2c), the request waits and nothing loads.
- **USER profiles:** until the device keeps them (M2a), the request waits and nothing
  changes.
- **Out:** a preset from the panel goes out as its program change in bank 0, with CC 0 and 32
  first when the last bank sent was another; a song loaded from the SONG menu goes out as
  CC 0 = 0, CC 32 = 1 and its program.

",
    );
    s
}

/// MIDI clock and transport for the song (app/midi_sync.h, M2e Task 2).
fn clock_section() -> String {
    let fs = 24e6 / 544.0;
    let ms = |samples: u32| f64::from(samples) / fs * 1e3;
    format!(
        "\
## Clock and transport

SIDOU's song follows a sequencer's MIDI clock, or leads one. The MIDI page's CLOCK (AUTO,
FOLLOW, OFF; AUTO at power-on) and CLOCK OUT (OFF at power-on) set it
([controls.md](controls.md)); both are kept in RAM until the settings are saved.

- **In** (TRS and USB, merged): timing clock (F8), start (FA), continue (FB), stop (FC),
  song position (F2) and song select (F3). With CLOCK at AUTO or FOLLOW and a clock arriving,
  the song's frame clock follows the clock: its rows land on the clock's 16ths, at the
  song's rows a beat (ROWS/BEAT, 4 by default: a row a 16th). The tempo comes from a fit
  through the last {window} clocks, known after {lock}; each row's error goes into the next
  beat's period (at most 1 % of a beat), and an error over {align:.1} ms re-aligns at once.
- **Start** plays the song from its start on the next clock; **continue** goes on on the
  next clock from where it stopped, or from a song position; **stop** stops it, its
  releases ringing as the chip's would. **Song position** (in 16ths: rows = 16ths x rows a
  beat / 4) is taken while the song is stopped or not started, and sought in the
  background; a continue that comes during the seek starts when it is done, as far on as
  the clock went meanwhile. A song position while the song plays is ignored. **Song
  select** picks the subtune the next start plays.
- **The delay:** each row sounds {delay:.1} ms after its clock reached the jack: the block
  being heard, the next one (already rendered), and the poll. It is a constant, for a
  sequencer's output latency compensation.
- **A clock that stops** ({lost} ms without one): with AUTO the song carries on at the last
  tempo; with FOLLOW it stops. CLOCK OFF ignores clock and transport: the song runs on its
  own frames, and start and stop come from the panel only.
- **Out** (USB only: the TRS jack is MIDI IN only), with CLOCK OUT on and no clock coming
  in: {ppq} clocks a beat at the song's tempo (a beat is its calls a row x rows a beat), each
  sent in the first pass after its time, within a pass and USB's 1 ms frame; start (FA) when
  the song starts, stop (FC) when it stops, and song position then continue (F2, FB) when it
  goes on from where it stopped. A clock that comes in is never sent back: while one is
  followed, nothing goes out.

",
        window = MIDI_CLOCK_WINDOW,
        lock = MIDI_CLOCK_LOCK,
        align = ms(MIDI_SYNC_ALIGN_SAMPLES),
        delay = ms(MIDI_SYNC_DELAY),
        lost = MIDI_CLOCK_LOST_MS,
        ppq = MIDI_CLOCK_PER_BEAT,
    )
}

fn cc_text(r: &Row) -> String {
    if r.has_cc() {
        r.cc.to_string()
    } else {
        "-".into()
    }
}

fn nrpn_text(r: &Row) -> String {
    if r.has_nrpn() {
        format!("{}/{} ({})", r.nrpn >> 7, r.nrpn & 0x7f, r.nrpn)
    } else {
        "-".into()
    }
}

fn curve_text(r: &Row) -> &'static str {
    if r.steps() {
        "steps"
    } else {
        "linear"
    }
}

fn doc() -> String {
    let rows = rows();
    let mut s = String::from(HEADER);
    s.push_str(&intro());
    s.push_str(&learn_section());
    s.push_str(&modes_section());
    s.push_str(&banks_section());
    s.push_str(&clock_section());
    s.push_str("## The map\n\nIn NRPN order.\n\n");
    s.push_str("| Group | Parameter | CC | NRPN (MSB/LSB) | Range | Curve | Default | Values |\n");
    s.push_str("|---|---|---|---|---|---|---|---|\n");
    let mut by_nrpn: Vec<&Row> = rows.iter().filter(|r| !r.reserved()).collect();
    by_nrpn.sort_by_key(|r| (r.nrpn, r.cc));
    for r in by_nrpn {
        s.push_str(&format!(
            "| {} | {} | {} | {} | 0..{} | {} | {} | {} |\n",
            r.group,
            r.name,
            cc_text(r),
            nrpn_text(r),
            r.max,
            curve_text(r),
            r.def,
            r.values
        ));
    }
    let reserved: Vec<&Row> = rows.iter().filter(|r| r.reserved()).collect();
    if !reserved.is_empty() {
        s.push_str(
            "\n## Reserved: not yet implemented\n\n\
             These numbers are kept for parameters to come and do nothing yet; a CC or NRPN \
             sent to them is ignored.\n\n",
        );
        s.push_str("| Group | Parameter | CC | NRPN (MSB/LSB) | Range | Values |\n");
        s.push_str("|---|---|---|---|---|---|\n");
    }
    for r in reserved {
        s.push_str(&format!(
            "| {} | {} | {} | {} | 0..{} | {} |\n",
            r.group,
            r.name,
            cc_text(r),
            nrpn_text(r),
            r.max,
            r.values
        ));
    }
    s.push_str("\n## By CC\n\n| CC | Parameter |\n|---|---|\n");
    let mut by_cc: Vec<&Row> = rows.iter().filter(|r| r.has_cc()).collect();
    by_cc.sort_by_key(|r| r.cc);
    for r in by_cc {
        let note = if r.reserved() { " (reserved)" } else { "" };
        s.push_str(&format!("| {} | {} {}{} |\n", r.cc, r.group, r.name, note));
    }
    s
}

#[test]
fn the_reference_is_up_to_date() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docs/reference/midi.md");
    let want = doc();
    if std::env::var("SIDOU_MIDI_DOC_UPDATE").as_deref() == Ok("1") {
        std::fs::write(&path, &want).unwrap();
        return;
    }
    let got = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        got == want,
        "{} is stale; rerun with SIDOU_MIDI_DOC_UPDATE=1",
        path.display()
    );
}

#[test]
fn midi_learn_leaves_alone_exactly_the_ccs_the_nrpn_receiver_takes() {
    assert_eq!(unlearnable(), [0, 6, 32, 38, 96, 97, 98, 99, 100, 101]);
    // Each of them changes the receiver's state; a learnable one goes to live_cc.
    for cc in 0..128u8 {
        let mut m = Midi::new();
        let before = m.rx.trs_map.ch[0];
        m.trs(&[0xB0, cc, 5]);
        let after = m.rx.trs_map.ch[0];
        let selection = (
            before.nrpn_msb != after.nrpn_msb || before.nrpn_lsb != after.nrpn_lsb,
            before.rpn_msb != after.rpn_msb || before.rpn_lsb != after.rpn_lsb,
        );
        if unlearnable().contains(&u32::from(cc)) && ![0, 6, 32, 38, 96, 97].contains(&cc) {
            assert!(selection.0 || selection.1, "CC {cc} selects nothing");
        }
    }
    assert!(learn_section().contains("except 0, 6, 32, 38, 96, 97, 98, 99, 100 and 101"));
}

#[test]
fn the_reference_names_every_row() {
    let d = doc();
    for r in rows() {
        assert!(d.contains(&format!("| {} |", r.name)), "{}", r.name);
        if r.has_cc() {
            assert!(d.contains(&format!("| {} | {} {}", r.cc, r.group, r.name)));
        }
    }
}

#[test]
fn the_rows_without_a_cc_are_the_ones_the_reference_names() {
    let mut no_cc: Vec<u32> = rows()
        .iter()
        .filter(|r| !r.has_cc())
        .map(|r| u32::from(r.param))
        .collect();
    no_cc.sort_unstable();
    let mut expected = vec![
        LIVE_PARAM_BEND_RANGE,
        LIVE_PARAM_VELOCITY_TARGET,
        LIVE_PARAM_MOD_TARGET,
        LIVE_PARAM_PRESSURE_TARGET,
    ];
    expected.extend(LIVE_PARAM_EXPERT..LIVE_PARAM_EXPERT + SID_EXPERT_COUNT);
    assert_eq!(no_cc, expected);
    assert!(intro().contains("expert layer also have a fixed CC"));
}
