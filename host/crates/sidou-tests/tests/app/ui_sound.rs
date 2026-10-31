// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::{record, Recorder, Rig, RELEASE_TO_ZERO_MS};

struct Ui {
    rig: Rig,
    us: Box<ui_sound>,
}

impl Ui {
    fn new(keys_on_b: bool) -> Self {
        let mut rig = Rig::new(SID_MODEL_6581);
        rig.live.two_chips = 1;
        rig.live.keys_chip = u8::from(keys_on_b);
        let mut us = Box::<ui_sound>::default();
        let rec: *mut Recorder = &mut *rig.rec;
        unsafe {
            ui_sound_init(&mut *us, &mut *rig.live, Some(record), rec.cast());
        }
        rig.clear();
        Ui { rig, us }
    }

    fn on(&mut self) {
        unsafe { ui_sound_set(&mut *self.us, 1) };
    }

    fn play(&mut self, id: ui_sound_id) {
        unsafe { ui_sound_play(&mut *self.us, id) };
    }

    fn tick(&mut self) {
        self.rig.tick();
        unsafe { ui_sound_tick(&mut *self.us) };
    }

    fn run_out_the_power_on_releases(&mut self) {
        for _ in 0..30 {
            self.tick();
        }
    }

    fn busy(&self) -> bool {
        (unsafe { ui_sound_busy(&*self.us) }) != 0
    }
}

fn sound_bytes(id: ui_sound_id) -> Vec<u8> {
    let p = unsafe { ui_sound_data(id) };
    let mut v = Vec::new();
    for i in 0..UI_SOUND_MAX_BYTES as usize {
        let b = unsafe { *p.add(i) };
        v.push(b);
        if i >= 3 && b == 0 {
            break;
        }
    }
    v
}

const TICK_MS: u32 = 10;
fn goattracker_format_writes(id: ui_sound_id, v: u8) -> Vec<Vec<(u8, u8)>> {
    let d = sound_bytes(id);
    let r = v * 7;
    let mut frames = vec![vec![
        (r + 4, 0),
        (r + 5, d[0]),
        (r + 6, d[1]),
        (r + 2, d[2]),
        (r + 3, d[2]),
    ]];
    let (mut i, mut wave) = (3, 0u8);
    loop {
        let mut b = d[i];
        i += 1;
        if (0x01..=0x81).contains(&b) {
            wave = b;
            b = d[i];
            i += 1;
        }
        let f = frames.len() - 1;
        if b == 0 {
            frames[f].push((r + 4, wave & !1));
            break;
        }
        assert!(
            (0x82..=0xdf).contains(&b),
            "a note out of the format's range: {b:#x}"
        );
        let freq = unsafe { live_freq_table[usize::from(b - 0x80) + 12] };
        frames[f].extend([(r, freq as u8), (r + 1, (freq >> 8) as u8), (r + 4, wave)]);
        frames.push(Vec::new());
    }
    frames
}

#[test]
fn each_sound_steps_as_goattrackers_format_says() {
    for id in 0..UI_SOUND_COUNT {
        let mut ui = Ui::new(false);
        ui.on();
        ui.play(id);
        let want = goattracker_format_writes(id, 2);
        let mut got = vec![ui.rig.log().to_vec()];
        assert_eq!(
            got[0][..2],
            [(0x17, 0), (0x18, 0x0f)],
            "sound {id}: chip B's filter and volume"
        );
        got[0].drain(..2);
        ui.rig.clear();
        for _ in 1..want.len() {
            ui.tick();
            assert!(
                ui.rig.log().is_empty(),
                "sound {id}: a write between two 50 Hz frames"
            );
            ui.tick();
            got.push(ui.rig.log().to_vec());
            ui.rig.clear();
        }
        assert_eq!(got, want, "sound {id}");
        assert!(ui.busy(), "sound {id}: its release");
        let sr = sound_bytes(id)[1];
        let ticks = RELEASE_TO_ZERO_MS[usize::from(sr & 15)] / TICK_MS + 1;
        for t in 1..=ticks {
            ui.tick();
            assert_eq!(
                ui.busy(),
                t < ticks,
                "sound {id}: busy at tick {t} of {ticks}"
            );
        }
        assert!(
            ui.rig.log().iter().all(|w| w.0 / 7 == 2 || w.0 >= 0x15),
            "only voice 3"
        );
    }
}

#[test]
fn off_writes_nothing() {
    let mut ui = Ui::new(false);
    for id in 0..UI_SOUND_COUNT {
        ui.play(id);
        ui.tick();
    }
    assert!(ui.rig.log().is_empty());
    assert!(!ui.busy());
}

#[test]
fn three_held_notes_leave_no_voice_for_a_sound() {
    let mut ui = Ui::new(true);
    ui.on();
    for n in [48, 55, 60] {
        ui.rig.note_on(n);
    }
    ui.tick();
    ui.rig.clear();
    for id in [
        UI_SOUND_BLIP,
        UI_SOUND_CONFIRM,
        UI_SOUND_CHIME,
        UI_SOUND_BUZZ,
    ] {
        ui.play(id);
        ui.tick();
        ui.tick();
    }
    assert!(!ui.busy(), "a sound found a voice");
    assert_eq!(ui.rig.live.ui_voice, 0);
    assert!(ui.rig.log().is_empty(), "{:?}", ui.rig.log());
}

#[test]
fn a_voice_still_releasing_is_not_free() {
    let mut ui = Ui::new(true);
    ui.on();
    for n in [48, 55, 60] {
        ui.rig.note_on(n);
    }
    ui.tick();
    ui.rig.note_off(55);
    ui.tick();
    ui.play(UI_SOUND_BLIP);
    assert!(!ui.busy(), "a sound on a voice still releasing");
    for _ in 0..21 {
        ui.tick();
    }
    ui.play(UI_SOUND_BLIP);
    assert!(ui.busy(), "no sound once the release ran out");
    let v = ui.rig.live.ui_voice;
    assert_eq!(v, 2, "the released voice (voice 2)");
}

#[test]
fn a_note_takes_the_voice_back_from_a_sound() {
    let mut ui = Ui::new(true);
    ui.on();
    ui.rig.note_on(48);
    ui.tick();
    ui.run_out_the_power_on_releases();
    ui.play(UI_SOUND_CHIME);
    let v = ui.rig.live.ui_voice;
    assert!(v == 2 || v == 3, "the sound's voice: {v}");
    let held = usize::from(v - 1);
    ui.rig.clear();
    unsafe {
        live_param_set(&mut *ui.rig.live, LIVE_PARAM_ATTACK, 12);
        live_param_set(&mut *ui.rig.live, LIVE_PARAM_FILTER_VOICES, 7);
    }
    let theirs: Vec<_> = ui
        .rig
        .log()
        .iter()
        .filter(|w| w.0 < 0x15 && usize::from(w.0 / 7) == held)
        .collect();
    assert!(
        theirs.is_empty(),
        "live wrote the sound's voice: {theirs:?}"
    );
    assert_eq!(
        ui.rig.reg(0x17) & (1 << held),
        0,
        "the sound's voice into the filter"
    );
    ui.rig.note_on(55);
    assert_eq!(ui.rig.live.ui_voice, v);
    ui.rig.clear();
    ui.rig.note_on(60);
    assert_eq!(ui.rig.live.ui_voice, 0, "the sound kept its voice");
    let ad = ui.rig.live.attack_decay;
    assert!(
        ui.rig.log().contains(&((held * 7 + 5) as u8, ad)),
        "the note's envelope"
    );
    assert!(ui.rig.gated(held as u8));
    ui.tick();
    ui.rig.clear();
    for _ in 0..20 {
        ui.tick();
    }
    assert!(!ui.busy());
    assert!(
        ui.rig
            .log()
            .iter()
            .all(|w| usize::from(w.0 / 7) != held || w.0 >= 0x15),
        "the sound wrote its voice after giving it back: {:?}",
        ui.rig.log()
    );
}

#[test]
fn the_voice_is_given_back_with_lives_registers() {
    let mut ui = Ui::new(true);
    ui.on();
    ui.run_out_the_power_on_releases();
    ui.play(UI_SOUND_BLIP);
    let v = ui.rig.live.ui_voice - 1;
    for _ in 0..40 {
        ui.tick();
    }
    assert!(!ui.busy());
    assert_eq!(ui.rig.live.ui_voice, 0);
    assert_eq!(ui.rig.reg(v * 7 + 5), ui.rig.live.attack_decay);
    assert_eq!(
        ui.rig.control(v),
        ui.rig.live.control,
        "live's waveform, gate off"
    );
    assert_eq!(
        ui.rig.reg(0x17) & 15,
        ui.rig.live.filter_voices,
        "the filter routing back"
    );
}
