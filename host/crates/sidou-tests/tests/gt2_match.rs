// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The song player against GoatTracker's own (M2c Tasks 4-5b; decision record 0007,
//! "Equivalence, defined"): for each frame, each call's SID writes from SIDOU's C player
//! equal those of GoatTracker 2.77's packed player, packed by gt2reloc with the same
//! options and run in gt2-ref's host 6502.

use gt2_ref::trace::trace;
use gt2_ref::Options;
use sidou_sys::*;
use sidou_tests::player::*;
use sidou_tests::song::*;

const FRAMES: usize = 3000;

/// gt2reloc's options for SIDOU's (song_ext.h's `GT2_PLAYER_*` to GoatTracker's
/// `PLAYER_*`).
fn ref_options(o: &gt2_options) -> Options {
    let bits = [
        (GT2_PLAYER_BUFFERED, gt2_ref::PLAYER_BUFFERED),
        (GT2_PLAYER_SOUNDEFFECTS, gt2_ref::PLAYER_SOUNDEFFECTS),
        (GT2_PLAYER_VOLUME, gt2_ref::PLAYER_VOLUME),
        (GT2_PLAYER_AUTHORINFO, gt2_ref::PLAYER_AUTHORINFO),
        (GT2_PLAYER_ZPGHOSTREGS, gt2_ref::PLAYER_ZPGHOSTREGS),
        (GT2_PLAYER_NOOPTIMIZATION, gt2_ref::PLAYER_NOOPTIMIZATION),
        (GT2_PLAYER_FULLBUFFERED, gt2_ref::PLAYER_FULLBUFFERED),
    ];
    let mut version = 0;
    for (ours, theirs) in bits {
        if o.player_flags as u32 & ours != 0 {
            version |= theirs;
        }
    }
    Options {
        hr_adsr: o.hr_adsr as u32,
        multiplier: o.multispeed as u32,
        ntsc: o.ntsc as u32,
        finevibrato: o.finevibrato as u32,
        optimize_pulse: o.optimize_pulse as u32,
        optimize_realtime: o.optimize_realtime as u32,
        player_version: version,
        sid_address: o.sid_address as u32,
        player_address: (o.player_page as u32) << 8,
        zeropage: o.zeropage as u32,
        sid_model: o.sid_model as u32,
        ..Options::default()
    }
}

fn hex(w: &[(u8, u8)]) -> String {
    w.iter()
        .map(|(r, v)| format!("{r:02X}={v:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Plays `frames` frames of the subtune on both players; Ok(writes compared), or the
/// first mismatch with both lists and the C player's channels.
fn compare(sng: &[u8], subtune: u8, frames: usize, o: &gt2_options) -> Result<usize, String> {
    let t = trace(sng, subtune, frames, &ref_options(o)).map_err(|e| format!("trace: {e}"))?;
    let mut song = Song::host();
    song.read(sng, 0).map_err(|i| format!("read: {}", i.msg))?;
    let mut p = Player::new(&song, subtune as u32, o).ok_or("the player refused the subtune")?;
    let mut writes = 0;
    let mut last: Vec<String> = Vec::new();
    for (f, frame) in t.frames.iter().enumerate() {
        for (k, want) in frame.calls.iter().enumerate() {
            let got = p.call();
            if got != *want {
                let chn: Vec<String> = p.raw().chn.iter().map(|c| format!("{c:?}")).collect();
                return Err(format!(
                    "frame {f} call {k}:\n  want {}\n  got  {}\n  earlier:\n    {}\n  channels after:\n    {}",
                    hex(want),
                    hex(&got),
                    last.join("\n    "),
                    chn.join("\n    ")
                ));
            }
            writes += got.len();
            last.push(format!("{f}.{k}: {}", hex(&got)));
            if last.len() > 6 {
                last.remove(0);
            }
        }
    }
    Ok(writes)
}

fn check(name: &str, sng: &[u8], o: &gt2_options) {
    let subs = sng[100];
    for sub in 0..subs {
        if let Err(e) = compare(sng, sub, FRAMES, o) {
            panic!("{name} subtune {sub}: {e}");
        }
    }
}

/// An instrument: AD, SR, the wave, pulse and filter pointers, the vibrato parameter and
/// delay, the gate-off timer, the first-frame waveform.
#[allow(clippy::too_many_arguments)]
fn ins(ad: u8, sr: u8, w: u8, p: u8, f: u8, vib: u8, vd: u8, gt: u8, fw: u8) -> [u8; 25] {
    let mut i = [0u8; 25];
    i[..9].copy_from_slice(&[ad, sr, w, p, f, vib, vd, gt, fw]);
    i
}

const R: [u8; 4] = [0xbd, 0, 0, 0];
const OFF: u8 = 0xbe;
const ON: u8 = 0xbf;

/// A note: octave and semitone (C-0 is 0).
fn n(octave: u8, semi: u8) -> u8 {
    0x60 + octave * 12 + semi
}

fn row(note: u8, instr: u8) -> [u8; 4] {
    [note, instr, 0, 0]
}

/// A song with one subtune: three orderlists, the instruments, the four tables, the
/// patterns.
fn song(
    orders: [(Vec<u8>, u8); 3],
    instr: Vec<[u8; 25]>,
    tables: [Vec<(u8, u8)>; 4],
    patterns: Vec<Vec<[u8; 4]>>,
) -> Vec<u8> {
    let mut s = SngSpec::new();
    s.orders = vec![orders.to_vec()];
    s.instr = instr;
    s.tables = tables.to_vec();
    s.patterns = patterns.into_iter().map(|p| (p, [0xff, 0, 0, 0])).collect();
    s.encode()
}

fn rests(k: usize) -> Vec<[u8; 4]> {
    vec![R; k]
}

/// Notes, rests, key off and on, instrument changes, transposes, repeats and restarts.
fn notes_song() -> Vec<u8> {
    let mut p0 = vec![
        row(n(3, 0), 1),
        R,
        R,
        row(n(3, 4), 0),
        R,
        row(OFF, 0),
        R,
        row(ON, 0),
        row(n(3, 7), 2),
        R,
        R,
        row(n(4, 0), 1),
    ];
    p0.extend(rests(4));
    let mut p1 = vec![row(n(2, 0), 2), R, row(n(2, 2), 0), R];
    p1.extend([row(n(2, 3), 1), row(OFF, 0), R, row(n(2, 5), 0)]);
    let p2 = vec![row(n(2, 0), 3), R, R, R, row(n(2, 7), 0), R, R, R];
    let p3 = vec![
        row(n(5, 0), 3),
        row(n(5, 3), 0),
        row(n(5, 7), 0),
        row(OFF, 0),
    ];
    let p4 = vec![row(n(0, 0), 2), R, row(ON, 0), R, row(n(7, 8), 1), R];
    // 128 rows: long runs of rests, which the packer packs 64 at a time
    let mut p5 = vec![row(n(3, 0), 1)];
    p5.extend(rests(70));
    p5.push(row(n(3, 5), 2));
    p5.extend(rests(56));
    song(
        [
            (vec![0, 0xf2, 0, 0xe5, 1, 0xf0, 4], 1),
            (vec![0xd2, 2, 3, 0xe3, 0xd1, 2], 0),
            (vec![4, 1, 0xfe, 3, 5], 2),
        ],
        vec![
            ins(0x09, 0x00, 1, 0, 0, 0, 0, 2, 0x09),
            ins(0x22, 0xa8, 3, 0, 0, 0, 0, 2, 0x09),
            ins(0x00, 0xf9, 5, 0, 0, 0, 0, 3, 0x81),
        ],
        [
            vec![
                (0x41, 0x00),
                (0xff, 0x00),
                (0x21, 0x00),
                (0xff, 0x00),
                (0x81, 0xd0),
                (0x11, 0x00),
                (0xff, 0x00),
            ],
            vec![],
            vec![],
            vec![],
        ],
        vec![p0, p1, p2, p3, p4, p5],
    )
}

/// The wavetable: delays, waveforms, inaudible waveforms, absolute and relative notes,
/// "keep the frequency", jumps, and its one-shot commands (AD, SR, wave, master volume).
fn wave_song(delays: bool) -> Vec<u8> {
    let d = |k: u8| if delays { k } else { 0x00 };
    let wt = vec![
        // 1: an arpeggio with delays, looping on itself
        (0x41, 0x00),
        (d(0x02), 0x04),
        (0x00, 0x07),
        (d(0x01), 0x0c),
        (0xe9, 0x80),
        (0xff, 0x02),
        // 7: a drum: absolute notes, noise, silent, then stop
        (0x81, 0xd0),
        (0x41, 0xaa),
        (0xe0, 0xa4),
        (0x80, 0xd4),
        (0x10, 0x7e),
        (0xff, 0x00),
        // 13: commands between notes
        (0x21, 0x00),
        (0xf5, 0x3a),
        (0x21, 0x74),
        (0xf6, 0x8c),
        (0xf7, 0x41),
        (0x00, 0x80),
        (0xfd, 0x09),
        (0x20, 0x03),
        (0xfd, 0x0f),
        (0xff, 0x0e),
        // 23: the $D0-$DF waveforms, which the packer recodes when delays are used
        (0xd1, 0x00),
        (0xd5, 0x06),
        (0x51, 0x80),
        (0xff, 0x00),
    ];
    let p0 = vec![
        row(n(3, 0), 1),
        R,
        R,
        R,
        R,
        R,
        row(n(3, 5), 0),
        R,
        R,
        R,
        R,
        R,
        row(n(4, 0), 4),
        R,
        R,
        R,
        R,
        R,
    ];
    let p1 = vec![row(n(2, 0), 2), R, R, R, row(n(2, 9), 0), R, R, R];
    let p2 = vec![row(n(4, 0), 3), R, R, R, R, R, R, R, R, R, R, R];
    song(
        [(vec![0], 0), (vec![1], 0), (vec![2], 0)],
        vec![
            ins(0x09, 0x80, 1, 0, 0, 0, 0, 2, 0x09),
            ins(0x00, 0xa0, 7, 0, 0, 0, 0, 2, 0x81),
            ins(0x11, 0xc4, 13, 0, 0, 0, 0, 2, 0x09),
            ins(0x0a, 0x00, 23, 0, 0, 0, 0, 2, 0x09),
        ],
        [wt, vec![], vec![], vec![]],
        vec![p0, p1, p2],
    )
}

/// The instrument classes and their first-frame waveforms: hard restart ($00), no hard
/// restart ($80), legato ($40, $C0); first waves $00, $FE, $FF and a waveform; gate-off
/// timers 1 to 3; instrument 63's AD as the default tempo.
fn instrument_song(tempo_from_63: bool) -> Vec<u8> {
    let mut instr = vec![
        ins(0x09, 0x00, 1, 0, 0, 0, 0, 0x82, 0x09),
        ins(0x22, 0xa8, 1, 0, 0, 0, 0, 0x02, 0x09),
        ins(0x00, 0xf9, 3, 0, 0, 0, 0, 0x41, 0x00),
        ins(0x13, 0x55, 3, 0, 0, 0, 0, 0x03, 0xfe),
        ins(0x31, 0x66, 1, 0, 0, 0, 0, 0xc1, 0xff),
        ins(0x44, 0x77, 3, 0, 0, 0, 0, 0x01, 0x00),
        ins(0x05, 0x88, 1, 0, 0, 0, 0, 0x42, 0x41),
    ];
    if tempo_from_63 {
        instr.resize(63, [0; 25]);
        instr[62][0] = 0x05;
    }
    let p0 = vec![
        row(n(3, 0), 1),
        R,
        row(n(3, 4), 2),
        R,
        row(n(3, 7), 3),
        R,
        row(n(4, 0), 4),
        R,
        row(n(3, 9), 5),
        row(n(3, 11), 6),
        row(n(4, 2), 7),
        R,
        row(OFF, 0),
        row(n(3, 0), 3),
        row(ON, 0),
        R,
    ];
    let p1 = vec![
        row(n(1, 0), 4),
        R,
        row(n(1, 0), 0),
        R,
        row(n(1, 7), 2),
        R,
        R,
        R,
    ];
    let p2 = vec![
        row(n(5, 0), 6),
        row(n(5, 1), 7),
        row(n(5, 2), 3),
        row(n(5, 3), 5),
    ];
    song(
        [(vec![0], 0), (vec![1], 0), (vec![2], 0)],
        instr,
        [
            vec![
                (0x41, 0x00),
                (0xff, 0x00),
                (0x21, 0x0c),
                (0x20, 0x00),
                (0xff, 0x00),
            ],
            vec![],
            vec![],
            vec![],
        ],
        vec![p0, p1, p2],
    )
}

/// Only some channels hold anything: the packer plays the others not at all.
fn channels_song(used: [bool; 3]) -> Vec<u8> {
    let content = vec![row(n(3, 0), 1), R, R, row(n(3, 7), 0), R, R, R, R];
    let pats: Vec<Vec<[u8; 4]>> = used
        .iter()
        .map(|&u| if u { content.clone() } else { rests(8) })
        .collect();
    song(
        [(vec![0], 0), (vec![1], 0), (vec![2], 0)],
        vec![ins(0x09, 0x00, 1, 0, 0, 0, 0, 2, 0x09)],
        [vec![(0x41, 0x00), (0xff, 0x00)], vec![], vec![], vec![]],
        pats,
    )
}

/// Orderlists breaking readme 3.1's rules (GoatTracker's editor stops on them; the packer
/// packs them, and player.s plays the bytes): a transpose after a repeat, a transpose
/// last, two transposes in a row, two repeats in a row, and R0 ($D0), which the packer
/// drops, before the restart position.
fn order_rules_song() -> Vec<u8> {
    let a = vec![row(n(3, 0), 1), R, row(n(3, 2), 0), R];
    let b = vec![row(n(4, 0), 1), R, R, R, R, R];
    song(
        [
            (vec![0xd2, 0xe4, 0, 1, 0xf3], 1),
            (vec![0xe2, 0xf5, 1, 0xd1, 0xd2, 0], 0),
            (vec![0, 0xd0, 1, 0xd0, 0xd0, 0, 1], 3),
        ],
        vec![ins(0x09, 0x00, 1, 0, 0, 0, 0, 2, 0x09)],
        [vec![(0x41, 0x00), (0xff, 0x00)], vec![], vec![], vec![]],
        vec![a, b],
    )
}

/// Several subtunes, each its own orderlists.
fn subtunes_song() -> Vec<u8> {
    let mut s = SngSpec::new();
    let o = |e: Vec<u8>| (e, 0u8);
    s.orders = vec![
        vec![o(vec![0]), o(vec![1]), o(vec![2])],
        vec![o(vec![1, 2]), o(vec![0]), o(vec![2, 0])],
        vec![o(vec![2]), o(vec![2]), o(vec![1])],
    ];
    s.instr = vec![
        ins(0x09, 0x00, 1, 0, 0, 0, 0, 2, 0x09),
        ins(0x22, 0xa8, 3, 0, 0, 0, 0, 2, 0x09),
    ];
    s.tables = vec![
        vec![(0x41, 0x00), (0xff, 0x00), (0x21, 0x00), (0xff, 0x00)],
        vec![],
        vec![],
        vec![],
    ];
    s.patterns = vec![
        (
            vec![row(n(3, 0), 1), R, row(n(3, 3), 2), R],
            [0xff, 0, 0, 0],
        ),
        (
            vec![row(n(2, 0), 2), R, R, row(n(2, 7), 1)],
            [0xff, 0, 0, 0],
        ),
        (vec![row(n(4, 0), 1), R, R, R, R, R], [0xff, 0, 0, 0]),
    ];
    s.encode()
}

#[test]
fn notes_orderlists_and_instruments_match() {
    check("notes", &notes_song(), &default_options());
}

#[test]
fn the_wavetable_matches() {
    check("wave (delays)", &wave_song(true), &default_options());
    check("wave (no delays)", &wave_song(false), &default_options());
}

#[test]
fn instrument_classes_and_first_waves_match() {
    check("instruments", &instrument_song(false), &default_options());
    check(
        "instruments, 63's tempo",
        &instrument_song(true),
        &default_options(),
    );
}

#[test]
fn unused_channels_are_not_played() {
    for used in [
        [true, false, false],
        [true, true, false],
        [false, false, true],
        [false, true, false],
        [false, false, false],
    ] {
        check(
            &format!("channels {used:?}"),
            &channels_song(used),
            &default_options(),
        );
    }
}

#[test]
fn orderlists_breaking_the_rules_play_as_packed() {
    check("order rules", &order_rules_song(), &default_options());
}

#[test]
fn subtunes_match() {
    check("subtunes", &subtunes_song(), &default_options());
}
