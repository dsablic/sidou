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

/// What a comparison found: the writes compared, and the calls whose only differences were
/// the frequency of a channel the C player marks as read from past the packed frequency
/// table (`gt2_player::freq_unknown`: the 6502 reads bytes there that only the packed
/// layout knows).
#[derive(Default, Debug, Clone, Copy)]
struct Matched {
    writes: usize,
    unknown_calls: usize,
}

/// Equal, or equal but for the marked channels' frequency values.
fn same(want: &[(u8, u8)], got: &[(u8, u8)], unknown: u8) -> Option<bool> {
    if want == got {
        return Some(true);
    }
    let tolerated = |r: u8| r < 21 && r % 7 < 2 && unknown & (1 << (r / 7)) != 0;
    let ok = want.len() == got.len()
        && want
            .iter()
            .zip(got)
            .all(|(w, g)| w.0 == g.0 && (w.1 == g.1 || tolerated(w.0)));
    ok.then_some(false)
}

/// Plays `frames` frames of the subtune on both players; the comparison, or the first
/// mismatch with both lists and the C player's channels.
fn compare(sng: &[u8], subtune: u8, frames: usize, o: &gt2_options) -> Result<Matched, String> {
    let t = trace(sng, subtune, frames, &ref_options(o)).map_err(|e| format!("trace: {e}"))?;
    let mut song = Song::host();
    song.read(sng, 0).map_err(|i| format!("read: {}", i.msg))?;
    let mut p = Player::new(&song, subtune as u32, o).ok_or("the player refused the subtune")?;
    let mut m = Matched::default();
    let mut last: Vec<String> = Vec::new();
    for (f, frame) in t.frames.iter().enumerate() {
        for (k, want) in frame.calls.iter().enumerate() {
            let got = p.call();
            match same(want, &got, p.raw().freq_unknown) {
                Some(exact) => m.unknown_calls += usize::from(!exact),
                None => {
                    let chn: Vec<String> = p.raw().chn.iter().map(|c| format!("{c:?}")).collect();
                    return Err(format!(
                        "frame {f} call {k}:\n  want {}\n  got  {}\n  earlier:\n    {}\n  channels after:\n    {}",
                        hex(want),
                        hex(&got),
                        last.join("\n    "),
                        chn.join("\n    ")
                    ));
                }
            }
            m.writes += got.len();
            last.push(format!("{f}.{k}: {}", hex(&got)));
            if last.len() > 6 {
                last.remove(0);
            }
        }
    }
    Ok(m)
}

/// Every subtune equal, exactly (a fixture never reads past the frequency table).
fn check(name: &str, sng: &[u8], o: &gt2_options) {
    let subs = sng[100];
    for sub in 0..subs {
        match compare(sng, sub, FRAMES, o) {
            Err(e) => panic!("{name} subtune {sub}: {e}"),
            Ok(m) => assert_eq!(m.unknown_calls, 0, "{name} subtune {sub}: {m:?}"),
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

/// Every example song GoatTracker ships, every subtune, with GoatTracker's defaults: the
/// songs, subtunes and frames compared.
/// The calls tolerated, by song and subtune.
type Unknown = Vec<(String, u32, usize)>;

fn examples_match(o: &gt2_options, frames: usize) -> (usize, usize, usize, Unknown) {
    let (mut songs, mut subtunes, mut writes) = (0, 0, 0);
    let mut unknown = Unknown::new();
    let mut failures = Vec::new();
    for (name, sng) in gt2_ref::examples() {
        songs += 1;
        for sub in 0..gt2_ref::subtunes(&sng) {
            subtunes += 1;
            match compare(&sng, sub as u8, frames, o) {
                Ok(m) => {
                    writes += m.writes;
                    if m.unknown_calls > 0 {
                        unknown.push((name.clone(), sub, m.unknown_calls));
                    }
                }
                Err(e) => failures.push(format!("{name} subtune {sub}: {e}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    (songs, subtunes, writes, unknown)
}

#[test]
fn every_example_matches() {
    let (songs, subtunes, writes, unknown) = examples_match(&default_options(), FRAMES);
    eprintln!(
        "{songs} songs, {subtunes} subtunes, {FRAMES} frames each: {writes} writes equal; \
         calls whose only difference is a frequency read past the packed table: {unknown:?}"
    );
    assert!(past_b7(&unknown), "{unknown:?}");
}

/// The examples that play notes past B-7, whose frequency's high byte the 6502 reads from
/// the packed song table after its frequency table: everlasting.sng's drum starts on an
/// absolute wavetable note of $F0 (note 112); hyperspace.sng arpeggiates note 85 up an
/// octave (note 97, after about 9 minutes of play).
fn past_b7(unknown: &Unknown) -> bool {
    unknown
        .iter()
        .all(|u| ["everlasting.sng", "hyperspace.sng"].contains(&u.0.as_str()))
}

#[test]
#[ignore = "long: every example for 30,000 frames (10 minutes of music)"]
fn every_example_matches_long() {
    let (songs, subtunes, writes, unknown) = examples_match(&default_options(), 30_000);
    eprintln!("{songs} songs, {subtunes} subtunes, 30000 frames: {writes} writes; {unknown:?}");
    assert!(past_b7(&unknown), "{unknown:?}");
}

fn cmd(note: u8, instr: u8, c: u8, d: u8) -> [u8; 4] {
    [note, instr, c, d]
}

/// The speedtable the command fixtures share: 1 vibrato, 2-3 portamento speeds, 4 a
/// note-independent vibrato, 5 a note-independent portamento, 6-7 funktempos, 8 a slow
/// shallow vibrato, 9 a divisor past 15.
fn speeds() -> Vec<(u8, u8)> {
    vec![
        (0x03, 0x40),
        (0x00, 0x20),
        (0x01, 0x00),
        (0x83, 0x02),
        (0x80, 0x01),
        (0x09, 0x06),
        (0x02, 0x02),
        (0x05, 0x04),
        (0x84, 0x14),
    ]
}

/// Every pattern command, the one-shot ones between continuous ones, and the tempo
/// command's every range.
fn commands_song(simple_pulse: bool) -> Vec<u8> {
    let lo = if simple_pulse { 0x40 } else { 0x48 };
    let pulse = vec![
        (0x88, 0x00),
        (0x20, lo),
        (0x40, 0xe0),
        (0xff, 0x02),
        (0x84, 0x30),
        (0x00, 0x10),
        (0xff, 0x00),
        (0x81, 0x00),
        (0x10, 0x20),
        (0xff, 0x00),
    ];
    let filter = vec![
        (0x90, 0xf1),
        (0x00, 0x40),
        (0x7f, 0x01),
        (0x10, 0xff),
        (0xff, 0x03),
        (0xa0, 0x87),
        (0x20, 0x02),
        (0xff, 0x00),
        (0xc0, 0xf2),
        (0x00, 0xf0),
        (0x80, 0x00),
        (0x90, 0xf2),
        (0x00, 0x50),
        (0xff, 0x00),
    ];
    let wave = vec![
        (0x41, 0x00),
        (0xff, 0x00),
        (0x21, 0x00),
        (0xff, 0x00),
        (0x11, 0x00),
        (0xff, 0x00),
    ];
    let p0 = vec![
        cmd(n(3, 0), 1, 0, 0),
        cmd(0xbd, 0, 4, 1),
        cmd(0xbd, 0, 4, 1),
        cmd(0xbd, 0, 6, 0x5a),
        cmd(0xbd, 0, 4, 1),
        cmd(0xbd, 0, 5, 0x22),
        cmd(0xbd, 0, 0, 0),
        cmd(n(3, 7), 0, 1, 2),
        cmd(0xbd, 0, 1, 2),
        cmd(0xbd, 0, 2, 3),
        cmd(0xbd, 0, 7, 0x21),
        cmd(0xbd, 0, 2, 3),
        cmd(n(4, 0), 0, 3, 2),
        cmd(0xbd, 0, 3, 2),
        cmd(0xbd, 0, 3, 2),
        cmd(n(3, 0), 0, 3, 0),
        cmd(n(3, 5), 0, 4, 4),
        cmd(0xbd, 0, 4, 4),
        cmd(0xbd, 0, 1, 5),
        cmd(0xbd, 0, 2, 5),
        cmd(n(2, 0), 0, 3, 5),
        cmd(0xbd, 0, 3, 5),
        cmd(n(3, 0), 2, 4, 8),
        cmd(0xbd, 0, 4, 9),
        cmd(0xbd, 0, 8, 3),
        cmd(0xbd, 0, 8, 0),
        cmd(n(3, 2), 1, 9, 5),
        cmd(0xbd, 0, 9, 0),
        cmd(0xbd, 0, 9, 8),
        cmd(0xbd, 0, 0xd, 0x08),
        cmd(0xbd, 0, 0xd, 0x3f),
        cmd(0xbd, 0, 0xd, 0x0f),
    ];
    let p1 = vec![
        cmd(n(2, 0), 3, 0xa, 1),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0xb, 0x87),
        cmd(0xbd, 0, 0xc, 0x20),
        cmd(n(2, 5), 0, 0xa, 6),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0xb, 0x00),
        cmd(0xbd, 0, 0xc, 0x80),
        cmd(n(2, 7), 0, 0xa, 9),
        cmd(0xbd, 0, 0xa, 0),
        cmd(0xbd, 0, 0xb, 0x31),
        cmd(n(2, 0), 0, 0xa, 1),
        cmd(0xbd, 0, 0, 0),
    ];
    let p2 = vec![
        cmd(n(4, 0), 2, 0xf, 0x04),
        cmd(0xbd, 0, 0, 0),
        cmd(n(4, 2), 0, 0xf, 0x86),
        cmd(0xbd, 0, 0, 0),
        cmd(n(4, 4), 0, 0xe, 6),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0, 0),
        cmd(n(4, 5), 0, 0xf, 0x00),
        cmd(0xbd, 0, 0, 0),
        cmd(n(4, 7), 0, 0xf, 0x81),
        cmd(0xbd, 0, 0, 0),
        cmd(n(4, 9), 0, 0xf, 0x02),
        cmd(0xbd, 0, 0, 0),
        cmd(n(4, 11), 0, 0xf, 0x82),
        cmd(0xbd, 0, 0, 0),
        cmd(n(5, 0), 0, 0xe, 7),
        cmd(0xbd, 0, 0, 0),
        cmd(n(4, 0), 0, 0xf, 0x03),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0xf, 0x7f),
        cmd(0xbd, 0, 0xf, 0x06),
    ];
    song(
        [(vec![0], 0), (vec![1], 0), (vec![2], 0)],
        vec![
            ins(0x09, 0x89, 1, 1, 0, 1, 4, 2, 0x09),
            ins(0x22, 0xa8, 3, 5, 0, 8, 1, 2, 0x09),
            ins(0x00, 0xf9, 5, 8, 1, 4, 2, 2, 0x81),
        ],
        [wave, pulse, filter, speeds()],
        vec![p0, p1, p2],
    )
}

/// The wavetable's commands 1-4 and 9-D, on any tick.
fn wave_commands_song() -> Vec<u8> {
    let wave = vec![
        (0x41, 0x00),
        (0xf1, 0x02),
        (0xf1, 0x02),
        (0xf2, 0x03),
        (0x00, 0x80),
        (0xf4, 0x01),
        (0xf4, 0x01),
        (0xf4, 0x04),
        (0xff, 0x06),
        (0x21, 0x0c),
        (0xf9, 0x01),
        (0xfa, 0x01),
        (0x21, 0x00),
        (0xfb, 0xa3),
        (0xfc, 0x60),
        (0x00, 0x80),
        (0xfd, 0x0a),
        (0xf3, 0x02),
        (0xf3, 0x02),
        (0xf3, 0x00),
        (0xfb, 0x00),
        (0xff, 0x00),
    ];
    let pulse = vec![(0x88, 0x00), (0x20, 0x40), (0xff, 0x02)];
    let filter = vec![(0x90, 0xf3), (0x00, 0x30), (0x20, 0x04), (0xff, 0x03)];
    let p0 = vec![row(n(3, 0), 1), R, R, R, R, R, R, R];
    let p1 = vec![row(n(3, 7), 2), R, R, R, R, R, R, R, R, R, R, R, R, R, R, R];
    song(
        [(vec![0], 0), (vec![1], 0), (vec![0, 1], 0)],
        vec![
            ins(0x09, 0x89, 1, 0, 0, 0, 0, 2, 0x09),
            ins(0x22, 0xa8, 10, 0, 0, 0, 0, 2, 0x09),
        ],
        [wave, pulse, filter, speeds()],
        vec![p0, p1],
    )
}

/// No pattern commands at all (the packer leaves the effect code out): the instrument
/// vibrato then follows the current instrument, also between a row's fetch and its tick 0.
fn no_commands_song() -> Vec<u8> {
    let p0 = vec![
        row(n(3, 0), 1),
        R,
        R,
        R,
        row(0xbd, 2),
        R,
        row(n(3, 4), 0),
        R,
        R,
        R,
        row(0xbd, 1),
        R,
        R,
    ];
    let p1 = vec![row(n(2, 0), 3), R, R, R, R, R, R, row(n(2, 7), 3), R];
    song(
        [(vec![0], 0), (vec![1], 0), (vec![0], 0)],
        vec![
            ins(0x09, 0x89, 1, 1, 0, 1, 2, 3, 0x09),
            ins(0x09, 0x89, 1, 0, 0, 4, 1, 3, 0x09),
            ins(0x22, 0xa8, 1, 1, 0, 8, 3, 2, 0x09),
        ],
        [
            vec![(0x41, 0x00), (0x00, 0x80), (0xff, 0x02)],
            vec![(0x88, 0x00), (0x20, 0x40), (0xff, 0x02)],
            vec![],
            speeds(),
        ],
        vec![p0, p1],
    )
}

#[test]
fn every_command_matches() {
    check("commands", &commands_song(false), &default_options());
    check(
        "commands, simple pulse",
        &commands_song(true),
        &default_options(),
    );
}

#[test]
fn the_wavetables_commands_match() {
    check("wave commands", &wave_commands_song(), &default_options());
}

#[test]
fn without_commands_the_vibrato_follows_the_instrument() {
    check("no commands", &no_commands_song(), &default_options());
}
