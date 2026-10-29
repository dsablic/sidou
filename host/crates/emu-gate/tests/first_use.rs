// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! First use, checked as a listener would (TESTING.md, "Emulator first, and the first-use
//! tests"): the panel driven the way a user drives it on a fresh song (the menu's SONG pages,
//! STEP, TRACKER, PLAY, GLO, the generative gestures, a power cycle), and what comes out judged
//! by what an ear notices: a sound loud enough over its length (RMS, not a peak a click passes),
//! its pitch (the chip's frequency register and the audio's own fundamental), on time (onsets at
//! the rows' times), and each instrument told apart from the others (pitch, brightness, length,
//! waveform), not sample for sample (the bit-exact tests are song.rs's and song_store.rs's).
//!
//! v0.4.0's device session found what these would have: a NEW song silent since v0.2.0 (its
//! lead at 0 Hz), NEW's PULSE and BASS sounding the same, and the first step press after
//! PRESETS auditioning the previous instrument. They found two more, both since fixed: the chip
//! kit's drums sounded about 55 ms after their row, not one frame
//! (the_kits_drums_sound_one_frame_after_their_row; the ADSR delay bug, decision record 0009),
//! and PLAY thudded before its first note (play_is_quiet_until_the_first_note).

mod common;

use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::song::Song;

const SELECT: u8 = PANEL_ENC_SELECT as u8;
const ALGORITHM: u8 = PANEL_ENC_ALGORITHM as u8;
const PRESETS: u8 = PANEL_ENC_PRESETS as u8;
const HOME: u8 = PANEL_BTN_HOME as u8;
const SAVE: u8 = PANEL_BTN_SAVE as u8;
const SEQ: u8 = PANEL_BTN_SEQ as u8;
const PLAY: u8 = PANEL_BTN_PLAY as u8;
const REC: u8 = PANEL_BTN_REC as u8;
const GLO: u8 = PANEL_BTN_GLO as u8;
const EDIT: u8 = PANEL_BTN_EDIT as u8;
const ARP: u8 = PANEL_BTN_ARP as u8;
const SCL: u8 = PANEL_BTN_SCL as u8;
const KEY0: u8 = PANEL_KEY_ID0 as u8;

/// app/views.c's SONG page actions, in their order (SELECT from the top).
const SONG_LOAD: i32 = 0;
const SONG_NEW: i32 = 1;
const SONG_SAVE_AS: i32 = 2;

/// A wipe or a raster over, with room (screen.rs SETTLE_MS).
const WIPE_MS: u64 = 700;
/// Song I/O's longest operation, with room (song_store.rs).
const IO_TIMEOUT_MS: u64 = 15_000;
/// A NEW song's tempo 6 on PAL: six 20 ms calls a row.
const ROW_MS: f64 = 120.0;
/// A NEW song's lanes play this note until PRESETS with a step held changes it (C-3).
const LANE_NOTE: u32 = 36;
/// The chip's clock (platform/fm1/audio.c: PAL).
const CHIP_HZ: f64 = SID_CLOCK_PAL as f64;

/// RMS over 10 ms that a sound a listener hears reaches at least (the chips' scale at the
/// pot's power-on reading). A click of a few samples stays far below it over 10 ms.
const AUDIBLE_RMS: f64 = 250.0;
/// RMS over 10 ms that silence stays below (the tail gate and the thump leave a little).
const SILENT_RMS: f64 = 40.0;

// ---- the panel, as a user's fingers ----

fn fixture() -> Option<Firmware> {
    let Some(build) = build_dir() else {
        skip_message("the first-use tests");
        return None;
    };
    Some(Firmware::load(&build))
}

/// A button pressed and let go, long enough for the key scan's debounce either way.
fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

/// `held` down, `id` pressed and let go, `held` up: EDIT + SCL and the like.
fn chord(m: &mut Machine, held: u8, id: u8) {
    m.key(held, true);
    m.run_ms(40);
    press(m, id);
    m.key(held, false);
    m.run_ms(40);
}

/// An encoder turned `clicks` detents; fm1-emu turns one every 8 ms, each waited out.
fn turn(m: &mut Machine, enc: u8, clicks: i32) {
    m.turn(enc, clicks);
    m.run_ms(10 * u64::from(clicks.unsigned_abs()) + 40);
}

fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, fw);
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

fn text_at(m: &mut Machine, address: u32, length: u32) -> String {
    (0..length)
        .map(|i| m.soc.read8(address + i) as char)
        .take_while(|&c| c != '\0')
        .collect()
}

fn toast(m: &mut Machine, fw: &Firmware) -> String {
    text_at(
        m,
        fw.symbol("ui") + ilp32::screen::toast as u32,
        SCREEN_TOAST_CHARS,
    )
}

/// The value popup's name and value (app/screen.h screen_popup_state).
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

/// Song I/O idle again (a load, a NEW, a save over).
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

/// The song in the device's region, as the host reads it (song_store.rs).
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

/// The menu (HOME), its SONG item opened (ALGORITHM clockwise), the cursor on `action`.
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

/// Out of the menu to LIVE: HOME let go until LIVE is on top.
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

/// SONG, NEW (on a song with no unsaved changes: no confirmation), back to LIVE.
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

/// SEQ from LIVE: STEP (the first song view after power-on).
fn open_step(m: &mut Machine, fw: &Firmware) {
    press(m, SEQ);
    m.run_ms(WIPE_MS);
    assert_view(m, fw, "step_view_view", "SEQ from LIVE");
}

/// A NEW song on STEP, the transport stopped.
fn new_song_on_step(fw: &Firmware) -> Machine {
    let mut m = booted(fw);
    new_song(&mut m, fw);
    open_step(&mut m, fw);
    m
}

/// The lane's instrument (STEP's, `step.instr[voice]`).
fn lane_instrument(m: &mut Machine, fw: &Firmware, voice: u32) -> u8 {
    m.soc
        .read8(fw.symbol("ui") + (ilp32::views::step + ilp32::step_view::instr) as u32 + voice)
}

/// PRESETS to instrument `to` on the selected lane, from `from`; its popup names it.
fn presets_to(m: &mut Machine, fw: &Firmware, voice: u32, to: u8, name: &str) {
    let from = lane_instrument(m, fw, voice);
    if from == to {
        // already there (a NEW song's lanes start at 01): away and back, as a user checks it
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

// ---- what a listener hears ----

/// Chip A's voice `v` as the interrupt renders it (song.rs chip_a_voice), with its pulse
/// width and its envelope's AD and SR as the registers hold them.
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

/// GoatTracker's note `n` (0 is C-0) in Hz, A-4 (57) at 440.
fn hz_of_note(n: u32) -> f64 {
    440.0 * 2f64.powf((f64::from(n) - 57.0) / 12.0)
}

/// Semitones from `want` to `hz`.
fn cents_off(hz: f64, want: f64) -> f64 {
    1200.0 * (hz / want).log2()
}

/// A sound as heard: the audio from the key's press and the voice's registers each ms.
#[derive(Clone, Debug)]
struct Heard {
    audio: Vec<f32>,
    regs: Vec<Voice>,
    /// Each ms, the gates of all three voices (bit v: voice v).
    gates: Vec<u8>,
}

/// 10 ms windows' RMS.
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

/// What tells two sounds apart for an ear: how loud (the loudest 10 ms), how long (10 ms
/// windows within 20 dB of the loudest), how bright (the spectral centroid over the first
/// 4096 samples from the loudest window's start), how busy (zero crossings a second over its
/// length), how much of it holds on and how pitched it is.
#[derive(Clone, Copy, Debug)]
struct Traits {
    loudest: f64,
    length_ms: f64,
    centroid: f64,
    crossings: f64,
    /// How much of it holds on: the mean 10 ms RMS from 50 to 150 ms after the loudest, over the
    /// loudest (a held note near 1, a hit's decay far less).
    body: f64,
    /// How pitched: the normalised autocorrelation's highest peak at lags of 2 to 20 ms over
    /// 2048 samples from 30 ms after the loudest (a tone near 1, noise near 0).
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

/// Step key `step` held for `hold_ms`, let go (a toggle), and `total_ms` from its press
/// recorded, with voice `voice`'s registers each ms.
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

/// `ms` of audio and registers (voice `voice`'s, and every voice's gate) each ms, `each`
/// called before each ms.
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

/// The frequency the voice settled on while gated (the last gated ms's), in Hz.
fn settled_hz(h: &Heard) -> Option<f64> {
    h.regs
        .iter()
        .rev()
        .find(|r| r.gate != 0 && r.freq != 0)
        .map(|r| hz_of_freq(r.freq))
}

// ---- the template's instruments ----

/// A NEW song's instruments (app/song_io.c's template; core/song/gt2_kit.c; decision record
/// 0009): number, name, and the note its pitch settles on where it has one (None: the lane's
/// note, the step's own; Some(n): the drum's table's last absolute note).
#[derive(Clone, Copy, Debug)]
enum Pitch {
    /// The step's note (the lead, the bass).
    Played,
    /// The drum's table ends on this absolute note (KICK's body, the toms).
    Fixed(u32),
    /// Noise or a sample: no pitch to hear.
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

/// The audio's fundamental over 8192 samples (186 ms) from 50 ms after the sound starts (its
/// first 5 ms over half the floor): past the onset's thump, which an estimate over the start
/// takes for a low note.
fn heard_pitch(x: &[f32]) -> Option<f64> {
    let w = (FS / 200.0) as usize;
    let k = envelope(x).iter().position(|&r| r > AUDIBLE_RMS / 2.0)?;
    let from = k * w + (FS / 20.0) as usize;
    (from + 8192 <= x.len()).then(|| pitch(&x[from..from + 8192]))
}

/// A sound as an ear takes it: the audio's traits, its pitch (where the instrument has one:
/// the chip's frequency where it settles), the waveforms it goes through (the control
/// register's waveform bits while gated, repeats dropped) and its pulse width.
#[derive(Clone, Debug)]
struct Sound {
    traits: Traits,
    pitch: Option<f64>,
    /// The audio's own fundamental over the note's first 8192 samples, for the played ones.
    heard_f0: Option<f64>,
    waves: Vec<u8>,
    pw: Option<u32>,
    /// AD and SR while gated (not part of telling sounds apart: an ear hardly hears PULSE's
    /// 19 A8 from BASS's 12 98 on a short note; the first-press test reads them).
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

/// Whether an ear tells a and b apart: a clear difference in loudness, length, brightness or
/// busyness, in how pitched they sound where both hold on (a tone from a hit's thud), in pitch (more than a semitone) where both have one, in the waveforms they go
/// through, or in pulse width (a tenth) where both are pulses.
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

/// Whether two presses sounded the same instrument: on the chip its envelope, its first
/// waveform, its pulse width and its pitch; a digi drum (its voice muted, the sample on the
/// volume register) by length and brightness, which a noise burst's hits do not vary much.
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

/// A step key's audition: held this long (a tap), listened to this long from its press.
const TAP_HOLD_MS: u64 = 200;
const TAP_MS: u64 = 600;
/// PRESETS turned, then a step pressed this soon after, as a user's hand moves from the knob
/// to the keys.
const KNOB_TO_KEY_MS: u64 = 150;

/// Every template instrument picked on V1 with PRESETS and auditioned on step 1, empty: the
/// first press after the change (it toggles the step on) and the second (off again, so the
/// step is empty for the next): (first, second) for each, in INSTRUMENTS' order.
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

/// The pitch an instrument should be heard at: the lane's note for the lead (and the bass's
/// pitch class: its octave is new_bass's), the drum's last table note for KICK and the toms.
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
            off = (off + 600.0).rem_euclid(1200.0) - 600.0; // its pitch class
        }
        assert!(
            off.abs() < 25.0,
            "{name}: {what} sounds at {hz:.1} Hz, {off:+.0} cents from {want:.1} Hz"
        );
    }
}

/// NEW, STEP, and every instrument of the template picked with PRESETS (its popup naming it)
/// and auditioned on an empty step: each is heard (RMS over 10 ms, not a peak), at its pitch
/// where it has one (PULSE at the lane's C-3, KICK's body at A#1, TOM LO and TOM HI ending at
/// C#3 and G#3), and an ear tells each from every other (PULSE from BASS is new_bass's test).
/// The second press of each is judged here, the first is the next test's.
#[test]
fn every_template_instrument_auditions_on_step_heard_at_its_pitch_and_each_its_own() {
    let Some(fw) = fixture() else { return };
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
                continue; // new_bass_sounds_an_octave_below_the_lead_and_thinner
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

/// The first press on an empty step 150 ms after PRESETS auditions the instrument PRESETS
/// picked, not the one before it: the same instrument as the second press (same_instrument:
/// its envelope, first waveform, pulse width and pitch; a digi drum by length and
/// brightness). v0.4.0's device session heard the previous instrument on the first press;
/// in fm1-emu it passes on c104d1e5 (also with SONG_SLICE_UNITS at 8 and with chip A handed
/// back first), so the device's case is not reproduced here yet: it stays on as the guard.
#[test]
fn the_first_step_press_after_presets_auditions_the_instrument_picked() {
    let Some(fw) = fixture() else { return };
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

/// NEW's BASS is heard as a bass, not as the lead again: on the same step an octave below
/// PULSE, and thinner (a quarter pulse, $400, against the lead's square).
#[test]
fn new_bass_sounds_an_octave_below_the_lead_and_thinner() {
    let Some(fw) = fixture() else { return };
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

// ---- PLAY: the steps heard on time ----

/// The RMS over 10 ms every 5 ms, from each 5 ms on (a kick's 58 Hz body does not flicker
/// in it as it does in a 5 ms window).
fn envelope(x: &[f32]) -> Vec<f64> {
    let hop = (FS / 200.0) as usize;
    (0..x.len() / hop)
        .map(|k| rms(&x[k * hop..(k * hop + 2 * hop).min(x.len())]))
        .collect()
}

/// Where a new sound starts near `at_ms` (searched from `at_ms` - 40 to + 80), in ms, as an
/// ear hears a hit start: a window over half the floor and at least 1.8 times the quietest of
/// the 20 ms before it (a rise, not the last note's tail); of those the nearest to `at_ms`.
/// None: nothing starts there.
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

/// When PLAY's first note went on: the first ms a voice's gate is on.
fn first_gate(h: &Heard) -> f64 {
    h.gates
        .iter()
        .position(|&g| g != 0)
        .expect("PLAY gated no voice") as f64
}

/// What PLAY played: the audio from the press and voice `v`'s registers each ms.
fn play_for(m: &mut Machine, fw: &Firmware, ms: u64, v: usize) -> Heard {
    m.key(PLAY, true);
    listen(m, fw, v, ms, |m, i| {
        if i == 30 {
            m.key(PLAY, false);
        }
    })
}

/// PLAY again: stopped, the releases rung out.
fn stop(m: &mut Machine, fw: &Firmware) {
    press(m, PLAY);
    m.run_ms(800);
    assert!(!playing(m, fw), "PLAY stops the song");
}

/// Step `row` of the selected lane toggled on with a short tap (ALGORITHM pages there), the
/// edit applied.
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

/// How far from its time an onset may be: half a frame of the player and a 5 ms window.
const ON_TIME_MS: f64 = 15.0;

/// When INSTRUMENTS[i] sounds after its row: the chip kit a frame later (its first frame is
/// the test bit; core/song/gt2_kit.c), the lead, the bass and the digi drums at once.
fn drum_delay(i: usize) -> f64 {
    if (2..10).contains(&i) {
        20.0
    } else {
        0.0
    }
}

/// Rows a NEW song's pattern plays in `ms`.
fn rows_ms(rows: u32) -> u64 {
    (f64::from(rows) * ROW_MS) as u64
}

/// What PLAY did with one instrument's step: its row's time and its onset (ms from the
/// press), the loudest 10 ms of its row's time, and whether the chip played its envelope.
struct Played {
    name: &'static str,
    row_at: f64,
    onset: Option<f64>,
    loudest: f64,
    own: bool,
}

/// NEW, and on V1's lane the fourteen instruments one after another (PRESETS, then a step:
/// rows 0, 4, 8 .. 52, over the four pages), then PLAY over them; the first note's time
/// (ms from the press) and each step's.
fn play_every_instrument(fw: &Firmware) -> (f64, Vec<Played>) {
    let mut m = new_song_on_step(fw);
    for (i, &(n, name, _)) in INSTRUMENTS.iter().enumerate() {
        presets_to(&mut m, fw, 0, n, name);
        toggle_row(&mut m, fw, 0, 4 * i as u32);
    }
    turn(&mut m, ALGORITHM, -4);
    m.run_ms(2_500); // the audition hands chip A back
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

/// NEW, the fourteen instruments one after another on V1's steps, PLAY: each is heard in its
/// row's time (RMS over a floor), plays its own instrument (the chip's envelope its AD and SR;
/// the digi drums mute their voice and play on the volume register), and is on time: the
/// lead, the bass and the digi drums within ON_TIME_MS of their row (rows 480 ms apart from
/// the first note), the chip kit within its row's first 80 ms (its exact frame is the next
/// test's). The first note comes within a row and 60 ms of PLAY: the player sounds row 0 a
/// row after it starts (140 ms at tempo 6; SIDOU LOOP's too, bit-exact with the host's
/// GoatTracker render: song.rs).
#[test]
fn every_template_instrument_on_steps_plays_heard_and_on_time() {
    let Some(fw) = fixture() else { return };
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

/// The chip kit's drums sound one frame after their row (decision record 0009: their first
/// frame is the test bit with the gate off, "so every drum sounds one frame after its row").
/// With a release of their own in that frame they sounded about 55 ms after it (the ADSR
/// delay bug), KICK's first gated frame lost, a flam against a lead note on the same row.
#[test]
fn the_kits_drums_sound_one_frame_after_their_row() {
    let Some(fw) = fixture() else { return };
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

/// GLO held and a lane's key (1-3) mutes it: NEW, V1 PULSE on rows 0, 4, 8, 12, V2 KICK on
/// 2, 6, 10, 14; PLAY: a hit on every even row. V1 muted: PLAY again, the kicks only (no onset
/// on V1's rows, its gate never on). V2 muted as well: silence. Both back: both heard.
#[test]
fn glo_mutes_a_lane_and_only_that_lane_falls_silent() {
    let Some(fw) = fixture() else { return };
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
    // Each PLAY's row 0 comes as long after the press as the first's: its first gate.
    let hits = |m: &mut Machine, t0: Option<f64>| -> (Vec<Option<f64>>, Heard, f64) {
        let h = play_for(m, &fw, rows_ms(16) + 300, 0);
        stop(m, &fw);
        let env = envelope(&h.audio);
        let t0 = t0.unwrap_or_else(|| first_gate(&h));
        // heard in its row's time (the kick's exact frame is the_kits_drums_sound_..'s)
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

// ---- TRACKER: REC enters a note, SEQ walks the song views ----

/// NEW, SEQ to STEP and TRACKER, REC: a note key writes its note and the instrument (01) into
/// row 0 and the cursor moves on; REC off and PLAY: the note is heard at its pitch from the
/// start (the audio's fundamental and the chip's). Then SEQ walks ORDER, INSTR, TABLE, STEP,
/// TRACKER: the view on top each time.
#[test]
fn tracker_rec_enters_a_note_that_plays_and_seq_walks_every_song_view() {
    let Some(fw) = fixture() else { return };
    let mut m = new_song_on_step(&fw);
    press(&mut m, SEQ);
    m.run_ms(WIPE_MS);
    assert_view(&mut m, &fw, "tracker_view_view", "SEQ from STEP");
    let sv = fw.symbol("ui") + ilp32::views::song as u32;
    press(&mut m, REC);
    m.run_ms(100);
    assert_eq!(m.soc.read8(sv + ilp32::song_view::rec as u32), 1, "REC on");
    let key = 7u8; // C4 on the panel at octave 0 (app/panel.h: key 0 is F3)
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

// ---- SONG: the built-in loop, SAVE AS and a power cycle ----

/// NEW (unsaved changes), then SONG's LOAD and its first entry, SIDOU LOOP, confirmed (YES):
/// the built-in song back as the boot had it, unnamed; PLAY: heard all through four seconds
/// (every half second over the floor), its voices moving through several notes.
#[test]
fn song_load_sidou_loop_plays_it_heard_all_through() {
    let Some(fw) = fixture() else { return };
    let mut m = booted(&fw);
    let builtin = song(&mut m, &fw).write(false);
    new_song(&mut m, &fw);
    assert_ne!(song(&mut m, &fw).write(false), builtin, "NEW replaced it");
    open_song_page(&mut m, &fw, SONG_LOAD);
    turn(&mut m, ALGORITHM, 1);
    m.run_ms(300);
    turn(&mut m, ALGORITHM, 1); // its first entry, SIDOU LOOP
    m.run_ms(200);
    turn(&mut m, SELECT, 1); // the confirmation's YES (NEW left changes)
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

/// NEW, four steps of PULSE on STEP, SONG's SAVE AS (NEW SONG, one character added: NEW SONG1,
/// SAVE), and SETTINGS' TRACKER ROWS changed to SCROLL; once the settings stored themselves,
/// a power cycle with the flash kept: the song is loaded at power-on by its name, its steps and
/// the setting are there, and PLAY plays the steps at C-3.
#[test]
fn save_as_and_a_power_cycle_bring_back_the_song_and_a_setting() {
    let Some(fw) = fixture() else { return };
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
    m.run_ms(6_000); // the settings' debounce: stored
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

// ---- STEP's generative play ----

/// The minor pentatonic on the lane's C-3 and the octave above (docs/reference/controls.md
/// "Chance, MUTATE and DICE").
fn in_key(note: u32) -> bool {
    note >= LANE_NOTE
        && [0, 3, 5, 7, 10].contains(&((note - LANE_NOTE) % 12))
        && note <= LANE_NOTE + 24
}

/// STEP's editor quiet: no generative run, no action queued, no edit in flight.
fn step_settled(m: &mut Machine, fw: &Firmware) {
    let st = fw.symbol("ui") + ilp32::views::step as u32;
    let base = fw.symbol("__song_region");
    let start = m.ms();
    loop {
        let quiet = m.soc.read8(st + ilp32::step_view::run as u32) == 0
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

/// NEW, STEP: EDIT + SCL held with K1 and K3 turned sets DICE's density (every step) and scale
/// (major pentatonic) in the song and rolls nothing at the release; the next EDIT + SCL rolls
/// them: a note on every row, all in the major pentatonic on the lane's C-3.
#[test]
fn dice_settings_on_step_are_kept_in_the_song_and_the_next_roll_uses_them() {
    let Some(fw) = fixture() else { return };
    let mut m = new_song_on_step(&fw);
    let before = song(&mut m, &fw).pattern(0).unwrap();
    m.key(EDIT, true);
    m.run_ms(40);
    m.key(SCL, true);
    m.run_ms(40);
    turn(&mut m, PANEL_ENC_K1 as u8, 4);
    turn(&mut m, PANEL_ENC_K1 as u8 + 2, 1);
    m.key(SCL, false);
    m.key(EDIT, false);
    m.run_ms(400);
    step_settled(&mut m, &fw);
    let s = song(&mut m, &fw);
    assert_eq!(s.pattern(0).unwrap(), before, "a turn while held: no roll");
    let (mut d, mut sc) = (0u8, 0u8);
    assert_eq!(unsafe { song_ext_get_dice(s.ptr(), &mut d, &mut sc) }, 1);
    assert_eq!((d, sc), (4, 1), "the song holds FULL and MAJ PENT");
    chord(&mut m, EDIT, SCL);
    assert_eq!(toast(&mut m, &fw), "NEW PATTERN");
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
    }
}

/// NEW, STEP: a step with a note held and ALGORITHM turned back five detents: the popup says
/// CHANCE 75%, the step keeps its note and the song holds the lock. EDIT + SCL (DICE): NEW
/// PATTERN, the lane's notes on the eighths, all in key (C minor pentatonic over two octaves),
/// a key off after each, and PLAY plays them, heard, through several pitches. EDIT + ARP
/// (MUTATE): MUTATED, one or two notes changed, still in key.
#[test]
fn chance_dice_and_mutate_on_step_do_what_they_say() {
    let Some(fw) = fixture() else { return };
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
    assert_eq!(toast(&mut m, &fw), "NEW PATTERN", "DICE's toast");
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
    assert_eq!(toast(&mut m, &fw), "MUTATED", "MUTATE's toast");
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

// ---- LIVE: the keys as a tuner and the eye see them ----

/// After the boot LIVE is silent until a key is played (every 10 ms of a second under the
/// silence floor), then each of the 27 note keys at OCTAVE 0 sounds its note (key 1 F3,
/// 174.61 Hz, up a semitone a key to G5): heard, the audio's fundamental within 0.5 % of the
/// note, and its LED lit while it is held.
#[test]
fn every_note_key_sounds_its_note_and_lights_while_held() {
    let Some(fw) = fixture() else { return };
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

/// PLAY on a fresh boot (SIDOU LOOP, from LIVE) is quiet until the song's first note: nothing
/// in the 140 ms the player takes to sound row 0. It thudded before (an RMS of about 700 over
/// 10 ms, from 9 ms after the press): chip B, never rendered since its boot reset, played its
/// power-on envelope release (sid_init's 0xaa) when the keys moved to it, under their setup;
/// chip B's new sid now plays it out at once (app/reg_ring.c power_on_out).
#[test]
fn play_is_quiet_until_the_first_note() {
    let Some(fw) = fixture() else { return };
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
