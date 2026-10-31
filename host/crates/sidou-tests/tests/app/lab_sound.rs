// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::{preset, preset_name, Rig};

const BYTES: usize = LAB_SOUND_BYTES as usize;
pub(crate) type Sound = [u8; BYTES];

fn apply(rig: &mut Rig, s: &Sound) {
    unsafe { live_sound_apply(&mut *rig.live, s.as_ptr()) };
}

pub(crate) fn valid(s: &Sound) -> bool {
    unsafe { lab_sound_valid(s.as_ptr()) != 0 }
}

fn edited(rig: &Rig) -> bool {
    unsafe { live_preset_edited(&*rig.live) != 0 }
}

pub(crate) fn u16_at(s: &Sound, at: u32) -> u32 {
    u32::from(s[at as usize]) | u32::from(s[at as usize + 1]) << 8
}

pub(crate) fn put16(s: &mut Sound, at: u32, v: u32) {
    s[at as usize] = v as u8;
    s[at as usize + 1] = (v >> 8) as u8;
}

fn base_name(rig: &Rig) -> String {
    let n: Vec<u8> = rig
        .live
        .base_name
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(n).unwrap()
}

pub(crate) struct Xs(pub u32);

impl Xs {
    pub fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    pub fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

pub(crate) fn random_sound(r: &mut Xs) -> Sound {
    let mut s = [0u8; BYTES];
    s[LAB_SOUND_CONTROL as usize] = (r.below(16) << 4 | r.below(2) << 2 | r.below(2) << 1) as u8;
    s[LAB_SOUND_AD as usize] = r.below(256) as u8;
    s[LAB_SOUND_SR as usize] = r.below(256) as u8;
    put16(&mut s, LAB_SOUND_PULSE, r.below(4096));
    put16(&mut s, LAB_SOUND_CUTOFF, r.below(2048));
    s[LAB_SOUND_RESONANCE as usize] = r.below(16) as u8;
    s[LAB_SOUND_FILTER_VOICES as usize] = r.below(8) as u8;
    s[LAB_SOUND_FILTER_MODE as usize] = (r.below(8) << 4) as u8;
    s[LAB_SOUND_MONO_SYNC as usize] = r.below(2) as u8;
    put16(
        &mut s,
        LAB_SOUND_SYNC_BASE,
        r.below(LAB_SOUND_SYNC_BASE_MAX + 1),
    );
    s[LAB_SOUND_SYNC_SEMIS as usize] = (r.below(25) as i32 - 12) as i8 as u8;
    put16(&mut s, LAB_SOUND_PWM_DEPTH, r.below(LIVE_PWM_DEPTH_MAX + 1));
    put16(&mut s, LAB_SOUND_PWM_RATE, r.below(LIVE_PWM_STEP_MAX + 1));
    s[LAB_SOUND_TUNE as usize] = (r.below(101) as i32 - 50) as i8 as u8;
    s[LAB_SOUND_BEND_RANGE as usize] = r.below(LIVE_BEND_RANGE_MAX + 1) as u8;
    s[LAB_SOUND_VELOCITY as usize] = r.below(4) as u8;
    s[LAB_SOUND_MOD as usize] = r.below(3) as u8;
    s[LAB_SOUND_PRESSURE as usize] = r.below(4) as u8;
    for f in [
        LAB_SOUND_VIBRATO_RATE,
        LAB_SOUND_VIBRATO_DEPTH,
        LAB_SOUND_VIBRATO_DELAY,
    ] {
        s[f as usize] = r.below(128) as u8;
    }
    let arp = LAB_SOUND_ARP as usize;
    s[arp + LIVE_ARP_ON as usize] = r.below(2) as u8;
    s[arp + LIVE_ARP_RATE as usize] = r.below(LIVE_ARP_RATES) as u8;
    s[arp + LIVE_ARP_ORDER as usize] = r.below(4) as u8;
    s[arp + LIVE_ARP_SPAN as usize] = 1 + r.below(LIVE_ARP_SPAN_MAX) as u8;
    for k in 0..LIVE_SLOTS {
        let at = (LAB_SOUND_SLOTS + k * LIVE_SLOT_FIELDS) as usize;
        s[at] = r.below(unsafe { live_slot_algorithms(k) }) as u8;
        for f in 1..LIVE_SLOT_FIELDS as usize {
            s[at + f] = r.below(LIVE_SLOT_MAX + 1) as u8;
        }
    }
    s[LAB_SOUND_FILTER_LFO as usize] = r.below(LIVE_FILTER_LFO_MAX + 1) as u8;
    for f in [LAB_SOUND_RESO_LINK, LAB_SOUND_PWM_LINK] {
        s[f as usize] = (r.below(2 * LIVE_LINK_MAX + 1) as i32 - LIVE_LINK_MAX as i32) as i8 as u8;
    }
    assert!(valid(&s), "{s:?}");
    s
}

#[test]
fn a_sound_round_trips_through_live() {
    let mut sounds = Vec::new();
    for p in 0..LIVE_PRESET_COUNT as u8 {
        let mut rig = Rig::new(SID_MODEL_6581);
        rig.set_preset(p);
        sounds.push(rig.sound());
    }
    let mut rig = Rig::new(SID_MODEL_6581);
    unsafe { live_slot_pick(&mut *rig.live, 1, 2) };
    for seed in [1u32, 7, 99, 12_345, 0xdead_beef] {
        unsafe { live_dice_roll(&mut *rig.live, seed) };
        sounds.push(rig.sound());
    }
    let mut r = Xs(0x1234_5678);
    for _ in 0..200 {
        sounds.push(random_sound(&mut r));
    }
    for (i, s) in sounds.iter().enumerate() {
        assert!(valid(s), "sound {i} is valid");
        let mut fresh = Rig::new(SID_MODEL_6581);
        apply(&mut fresh, s);
        assert_eq!(&fresh.sound(), s, "sound {i} through a fresh instrument");
        let mut busy = Rig::new(SID_MODEL_8580);
        apply(&mut busy, &sounds[(i + 3) % sounds.len()]);
        busy.note_on(60);
        busy.note_on(64);
        apply(&mut busy, s);
        assert_eq!(&busy.sound(), s, "sound {i} over another");
    }
}

#[test]
fn the_base_is_the_preset_as_loaded() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert_eq!(rig.live.base, rig.sound(), "SAW LEAD at power-on");
    unsafe { live_slot_pick(&mut *rig.live, 0, 3) };
    let slots = rig.live.slot;
    for p in 0..LIVE_PRESET_COUNT as u8 {
        rig.set_preset(p);
        let f = preset(p);
        let b = rig.live.base;
        assert_eq!(base_name(&rig), preset_name(f));
        assert_eq!(
            (
                b[LAB_SOUND_CONTROL as usize],
                b[LAB_SOUND_AD as usize],
                b[LAB_SOUND_SR as usize]
            ),
            (f.control, f.attack_decay, f.sustain_release)
        );
        assert_eq!(u16_at(&b, LAB_SOUND_PULSE), u32::from(f.pulse_width));
        assert_eq!(u16_at(&b, LAB_SOUND_CUTOFF), u32::from(f.cutoff));
        assert_eq!(
            (
                b[LAB_SOUND_RESONANCE as usize],
                b[LAB_SOUND_FILTER_VOICES as usize],
                b[LAB_SOUND_FILTER_MODE as usize],
                b[LAB_SOUND_MONO_SYNC as usize],
            ),
            (f.resonance, f.filter_voices, f.filter_mode, f.mono_sync)
        );
        assert_eq!(u16_at(&b, LAB_SOUND_SYNC_BASE), u32::from(f.sync_ratio_q8));
        assert_eq!(u16_at(&b, LAB_SOUND_PWM_DEPTH), u32::from(f.pwm_depth));
        assert_eq!(u16_at(&b, LAB_SOUND_PWM_RATE), u32::from(f.pwm_rate));
        assert_eq!(
            (b[LAB_SOUND_SYNC_SEMIS as usize], b[LAB_SOUND_TUNE as usize]),
            (0, 0)
        );
        assert_eq!(
            b[LAB_SOUND_BEND_RANGE as usize..=LAB_SOUND_VIBRATO_DELAY as usize],
            [
                f.bend_range,
                f.velocity_target,
                f.mod_target,
                f.pressure_target,
                f.vibrato_rate,
                f.vibrato_depth,
                f.vibrato_delay
            ]
        );
        assert_eq!(
            b[LAB_SOUND_ARP as usize..LAB_SOUND_SLOTS as usize],
            [0, LIVE_ARP_RATE_DEFAULT as u8, LIVE_ARP_UP as u8, 1]
        );
        assert_eq!(
            &b[LAB_SOUND_SLOTS as usize..LAB_SOUND_BYTES_1_0 as usize],
            slots.as_flattened(),
            "a built-in has no slots of its own: they stay as they are"
        );
        assert_eq!(
            b[LAB_SOUND_BYTES_1_0 as usize..],
            [0, 0, 0],
            "no filter LFO, no links"
        );
        assert_eq!(b, rig.sound(), "loaded, so not edited");
        assert!(!edited(&rig));
        assert!(valid(&b));
    }
    unsafe { live_bend_range_rpn(&mut *rig.live, 12) };
    rig.set_preset(0);
    assert_eq!(
        rig.live.base[LAB_SOUND_BEND_RANGE as usize],
        preset(0).bend_range,
        "a bend range set by RPN 0 is the channel's, not the sound's"
    );
    assert!(!edited(&rig));
    rig.preset_step(4);
    assert_eq!(rig.live.preset, LIVE_PRESET_DICE as u8);
    assert_eq!(rig.live.base, rig.sound(), "DICE's base is its roll");
    assert_eq!(base_name(&rig), "DICE");
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_RESONANCE, 15) };
    let left = rig.sound();
    rig.preset_step(-1);
    rig.preset_step(1);
    assert_eq!(
        rig.live.base, left,
        "DICE's base is the roll as it was left"
    );
}

fn one_field_changes() -> Vec<(&'static str, Sound)> {
    let rig = Rig::new(SID_MODEL_6581);
    let base = rig.sound();
    let mut out = Vec::new();
    let mut with = |name: &'static str, f: &dyn Fn(&mut Sound)| {
        let mut s = base;
        f(&mut s);
        assert_ne!(s, base, "{name} changes");
        assert!(valid(&s), "{name} is valid");
        out.push((name, s));
    };
    with("waveform", &|s| s[LAB_SOUND_CONTROL as usize] = 0x40);
    with("ring", &|s| s[LAB_SOUND_CONTROL as usize] |= 0x04);
    with("sync", &|s| s[LAB_SOUND_CONTROL as usize] |= 0x02);
    with("attack", &|s| s[LAB_SOUND_AD as usize] = 0x19);
    with("decay", &|s| s[LAB_SOUND_AD as usize] = 0x0a);
    with("sustain", &|s| s[LAB_SOUND_SR as usize] = 0xb6);
    with("release", &|s| s[LAB_SOUND_SR as usize] = 0xa7);
    with("pulse width", &|s| put16(s, LAB_SOUND_PULSE, 1000));
    with("cutoff", &|s| put16(s, LAB_SOUND_CUTOFF, 1000));
    with("resonance", &|s| s[LAB_SOUND_RESONANCE as usize] = 7);
    with("filter voices", &|s| {
        s[LAB_SOUND_FILTER_VOICES as usize] = 3
    });
    with("filter mode", &|s| s[LAB_SOUND_FILTER_MODE as usize] = 0x20);
    with("mono sync", &|s| s[LAB_SOUND_MONO_SYNC as usize] = 1);
    with("sync base", &|s| put16(s, LAB_SOUND_SYNC_BASE, 512));
    with("sync up", &|s| s[LAB_SOUND_SYNC_SEMIS as usize] = 3);
    with("sync down", &|s| {
        s[LAB_SOUND_SYNC_SEMIS as usize] = -3i8 as u8
    });
    with("pwm depth", &|s| put16(s, LAB_SOUND_PWM_DEPTH, 100));
    with("pwm rate", &|s| put16(s, LAB_SOUND_PWM_RATE, 300));
    with("tune up", &|s| s[LAB_SOUND_TUNE as usize] = 10);
    with("tune down", &|s| s[LAB_SOUND_TUNE as usize] = -10i8 as u8);
    with("bend range", &|s| s[LAB_SOUND_BEND_RANGE as usize] = 5);
    with("velocity", &|s| s[LAB_SOUND_VELOCITY as usize] = 1);
    with("mod", &|s| s[LAB_SOUND_MOD as usize] = 2);
    with("pressure", &|s| s[LAB_SOUND_PRESSURE as usize] = 3);
    with("vibrato rate", &|s| s[LAB_SOUND_VIBRATO_RATE as usize] = 10);
    with("vibrato depth", &|s| {
        s[LAB_SOUND_VIBRATO_DEPTH as usize] = 20
    });
    with("vibrato delay", &|s| {
        s[LAB_SOUND_VIBRATO_DELAY as usize] = 30
    });
    with("arp on", &|s| s[(LAB_SOUND_ARP + LIVE_ARP_ON) as usize] = 1);
    with("arp rate", &|s| {
        s[(LAB_SOUND_ARP + LIVE_ARP_RATE) as usize] = 4
    });
    with("arp order", &|s| {
        s[(LAB_SOUND_ARP + LIVE_ARP_ORDER) as usize] = 2
    });
    with("arp span", &|s| {
        s[(LAB_SOUND_ARP + LIVE_ARP_SPAN) as usize] = 2
    });
    const SLOT_FIELD: [&str; 15] = [
        "slot 1 algorithm",
        "slot 1 amount",
        "slot 1 a",
        "slot 1 b",
        "slot 1 mix",
        "slot 2 algorithm",
        "slot 2 amount",
        "slot 2 a",
        "slot 2 b",
        "slot 2 mix",
        "slot 3 algorithm",
        "slot 3 amount",
        "slot 3 a",
        "slot 3 b",
        "slot 3 mix",
    ];
    for (i, name) in SLOT_FIELD.iter().enumerate() {
        with(name, &|s| s[LAB_SOUND_SLOTS as usize + i] = 1);
    }
    out
}

#[test]
fn an_edit_marks_the_sound_and_a_revert_clears_it() {
    let mut rig = Rig::new(SID_MODEL_6581);
    let base = rig.live.base;
    for (name, s) in one_field_changes() {
        apply(&mut rig, &s);
        assert!(edited(&rig), "{name} marks the sound");
        assert_eq!(rig.live.base, base, "{name}: the base stays");
        apply(&mut rig, &base);
        assert!(!edited(&rig), "{name} put back clears the mark");
    }
    unsafe { live_slot_pick(&mut *rig.live, 2, 1) };
    assert!(edited(&rig), "a slot change marks the sound");
    rig.set_preset(1);
    assert!(
        !edited(&rig),
        "a built-in's load leaves the slots as they are"
    );
    assert_eq!(rig.live.slot[2][LIVE_SLOT_ALGORITHM as usize], 1);
    rig.knob(LIVE_KNOB_RESONANCE, 2);
    assert!(edited(&rig));
    let kept = rig.sound();
    let name = c"KEPT ONE";
    unsafe { live_sound_set_base(&mut *rig.live, kept.as_ptr(), name.as_ptr()) };
    assert!(!edited(&rig), "setting the base moves the mark");
    assert_eq!(base_name(&rig), "KEPT ONE");
    assert_eq!(rig.sound(), kept, "nothing sounds different");
    let mut buf = [0 as std::ffi::c_char; LIVE_PRESET_NAME_CHARS as usize];
    unsafe { live_preset_name(&*rig.live, buf.as_mut_ptr()) };
    let shown = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
    assert_eq!(shown.to_str().unwrap(), "KEPT ONE");
    unsafe { live_sound_set_base(&mut *rig.live, kept.as_ptr(), std::ptr::null()) };
    assert_eq!(base_name(&rig), "KEPT ONE", "a NULL name keeps the name");
    let long = c"ABCDEFGHIJKLMNOP";
    unsafe { live_sound_set_base(&mut *rig.live, kept.as_ptr(), long.as_ptr()) };
    assert_eq!(
        base_name(&rig),
        "ABCDEFGHIJKL",
        "a long name is cut to LAB_SOUND_NAME characters"
    );
}

#[test]
fn pwm_and_tune_are_kept_per_preset_as_the_knobs_are() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.preset_step(1);
    assert_eq!(rig.live.preset, 1, "PWM BASS");
    let p = preset(1);
    assert_eq!(
        (rig.live.pwm_depth, rig.live.pwm_rate, rig.live.tune),
        (p.pwm_depth, p.pwm_rate, 0)
    );
    let mut s = rig.sound();
    put16(&mut s, LAB_SOUND_PWM_DEPTH, 1500);
    put16(&mut s, LAB_SOUND_PWM_RATE, 655);
    s[LAB_SOUND_TUNE as usize] = -20i8 as u8;
    apply(&mut rig, &s);
    assert!(edited(&rig));
    rig.preset_step(1);
    assert_eq!(
        (rig.live.pwm_depth, rig.live.pwm_rate, rig.live.tune),
        (0, 0, 0),
        "SYNC LEAD has none of its own"
    );
    assert_eq!(
        (rig.live.mono_sync, rig.live.sync_base_q8),
        (1, preset(2).sync_ratio_q8)
    );
    rig.preset_step(-1);
    assert_eq!(
        (rig.live.pwm_depth, rig.live.pwm_rate, rig.live.tune),
        (1500, 655, -20),
        "PWM BASS's edits come back with it"
    );
    assert!(edited(&rig));
    rig.set_preset(1);
    assert_eq!(
        (rig.live.pwm_depth, rig.live.pwm_rate, rig.live.tune),
        (p.pwm_depth, p.pwm_rate, 0),
        "a program change loads the factory's over the kept ones"
    );
    assert!(!edited(&rig));
    put16(&mut s, LAB_SOUND_PWM_RATE, 1000);
    apply(&mut rig, &s);
    let phase = rig.live.lfo_phase;
    rig.tick();
    assert_eq!(
        rig.live.lfo_phase,
        phase.wrapping_add(1000),
        "a tick moves the LFO's phase by the edited rate"
    );
}

#[test]
fn a_fine_tune_moves_the_frequency_by_cents() {
    let at = |note: u8, offset: i32| unsafe { live_freq_at(note, offset) };
    for (tune, offset) in [(0i8, 0), (50, 4096), (-50, -4096), (1, 81), (-1, -81)] {
        let mut rig = Rig::new(SID_MODEL_6581);
        let mut s = rig.sound();
        s[LAB_SOUND_TUNE as usize] = tune as u8;
        apply(&mut rig, &s);
        rig.note_on(60);
        assert_eq!(rig.voice_freq(0), at(60, offset), "{tune} cents");
        let ratio = f64::from(rig.voice_freq(0)) / f64::from(at(60, 0));
        let cents = 1200.0 * ratio.log2();
        assert!(
            (cents - f64::from(tune)).abs() < 0.5,
            "{tune} cents sounds {cents:.2}"
        );
        rig.tick();
        assert_eq!(
            rig.voice_freq(0),
            at(60, offset),
            "{tune} cents hold through the main loop's frequency writes"
        );
    }
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.note_on(69);
    let mut s = rig.sound();
    s[LAB_SOUND_TUNE as usize] = 25;
    apply(&mut rig, &s);
    assert_eq!(
        rig.voice_freq(0),
        at(69, 2048),
        "a tune turned under a sounding note moves it at once"
    );
    rig.set_preset(2);
    let mut s = rig.sound();
    s[LAB_SOUND_TUNE as usize] = -50i8 as u8;
    apply(&mut rig, &s);
    rig.note_on(60);
    assert_eq!(rig.voice_freq(2), at(60, -4096));
    assert_eq!(
        u32::from(rig.voice_freq(0)),
        (u32::from(at(60, -4096)) * u32::from(rig.live.sync_q8)) >> 8,
        "SYNC LEAD's two voices move together"
    );
}

#[test]
fn an_invalid_sound_is_refused() {
    for p in 0..LIVE_PRESET_SLOTS as u8 {
        let mut rig = Rig::new(SID_MODEL_6581);
        rig.set_preset(p);
        assert!(valid(&rig.live.base), "preset {p}");
    }
    let base = Rig::new(SID_MODEL_6581).live.base;
    let bad = |name: &str, f: &dyn Fn(&mut Sound)| {
        let mut s = base;
        f(&mut s);
        assert!(!valid(&s), "{name} is refused");
    };
    bad("the gate", &|s| s[LAB_SOUND_CONTROL as usize] |= 0x01);
    bad("filter LFO", &|s| {
        s[LAB_SOUND_FILTER_LFO as usize] = LIVE_FILTER_LFO_MAX as u8 + 1
    });
    bad("reso link up", &|s| {
        s[LAB_SOUND_RESO_LINK as usize] = LIVE_LINK_MAX as u8 + 1
    });
    bad("pwm link down", &|s| {
        s[LAB_SOUND_PWM_LINK as usize] = (-(LIVE_LINK_MAX as i8) - 1) as u8
    });
    bad("the test bit", &|s| s[LAB_SOUND_CONTROL as usize] |= 0x08);
    bad("pulse width", &|s| put16(s, LAB_SOUND_PULSE, 4096));
    bad("cutoff", &|s| put16(s, LAB_SOUND_CUTOFF, 2048));
    bad("resonance", &|s| s[LAB_SOUND_RESONANCE as usize] = 16);
    bad("filter voices", &|s| {
        s[LAB_SOUND_FILTER_VOICES as usize] = 8
    });
    bad("filter mode high", &|s| {
        s[LAB_SOUND_FILTER_MODE as usize] = 0x80
    });
    bad("filter mode low", &|s| {
        s[LAB_SOUND_FILTER_MODE as usize] = 0x11
    });
    bad("mono sync", &|s| s[LAB_SOUND_MONO_SYNC as usize] = 2);
    bad("sync base", &|s| {
        put16(s, LAB_SOUND_SYNC_BASE, LAB_SOUND_SYNC_BASE_MAX + 1)
    });
    bad("sync up", &|s| s[LAB_SOUND_SYNC_SEMIS as usize] = 13);
    bad("sync down", &|s| {
        s[LAB_SOUND_SYNC_SEMIS as usize] = -13i8 as u8
    });
    bad("pwm depth", &|s| {
        put16(s, LAB_SOUND_PWM_DEPTH, LIVE_PWM_DEPTH_MAX + 1)
    });
    bad("pwm rate", &|s| {
        put16(s, LAB_SOUND_PWM_RATE, LIVE_PWM_STEP_MAX + 1)
    });
    bad("tune up", &|s| s[LAB_SOUND_TUNE as usize] = 51);
    bad("tune down", &|s| s[LAB_SOUND_TUNE as usize] = -51i8 as u8);
    bad("bend range", &|s| {
        s[LAB_SOUND_BEND_RANGE as usize] = LIVE_BEND_RANGE_MAX as u8 + 1
    });
    bad("velocity", &|s| s[LAB_SOUND_VELOCITY as usize] = 4);
    bad("mod", &|s| s[LAB_SOUND_MOD as usize] = 3);
    bad("pressure", &|s| s[LAB_SOUND_PRESSURE as usize] = 4);
    bad("vibrato rate", &|s| {
        s[LAB_SOUND_VIBRATO_RATE as usize] = 128
    });
    bad("vibrato depth", &|s| {
        s[LAB_SOUND_VIBRATO_DEPTH as usize] = 128
    });
    bad("vibrato delay", &|s| {
        s[LAB_SOUND_VIBRATO_DELAY as usize] = 128
    });
    bad("arp on", &|s| s[(LAB_SOUND_ARP + LIVE_ARP_ON) as usize] = 2);
    bad("arp rate", &|s| {
        s[(LAB_SOUND_ARP + LIVE_ARP_RATE) as usize] = LIVE_ARP_RATES as u8
    });
    bad("arp order", &|s| {
        s[(LAB_SOUND_ARP + LIVE_ARP_ORDER) as usize] = 4
    });
    bad("arp span 0", &|s| {
        s[(LAB_SOUND_ARP + LIVE_ARP_SPAN) as usize] = 0
    });
    bad("arp span 3", &|s| {
        s[(LAB_SOUND_ARP + LIVE_ARP_SPAN) as usize] = 3
    });
    for k in 0..LIVE_SLOTS {
        let at = (LAB_SOUND_SLOTS + k * LIVE_SLOT_FIELDS) as usize;
        let n = unsafe { live_slot_algorithms(k) } as u8;
        bad("a slot's algorithm", &|s| s[at] = n);
        for f in 1..LIVE_SLOT_FIELDS as usize {
            bad("a slot's control", &|s| s[at + f] = LIVE_SLOT_MAX as u8 + 1);
        }
    }
    let mut other = base;
    let equal = |a: &Sound, b: &Sound| unsafe { lab_sound_equal(a.as_ptr(), b.as_ptr()) } != 0;
    assert!(equal(&base, &other));
    other[BYTES - 1] ^= 1;
    assert!(!equal(&base, &other), "equal is byte for byte");
}

#[test]
fn pwm_rates_step_by_a_twentieth_of_a_hertz_so_pwm_bass_is_rate_10() {
    assert_eq!(unsafe { live_pwm_step(10) }, 328, "rate 10 is a 2 s cycle");
    assert_eq!(u32::from(preset(1).pwm_rate), unsafe { live_pwm_step(10) });
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.preset_step(1);
    let get = |rig: &Rig, p| unsafe { live_param_get(&*rig.live, p) };
    assert_eq!(get(&rig, LIVE_PARAM_PWM_RATE), 10);
    assert_eq!(
        get(&rig, LIVE_PARAM_PWM_DEPTH),
        u32::from(preset(1).pwm_depth)
    );
    assert_eq!(get(&rig, LIVE_PARAM_TUNE), LIVE_TUNE_CENTS, "in tune");
    for r in 0..=LIVE_PWM_RATE_MAX {
        assert_eq!(unsafe { live_pwm_rate(live_pwm_step(r)) }, r, "rate {r}");
    }
    assert_eq!(
        unsafe { live_pwm_step(LIVE_PWM_RATE_MAX) },
        LIVE_PWM_STEP_MAX,
        "the top rate is the sound's largest step"
    );
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_PWM_RATE, 20) };
    assert_eq!(rig.live.pwm_rate, 655);
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_PWM_DEPTH, 2048) };
    assert_eq!(rig.live.pwm_depth, 2048);
    rig.note_on(60);
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_TUNE, 100) };
    assert_eq!(rig.live.tune, 50);
    assert_eq!(rig.voice_freq(0), unsafe { live_freq_at(60, 4096) });
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_TUNE, 0) };
    assert_eq!(rig.voice_freq(0), unsafe { live_freq_at(60, -4096) });
    assert!(edited(&rig));
}
