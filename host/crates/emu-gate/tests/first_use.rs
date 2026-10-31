// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::panel::{self, press, turn, *};
use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::song::Song;

const CHORD_RELEASE_MS: u64 = 40;
const AUDITION_HANDS_CHIP_A_BACK_MS: u64 = 2_500;
const C4_KEY_INDEX: u8 = note_key(60) - KEY0;
const SETTINGS_DEBOUNCE_MS: u64 = 6_000;
const JAM_ROOT_NOTE: u8 = 48;
const SONG_LOAD: i32 = 0;
const SONG_NEW: i32 = 1;
const SONG_SAVE_AS: i32 = 2;

const WIPE_MS: u64 = 700;
const IO_TIMEOUT_MS: u64 = 15_000;
const ROW_MS: f64 = 120.0;
const LANE_NOTE: u32 = 36;
const CHIP_HZ: f64 = SID_CLOCK_PAL as f64;

const AUDIBLE_RMS: f64 = 250.0;
const SILENT_RMS: f64 = 40.0;

fn chord(m: &mut Machine, held: u8, id: u8) {
    panel::chord(m, held, id);
    m.run_ms(CHORD_RELEASE_MS);
}

fn booted(fw: &Firmware) -> Machine {
    let mut m = common::booted(fw);
    idle(&mut m, fw);
    m
}

fn view_is(m: &mut Machine, fw: &Firmware, symbol: &str) -> bool {
    top_view(m, fw) == fw.symbol(symbol)
}

fn assert_view(m: &mut Machine, fw: &Firmware, symbol: &str, what: &str) {
    assert!(
        view_is(m, fw, symbol),
        "{what}: the view on top is not {symbol}"
    );
}

fn popup(m: &mut Machine, fw: &Firmware) -> (String, String) {
    let at = fw.symbol("ui") + ilp32::views::popup as u32;
    (
        text_at(
            m,
            at + ilp32::screen_popup_state::name as u32,
            SCREEN_POPUP_TEXT,
        ),
        text_at(
            m,
            at + ilp32::screen_popup_state::value as u32,
            SCREEN_POPUP_TEXT,
        ),
    )
}

fn io_byte(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    m.soc.read8(fw.symbol("songs") + offset as u32)
}

fn song_name(m: &mut Machine, fw: &Firmware) -> String {
    text_at(
        m,
        fw.symbol("songs") + ilp32::song_io::name as u32,
        STORE_NAME_MAX + 1,
    )
}

fn idle(m: &mut Machine, fw: &Firmware) {
    let start = m.ms();
    while io_byte(m, fw, ilp32::song_io::state) != SONG_IO_IDLE as u8 {
        assert!(m.ms() - start < IO_TIMEOUT_MS, "song I/O stayed busy");
        m.run_ms(5);
    }
}

fn ctl_byte(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    m.soc.read8(fw.symbol("song_transport") + offset as u32)
}

fn playing(m: &mut Machine, fw: &Firmware) -> bool {
    ctl_byte(m, fw, ilp32::song_ctl::isr_playing) != 0
}

fn song(m: &mut Machine, fw: &Firmware) -> Song {
    let mut song = Song::device();
    let address = fw.symbol("__song_region");
    assert_eq!(ilp32::gt2_song::SIZE, size_of::<gt2_song>());
    for i in 0..SONG_REGION_BYTES {
        unsafe {
            song.ptr_mut()
                .cast::<u8>()
                .add(i as usize)
                .write(m.soc.read8(address + i))
        };
    }
    song
}

fn open_song_page(m: &mut Machine, fw: &Firmware, action: i32) {
    home_to_live(m, fw);
    press(m, HOME);
    m.run_ms(WIPE_MS);
    assert_view(m, fw, "views_menu", "HOME");
    turn(m, SELECT, -(VIEWS_MENU_ITEMS as i32));
    turn(m, SELECT, VIEWS_MENU_SONG as i32);
    turn(m, ALGORITHM, 1);
    m.run_ms(WIPE_MS);
    assert_view(m, fw, "views_song_menu_view", "the menu's SONG");
    turn(m, SELECT, -4);
    turn(m, SELECT, action);
}

fn home_to_live(m: &mut Machine, fw: &Firmware) {
    for _ in 0..4 {
        if view_is(m, fw, "live_view_view") {
            return;
        }
        press(m, HOME);
        m.run_ms(WIPE_MS);
    }
    assert_view(m, fw, "live_view_view", "HOME back to LIVE");
}

fn new_song(m: &mut Machine, fw: &Firmware) {
    open_song_page(m, fw, SONG_NEW);
    turn(m, ALGORITHM, 1);
    idle(m, fw);
    let s = song(m, fw);
    assert_eq!(
        &s.raw().instr[0][GT2_INS_NAME as usize..][..5],
        b"PULSE",
        "NEW made the template"
    );
    home_to_live(m, fw);
}

fn open_step(m: &mut Machine, fw: &Firmware) {
    press(m, SEQ);
    m.run_ms(WIPE_MS);
    assert_view(m, fw, "step_view_view", "SEQ from LIVE");
}

fn new_song_on_step(fw: &Firmware) -> Machine {
    let mut m = booted(fw);
    new_song(&mut m, fw);
    open_step(&mut m, fw);
    m
}

fn lane_instrument(m: &mut Machine, fw: &Firmware, voice: u32) -> u8 {
    m.soc
        .read8(fw.symbol("ui") + (ilp32::views::step + ilp32::step_view::instr) as u32 + voice)
}

fn presets_to(m: &mut Machine, fw: &Firmware, voice: u32, to: u8, name: &str) {
    let from = lane_instrument(m, fw, voice);
    if from == to {
        turn(m, PRESETS, 1);
        turn(m, PRESETS, -1);
    } else {
        turn(m, PRESETS, i32::from(to) - i32::from(from));
    }
    assert_eq!(lane_instrument(m, fw, voice), to, "PRESETS to {to:02X}");
    let (label, value) = popup(m, fw);
    assert_eq!(label, "INSTRUMENT", "PRESETS' popup");
    assert_eq!(value, format!("{to:02X} {name}"), "PRESETS' popup names it");
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Voice {
    freq: u32,
    control: u8,
    gate: u8,
    pw: u32,
    ad: u8,
    sr: u8,
}

fn chip_a_voice(m: &mut Machine, fw: &Firmware, v: usize) -> Voice {
    let chips = fw.chips();
    let active = m.soc.read8(chips + ilp32::reg_ring_chips::active as u32);
    let chip =
        chips + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32;
    let o = chip + (ilp32::sid::osc + v * ilp32::sid_osc::SIZE) as u32;
    let e = chip + (ilp32::sid::env + v * ilp32::sid_env::SIZE) as u32;
    let env = |m: &mut Machine, f: usize| m.soc.read8(e + f as u32);
    Voice {
        freq: m.soc.read32(o + ilp32::sid_osc::freq as u32),
        control: m.soc.read8(o + ilp32::sid_osc::control as u32),
        gate: env(m, ilp32::sid_env::gate),
        pw: m.soc.read32(o + ilp32::sid_osc::pw as u32),
        ad: env(m, ilp32::sid_env::attack) << 4 | env(m, ilp32::sid_env::decay),
        sr: (env(m, ilp32::sid_env::sustain) & 15) << 4 | env(m, ilp32::sid_env::release),
    }
}

fn hz_of_freq(freq: u32) -> f64 {
    f64::from(freq) * CHIP_HZ / 16_777_216.0
}

fn hz_of_note(n: u32) -> f64 {
    440.0 * 2f64.powf((f64::from(n) - 57.0) / 12.0)
}

fn cents_off(hz: f64, want: f64) -> f64 {
    1200.0 * (hz / want).log2()
}

#[derive(Clone, Debug)]
struct Heard {
    audio: Vec<f32>,
    regs: Vec<Voice>,
    gates: Vec<u8>,
}

fn rms_windows(x: &[f32]) -> Vec<f64> {
    let w = (FS / 100.0) as usize;
    x.chunks(w)
        .filter(|c| c.len() == w)
        .map(|c| (c.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / w as f64).sqrt())
        .collect()
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

#[derive(Clone, Copy, Debug)]
struct Traits {
    loudest: f64,
    length_ms: f64,
    centroid: f64,
    crossings: f64,
    body: f64,
    periodic: f64,
}

fn traits(x: &[f32]) -> Traits {
    let w = rms_windows(x);
    let (at, &loudest) = w
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .expect("no audio");
    let length = w.iter().filter(|&&r| r >= loudest / 10.0).count();
    let win = (FS / 100.0) as usize;
    let from = at.saturating_sub(1) * win;
    let seg = &x[from..(from + 4096).min(x.len())];
    let mean = seg.iter().map(|&v| f64::from(v)).sum::<f64>() / seg.len() as f64;
    let n = seg.len();
    let (mut num, mut den) = (0.0, 0.0);
    for k in 1..n / 2 {
        let wk = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &s) in seg.iter().enumerate() {
            let a = wk * i as f64;
            let v = f64::from(s) - mean;
            re += v * a.cos();
            im -= v * a.sin();
        }
        let p = (re * re + im * im).sqrt();
        num += p * k as f64 * FS / n as f64;
        den += p;
    }
    let span = &x[from..(from + length * win).min(x.len())];
    let zc = span
        .windows(2)
        .filter(|p| (p[0] < 0.0) != (p[1] < 0.0))
        .count();
    let held = &w[(at + 5).min(w.len())..(at + 15).min(w.len())];
    let body = held.iter().sum::<f64>() / held.len().max(1) as f64 / loudest;
    let p0 = ((at + 3) * win).min(x.len());
    let seg = &x[p0..(p0 + 2048 + (FS / 50.0) as usize).min(x.len())];
    let n = 2048.min(seg.len());
    let energy = |a: usize| {
        seg[a..a + n.min(seg.len() - a)]
            .iter()
            .map(|&v| f64::from(v).powi(2))
            .sum::<f64>()
    };
    let periodic = ((FS / 500.0) as usize..(FS / 50.0) as usize)
        .filter(|&lag| lag + n <= seg.len())
        .map(|lag| {
            let c: f64 = (0..n)
                .map(|i| f64::from(seg[i]) * f64::from(seg[i + lag]))
                .sum();
            c / (energy(0) * energy(lag)).sqrt().max(1e-9)
        })
        .fold(0.0, f64::max);
    Traits {
        loudest,
        length_ms: 10.0 * length as f64,
        centroid: num / den.max(1e-9),
        crossings: zc as f64 * FS / span.len().max(1) as f64,
        body,
        periodic,
    }
}

fn step_tap(
    m: &mut Machine,
    fw: &Firmware,
    step: u8,
    voice: usize,
    hold_ms: u64,
    total_ms: u64,
) -> Heard {
    m.key(KEY0 + step, true);
    listen(m, fw, voice, total_ms, |m, ms| {
        if ms == hold_ms {
            m.key(KEY0 + step, false);
        }
    })
}

fn listen(
    m: &mut Machine,
    fw: &Firmware,
    voice: usize,
    ms: u64,
    mut each: impl FnMut(&mut Machine, u64),
) -> Heard {
    let from = m.soc.periph.audio.len() / 2;
    let (mut regs, mut gates) = (Vec::new(), Vec::new());
    for i in 0..ms {
        each(m, i);
        m.run_ms(1);
        regs.push(chip_a_voice(m, fw, voice));
        gates.push((0..3).fold(0, |g, v| g | (chip_a_voice(m, fw, v).gate & 1) << v));
    }
    let audio = m.left()[from..].iter().map(|&s| f32::from(s)).collect();
    Heard { audio, regs, gates }
}

fn settled_hz(h: &Heard) -> Option<f64> {
    h.regs
        .iter()
        .rev()
        .find(|r| r.gate != 0 && r.freq != 0)
        .map(|r| hz_of_freq(r.freq))
}

#[derive(Clone, Copy, Debug)]
enum Pitch {
    Played,
    Fixed(u32),
    None,
}

const INSTRUMENTS: [(u8, &str, Pitch); 14] = [
    (1, "PULSE", Pitch::Played),
    (2, "BASS", Pitch::Played),
    (3, "KICK", Pitch::Fixed(22)),
    (4, "SNARE", Pitch::None),
    (5, "CL HAT", Pitch::None),
    (6, "OP HAT", Pitch::None),
    (7, "CLAP", Pitch::None),
    (8, "TOM LO", Pitch::Fixed(37)),
    (9, "TOM HI", Pitch::Fixed(44)),
    (10, "RIM", Pitch::None),
    (11, "DG KICK", Pitch::None),
    (12, "DG SNARE", Pitch::None),
    (13, "DG CLAP", Pitch::None),
    (14, "DG HAT", Pitch::None),
];

fn heard_pitch(x: &[f32]) -> Option<f64> {
    let w = (FS / 200.0) as usize;
    let k = envelope(x).iter().position(|&r| r > AUDIBLE_RMS / 2.0)?;
    let from = k * w + (FS / 20.0) as usize;
    (from + 8192 <= x.len()).then(|| pitch(&x[from..from + 8192]))
}

#[derive(Clone, Debug)]
struct Sound {
    traits: Traits,
    pitch: Option<f64>,
    heard_f0: Option<f64>,
    waves: Vec<u8>,
    pw: Option<u32>,
    adsr: Vec<(u8, u8)>,
}

fn sound(h: &Heard, kind: Pitch) -> Sound {
    let gated: Vec<Voice> = h.regs.iter().copied().filter(|r| r.gate != 0).collect();
    let mut waves: Vec<u8> = gated.iter().map(|r| r.control & 0xf0).collect();
    waves.dedup();
    let pulse: Vec<u32> = gated
        .iter()
        .filter(|r| r.control & 0x40 != 0)
        .map(|r| r.pw)
        .collect();
    let mut adsr: Vec<(u8, u8)> = gated.iter().map(|r| (r.ad, r.sr)).collect();
    adsr.dedup();
    Sound {
        traits: traits(&h.audio),
        pitch: match kind {
            Pitch::None => None,
            _ => settled_hz(h),
        },
        heard_f0: match kind {
            Pitch::Played => heard_pitch(&h.audio),
            _ => None,
        },
        waves,
        pw: pulse.last().copied(),
        adsr,
    }
}

fn tell_apart(a: &Sound, b: &Sound) -> bool {
    let ratio = |x: f64, y: f64| x.max(y) / x.min(y).max(1e-9);
    let (s, t) = (&a.traits, &b.traits);
    ratio(s.loudest, t.loudest) > 1.6
        || ratio(s.length_ms, t.length_ms) > 1.5
        || ratio(s.centroid, t.centroid) > 1.25
        || ratio(s.crossings, t.crossings) > 1.25
        || (s.body > 0.1 && t.body > 0.1 && (s.periodic - t.periodic).abs() > 0.15)
        || matches!((a.pitch, b.pitch), (Some(x), Some(y)) if cents_off(x, y).abs() > 100.0)
        || (!a.waves.is_empty() && !b.waves.is_empty() && a.waves != b.waves)
        || matches!((a.pw, b.pw), (Some(x), Some(y)) if ratio(f64::from(x), f64::from(y)) > 1.1)
}

fn same_instrument(a: &Sound, b: &Sound) -> bool {
    let ratio = |x: f64, y: f64| x.max(y) / x.min(y).max(1e-9);
    if a.waves.is_empty() || b.waves.is_empty() {
        return a.waves.is_empty()
            && b.waves.is_empty()
            && ratio(a.traits.length_ms, b.traits.length_ms) < 1.5
            && ratio(a.traits.centroid, b.traits.centroid) < 1.25;
    }
    a.adsr == b.adsr
        && a.waves[0] == b.waves[0]
        && a.pw == b.pw
        && match (a.pitch, b.pitch) {
            (Some(x), Some(y)) => cents_off(x, y).abs() < 10.0,
            (x, y) => x.is_none() && y.is_none(),
        }
}

const TAP_HOLD_MS: u64 = 200;
const TAP_MS: u64 = 600;
const KNOB_TO_KEY_MS: u64 = 150;

fn audition_every_instrument(fw: &Firmware) -> Vec<(Sound, Sound)> {
    let mut m = new_song_on_step(fw);
    let mut heard = Vec::new();
    for &(n, name, kind) in &INSTRUMENTS {
        presets_to(&mut m, fw, 0, n, name);
        m.run_ms(KNOB_TO_KEY_MS);
        let first = step_tap(&mut m, fw, 0, 0, TAP_HOLD_MS, TAP_MS);
        let row = song(&mut m, fw).pattern(0).unwrap()[0];
        assert_eq!(
            (row[0], row[1]),
            (GT2_FIRSTNOTE as u8 + LANE_NOTE as u8, n),
            "{name}: the first press put the lane's note and {n:02X} on the step"
        );
        let second = step_tap(&mut m, fw, 0, 0, TAP_HOLD_MS, TAP_MS);
        assert_eq!(
            song(&mut m, fw).pattern(0).unwrap()[0][0],
            GT2_REST as u8,
            "{name}: the second press took the note away"
        );
        heard.push((sound(&first, kind), sound(&second, kind)));
    }
    heard
}

fn assert_audible(what: &str, s: &Sound) {
    assert!(
        s.traits.loudest > AUDIBLE_RMS,
        "{what} is not heard: its loudest 10 ms RMS {:.0} (floor {AUDIBLE_RMS})",
        s.traits.loudest
    );
}

fn assert_pitch(name: &str, kind: Pitch, s: &Sound) {
    let want = match kind {
        Pitch::Played => hz_of_note(LANE_NOTE),
        Pitch::Fixed(n) => hz_of_note(n),
        Pitch::None => return,
    };
    let register = s
        .pitch
        .unwrap_or_else(|| panic!("{name}: no pitch on the chip"));
    for (what, hz) in [("the chip", Some(register)), ("the audio", s.heard_f0)] {
        let Some(hz) = hz else { continue };
        let mut off = cents_off(hz, want);
        if name == "BASS" {
            off = (off + 600.0).rem_euclid(1200.0) - 600.0;
        }
        assert!(
            off.abs() < 25.0,
            "{name}: {what} sounds at {hz:.1} Hz, {off:+.0} cents from {want:.1} Hz"
        );
    }
}

#[test]
fn every_template_instrument_auditions_on_step_heard_at_its_pitch_and_each_its_own() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let heard = audition_every_instrument(&fw);
    for (&(n, name, kind), (first, second)) in INSTRUMENTS.iter().zip(&heard) {
        println!(
            "{n:02X} {name:8} {:?} pitch {:?} waves {:02X?} pw {:?} adsr {:02X?}",
            second.traits, second.pitch, second.waves, second.pw, second.adsr
        );
        assert_audible(&format!("{name}'s first press"), first);
        assert_audible(&format!("{name}'s audition"), second);
        assert_pitch(name, kind, second);
    }
    let mut alike = Vec::new();
    for i in 0..INSTRUMENTS.len() {
        for j in i + 1..INSTRUMENTS.len() {
            let (a, b) = (INSTRUMENTS[i].1, INSTRUMENTS[j].1);
            if (a, b) == ("PULSE", "BASS") {
                continue;
            }
            if !tell_apart(&heard[i].1, &heard[j].1) {
                alike.push((a, b));
            }
        }
    }
    assert!(
        alike.is_empty(),
        "an ear cannot tell these apart: {alike:?}"
    );
}

#[test]
fn the_first_step_press_after_presets_auditions_the_instrument_picked() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let heard = audition_every_instrument(&fw);
    let mut wrong = Vec::new();
    for (&(n, name, _), (first, second)) in INSTRUMENTS.iter().zip(&heard) {
        println!(
            "{n:02X} {name:8} first adsr {:02X?} waves {:02X?}, second adsr {:02X?} waves {:02X?}",
            first.adsr, first.waves, second.adsr, second.waves
        );
        if !same_instrument(first, second) {
            wrong.push(name);
        }
    }
    assert!(
        wrong.is_empty(),
        "the first press after PRESETS did not sound the instrument picked: {wrong:?}"
    );
}

#[test]
fn new_bass_sounds_an_octave_below_the_lead_and_thinner() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    let tap = |m: &mut Machine, n: u8, name: &str| {
        presets_to(m, &fw, 0, n, name);
        let _ = step_tap(m, &fw, 0, 0, TAP_HOLD_MS, TAP_MS);
        sound(&step_tap(m, &fw, 0, 0, TAP_HOLD_MS, TAP_MS), Pitch::Played)
    };
    let lead = tap(&mut m, 1, "PULSE");
    let bass = tap(&mut m, 2, "BASS");
    println!("lead {lead:?}\nbass {bass:?}");
    for (what, l, b) in [
        ("register", lead.pitch, bass.pitch),
        ("audio", lead.heard_f0, bass.heard_f0),
    ] {
        let (l, b) = (l.unwrap(), b.expect("the bass has a pitch"));
        let off = cents_off(b, l / 2.0);
        assert!(
            off.abs() < 25.0,
            "{what}: the bass at {b:.1} Hz is not an octave below the lead's {l:.1} Hz \
             ({off:+.0} cents)"
        );
    }
    let pw = bass.pw.expect("the bass is a pulse");
    assert!(
        (0x380..=0x480).contains(&pw),
        "the bass's pulse width {pw:#05x} is not about a quarter ($400)"
    );
    assert!(
        tell_apart(&lead, &bass),
        "an ear cannot tell the bass from the lead"
    );
}

fn envelope(x: &[f32]) -> Vec<f64> {
    let hop = (FS / 200.0) as usize;
    (0..x.len() / hop)
        .map(|k| rms(&x[k * hop..(k * hop + 2 * hop).min(x.len())]))
        .collect()
}

fn onset_near(env: &[f64], at_ms: f64) -> Option<f64> {
    let k = |ms: f64| (ms.max(20.0) / 5.0) as usize;
    (k(at_ms - 40.0)..k(at_ms + 80.0).min(env.len()))
        .filter(|&i| {
            let base = env[i - 4..i].iter().copied().fold(f64::MAX, f64::min);
            env[i] > AUDIBLE_RMS / 2.0 && env[i] > 1.8 * base
        })
        .map(|i| i as f64 * 5.0)
        .min_by(|a, b| (a - at_ms).abs().total_cmp(&(b - at_ms).abs()))
}

fn first_gate(h: &Heard) -> f64 {
    h.gates
        .iter()
        .position(|&g| g != 0)
        .expect("PLAY gated no voice") as f64
}

fn play_for(m: &mut Machine, fw: &Firmware, ms: u64, v: usize) -> Heard {
    m.key(PLAY, true);
    listen(m, fw, v, ms, |m, i| {
        if i == 30 {
            m.key(PLAY, false);
        }
    })
}

fn stop(m: &mut Machine, fw: &Firmware) {
    press(m, PLAY);
    m.run_ms(800);
    assert!(!playing(m, fw), "PLAY stops the song");
}

fn toggle_row(m: &mut Machine, fw: &Firmware, voice: u32, row: u32) {
    let page = m
        .soc
        .read8(fw.symbol("ui") + (ilp32::views::step + ilp32::step_view::page) as u32);
    let want = (row / 16) as i32;
    if i32::from(page) != want {
        turn(m, ALGORITHM, want - i32::from(page));
    }
    let _ = step_tap(m, fw, (row % 16) as u8, voice as usize, 80, 250);
    let start = m.ms();
    while !is_note(song(m, fw).pattern(voice).unwrap()[row as usize][0]) {
        assert!(
            m.ms() - start < 2_000,
            "step {row} of V{} did not take its note",
            voice + 1
        );
        m.run_ms(20);
    }
}

fn is_note(b: u8) -> bool {
    (GT2_FIRSTNOTE as u8..=GT2_LASTNOTE as u8).contains(&b)
}

const ON_TIME_MS: f64 = 15.0;

fn drum_delay(i: usize) -> f64 {
    if (2..10).contains(&i) {
        20.0
    } else {
        0.0
    }
}

fn rows_ms(rows: u32) -> u64 {
    (f64::from(rows) * ROW_MS) as u64
}

struct Played {
    name: &'static str,
    row_at: f64,
    onset: Option<f64>,
    loudest: f64,
    own: bool,
}

fn play_every_instrument(fw: &Firmware) -> (f64, Vec<Played>) {
    let mut m = new_song_on_step(fw);
    for (i, &(n, name, _)) in INSTRUMENTS.iter().enumerate() {
        presets_to(&mut m, fw, 0, n, name);
        toggle_row(&mut m, fw, 0, 4 * i as u32);
    }
    turn(&mut m, ALGORITHM, -4);
    m.run_ms(AUDITION_HANDS_CHIP_A_BACK_MS);
    let instr = song(&mut m, fw).raw().instr;
    let h = play_for(&mut m, fw, rows_ms(4 * INSTRUMENTS.len() as u32) + 400, 0);
    stop(&mut m, fw);
    let env = envelope(&h.audio);
    let t0 = first_gate(&h);
    let slot_ms = 4.0 * ROW_MS;
    let played = INSTRUMENTS
        .iter()
        .enumerate()
        .map(|(i, &(n, name, _))| {
            let row_at = t0 + i as f64 * slot_ms;
            let from = (row_at * FS / 1000.0) as usize;
            let to = (from + (slot_ms * FS / 1000.0) as usize).min(h.audio.len());
            let ins = &instr[n as usize - 1];
            let adsr = (ins[GT2_INS_AD as usize], ins[GT2_INS_SR as usize]);
            let p = Played {
                name,
                row_at,
                onset: onset_near(&env, row_at + drum_delay(i)),
                loudest: rms_windows(&h.audio[from..to]).into_iter().fold(0.0, f64::max),
                own: h.regs[row_at as usize..(row_at + slot_ms) as usize]
                    .iter()
                    .any(|r| r.gate != 0 && (r.ad, r.sr) == adsr),
            };
            println!(
                "{n:02X} {name:8} row {:>2}: onset {:+.0} ms from its row, loudest {:.0}, own envelope {}",
                4 * i,
                p.onset.map_or(f64::NAN, |t| t - row_at),
                p.loudest,
                p.own
            );
            p
        })
        .collect();
    (t0, played)
}

#[test]
fn every_template_instrument_on_steps_plays_heard_and_on_time() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let (t0, played) = play_every_instrument(&fw);
    println!("the first note {t0} ms after PLAY");
    assert!(
        t0 < ROW_MS + 60.0,
        "PLAY's first note came {t0} ms after the press"
    );
    let mut late = Vec::new();
    for (i, p) in played.iter().enumerate() {
        assert!(
            p.loudest > AUDIBLE_RMS,
            "{} on row {} is not heard ({:.0})",
            p.name,
            4 * i,
            p.loudest
        );
        assert!(
            i >= 10 || p.own,
            "{} on row {}: the chip did not play its envelope",
            p.name,
            4 * i
        );
        let window = if drum_delay(i) > 0.0 {
            -ON_TIME_MS..=80.0
        } else {
            -ON_TIME_MS..=ON_TIME_MS
        };
        if !p.onset.is_some_and(|t| window.contains(&(t - p.row_at))) {
            late.push((p.name, p.onset.map(|t| t - p.row_at)));
        }
    }
    assert!(
        late.is_empty(),
        "not on time (name, onset ms after its row): {late:?}"
    );
}

#[test]
fn the_kits_drums_sound_one_frame_after_their_row() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let (_, played) = play_every_instrument(&fw);
    let late: Vec<_> = played
        .iter()
        .enumerate()
        .filter(|&(i, _)| drum_delay(i) > 0.0)
        .filter(|(i, p)| {
            !p.onset
                .is_some_and(|t| (t - p.row_at - drum_delay(*i)).abs() <= ON_TIME_MS)
        })
        .map(|(_, p)| (p.name, p.onset.map(|t| t - p.row_at)))
        .collect();
    assert!(
        late.is_empty(),
        "not a frame after their row (name, onset ms after its row): {late:?}"
    );
}

#[test]
fn glo_mutes_a_lane_and_only_that_lane_falls_silent() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    for row in [0, 4, 8, 12] {
        toggle_row(&mut m, &fw, 0, row);
    }
    turn(&mut m, SELECT, 1);
    presets_to(&mut m, &fw, 1, 3, "KICK");
    for row in [2, 6, 10, 14] {
        toggle_row(&mut m, &fw, 1, row);
    }
    m.run_ms(2_500);
    let glo = |m: &mut Machine, key: u8| {
        m.key(GLO, true);
        m.run_ms(40);
        press(m, KEY0 + key);
        m.key(GLO, false);
        m.run_ms(100);
    };
    let mute = |m: &mut Machine| {
        m.soc
            .read8(fw.symbol("ui") + (ilp32::views::song + ilp32::song_view::mute) as u32)
    };
    let hits = |m: &mut Machine, t0: Option<f64>| -> (Vec<Option<f64>>, Heard, f64) {
        let h = play_for(m, &fw, rows_ms(16) + 300, 0);
        stop(m, &fw);
        let env = envelope(&h.audio);
        let t0 = t0.unwrap_or_else(|| first_gate(&h));
        let at = |row: u32| t0 + f64::from(row) * ROW_MS;
        let onsets = (0..8)
            .map(|i| {
                let row = 2 * i;
                let late = if row % 4 == 2 { 80.0 } else { ON_TIME_MS };
                onset_near(&env, at(row) + if row % 4 == 2 { 20.0 } else { 0.0 })
                    .filter(|t| (-ON_TIME_MS..=late).contains(&(t - at(row))))
            })
            .collect();
        (onsets, h, t0)
    };
    let (both, _, t0) = hits(&mut m, None);
    println!("both: {both:?}");
    assert!(
        both.iter().all(Option::is_some),
        "a hit on every even row: {both:?}"
    );
    glo(&mut m, 0);
    assert_eq!(mute(&mut m), 1, "GLO + key 1 mutes V1");
    let (kicks, h, _) = hits(&mut m, Some(t0));
    println!("V1 muted: {kicks:?}");
    for (i, o) in kicks.iter().enumerate() {
        let row = 2 * i;
        assert_eq!(
            o.is_some(),
            row % 4 == 2,
            "V1 muted: row {row}'s onset {o:?}"
        );
    }
    assert!(
        h.regs.iter().all(|r| r.gate == 0),
        "V1 muted: its gate went on"
    );
    glo(&mut m, 1);
    assert_eq!(mute(&mut m), 3, "GLO + key 2 mutes V2");
    let h = play_for(&mut m, &fw, rows_ms(16), 0);
    stop(&mut m, &fw);
    let loudest = rms_windows(&h.audio).into_iter().fold(0.0, f64::max);
    println!("both muted: loudest {loudest:.1}");
    assert!(
        loudest < SILENT_RMS,
        "both lanes muted, yet heard: {loudest:.0}"
    );
    glo(&mut m, 0);
    glo(&mut m, 1);
    assert_eq!(mute(&mut m), 0, "GLO again lets them sound");
    let (again, _, _) = hits(&mut m, Some(t0));
    assert!(again.iter().all(Option::is_some), "both back: {again:?}");
}

#[test]
fn tracker_rec_enters_a_note_that_plays_and_seq_walks_every_song_view() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    press(&mut m, SEQ);
    m.run_ms(WIPE_MS);
    assert_view(&mut m, &fw, "tracker_view_view", "SEQ from STEP");
    let sv = fw.symbol("ui") + ilp32::views::song as u32;
    press(&mut m, REC);
    m.run_ms(100);
    assert_eq!(m.soc.read8(sv + ilp32::song_view::rec as u32), 1, "REC on");
    let key = C4_KEY_INDEX;
    m.key(KEY0 + key, true);
    m.run_ms(200);
    m.key(KEY0 + key, false);
    let start = m.ms();
    let cell = loop {
        let c = song(&mut m, &fw).pattern(0).unwrap()[0];
        if is_note(c[0]) {
            break c;
        }
        assert!(m.ms() - start < 2_000, "REC's key wrote nothing on row 0");
        m.run_ms(20);
    };
    let note = u32::from(cell[0] - GT2_FIRSTNOTE as u8);
    let midi = PANEL_BASE_NOTE + u32::from(key);
    println!(
        "key {key}: MIDI {midi}, written {note} instrument {:02X}",
        cell[1]
    );
    assert_eq!(cell[1], 1, "the current instrument, 01 PULSE");
    assert_eq!(
        note + 12,
        midi,
        "the key's own note (GoatTracker's C-0 is MIDI 12)"
    );
    assert_eq!(
        m.soc.read8(sv + ilp32::song_view::row as u32),
        1,
        "the cursor moved on a row"
    );
    press(&mut m, REC);
    m.run_ms(2_500);
    let h = play_for(&mut m, &fw, 700, 0);
    stop(&mut m, &fw);
    let from = ((first_gate(&h) + 50.0) * FS / 1000.0) as usize;
    let heard = pitch(&h.audio[from..from + 8192]);
    let want = hz_of_note(note);
    let chip = settled_hz(&h).expect("the chip played no note");
    println!("played: {heard:.1} Hz heard, {chip:.1} Hz on the chip, want {want:.1}");
    for (what, hz) in [("heard", heard), ("on the chip", chip)] {
        assert!(
            cents_off(hz, want).abs() < 25.0,
            "{what}: {hz:.1} Hz, not {want:.1}"
        );
    }
    assert!(
        rms(&h.audio[from..from + 8192]) > AUDIBLE_RMS,
        "the note is not heard"
    );
    for (symbol, what) in [
        ("song_pages_order_view", "ORDER"),
        ("song_pages_instr_view", "INSTR"),
        ("song_pages_table_view", "TABLE"),
        ("step_view_view", "STEP"),
        ("tracker_view_view", "TRACKER"),
    ] {
        press(&mut m, SEQ);
        m.run_ms(WIPE_MS);
        assert_view(&mut m, &fw, symbol, &format!("SEQ to {what}"));
    }
}

fn heard_cycle(m: &Machine, x: f64) -> f64 {
    let hz = m.soc.periph.hz as f64;
    for snap in m.buffer_snapshots.iter().rev() {
        if let Some(&start) = m.half_played.iter().find(|&&c| c > snap.entry) {
            let after = u32::from_le_bytes(snap.bytes[..4].try_into().unwrap());
            let first = f64::from(after) - HALF_FRAMES as f64;
            return start as f64 + (x - first) * hz / FS;
        }
    }
    panic!("no block heard yet");
}

fn rec_trial(m: &mut Machine, fw: &Firmware, quant: u32, row: u32, d_ms: f64) -> usize {
    let ui = fw.symbol("ui");
    m.soc.write8(
        ui + (ilp32::views::song + ilp32::song_view::quant) as u32,
        quant as u8,
    );
    let ctl = fw.symbol("song_transport");
    let seen = fw.symbol("song_seen");
    let row_now = u32::from(m.soc.read8(seen + ilp32::song_status::row as u32));
    let row_call = m.soc.read16(seen + ilp32::song_status::row_call as u32);
    let calls = m.soc.read32(ctl + ilp32::song_ctl::calls as u32);
    let at = m.soc.read32(ctl + ilp32::song_ctl::call_sample as u32);
    let p = ctl + ilp32::song_ctl::period as u32;
    let (q, r, den) = (
        m.soc.read32(p + ilp32::song_period::q as u32),
        m.soc.read32(p + ilp32::song_period::r as u32),
        m.soc.read32(p + ilp32::song_period::den as u32),
    );
    let period = f64::from(q) + f64::from(r) / f64::from(den.max(1));
    let row_start_call = f64::from(row_call) + 6.0 * f64::from(row - row_now);
    let sample = f64::from(at) + (row_start_call - f64::from(calls - 1)) * period;
    let press = heard_cycle(m, sample) + d_ms * m.soc.periph.hz as f64 / 1000.0;
    let now = m.soc.periph.cycles as f64;
    assert!(
        press > now + 1000.0,
        "row {row} is too close: plan further ahead"
    );
    let before = song(m, fw).pattern(0).unwrap();
    m.run_cycles((press - now) as u64);
    m.key(KEY0 + 7, true);
    m.run_ms(40);
    m.key(KEY0 + 7, false);
    let start = m.ms();
    loop {
        let after = song(m, fw).pattern(0).unwrap();
        let new: Vec<usize> = (0..after.len())
            .filter(|&i| is_note(after[i][0]) && after[i] != before[i])
            .collect();
        if let [i] = new[..] {
            return i;
        }
        assert!(new.is_empty(), "notes on rows {new:?}");
        assert!(m.ms() - start < 1_000, "the key recorded nothing");
        m.run_ms(10);
    }
}

#[test]
fn rec_places_a_key_where_it_was_heard() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    press(&mut m, SEQ);
    m.run_ms(WIPE_MS);
    assert_view(&mut m, &fw, "tracker_view_view", "SEQ from STEP");
    press(&mut m, REC);
    let samples = fw.symbol("song_transport") + ilp32::song_ctl::samples as u32;
    m.snapshot_bytes = (0..4).map(|i| samples + i).collect();
    m.snapshot_buffer(Some((fw.symbol("abuf"), 0)));
    m.key(PLAY, true);
    m.run_ms(30);
    m.key(PLAY, false);
    let start = m.ms();
    while !playing(&mut m, &fw) {
        assert!(m.ms() - start < 2_000, "PLAY plays");
        m.run_ms(5);
    }
    m.run_ms(300);
    let rows = song(&mut m, &fw).pattern(0).unwrap().len() as u32;
    let half_row = ROW_MS / 2.0;
    let (off, near) = (SONG_VIEW_QUANT_OFF, SONG_VIEW_QUANT_NEAREST);
    let cases: [(u32, f64, i32); 4] = [
        (off, -5.0, -1),
        (off, 5.0, 0),
        (near, half_row - 5.0, 0),
        (near, half_row + 5.0, 1),
    ];
    let seen = fw.symbol("song_seen");
    let mut next = u32::from(m.soc.read8(seen + ilp32::song_status::row as u32)) + 3;
    for (quant, d, want) in cases {
        let got = rec_trial(&mut m, &fw, quant, next, d);
        let expect = (next as i32 + want).rem_euclid(rows as i32) as usize;
        println!("quantise {quant}, {d:+.1} ms from row {next}'s sound: row {got}");
        assert_eq!(
            got, expect,
            "quantise {quant}, a key {d:+.1} ms from row {next}"
        );
        next += 4;
    }
    let mut flip = None;
    for d in [-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0] {
        let got = rec_trial(&mut m, &fw, off, next, d);
        let on_row = got == (next % rows) as usize;
        println!(
            "OFF, {d:+.1} ms: {}",
            if on_row { "the row" } else { "the row before" }
        );
        if on_row && flip.is_none() {
            flip = Some(d);
        }
        next += 4;
    }
    println!(
        "compensation {:.1} ms (265 samples); OFF turns to the row at {flip:?} ms from its sound",
        265.0 * 1000.0 / FS
    );
    let flip = flip.expect("OFF never turned to the row within 3 ms");
    assert!(
        flip > -3.0,
        "the row from {flip} ms: the compensation is short"
    );
    stop(&mut m, &fw);
}

#[test]
fn song_load_sidou_loop_plays_it_heard_all_through() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = booted(&fw);
    let builtin = song(&mut m, &fw).write(false);
    new_song(&mut m, &fw);
    assert_ne!(song(&mut m, &fw).write(false), builtin, "NEW replaced it");
    open_song_page(&mut m, &fw, SONG_LOAD);
    turn(&mut m, ALGORITHM, 1);
    m.run_ms(300);
    turn(&mut m, ALGORITHM, 1);
    m.run_ms(200);
    turn(&mut m, SELECT, 1);
    turn(&mut m, ALGORITHM, 1);
    idle(&mut m, &fw);
    assert_eq!(
        song(&mut m, &fw).write(false),
        builtin,
        "LOAD put SIDOU LOOP back"
    );
    assert_eq!(song_name(&mut m, &fw), "", "the built-in stays unnamed");
    home_to_live(&mut m, &fw);
    let mut notes = std::collections::BTreeSet::new();
    let from = m.soc.periph.audio.len() / 2;
    press(&mut m, PLAY);
    for _ in 0..400 {
        m.run_ms(10);
        for v in 0..3 {
            let r = chip_a_voice(&mut m, &fw, v);
            if r.gate != 0 {
                notes.insert((v, r.freq));
            }
        }
    }
    stop(&mut m, &fw);
    let audio: Vec<f32> = m.left()[from..].iter().map(|&s| f32::from(s)).collect();
    let half = (FS / 2.0) as usize;
    let quiet: Vec<_> = audio[..8 * half]
        .chunks(half)
        .map(rms)
        .enumerate()
        .filter(|&(_, r)| r < AUDIBLE_RMS)
        .collect();
    println!(
        "{} notes on the chip; quiet half-seconds {quiet:?}",
        notes.len()
    );
    assert!(quiet.is_empty(), "SIDOU LOOP falls quiet: {quiet:?}");
    assert!(notes.len() >= 6, "SIDOU LOOP plays few notes: {notes:?}");
}

#[test]
fn save_as_and_a_power_cycle_bring_back_the_song_and_a_setting() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    for row in [0, 4, 8, 12] {
        toggle_row(&mut m, &fw, 0, row);
    }
    let made = song(&mut m, &fw).write(false);
    open_song_page(&mut m, &fw, SONG_SAVE_AS);
    turn(&mut m, ALGORITHM, 1);
    turn(&mut m, SELECT, 8);
    turn(&mut m, ALGORITHM, i32::from(b'1' - b' '));
    press(&mut m, SAVE);
    idle(&mut m, &fw);
    assert_eq!(song_name(&mut m, &fw), "NEW SONG1", "SAVE AS named it");
    let scroll = |m: &mut Machine| {
        m.soc
            .read8(fw.symbol("ui") + (ilp32::views::song + ilp32::song_view::scroll) as u32)
    };
    home_to_live(&mut m, &fw);
    press(&mut m, HOME);
    m.run_ms(WIPE_MS);
    settings_item(&mut m, VIEWS_SETTINGS_ROWS);
    turn(&mut m, ALGORITHM, 1);
    assert_eq!(scroll(&mut m), 1, "TRACKER ROWS: SCROLL");
    home_to_live(&mut m, &fw);
    m.run_ms(SETTINGS_DEBOUNCE_MS);
    let flash = m.soc.flash.mem.clone();
    drop(m);
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(&flash);
    run_past_boot(&mut m, &fw);
    idle(&mut m, &fw);
    assert_eq!(
        song_name(&mut m, &fw),
        "NEW SONG1",
        "the last song loads at power-on"
    );
    assert_eq!(song(&mut m, &fw).write(false), made, "with its steps");
    assert_eq!(scroll(&mut m), 1, "and TRACKER ROWS is still SCROLL");
    open_step(&mut m, &fw);
    let h = play_for(&mut m, &fw, rows_ms(16), 0);
    stop(&mut m, &fw);
    let env = envelope(&h.audio);
    let t0 = first_gate(&h);
    for row in [0u32, 4, 8, 12] {
        let at = t0 + f64::from(row) * ROW_MS;
        let o = onset_near(&env, at);
        assert!(
            matches!(o, Some(t) if (t - at).abs() <= ON_TIME_MS),
            "row {row}: onset {o:?}, want {at}"
        );
    }
    let hz = settled_hz(&h).expect("no note on the chip");
    assert!(
        cents_off(hz, hz_of_note(LANE_NOTE)).abs() < 25.0,
        "the steps at {hz:.1} Hz"
    );
}

fn in_key(note: u32) -> bool {
    note >= LANE_NOTE
        && [0, 3, 5, 7, 10].contains(&((note - LANE_NOTE) % 12))
        && note <= LANE_NOTE + 24
}

fn step_settled(m: &mut Machine, fw: &Firmware) {
    let st = fw.symbol("ui") + ilp32::views::step as u32;
    let gen_at = fw.symbol("ui") + (ilp32::views::song + ilp32::song_view::gen_) as u32;
    let base = fw.symbol("__song_region");
    let start = m.ms();
    loop {
        let quiet = m.soc.read8(gen_at + ilp32::song_gen::run as u32) == 0
            && m.soc.read8(gen_at + ilp32::song_gen::follow as u32) == 0
            && m.soc.read8(st + ilp32::step_view::qn as u32) == 0
            && m.soc.read8(st + ilp32::step_view::follow as u32) == 0
            && m.soc.read16(base + ilp32::gt2_song::pending as u32) == 0
            && m.soc
                .read32(fw.symbol("song_transport") + ilp32::song_ctl::inflight_n as u32)
                == 0;
        if quiet {
            return;
        }
        assert!(m.ms() - start < 10_000, "STEP's edits did not finish");
        m.run_ms(20);
    }
}

fn notes_of(p: &[[u8; 4]]) -> Vec<(usize, u32)> {
    p.iter()
        .enumerate()
        .filter(|(_, c)| is_note(c[0]))
        .map(|(r, c)| (r, u32::from(c[0] - GT2_FIRSTNOTE as u8)))
        .collect()
}

#[test]
fn dice_settings_on_step_are_kept_in_the_song_and_the_next_roll_uses_them() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    let before = song(&mut m, &fw).pattern(0).unwrap();
    m.key(EDIT, true);
    m.run_ms(40);
    m.key(SCL, true);
    turn(&mut m, K1, 4);
    turn(&mut m, K1 + 1, -1);
    turn(&mut m, K1 + 2, 1);
    turn(&mut m, K1 + 3, 3);
    m.key(SCL, false);
    m.key(EDIT, false);
    m.run_ms(400);
    step_settled(&mut m, &fw);
    let s = song(&mut m, &fw);
    assert_eq!(s.pattern(0).unwrap(), before, "a turn while held: no roll");
    let mut dice = song_dice::default();
    assert_eq!(unsafe { song_ext_get_dice(s.ptr(), &mut dice) }, 1);
    assert_eq!(
        (dice.density, dice.range, dice.scale[0], dice.rhythm),
        (4, 0, 1, 3),
        "the song holds FULL, 1 OCT, MAJ PENT and FOUR"
    );
    chord(&mut m, EDIT, SCL);
    assert_eq!(toast_text(&mut m, &fw), "NEW PATTERN");
    step_settled(&mut m, &fw);
    let diced = song(&mut m, &fw).pattern(0).unwrap();
    let notes = notes_of(&diced);
    assert_eq!(
        notes.len(),
        diced.len() - 1,
        "every step has a note (the end row aside)"
    );
    for &(row, note) in &notes {
        assert!(
            [0, 2, 4, 7, 9].contains(&((note + 120 - LANE_NOTE) % 12)),
            "row {row}: note {note} is out of the major pentatonic"
        );
        assert!(
            (LANE_NOTE..LANE_NOTE + 12).contains(&note),
            "row {row}: note {note} is out of the one octave"
        );
    }
}

#[test]
fn tracker_dices_the_shown_voice_on_its_root_and_sweeps_its_filter() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    press(&mut m, SEQ);
    m.run_ms(WIPE_MS);
    assert_view(&mut m, &fw, "tracker_view_view", "SEQ from STEP");
    let before = song(&mut m, &fw).pattern(0).unwrap();
    let sv = fw.symbol("ui") + ilp32::views::song as u32;
    let gen_at = sv + ilp32::song_view::gen_ as u32;
    m.key(EDIT, true);
    m.run_ms(40);
    m.key(SCL, true);
    turn(&mut m, K1, 2);
    turn(&mut m, K1 + 1, -1);
    let key = C4_KEY_INDEX;
    m.key(KEY0 + key, true);
    m.run_ms(100);
    m.key(KEY0 + key, false);
    m.run_ms(100);
    let canvas = sv + (ilp32::song_view::cv + ilp32::grid_canvas::ch) as u32;
    let top = TRACKER_VIEW_TOP + TRACKER_VIEW_ROW_CELLS * TRACKER_VIEW_DICE_ROW;
    assert_eq!(
        text_at(&mut m, canvas + top * GRID_COLS + 2, 23),
        " DICE - LET GO TO ROLL ",
        "the frame"
    );
    let root = JAM_ROOT_NOTE;
    assert_eq!(
        m.soc.read8(gen_at + ilp32::song_gen::root as u32),
        root,
        "the key set V1's root"
    );
    m.key(SCL, false);
    m.key(EDIT, false);
    m.run_ms(400);
    step_settled(&mut m, &fw);
    let s = song(&mut m, &fw);
    assert_eq!(s.pattern(0).unwrap(), before, "a turn while held: no roll");
    let mut dice = song_dice::default();
    assert_eq!(unsafe { song_ext_get_dice(s.ptr(), &mut dice) }, 1);
    assert_eq!((dice.density, dice.range), (4, 0), "FULL and 1 OCT");
    chord(&mut m, EDIT, SCL);
    assert_eq!(toast_text(&mut m, &fw), "NEW PATTERN");
    step_settled(&mut m, &fw);
    let diced = song(&mut m, &fw).pattern(0).unwrap();
    let notes = notes_of(&diced);
    assert_eq!(notes.len(), diced.len() - 1, "every row a note");
    for &(row, note) in &notes {
        let d = note - u32::from(root);
        assert!(
            d < 12 && [0, 3, 5, 7, 10].contains(&d),
            "row {row}: note {note} is off C-4's minor pentatonic"
        );
    }
    chord(&mut m, EDIT, PANEL_BTN_ENV as u8);
    let said = toast_text(&mut m, &fw);
    assert!(said == "SWEEP UP" || said == "SWEEP DOWN", "{said}");
    step_settled(&mut m, &fw);
    let swept = song(&mut m, &fw).pattern(0).unwrap();
    for (row, c) in swept.iter().enumerate().take(diced.len() - 1) {
        let want = if row % 2 == 0 { 0x0c } else { diced[row][2] };
        assert_eq!(c[2], want, "row {row}: {c:02x?}");
        assert_eq!(c[..2], diced[row][..2], "row {row}: the note stays");
    }
}

#[test]
fn a_bass_and_drums_lane_rolls_the_kit_and_the_bass_time_shared() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    let waves = song(&mut m, &fw).raw().ltable;
    m.key(EDIT, true);
    m.run_ms(40);
    m.key(SCL, true);
    turn(&mut m, PRESETS, 3);
    m.run_ms(100);
    let sv = fw.symbol("ui") + ilp32::views::song as u32;
    let canvas = sv + (ilp32::song_view::cv + ilp32::grid_canvas::ch) as u32;
    let top = STEP_VIEW_HELD_ROW;
    assert_eq!(
        text_at(&mut m, canvas + (top + 9) * GRID_COLS + 19, 8),
        "PRE ROLE"
    );
    assert_eq!(
        text_at(&mut m, canvas + (top + 10) * GRID_COLS + 19, 10),
        "BASS+DRUMS"
    );
    m.key(SCL, false);
    m.key(EDIT, false);
    m.run_ms(400);
    step_settled(&mut m, &fw);
    let s = song(&mut m, &fw);
    let mut scene = song_scene::default();
    assert_eq!(unsafe { song_ext_get_scene(s.ptr(), &mut scene) }, 1);
    assert_eq!(
        (scene.count, scene.role[0]),
        (3, SONG_ROLE_BASS_DRUMS as u8)
    );
    chord(&mut m, EDIT, SCL);
    assert_eq!(toast_text(&mut m, &fw), "NEW PATTERN");
    step_settled(&mut m, &fw);
    let s = song(&mut m, &fw);
    let p = s.pattern(0).unwrap();
    let mut bass = 0;
    for (row, c) in p.iter().enumerate().take(p.len() - 1) {
        match row % 8 {
            0 => assert_eq!(c[..2], [GT2_FIRSTNOTE as u8 + 36, 3], "row {row}: the kick"),
            4 => assert_eq!(
                c[..2],
                [GT2_FIRSTNOTE as u8 + 36, 4],
                "row {row}: the snare"
            ),
            _ if is_note(c[0]) => {
                assert_eq!(c[1], 1, "row {row}: the lane's instrument");
                bass += 1;
            }
            _ => {}
        }
    }
    assert!(bass > 4, "the bass between: {bass}");
    assert_eq!(s.raw().ltable, waves, "no table writes");
}

#[test]
fn an_arp_lane_rolls_chords_in_tracker() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    press(&mut m, SEQ);
    m.run_ms(WIPE_MS);
    assert_view(&mut m, &fw, "tracker_view_view", "SEQ from STEP");
    m.key(EDIT, true);
    m.run_ms(40);
    m.key(SCL, true);
    turn(&mut m, PRESETS, 4);
    m.run_ms(100);
    m.key(SCL, false);
    m.key(EDIT, false);
    m.run_ms(400);
    step_settled(&mut m, &fw);
    chord(&mut m, EDIT, SCL);
    assert_eq!(toast_text(&mut m, &fw), "NEW CHORDS");
    let start = m.ms();
    loop {
        step_settled(&mut m, &fw);
        let s = song(&mut m, &fw);
        if s.pattern(0).unwrap().iter().any(|c| c[2] == 8) {
            break;
        }
        assert!(m.ms() - start < 20_000, "no chords written");
        m.run_ms(100);
    }
    m.run_ms(400);
    step_settled(&mut m, &fw);
    let s = song(&mut m, &fw);
    let starts: Vec<u32> = (0..SONG_GEN_CHORDS)
        .map(|c| unsafe { song_gen_chord_find(s.ptr(), c) })
        .collect();
    assert!(starts.iter().all(|&p| p > 0), "the programs: {starts:?}");
    let p = s.pattern(0).unwrap();
    let hits: Vec<&[u8; 4]> = p.iter().filter(|c| is_note(c[0])).collect();
    assert!(hits.len() > 8);
    for c in hits {
        assert_eq!(c[2], 8, "{c:02x?}");
        assert!(starts.contains(&u32::from(c[3])), "{c:02x?}");
    }
}

#[test]
fn scl_before_edit_is_dices_chord_too() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    let before = song(&mut m, &fw).pattern(0).unwrap();
    let live = fw.symbol("instrument");
    let fx = live + ilp32::live::fx_select as u32;
    let slots = live + ilp32::live::slot as u32;
    let slot = m.soc.read8(fx);
    let amounts: Vec<u8> = (0..LIVE_SLOTS * LIVE_SLOT_FIELDS)
        .map(|i| m.soc.read8(slots + i))
        .collect();
    m.key(SCL, true);
    m.run_ms(5);
    m.key(EDIT, true);
    turn(&mut m, K1, 4);
    turn(&mut m, K1 + 2, 1);
    m.key(SCL, false);
    m.key(EDIT, false);
    m.run_ms(400);
    step_settled(&mut m, &fw);
    assert_eq!(m.soc.read8(fx), slot, "SCL selected no slot");
    let after: Vec<u8> = (0..LIVE_SLOTS * LIVE_SLOT_FIELDS)
        .map(|i| m.soc.read8(slots + i))
        .collect();
    assert_eq!(after, amounts, "no effect knob turned");
    let s = song(&mut m, &fw);
    assert_eq!(s.pattern(0).unwrap(), before, "a turn while held: no roll");
    let mut dice = song_dice::default();
    assert_eq!(unsafe { song_ext_get_dice(s.ptr(), &mut dice) }, 1);
    assert_eq!(
        (dice.density, dice.scale[0]),
        (4, 1),
        "the song holds FULL and MAJ PENT"
    );
}

#[test]
fn arp_or_lfo_before_edit_mutate_and_undo_on_step() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    chord(&mut m, EDIT, SCL);
    m.run_ms(400);
    step_settled(&mut m, &fw);
    let diced = song(&mut m, &fw).pattern(0).unwrap();
    let gate = fw.symbol("ui") + (ilp32::views::step + ilp32::step_view::gate) as u32;
    let lanes = m.soc.read8(gate);
    let lead = |m: &mut Machine, first: u8| {
        m.key(first, true);
        m.run_ms(5);
        m.key(EDIT, true);
        m.run_ms(60);
        m.key(first, false);
        m.key(EDIT, false);
        m.run_ms(400);
        step_settled(m, &fw);
    };
    lead(&mut m, ARP);
    assert_eq!(toast_text(&mut m, &fw), "MUTATED");
    let mutated = song(&mut m, &fw).pattern(0).unwrap();
    assert_ne!(mutated, diced, "mutated");
    assert_eq!(m.soc.read8(gate), lanes, "ARP's gate left alone");
    lead(&mut m, PANEL_BTN_LFO as u8);
    assert_ne!(
        song(&mut m, &fw).pattern(0).unwrap(),
        mutated,
        "an edit undone"
    );
}

#[test]
fn chance_dice_and_mutate_on_step_do_what_they_say() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = new_song_on_step(&fw);
    toggle_row(&mut m, &fw, 0, 0);
    m.key(KEY0, true);
    m.run_ms(80);
    m.turn(ALGORITHM, -5);
    m.run_ms(200);
    let (label, value) = popup(&mut m, &fw);
    m.key(KEY0, false);
    m.run_ms(200);
    assert_eq!(
        (label.as_str(), value.as_str()),
        ("CHANCE", "75%"),
        "the chance popup"
    );
    step_settled(&mut m, &fw);
    let s = song(&mut m, &fw);
    assert!(
        is_note(s.pattern(0).unwrap()[0][0]),
        "the held step kept its note"
    );
    let mut locks = vec![song_lock::default(); SONG_LOCKS_MAX as usize];
    let n = unsafe { song_ext_get_locks(s.ptr(), locks.as_mut_ptr(), SONG_LOCKS_MAX) } as usize;
    let chance: Vec<_> = locks[..n]
        .iter()
        .filter(|l| u32::from(l.param) == SONG_LOCK_CHANCE)
        .map(|l| (l.pattern, l.row, l.value))
        .collect();
    assert_eq!(chance, [(0, 0, 75)], "the song holds the chance on row 0");

    chord(&mut m, EDIT, SCL);
    assert_eq!(toast_text(&mut m, &fw), "NEW PATTERN", "DICE's toast");
    step_settled(&mut m, &fw);
    let diced = song(&mut m, &fw).pattern(0).unwrap();
    let notes = notes_of(&diced);
    println!("DICE: {notes:?}");
    assert!(notes.len() >= 8, "DICE wrote few notes: {notes:?}");
    for &(row, note) in &notes {
        assert_eq!(row % 2, 0, "DICE: a note off the eighths, row {row}");
        assert!(in_key(note), "DICE: note {note} on row {row} is out of key");
        let next = diced[row + 1][0];
        assert!(
            next == GT2_KEYOFF as u8 || is_note(next),
            "DICE: no key off after row {row} on a staccato lane"
        );
    }
    m.run_ms(2_500);
    let mut freqs = std::collections::BTreeSet::new();
    let h = play_for(&mut m, &fw, rows_ms(32), 0);
    stop(&mut m, &fw);
    for r in &h.regs {
        if r.gate != 0 {
            freqs.insert(r.freq);
        }
    }
    let loudest = rms_windows(&h.audio).into_iter().fold(0.0, f64::max);
    assert!(
        loudest > AUDIBLE_RMS,
        "DICE's pattern is not heard ({loudest:.0})"
    );
    assert!(
        freqs.len() >= 3,
        "DICE's pattern plays {} pitches",
        freqs.len()
    );

    chord(&mut m, EDIT, ARP);
    assert_eq!(toast_text(&mut m, &fw), "MUTATED", "MUTATE's toast");
    step_settled(&mut m, &fw);
    let mutated = song(&mut m, &fw).pattern(0).unwrap();
    let changed: Vec<usize> = (0..diced.len())
        .filter(|&r| {
            (is_note(diced[r][0]) || is_note(mutated[r][0])) && diced[r][0] != mutated[r][0]
        })
        .collect();
    println!("MUTATE changed rows {changed:?}");
    assert!(
        (1..=2).contains(&changed.len()),
        "MUTATE changed {} notes: rows {changed:?}",
        changed.len()
    );
    for (row, note) in notes_of(&mutated) {
        assert!(
            in_key(note),
            "MUTATE: note {note} on row {row} is out of key"
        );
    }
}

#[test]
fn every_note_key_sounds_its_note_and_lights_while_held() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = booted(&fw);
    let quiet = record(&mut m, 1_000);
    let loudest = rms_windows(&quiet).into_iter().fold(0.0, f64::max);
    assert!(
        loudest < SILENT_RMS,
        "LIVE sounds before a key: {loudest:.0}"
    );
    let mut wrong = Vec::new();
    for k in 0..PANEL_NOTE_KEYS as u8 {
        let want = hz_of_note(PANEL_BASE_NOTE - 12 + u32::from(k));
        m.soc.periph.led_take();
        m.key(KEY0 + k, true);
        m.run_ms(60);
        let x = record(&mut m, 200);
        let lit = m.soc.periph.led_take()[usize::from(KEY0 + k)];
        m.key(KEY0 + k, false);
        m.run_ms(350);
        let f0 = pitch(&x);
        let level = rms(&x);
        if (f0 / want - 1.0).abs() > 0.005 || level < AUDIBLE_RMS || lit <= 0.0 {
            wrong.push((k + 1, f0, want, level, lit));
        }
    }
    assert!(
        wrong.is_empty(),
        "keys off (key, Hz heard, Hz wanted, RMS, LED): {wrong:?}"
    );
}

#[test]
fn play_is_quiet_until_the_first_note() {
    let Some(fw) = firmware("the first-use tests") else {
        return;
    };
    let mut m = booted(&fw);
    let h = play_for(&mut m, &fw, 300, 0);
    stop(&mut m, &fw);
    let first = first_gate(&h);
    let before = &h.audio[..((first - 10.0) * FS / 1000.0) as usize];
    let loudest = rms_windows(before).into_iter().fold(0.0, f64::max);
    println!("before the first note ({first} ms): loudest {loudest:.0}");
    assert!(
        loudest < SILENT_RMS,
        "PLAY sounds before its first note: {loudest:.0} over 10 ms"
    );
}
