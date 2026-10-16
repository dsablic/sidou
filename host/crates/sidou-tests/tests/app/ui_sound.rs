// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! UI sounds on chip B's spare voice (`app/ui_sound.c`): GoatTracker's sound-effect format
//! stepped into chip B's registers, and never a voice the keys use. live and the sounds write
//! one Recorder here: chip B (with the keys on it, `live.keys_chip` 1, as during a song).

use sidou_sys::*;

use crate::{record, Recorder, Rig};

struct Ui {
    rig: Rig,
    us: Box<ui_sound>,
}

impl Ui {
    /// `keys_on_b`: the keys play chip B (a song on chip A), live writing chip B.
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

    /// A live_tick, then the sounds' tick (platform/fm1/main.c).
    fn tick(&mut self) {
        self.rig.tick();
        unsafe { ui_sound_tick(&mut *self.us) };
    }

    fn busy(&self) -> bool {
        (unsafe { ui_sound_busy(&*self.us) }) != 0
    }
}

fn data(id: ui_sound_id) -> Vec<u8> {
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

/// The writes GoatTracker's sound-effect format gives, frame by frame, for a sound on voice v
/// (readme 6.3): the setup (gate off, AD, SR, the pulse byte to both pulse registers), then
/// each frame's waveform (kept until the next) and note, and the end's gate off.
fn expected(id: ui_sound_id, v: u8) -> Vec<Vec<(u8, u8)>> {
    let d = data(id);
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

/// Every sound, with the keys on chip A (chip B all free: voice 3, no filter, its volume up):
/// the writes are the format's, a frame every other tick (50 Hz), and chip B is in use until
/// the sound's release has run out.
#[test]
fn each_sound_steps_as_goattrackers_format_says() {
    for id in 0..UI_SOUND_COUNT {
        let mut ui = Ui::new(false);
        ui.on();
        ui.play(id);
        let want = expected(id, 2);
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
        let sr = data(id)[1];
        let rel = [
            6u32, 24, 48, 72, 114, 168, 204, 240, 300, 750, 1500, 2400, 3000, 9000, 15000, 24000,
        ];
        let ticks = rel[usize::from(sr & 15)] / 10 + 1;
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

/// Off (the default): nothing is ever written, and chip B is never in use for a sound.
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

/// With the keys on chip B, a chord of three notes held while the menu is used: no sound, no
/// voice touched (nothing written but live's own).
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
    // Only live's ticks wrote (none here: no LFO, no vibrato on SAW LEAD).
    assert!(ui.rig.log().is_empty(), "{:?}", ui.rig.log());
}

/// A voice just released is not free until its release has run out (SAW LEAD: release 6,
/// 204 ms): the sound waits for none, it simply does not play; after the release it does,
/// on that voice.
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

/// While a sound holds a voice, live leaves it alone (its allocator skips it; its writes to it
/// and its filter bit are held back); a note that finds the other two held takes it back at
/// once, the sound writes nothing more, and the note plays with the instrument's envelope.
#[test]
fn a_note_takes_the_voice_back_from_a_sound() {
    let mut ui = Ui::new(true);
    ui.on();
    ui.rig.note_on(48);
    ui.tick();
    for _ in 0..30 {
        ui.tick(); // the other voices' power-on releases long run out
    }
    ui.play(UI_SOUND_CHIME);
    let v = ui.rig.live.ui_voice;
    assert!(v == 2 || v == 3, "the sound's voice: {v}");
    let held = usize::from(v - 1);
    ui.rig.clear();
    // live writes every voice's envelope: not the sound's; and the filter without its bit.
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
    // A note, the second held: a free voice remains (the third), the sound keeps its own.
    ui.rig.note_on(55);
    assert_eq!(ui.rig.live.ui_voice, v);
    // A third: the other two held, so it takes the sound's voice.
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

/// When the sound and its release are done, the voice gets live's registers back.
#[test]
fn the_voice_is_given_back_with_lives_registers() {
    let mut ui = Ui::new(true);
    ui.on();
    for _ in 0..30 {
        ui.tick();
    }
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
