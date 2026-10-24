// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The SID drum kit (core/song/gt2_kit.h, decision record 0009): each drum rendered through
//! the song transport on both chip models, and the NEW song that holds it as GoatTracker
//! reads and writes it.

use sidou_sys::*;
use sidou_tests::signal_metrics::*;
use sidou_tests::song::*;
use sidou_tests::transport::*;

const DRUMS: usize = GT2_KIT_DRUMS as usize;
const NAMES: [&str; DRUMS] = [
    "KICK", "SNARE", "CL HAT", "OP HAT", "CLAP", "TOM LO", "TOM HI", "RIM",
];
const FS: f64 = SAMPLE_RATE as f64;
/// A PAL frame in samples, about.
const FRAME: usize = 882;

fn kit_instr(d: usize) -> [u8; 25] {
    // SAFETY: gt2_kit_drum is 25 bytes, an instrument's.
    unsafe { std::mem::transmute::<gt2_kit_drum, [u8; 25]>(gt2_kit_instr[d]) }
}

/// The kit as a song: its instruments 1-8 and its rows, the drum `d` at row 0 of voice 1.
fn drum_song(d: usize) -> Vec<u8> {
    let mut s = SngSpec::new();
    s.instr = (0..DRUMS).map(kit_instr).collect();
    // SAFETY: plain constant data.
    let (wave, pulse) = unsafe {
        (
            (&raw const gt2_kit_wave).read(),
            (&raw const gt2_kit_pulse).read(),
        )
    };
    s.tables = vec![
        wave[0]
            .iter()
            .copied()
            .zip(wave[1].iter().copied())
            .collect(),
        pulse[0]
            .iter()
            .copied()
            .zip(pulse[1].iter().copied())
            .collect(),
        vec![],
        vec![],
    ];
    let mut p0 = vec![[0xbdu8, 0, 0, 0]; 64];
    p0[0] = [0x60 + 36, d as u8 + 1, 0, 0];
    s.patterns[0].0 = p0;
    s.encode()
}

/// The drum on chip A of `model` from its row's call, `ms` milliseconds, as floats.
fn render(d: usize, model: sid_model, ms: usize) -> Vec<f32> {
    let mut song = Song::host();
    song.read(&drum_song(d), 0).unwrap();
    let mut t = Transport::new(&mut song);
    unsafe { sid_init(&mut *t.chip, model, SID_CLOCK_PAL, SAMPLE_RATE) };
    assert!(t.play(0, 0));
    assert!(t.run_until(1000, |t| t.calls() > 0));
    let n = ms * SAMPLE_RATE as usize / 1000;
    let mut out = Vec::with_capacity(n + BLOCK);
    let mut b = [0i16; BLOCK];
    while out.len() < n {
        t.pass();
        t.block(&mut b);
        out.extend(b.iter().map(|&v| f32::from(v) / 32768.0));
    }
    out.truncate(n);
    out
}

fn onset(x: &[f32]) -> usize {
    x.iter()
        .position(|v| v.abs() > 0.02)
        .expect("the drum sounds")
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / x.len() as f64).sqrt()
}

fn db(v: f64) -> f64 {
    20.0 * v.max(1e-9).log10()
}

/// x through two one-pole low-passes at fc.
fn low_pass(x: &[f32], fc: f64) -> Vec<f32> {
    let a = 1.0 - (-2.0 * std::f64::consts::PI * fc / FS).exp();
    let (mut y1, mut y2) = (0.0f64, 0.0f64);
    x.iter()
        .map(|&v| {
            y1 += a * (f64::from(v) - y1);
            y2 += a * (y1 - y2);
            y2 as f32
        })
        .collect()
}

fn models() -> [(sid_model, &'static str); 2] {
    [(SID_MODEL_6581, "6581"), (SID_MODEL_8580, "8580")]
}

/// Every drum sounds on both chips, from the frame after its row (its first frame is the
/// test bit), loud at its start and fading to silence.
#[test]
fn every_drum_sounds_on_both_chips_and_fades() {
    for (d, name) in NAMES.iter().enumerate() {
        for (model, m) in models() {
            let x = render(d, model, 1500);
            let on = onset(&x);
            let start = db(rms(&x[on..on + FRAME / 2]));
            let end = db(rms(&x[x.len() - FRAME * 5..]));
            println!(
                "{} {m}: onset at {on}, {start:.1} dB, the last 100 ms {end:.1} dB",
                name
            );
            assert!(start > -32.0, "{} {m}: {start:.1} dB at its start", name);
            assert!(
                end < start - 30.0,
                "{} {m}: still {end:.1} dB after 1.5 s",
                name
            );
        }
    }
}

/// The pitched drums fall: the kick from its click to a body near 58 Hz, the toms by a
/// third or more, the high tom above the low one.
#[test]
fn the_kick_and_the_toms_fall_in_pitch() {
    for (model, m) in models() {
        let f0 = |d: usize, frame: usize| {
            let x = render(d, model, 400);
            let at = onset(&x) + frame * FRAME;
            estimate_f0(&x[at..at + FRAME], FS, 30.0, 1000.0)
        };
        let (kick_early, kick_body) = (f0(GT2_KIT_KICK as usize, 0), f0(GT2_KIT_KICK as usize, 6));
        println!("{m}: kick {kick_early:.0} Hz, then {kick_body:.0} Hz");
        assert!(kick_early > 1.4 * kick_body, "{m}: the kick does not fall");
        assert!(
            (50.0..68.0).contains(&kick_body),
            "{m}: the kick's body at {kick_body:.0} Hz"
        );
        for tom in [GT2_KIT_LOW_TOM as usize, GT2_KIT_HIGH_TOM as usize] {
            let (a, b) = (f0(tom, 0), f0(tom, 6));
            println!("{m}: {} {a:.0} Hz, then {b:.0} Hz", NAMES[tom]);
            assert!(a > 1.15 * b, "{m}: {} does not fall", NAMES[tom]);
        }
        let ratio = f0(GT2_KIT_HIGH_TOM as usize, 6) / f0(GT2_KIT_LOW_TOM as usize, 6);
        assert!(
            (1.4..1.6).contains(&ratio),
            "{m}: the toms are {ratio:.2} apart, not a fifth"
        );
    }
}

/// The kick has weight on both models: most of it is below 120 Hz, as much on the 8580 as on
/// the 6581.
#[test]
fn the_kick_has_weight_on_both_models() {
    let mut share = Vec::new();
    for (model, m) in models() {
        let x = render(GT2_KIT_KICK as usize, model, 400);
        let x = &x[onset(&x)..];
        let low = db(rms(&low_pass(x, 120.0))) - db(rms(x));
        println!("{m}: the kick's band below 120 Hz is {low:.1} dB of it");
        assert!(low > -4.0, "{m}: the kick's low band is {low:.1} dB of it");
        share.push(low);
    }
    assert!(
        (share[0] - share[1]).abs() < 2.0,
        "the models' kicks differ: {share:?}"
    );
}

/// The hats are noise at its brightest; the closed one is short, the open one long.
#[test]
fn the_hats_are_bright_and_the_closed_one_short() {
    for (model, m) in models() {
        let fall = |d: usize| {
            let x = render(d, model, 800);
            let x = &x[onset(&x)..];
            let crossings = x[..FRAME]
                .windows(2)
                .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                .count();
            let env = rms_envelope_db(x, FRAME / 2);
            let below = env.iter().position(|&e| e < env[0] - 24.0).unwrap();
            (
                crossings as f64 * FS / FRAME as f64 / 2.0,
                below * FRAME / 2,
            )
        };
        let (closed_hz, closed) = fall(GT2_KIT_CLOSED_HAT as usize);
        let (open_hz, open) = fall(GT2_KIT_OPEN_HAT as usize);
        println!(
            "{m}: hats at {closed_hz:.0} and {open_hz:.0} Hz, -24 dB after {closed} and {open}"
        );
        assert!(
            closed_hz > 4000.0 && open_hz > 4000.0,
            "{m}: the hats are dull"
        );
        assert!(
            closed * 3 < open,
            "{m}: the closed hat is not short against the open one"
        );
        assert!(
            closed < FRAME * 5,
            "{m}: the closed hat rings {closed} samples"
        );
    }
}

/// The clap is three bursts: its level dips between them.
#[test]
fn the_clap_is_three_bursts() {
    for (model, m) in models() {
        let x = render(GT2_KIT_CLAP as usize, model, 400);
        let x = &x[onset(&x)..];
        let env: Vec<f64> = rms_envelope_db(x, FRAME / 4);
        let dips = (1..16)
            .filter(|&i| env[i] < env[i - 1] - 3.0 && env[i + 2] > env[i] + 3.0)
            .count();
        println!(
            "{m}: clap {:?}",
            env[..16].iter().map(|e| e.round()).collect::<Vec<_>>()
        );
        assert!(dips >= 2, "{m}: the clap has {dips} dips");
    }
}

fn template() -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        let n = unsafe {
            song_io_template(
                std::ptr::null_mut(),
                out.len() as u32,
                buf.as_mut_ptr(),
                256,
            )
        };
        if n == 0 {
            return out;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
}

/// NEW's song holds the kit from SONG_IO_KIT, its rows first, and is canonical: GoatTracker
/// reads it and writes it back byte for byte, and so does SIDOU.
#[test]
fn the_new_song_holds_the_kit_and_goattracker_keeps_it() {
    let sng = template();
    let mut song = Song::host();
    song.read(&sng, 0).unwrap();
    let raw = song.raw();
    for (d, name) in NAMES.iter().enumerate() {
        assert_eq!(
            raw.instr[SONG_IO_KIT as usize - 1 + d],
            kit_instr(d),
            "{name}"
        );
    }
    assert_eq!(
        gt2_ref::canonical(&sng).expect("GoatTracker loads it"),
        sng,
        "GoatTracker saves NEW's song differently"
    );
    assert_eq!(song.write(false), sng, "SIDOU saves NEW's song differently");
}

/// NEW's bass is the lead an octave down (a wavetable right column of -12, $74) with a
/// quarter-width pulse of its own row after the kit's; the lead keeps the square's $800.
#[test]
fn the_new_bass_is_an_octave_down_with_a_thinner_pulse() {
    let sng = template();
    let mut song = Song::host();
    song.read(&sng, 0).unwrap();
    let raw = song.raw();
    let (lead, bass) = (&raw.instr[0], &raw.instr[1]);
    let pulse_row = |i: &[u8; 25]| usize::from(i[3]) - 1;
    assert_eq!(
        raw.ltable[1][pulse_row(lead)],
        0x88,
        "the lead's width $800"
    );
    assert_eq!(
        raw.ltable[1][pulse_row(bass)],
        0x84,
        "the bass's width $400"
    );
    assert_eq!(raw.ltable[1][pulse_row(bass) + 1], 0xff, "the pulse stops");
    let wave_row = |i: &[u8; 25]| usize::from(i[2]) - 1;
    assert_eq!(raw.rtable[0][wave_row(lead)], 0x00, "the lead at the note");
    assert_eq!(raw.rtable[0][wave_row(bass)], 0x74, "the bass at -12");
    assert_eq!(raw.ltable[0][wave_row(bass) + 1], 0xff, "the wave stops");
}
