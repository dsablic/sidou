// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::{c_char, c_void};

use sidou_sys::*;

use crate::{
    c_text, preset, record, shown_preset_name, Recorder, Rig, NOISE_PERC, PWM_BASS, SAW_LEAD,
    SYNC_LEAD,
};

pub(crate) const C4: u8 = 60;
const SOUND: usize = LAB_SOUND_BYTES as usize;
const NAME: usize = LAB_SOUND_NAME as usize + 1;

pub(crate) type Sound = [u8; SOUND];

#[derive(Default)]
pub(crate) struct Users(pub(crate) Vec<Option<(String, Sound)>>);

unsafe extern "C" fn read_user(
    ctx: *mut c_void,
    slot: u32,
    name: *mut c_char,
    sound: *mut u8,
) -> i32 {
    let users = unsafe { &*ctx.cast::<Users>() };
    let Some(Some((n, s))) = users.0.get(slot as usize) else {
        return 0;
    };
    let name = unsafe { std::slice::from_raw_parts_mut(name.cast::<u8>(), NAME) };
    name.fill(0);
    name[..n.len()].copy_from_slice(n.as_bytes());
    unsafe { std::slice::from_raw_parts_mut(sound, SOUND) }.copy_from_slice(s);
    1
}

#[derive(Default)]
pub(crate) struct Taken(pub(crate) Vec<u32>);

unsafe extern "C" fn taking(ctx: *mut c_void, voices: u32) {
    unsafe { &mut *ctx.cast::<Taken>() }.0.push(voices);
}

pub(crate) struct Parts {
    pub(crate) m: Box<multi>,
    pub(crate) chip: Box<Recorder>,
    pub(crate) kr: Box<keys_route>,
    pub(crate) users: Box<Users>,
    pub(crate) taken: Box<Taken>,
}

impl Parts {
    pub(crate) fn new(l: &live) -> Self {
        let mut p = Parts {
            m: Box::default(),
            chip: Box::default(),
            kr: Box::default(),
            users: Box::new(Users(vec![None; LIVE_SOUNDS as usize])),
            taken: Box::default(),
        };
        let chip: *mut Recorder = &mut *p.chip;
        unsafe { multi_init(&mut *p.m, Some(record), chip.cast(), l, &*p.kr) };
        p.m.user = Some(read_user);
        p.m.user_ctx = (&mut *p.users as *mut Users).cast();
        p.m.taking = Some(taking);
        p.m.taking_ctx = (&mut *p.taken as *mut Taken).cast();
        p
    }

    pub(crate) fn reg(&self, reg: u8) -> u8 {
        self.chip.regs[usize::from(reg)]
    }

    pub(crate) fn freq(&self, v: u8) -> u16 {
        u16::from(self.reg(v * 7)) | u16::from(self.reg(v * 7 + 1)) << 8
    }

    pub(crate) fn pw(&self, v: u8) -> u16 {
        u16::from(self.reg(v * 7 + 2)) | u16::from(self.reg(v * 7 + 3)) << 8
    }

    pub(crate) fn gated(&self, v: u8) -> bool {
        self.reg(v * 7 + 4) & 1 != 0
    }

    pub(crate) fn cutoff(&self) -> u16 {
        u16::from(self.reg(0x15) & 7) | u16::from(self.reg(0x16)) << 3
    }

    pub(crate) fn writes_to(&self, v: u8) -> usize {
        self.chip
            .log
            .iter()
            .filter(|w| w.0 / 7 == v && w.0 < 0x15)
            .count()
    }

    pub(crate) fn filter_writes(&self) -> usize {
        self.chip.log.iter().filter(|w| w.0 >= 0x15).count()
    }

    pub(crate) fn name(&self, part: usize) -> String {
        c_text(self.m.p[part].s.name.as_ptr())
    }

    pub(crate) fn save_user(&mut self, rig: &mut Rig, slot: usize, name: &str, s: Sound) {
        self.users.0[slot] = Some((name.to_owned(), s));
        rig.live.sounds_valid |= 1 << slot;
    }

    fn program(&mut self, part: u32, program: u32) -> bool {
        unsafe { multi_program(&mut *self.m, part, program) != 0 }
    }

    fn on(&mut self, part: u32, note: u8, velocity: u8) {
        unsafe { multi_note_on(&mut *self.m, part, note, velocity, 0) }
    }

    fn off(&mut self, part: u32) {
        unsafe { multi_note_off(&mut *self.m, part) }
    }

    fn tick(&mut self) {
        unsafe { multi_tick(&mut *self.m) }
    }
}

pub(crate) fn freq_at(note: u8, offset: i32) -> u16 {
    unsafe { live_freq_at(note, offset) }
}

pub(crate) fn preset_sound(index: u32) -> Sound {
    let mut s = [0u8; SOUND];
    unsafe { live_preset_sound(index, s.as_mut_ptr()) };
    s
}

fn rig_parts() -> (Rig, Parts) {
    let rig = Rig::new(SID_MODEL_6581);
    let parts = Parts::new(&rig.live);
    (rig, parts)
}

#[test]
fn a_preset_as_a_sound_is_the_instruments_preset_byte_for_byte() {
    let mut rig = Rig::new(SID_MODEL_6581);
    for i in [SAW_LEAD, PWM_BASS, SYNC_LEAD, NOISE_PERC] {
        rig.set_preset(i);
        assert_eq!(preset_sound(u32::from(i)), rig.sound(), "preset {i}");
    }
}

#[test]
fn the_parts_start_as_saw_lead_pwm_bass_and_noise_perc_and_sound_nothing() {
    let (_, p) = rig_parts();
    assert_eq!(
        [p.name(0), p.name(1), p.name(2)],
        ["SAW LEAD", "PWM BASS", "NOISE PERC"]
    );
    assert!(p.chip.log.is_empty(), "nothing written before MIDI plays");
}

#[test]
fn a_part_plays_its_sound_on_its_own_voice_alone() {
    let (_, mut p) = rig_parts();
    p.on(1, C4, 127);
    let bass = preset(PWM_BASS);
    assert_eq!(
        p.writes_to(0) + p.writes_to(2),
        0,
        "the other voices untouched"
    );
    assert_eq!(p.reg(7 + 4), bass.control | 1);
    assert_eq!(p.reg(7 + 5), bass.attack_decay);
    assert_eq!(p.reg(7 + 6), bass.sustain_release);
    assert_eq!(p.freq(1), freq_at(C4, 0));
    let pw = p.pw(1);
    for _ in 0..10 {
        p.tick();
    }
    assert_ne!(p.pw(1), pw, "its pulse width modulation runs");
    p.off(1);
    assert!(!p.gated(1));
    assert_eq!(p.reg(7 + 4), bass.control, "the release keeps the waveform");
}

#[test]
fn hard_sync_and_ring_need_a_second_voice_so_a_part_plays_without_them() {
    let (_, mut p) = rig_parts();
    assert!(p.program(0, u32::from(SYNC_LEAD)));
    p.on(0, C4, 127);
    assert_eq!(preset(SYNC_LEAD).control & 0x06, 0x02, "the preset syncs");
    assert_eq!(p.reg(4), (preset(SYNC_LEAD).control & !0x06) | 1);
}

#[test]
fn program_4_copies_the_instruments_sound_as_it_is_now() {
    let (mut rig, mut p) = rig_parts();
    rig.set_preset(PWM_BASS);
    rig.set_param(LIVE_PARAM_ATTACK, 9);
    assert!(p.program(2, MULTI_PROGRAM_KEYS));
    assert_eq!(p.name(2), shown_preset_name(&rig.live));
    rig.set_param(LIVE_PARAM_ATTACK, 2);
    p.on(2, C4, 127);
    assert_eq!(p.reg(14 + 5) >> 4, 9, "a copy, not a link");
}

#[test]
fn user_programs_play_their_slot_and_an_empty_slot_or_a_gap_changes_nothing() {
    let (mut rig, mut p) = rig_parts();
    let mut s = preset_sound(0);
    s[LAB_SOUND_AD as usize] = 0x4c;
    p.save_user(&mut rig, 3, "MY PAD", s);
    assert!(p.program(0, MULTI_PROGRAM_USER + 3));
    assert_eq!(p.name(0), "MY PAD");
    for program in [MULTI_PROGRAM_USER + 4, 5, 15, MULTI_PROGRAMS] {
        assert!(!p.program(0, program), "program {program}");
        assert_eq!(p.name(0), "MY PAD");
    }
    p.on(0, C4, 127);
    assert_eq!(p.reg(5), 0x4c);
}

#[test]
fn stepping_a_parts_program_walks_the_presets_the_keys_and_the_saved_user_sounds() {
    let (mut rig, mut p) = rig_parts();
    p.save_user(&mut rig, 2, "A", preset_sound(0));
    p.save_user(&mut rig, 9, "B", preset_sound(1));
    let step = |from: u32, by: i32| unsafe { multi_program_step(&*p.m, from, by) };
    let walk: Vec<u32> = (0..8).map(|n| step(0, n)).collect();
    let u = MULTI_PROGRAM_USER;
    assert_eq!(walk, [0, 1, 2, 3, 4, u + 2, u + 9, u + 9]);
    assert_eq!(step(u + 9, -2), 4);
    assert_eq!(step(0, -1), 0);
}

#[test]
fn velocity_scales_the_sustain_where_the_sound_asks() {
    let (mut rig, mut p) = rig_parts();
    let mut s = preset_sound(0);
    s[LAB_SOUND_VELOCITY as usize] = LIVE_VELOCITY_SUSTAIN as u8;
    s[LAB_SOUND_SR as usize] = 0xf6;
    p.save_user(&mut rig, 0, "SOFT", s);
    assert!(p.program(1, MULTI_PROGRAM_USER));
    p.on(1, C4, 127);
    assert_eq!(p.reg(7 + 6), 0xf6);
    p.on(1, C4, 64);
    assert_eq!(p.reg(7 + 6), 0x86);
    p.on(0, C4, 1);
    assert_eq!(
        p.reg(6),
        preset(SAW_LEAD).sustain_release,
        "SAW LEAD ignores velocity"
    );
}

#[test]
fn chip_as_filter_is_channel_ns_sound_and_each_part_routes_its_own_voice() {
    let (mut rig, mut p) = rig_parts();
    let mut dry = preset_sound(0);
    dry[LAB_SOUND_FILTER_VOICES as usize] = 0x06;
    dry[LAB_SOUND_CUTOFF as usize..][..2].copy_from_slice(&300u16.to_le_bytes());
    p.save_user(&mut rig, 0, "DRY", dry);
    p.on(1, C4, 127);
    let lead = preset(SAW_LEAD);
    assert_eq!(p.cutoff(), lead.cutoff);
    assert_eq!(p.reg(0x17), lead.resonance << 4 | 0b111);
    assert_eq!(p.reg(0x18), lead.filter_mode | 0x0f);
    assert!(p.program(1, MULTI_PROGRAM_USER));
    assert_eq!(p.reg(0x17) & 7, 0b101, "N + 1's sound keeps its voice dry");
    assert_eq!(p.cutoff(), lead.cutoff, "only channel N's cutoff counts");
    assert!(p.program(0, MULTI_PROGRAM_USER));
    assert_eq!((p.cutoff(), p.reg(0x17) & 7), (300, 0b100));
    unsafe {
        multi_filter(
            &mut *p.m,
            MULTI_LOCK_CUTOFF | MULTI_LOCK_RESONANCE,
            2047,
            15,
        )
    };
    assert_eq!(
        (p.cutoff(), p.reg(0x17) >> 4),
        (2047, 15),
        "CC 74 and 71 lock it"
    );
    unsafe { multi_filter(&mut *p.m, 0, 0, 0) };
    assert_eq!(p.cutoff(), 300, "until CC 121");
    let mut open = preset_sound(0);
    open[LAB_SOUND_FILTER_MODE as usize] = 0;
    p.save_user(&mut rig, 1, "OPEN", open);
    assert!(p.program(0, MULTI_PROGRAM_USER + 1));
    assert_eq!(
        (p.reg(0x17) & 7, p.reg(0x18)),
        (0, 0x0f),
        "no filter type: no voice routed into silence"
    );
}

#[test]
fn the_wheel_and_aftertouch_add_vibrato_to_their_own_part_only() {
    let (_, mut p) = rig_parts();
    for v in 0..3 {
        p.on(v, C4, 127);
    }
    unsafe {
        multi_wheel(&mut *p.m, 0, 127);
        multi_pressure(&mut *p.m, 2, 127);
    }
    for _ in 0..4 {
        p.tick();
    }
    let still = freq_at(C4, 0);
    assert_ne!(p.freq(0), still, "the wheel shakes N");
    assert_eq!(p.freq(1), still);
    assert_ne!(p.freq(2), still, "aftertouch shakes N + 2");
    unsafe { multi_reset_controllers(&mut *p.m, 0) };
    for _ in 0..2 {
        p.tick();
    }
    assert_eq!(p.freq(0), still, "CC 121 stops it");
}

#[test]
fn a_parts_bend_and_its_sounds_tuning_move_its_pitch() {
    let (mut rig, mut p) = rig_parts();
    let mut up = preset_sound(0);
    up[LAB_SOUND_LAYER_TUNING as usize] = 12;
    up[LAB_SOUND_TUNE as usize] = 10;
    p.save_user(&mut rig, 0, "UP", up);
    assert!(p.program(1, MULTI_PROGRAM_USER));
    p.on(1, C4, 127);
    let tuned = 12 * 8192 + 10 * 8192 / 100;
    assert_eq!(p.freq(1), freq_at(C4, tuned));
    unsafe { multi_bend(&mut *p.m, 1, -2 * 8192) };
    p.tick();
    assert_eq!(p.freq(1), freq_at(C4, tuned - 2 * 8192));
}

#[test]
fn a_sounds_table_steps_its_parts_waveform_and_pitch() {
    let (mut rig, mut p) = rig_parts();
    let mut kick = preset_sound(0);
    let t = LAB_SOUND_TABLE as usize;
    kick[t] = LAB_TABLE_ON as u8 | 2;
    kick[LAB_SOUND_TABLE_STEPS as usize..][..4].copy_from_slice(&[0x81, 7, 0x11, 0x80]);
    p.save_user(&mut rig, 0, "KICK", kick);
    assert!(p.program(2, MULTI_PROGRAM_USER));
    p.on(2, C4, 127);
    assert_eq!((p.reg(14 + 4), p.freq(2)), (0x81, freq_at(C4, 7 * 8192)));
    for _ in 0..2 {
        p.tick();
    }
    assert_eq!((p.reg(14 + 4), p.freq(2)), (0x11, freq_at(C4, 0)));
}

#[test]
fn under_a_song_a_note_takes_its_voice_and_the_filter_stays_the_songs() {
    let (_, mut p) = rig_parts();
    p.kr.song_on = 1;
    p.tick();
    p.chip.log.clear();
    p.on(1, C4, 127);
    assert_eq!(p.taken.0, [0b010], "the song gives voice 2 up at once");
    assert_eq!(unsafe { multi_taken(&*p.m) }, 0b010);
    assert_eq!(p.filter_writes(), 0);
    assert!(p.gated(1));
    p.off(1);
    assert_eq!(
        unsafe { multi_taken(&*p.m) },
        0,
        "and gets it back at the release"
    );
    p.chip.log.clear();
    unsafe { multi_wheel(&mut *p.m, 1, 127) };
    for _ in 0..4 {
        p.tick();
    }
    assert!(p.chip.log.is_empty(), "a released voice is the song's");
    p.kr.song_on = 0;
    p.tick();
    assert_eq!(
        p.reg(0x18),
        preset(SAW_LEAD).filter_mode | 0x0f,
        "the filter back"
    );
    assert!(p.writes_to(0) > 0);
}

#[test]
fn a_sid_tune_owns_chip_a_and_the_parts_come_back_after_it() {
    let (_, mut p) = rig_parts();
    p.kr.tune = 1;
    p.tick();
    p.on(0, C4, 127);
    p.tick();
    assert!(p.chip.log.is_empty());
    p.kr.tune = 0;
    p.tick();
    assert!(p.gated(0));
    assert_eq!(p.freq(0), freq_at(C4, 0));
}
