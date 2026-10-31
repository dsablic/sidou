// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use gt2_ref::trace::trace;
use gt2_ref::Options;
use sidou_sys::*;
use sidou_tests::player::*;
use sidou_tests::song::*;

const FRAMES: usize = 3000;

const PLAYER_FLAGS_AS_GT2RELOCS: [(u32, u32); 7] = [
    (GT2_PLAYER_BUFFERED, gt2_ref::PLAYER_BUFFERED),
    (GT2_PLAYER_SOUNDEFFECTS, gt2_ref::PLAYER_SOUNDEFFECTS),
    (GT2_PLAYER_VOLUME, gt2_ref::PLAYER_VOLUME),
    (GT2_PLAYER_AUTHORINFO, gt2_ref::PLAYER_AUTHORINFO),
    (GT2_PLAYER_ZPGHOSTREGS, gt2_ref::PLAYER_ZPGHOSTREGS),
    (GT2_PLAYER_NOOPTIMIZATION, gt2_ref::PLAYER_NOOPTIMIZATION),
    (GT2_PLAYER_FULLBUFFERED, gt2_ref::PLAYER_FULLBUFFERED),
];

fn ref_options(o: &gt2_options) -> Options {
    let mut version = 0;
    for (ours, theirs) in PLAYER_FLAGS_AS_GT2RELOCS {
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

#[derive(Default, Debug, Clone, Copy)]
struct Matched {
    writes: usize,
    unknown_calls: usize,
}

fn equal_but_for_unknown_freqs(want: &[(u8, u8)], got: &[(u8, u8)], unknown: u8) -> Option<bool> {
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

fn compare(sng: &[u8], subtune: u8, frames: usize, o: &gt2_options) -> Result<Matched, String> {
    let t = trace(sng, subtune, frames, &ref_options(o)).map_err(|e| format!("trace: {e}"))?;
    let mut song = Song::host();
    song.read(sng, 0).map_err(|i| format!("read: {}", i.msg))?;
    let mut p = Player::new(&song, subtune as u32, o).ok_or("the player refused the subtune")?;
    let mut m = Matched::default();
    let mut last: Vec<String> = Vec::new();
    let writes_lag_a_call = o.player_flags as u32 & GT2_PLAYER_FULLBUFFERED != 0
        && o.player_flags as u32 & GT2_PLAYER_ZPGHOSTREGS == 0;
    let mut previous = 0;
    for (f, frame) in t.frames.iter().enumerate() {
        for (k, want) in frame.calls.iter().enumerate() {
            let got = p.call();
            let unknown = if writes_lag_a_call {
                previous
            } else {
                p.raw().freq_unknown
            };
            previous = p.raw().freq_unknown;
            match equal_but_for_unknown_freqs(want, &got, unknown) {
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

fn check_every_subtune_exactly(name: &str, sng: &[u8], o: &gt2_options) {
    let subs = sng[100];
    for sub in 0..subs {
        match compare(sng, sub, FRAMES, o) {
            Err(e) => panic!("{name} subtune {sub}: {e}"),
            Ok(m) => assert_eq!(m.unknown_calls, 0, "{name} subtune {sub}: {m:?}"),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn ins(ad: u8, sr: u8, w: u8, p: u8, f: u8, vib: u8, vd: u8, gt: u8, fw: u8) -> [u8; 25] {
    let mut i = [0u8; 25];
    i[..9].copy_from_slice(&[ad, sr, w, p, f, vib, vd, gt, fw]);
    i
}

fn note(octave: u8, semi: u8) -> u8 {
    0x60 + octave * 12 + semi
}

fn one_subtune_song(
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
    vec![REST; k]
}

fn notes_song() -> Vec<u8> {
    let mut p0 = vec![
        row(note(3, 0), 1),
        REST,
        REST,
        row(note(3, 4), 0),
        REST,
        row(KEY_OFF, 0),
        REST,
        row(KEY_ON, 0),
        row(note(3, 7), 2),
        REST,
        REST,
        row(note(4, 0), 1),
    ];
    p0.extend(rests(4));
    let mut p1 = vec![row(note(2, 0), 2), REST, row(note(2, 2), 0), REST];
    p1.extend([
        row(note(2, 3), 1),
        row(KEY_OFF, 0),
        REST,
        row(note(2, 5), 0),
    ]);
    let p2 = vec![
        row(note(2, 0), 3),
        REST,
        REST,
        REST,
        row(note(2, 7), 0),
        REST,
        REST,
        REST,
    ];
    let p3 = vec![
        row(note(5, 0), 3),
        row(note(5, 3), 0),
        row(note(5, 7), 0),
        row(KEY_OFF, 0),
    ];
    let p4 = vec![
        row(note(0, 0), 2),
        REST,
        row(KEY_ON, 0),
        REST,
        row(note(7, 8), 1),
        REST,
    ];
    let mut p5 = vec![row(note(3, 0), 1)];
    p5.extend(rests(70));
    p5.push(row(note(3, 5), 2));
    p5.extend(rests(56));
    one_subtune_song(
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

fn wave_song(delays: bool) -> Vec<u8> {
    let d = |k: u8| if delays { k } else { 0x00 };
    let wt = vec![
        (0x41, 0x00),
        (d(0x02), 0x04),
        (0x00, 0x07),
        (d(0x01), 0x0c),
        (0xe9, 0x80),
        (0xff, 0x02),
        (0x81, 0xd0),
        (0x41, 0xaa),
        (0xe0, 0xa4),
        (0x80, 0xd4),
        (0x10, 0x7e),
        (0xff, 0x00),
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
        (0xd1, 0x00),
        (0xd5, 0x06),
        (0x51, 0x80),
        (0xff, 0x00),
    ];
    let p0 = vec![
        row(note(3, 0), 1),
        REST,
        REST,
        REST,
        REST,
        REST,
        row(note(3, 5), 0),
        REST,
        REST,
        REST,
        REST,
        REST,
        row(note(4, 0), 4),
        REST,
        REST,
        REST,
        REST,
        REST,
    ];
    let p1 = vec![
        row(note(2, 0), 2),
        REST,
        REST,
        REST,
        row(note(2, 9), 0),
        REST,
        REST,
        REST,
    ];
    let p2 = vec![
        row(note(4, 0), 3),
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
    ];
    one_subtune_song(
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
        row(note(3, 0), 1),
        REST,
        row(note(3, 4), 2),
        REST,
        row(note(3, 7), 3),
        REST,
        row(note(4, 0), 4),
        REST,
        row(note(3, 9), 5),
        row(note(3, 11), 6),
        row(note(4, 2), 7),
        REST,
        row(KEY_OFF, 0),
        row(note(3, 0), 3),
        row(KEY_ON, 0),
        REST,
    ];
    let p1 = vec![
        row(note(1, 0), 4),
        REST,
        row(note(1, 0), 0),
        REST,
        row(note(1, 7), 2),
        REST,
        REST,
        REST,
    ];
    let p2 = vec![
        row(note(5, 0), 6),
        row(note(5, 1), 7),
        row(note(5, 2), 3),
        row(note(5, 3), 5),
    ];
    one_subtune_song(
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

fn channels_song(used: [bool; 3]) -> Vec<u8> {
    let content = vec![
        row(note(3, 0), 1),
        REST,
        REST,
        row(note(3, 7), 0),
        REST,
        REST,
        REST,
        REST,
    ];
    let pats: Vec<Vec<[u8; 4]>> = used
        .iter()
        .map(|&u| if u { content.clone() } else { rests(8) })
        .collect();
    one_subtune_song(
        [(vec![0], 0), (vec![1], 0), (vec![2], 0)],
        vec![ins(0x09, 0x00, 1, 0, 0, 0, 0, 2, 0x09)],
        [vec![(0x41, 0x00), (0xff, 0x00)], vec![], vec![], vec![]],
        pats,
    )
}

fn order_rules_song() -> Vec<u8> {
    let a = vec![row(note(3, 0), 1), REST, row(note(3, 2), 0), REST];
    let b = vec![row(note(4, 0), 1), REST, REST, REST, REST, REST];
    one_subtune_song(
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
            vec![row(note(3, 0), 1), REST, row(note(3, 3), 2), REST],
            [0xff, 0, 0, 0],
        ),
        (
            vec![row(note(2, 0), 2), REST, REST, row(note(2, 7), 1)],
            [0xff, 0, 0, 0],
        ),
        (
            vec![row(note(4, 0), 1), REST, REST, REST, REST, REST],
            [0xff, 0, 0, 0],
        ),
    ];
    s.encode()
}

fn kit_song() -> Vec<u8> {
    let kit: Vec<[u8; 25]> = (0..GT2_KIT_DRUMS as usize).map(kit_instr).collect();
    let beat = |a: u8, b: u8| {
        let mut p = Vec::new();
        for k in 0..8u8 {
            p.push(row(note(3, k % 5), if k % 2 == 0 { a } else { b }));
            p.push(if k % 3 == 0 { row(KEY_OFF, 0) } else { REST });
        }
        p
    };
    one_subtune_song(
        [(vec![0, 3], 0), (vec![1], 0), (vec![2], 0)],
        kit,
        kit_tables(),
        vec![beat(1, 2), beat(3, 4), beat(5, 6), beat(7, 8)],
    )
}

#[test]
fn the_drum_kit_matches() {
    check_every_subtune_exactly("kit", &kit_song(), &default_options());
    let mut o = default_options();
    o.multispeed = 2;
    check_every_subtune_exactly("kit x2", &kit_song(), &o);
}

#[test]
fn notes_orderlists_and_instruments_match() {
    check_every_subtune_exactly("notes", &notes_song(), &default_options());
}

#[test]
fn the_wavetable_matches() {
    check_every_subtune_exactly("wave (delays)", &wave_song(true), &default_options());
    check_every_subtune_exactly("wave (no delays)", &wave_song(false), &default_options());
}

#[test]
fn instrument_classes_and_first_waves_match() {
    check_every_subtune_exactly("instruments", &instrument_song(false), &default_options());
    check_every_subtune_exactly(
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
        check_every_subtune_exactly(
            &format!("channels {used:?}"),
            &channels_song(used),
            &default_options(),
        );
    }
}

#[test]
fn orderlists_breaking_the_rules_play_as_packed() {
    check_every_subtune_exactly("order rules", &order_rules_song(), &default_options());
}

#[test]
fn subtunes_match() {
    check_every_subtune_exactly("subtunes", &subtunes_song(), &default_options());
}

type ToleratedCalls = Vec<(String, u32, usize)>;

fn examples_match(o: &gt2_options, frames: usize) -> (usize, usize, usize, ToleratedCalls) {
    let (mut songs, mut subtunes, mut writes) = (0, 0, 0);
    let mut unknown = ToleratedCalls::new();
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
fn the_first_song_matches() {
    let sng = sidou_tests::song_embed::sng();
    check_every_subtune_exactly("sidou-loop", &sng, &default_options());
    let mut o = default_options();
    o.multispeed = 4;
    check_every_subtune_exactly("sidou-loop x4", &sng, &o);
}

#[test]
fn every_example_matches() {
    let (songs, subtunes, writes, unknown) = examples_match(&default_options(), FRAMES);
    eprintln!(
        "{songs} songs, {subtunes} subtunes, {FRAMES} frames each: {writes} writes equal; \
         calls whose only difference is a frequency read past the packed table: {unknown:?}"
    );
    assert!(only_notes_past_b7_were_tolerated(&unknown), "{unknown:?}");
}

fn only_notes_past_b7_were_tolerated(unknown: &ToleratedCalls) -> bool {
    unknown
        .iter()
        .all(|u| ["everlasting.sng", "hyperspace.sng"].contains(&u.0.as_str()))
}

#[test]
#[ignore = "long: every example for 30,000 frames (10 minutes of music)"]
fn every_example_matches_long() {
    let (songs, subtunes, writes, unknown) = examples_match(&default_options(), 30_000);
    eprintln!("{songs} songs, {subtunes} subtunes, 30000 frames: {writes} writes; {unknown:?}");
    assert!(only_notes_past_b7_were_tolerated(&unknown), "{unknown:?}");
}

fn cmd(note: u8, instr: u8, c: u8, d: u8) -> [u8; 4] {
    [note, instr, c, d]
}

fn shared_speedtable() -> Vec<(u8, u8)> {
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
        cmd(note(3, 0), 1, 0, 0),
        cmd(0xbd, 0, 4, 1),
        cmd(0xbd, 0, 4, 1),
        cmd(0xbd, 0, 6, 0x5a),
        cmd(0xbd, 0, 4, 1),
        cmd(0xbd, 0, 5, 0x22),
        cmd(0xbd, 0, 0, 0),
        cmd(note(3, 7), 0, 1, 2),
        cmd(0xbd, 0, 1, 2),
        cmd(0xbd, 0, 2, 3),
        cmd(0xbd, 0, 7, 0x21),
        cmd(0xbd, 0, 2, 3),
        cmd(note(4, 0), 0, 3, 2),
        cmd(0xbd, 0, 3, 2),
        cmd(0xbd, 0, 3, 2),
        cmd(note(3, 0), 0, 3, 0),
        cmd(note(3, 5), 0, 4, 4),
        cmd(0xbd, 0, 4, 4),
        cmd(0xbd, 0, 1, 5),
        cmd(0xbd, 0, 2, 5),
        cmd(note(2, 0), 0, 3, 5),
        cmd(0xbd, 0, 3, 5),
        cmd(note(3, 0), 2, 4, 8),
        cmd(0xbd, 0, 4, 9),
        cmd(0xbd, 0, 8, 3),
        cmd(0xbd, 0, 8, 0),
        cmd(note(3, 2), 1, 9, 5),
        cmd(0xbd, 0, 9, 0),
        cmd(0xbd, 0, 9, 8),
        cmd(0xbd, 0, 0xd, 0x08),
        cmd(0xbd, 0, 0xd, 0x3f),
        cmd(0xbd, 0, 0xd, 0x0f),
    ];
    let p1 = vec![
        cmd(note(2, 0), 3, 0xa, 1),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0xb, 0x87),
        cmd(0xbd, 0, 0xc, 0x20),
        cmd(note(2, 5), 0, 0xa, 6),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0xb, 0x00),
        cmd(0xbd, 0, 0xc, 0x80),
        cmd(note(2, 7), 0, 0xa, 9),
        cmd(0xbd, 0, 0xa, 0),
        cmd(0xbd, 0, 0xb, 0x31),
        cmd(note(2, 0), 0, 0xa, 1),
        cmd(0xbd, 0, 0, 0),
    ];
    let p2 = vec![
        cmd(note(4, 0), 2, 0xf, 0x04),
        cmd(0xbd, 0, 0, 0),
        cmd(note(4, 2), 0, 0xf, 0x86),
        cmd(0xbd, 0, 0, 0),
        cmd(note(4, 4), 0, 0xe, 6),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0, 0),
        cmd(note(4, 5), 0, 0xf, 0x00),
        cmd(0xbd, 0, 0, 0),
        cmd(note(4, 7), 0, 0xf, 0x81),
        cmd(0xbd, 0, 0, 0),
        cmd(note(4, 9), 0, 0xf, 0x02),
        cmd(0xbd, 0, 0, 0),
        cmd(note(4, 11), 0, 0xf, 0x82),
        cmd(0xbd, 0, 0, 0),
        cmd(note(5, 0), 0, 0xe, 7),
        cmd(0xbd, 0, 0, 0),
        cmd(note(4, 0), 0, 0xf, 0x03),
        cmd(0xbd, 0, 0, 0),
        cmd(0xbd, 0, 0xf, 0x7f),
        cmd(0xbd, 0, 0xf, 0x06),
    ];
    one_subtune_song(
        [(vec![0], 0), (vec![1], 0), (vec![2], 0)],
        vec![
            ins(0x09, 0x89, 1, 1, 0, 1, 4, 2, 0x09),
            ins(0x22, 0xa8, 3, 5, 0, 8, 1, 2, 0x09),
            ins(0x00, 0xf9, 5, 8, 1, 4, 2, 2, 0x81),
        ],
        [wave, pulse, filter, shared_speedtable()],
        vec![p0, p1, p2],
    )
}

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
    let p0 = vec![row(note(3, 0), 1), REST, REST, REST, REST, REST, REST, REST];
    let p1 = vec![
        row(note(3, 7), 2),
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
    ];
    one_subtune_song(
        [(vec![0], 0), (vec![1], 0), (vec![0, 1], 0)],
        vec![
            ins(0x09, 0x89, 1, 0, 0, 0, 0, 2, 0x09),
            ins(0x22, 0xa8, 10, 0, 0, 0, 0, 2, 0x09),
        ],
        [wave, pulse, filter, shared_speedtable()],
        vec![p0, p1],
    )
}

fn no_commands_song() -> Vec<u8> {
    let p0 = vec![
        row(note(3, 0), 1),
        REST,
        REST,
        REST,
        row(0xbd, 2),
        REST,
        row(note(3, 4), 0),
        REST,
        REST,
        REST,
        row(0xbd, 1),
        REST,
        REST,
    ];
    let p1 = vec![
        row(note(2, 0), 3),
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        row(note(2, 7), 3),
        REST,
    ];
    one_subtune_song(
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
            shared_speedtable(),
        ],
        vec![p0, p1],
    )
}

#[test]
fn every_command_matches() {
    check_every_subtune_exactly("commands", &commands_song(false), &default_options());
    check_every_subtune_exactly(
        "commands, simple pulse",
        &commands_song(true),
        &default_options(),
    );
}

#[test]
fn the_wavetables_commands_match() {
    check_every_subtune_exactly("wave commands", &wave_commands_song(), &default_options());
}

#[test]
fn without_commands_the_vibrato_follows_the_instrument() {
    check_every_subtune_exactly("no commands", &no_commands_song(), &default_options());
}

type OptionSet = (&'static str, fn(&mut gt2_options));

const OPTION_SETS: [OptionSet; 16] = [
    ("-O0 (no pulse optimisation)", |o| o.optimize_pulse = 0),
    ("-R0 (no realtime optimisation)", |o| {
        o.optimize_realtime = 0
    }),
    ("-O0 -R0", |o| {
        o.optimize_pulse = 0;
        o.optimize_realtime = 0
    }),
    ("-S2", |o| o.multispeed = 2),
    ("-S4", |o| o.multispeed = 4),
    ("-S8 (host only)", |o| o.multispeed = 8),
    ("-S0 (25 Hz)", |o| o.multispeed = 0),
    ("-N (NTSC)", |o| o.ntsc = 1),
    ("-B (buffered)", |o| {
        o.player_flags |= GT2_PLAYER_BUFFERED as u16
    }),
    ("-C (zero-page ghost registers)", |o| {
        o.player_flags |= GT2_PLAYER_ZPGHOSTREGS as u16;
        o.zeropage = 0xe0
    }),
    ("-J (full buffering)", |o| {
        o.player_flags |= GT2_PLAYER_FULLBUFFERED as u16
    }),
    ("-D (sound effects)", |o| {
        o.player_flags |= GT2_PLAYER_SOUNDEFFECTS as u16
    }),
    ("-H (author info)", |o| {
        o.player_flags |= GT2_PLAYER_AUTHORINFO as u16
    }),
    ("-I0 (no optimisation)", |o| {
        o.player_flags |= GT2_PLAYER_NOOPTIMIZATION as u16
    }),
    ("-AF000 (altplayer.s)", |o| o.hr_adsr = 0xf000),
    ("-A0A00", |o| o.hr_adsr = 0x0a00),
];

#[test]
fn every_example_matches_with_each_option() {
    let mut failures = Vec::new();
    for (name, f) in OPTION_SETS {
        let mut o = default_options();
        f(&mut o);
        let r = std::panic::catch_unwind(|| examples_match(&o, FRAMES));
        match r {
            Ok((_, subs, writes, unknown)) => {
                eprintln!("{name}: {subs} subtunes, {writes} writes; {unknown:?}");
                if !only_notes_past_b7_were_tolerated(&unknown) {
                    failures.push(format!("{name}: {unknown:?}"));
                }
            }
            Err(e) => failures.push(format!(
                "{name}: {}",
                e.downcast_ref::<String>().cloned().unwrap_or_default()
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn three_examples_match_with_the_full_option_matrix() {
    let songs = ["funktest.sng", "ghosttrackers.sng", "hyperspace.sng"];
    let writes: [u16; 4] = [
        0,
        GT2_PLAYER_BUFFERED as u16,
        GT2_PLAYER_ZPGHOSTREGS as u16,
        GT2_PLAYER_FULLBUFFERED as u16,
    ];
    let examples: Vec<(String, Vec<u8>)> = gt2_ref::examples()
        .into_iter()
        .filter(|(name, _)| songs.contains(&name.as_str()))
        .collect();
    let sets = 2 * 2 * 4 * 2 * 4 * 2;
    let failures: Vec<String> = std::thread::scope(|scope| {
        let runs: Vec<_> = examples
            .iter()
            .map(|(name, sng)| {
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    for bits in 0..sets {
                        let mut o = default_options();
                        o.optimize_pulse = (bits & 1) as u8;
                        o.optimize_realtime = (bits >> 1 & 1) as u8;
                        o.multispeed = [1, 2, 4, 8][bits >> 2 & 3];
                        o.ntsc = (bits >> 4 & 1) as u8;
                        o.player_flags = writes[bits >> 5 & 3];
                        o.zeropage = 0xe0;
                        o.hr_adsr = if bits >> 7 & 1 != 0 { 0xf000 } else { 0x0f00 };
                        for sub in 0..gt2_ref::subtunes(sng) {
                            if let Err(e) = compare(sng, sub as u8, FRAMES, &o) {
                                failures.push(format!("{name} {sub} with {o:?}: {e}"));
                            }
                        }
                    }
                    failures
                })
            })
            .collect();
        runs.into_iter().flat_map(|r| r.join().unwrap()).collect()
    });
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    eprintln!("{sets} option sets over {songs:?}, {FRAMES} frames each");
}

fn sidou_options(r: &Options) -> gt2_options {
    let mut o = default_options();
    o.hr_adsr = r.hr_adsr as u16;
    o.multispeed = r.multiplier as u8;
    o.ntsc = r.ntsc as u8;
    o.optimize_pulse = r.optimize_pulse as u8;
    o.optimize_realtime = r.optimize_realtime as u8;
    o.zeropage = r.zeropage as u8;
    o.player_flags = 0;
    for (ours, theirs) in PLAYER_FLAGS_AS_GT2RELOCS {
        if r.player_version & theirs != 0 {
            o.player_flags |= ours as u16;
        }
    }
    o
}

fn generated_songs_match_for(seeds: std::ops::Range<u64>) {
    let failures: Vec<String> = std::thread::scope(|scope| {
        let threads = 8u64;
        let runs: Vec<_> = (0..threads)
            .map(|t| {
                let seeds = seeds.clone();
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    for seed in seeds.filter(|s| s % threads == t) {
                        let g = gt2_ref::gen::generate(seed);
                        let o = sidou_options(&g.options);
                        for sub in 0..g.sng[100] {
                            match compare(&g.sng, sub, FRAMES, &o) {
                                Ok(m) if m.unknown_calls == 0 => {}
                                Ok(m) => failures.push(format!("seed {seed} subtune {sub}: {m:?}")),
                                Err(e) => failures.push(format!(
                                    "seed {seed} subtune {sub} ({:?}): {e}",
                                    g.options
                                )),
                            }
                        }
                    }
                    failures
                })
            })
            .collect();
        runs.into_iter().flat_map(|r| r.join().unwrap()).collect()
    });
    let shown: Vec<&String> = failures.iter().take(3).collect();
    assert!(
        failures.is_empty(),
        "{} failing: {:?}",
        failures.len(),
        shown
    );
}

#[test]
fn generated_songs_match() {
    generated_songs_match_for(0..300);
}

#[test]
#[ignore = "long: 5,000 generated songs"]
fn generated_songs_match_long() {
    generated_songs_match_for(300..5300);
}

fn waveform_commands_song() -> Vec<u8> {
    let wave = vec![
        (0x41, 0x00),
        (0x02, 0x00),
        (0xd5, 0x9a),
        (0x41, 0x00),
        (0xdd, 0x86),
        (0x21, 0x00),
        (0xdf, 0x85),
        (0x41, 0x0c),
        (0xf6, 0x48),
        (0xff, 0x00),
    ];
    let p0 = vec![
        row(note(3, 0), 1),
        REST,
        REST,
        REST,
        cmd(0xbd, 0, 0xd, 0x0c),
        REST,
        REST,
        REST,
        cmd(note(3, 5), 0, 0xf, 0x06),
        REST,
        REST,
        REST,
    ];
    one_subtune_song(
        [(vec![0], 0), (vec![0], 0), (vec![0], 0)],
        vec![ins(0x09, 0x89, 1, 0, 0, 0, 0, 2, 0x09)],
        [wave, vec![], vec![], vec![]],
        vec![p0],
    )
}

#[test]
fn waveforms_read_as_commands_reach_the_assembled_handlers() {
    check_every_subtune_exactly(
        "waveform commands",
        &waveform_commands_song(),
        &default_options(),
    );
}

#[derive(Default, Debug)]
struct Events {
    rows: usize,
    notes: usize,
    gate_ons: usize,
}

fn check_events(sng: &[u8], subtune: u8, o: &gt2_options, e: &mut Events) -> Result<(), String> {
    let t = trace(sng, subtune, FRAMES, &ref_options(o)).map_err(|e| format!("trace: {e}"))?;
    let mut song = Song::host();
    song.read(sng, 0).map_err(|i| format!("read: {}", i.msg))?;
    let mut p = Player::new(&song, subtune as u32, o).ok_or("refused")?;
    let channels = p.shape().channels as usize;
    let sub = p.shape().subtune as u32;
    let mut wave = [0u8; 3];
    let mut last_row: [Option<(u8, u8)>; 3] = [None; 3];
    for (f, frame) in t.frames.iter().enumerate() {
        for want in &frame.calls {
            let before = wave;
            for &(r, v) in want {
                if r < 21 && r % 7 == 4 {
                    wave[(r / 7) as usize] = v;
                }
            }
            p.call();
            for ch in 0..3 {
                let ev = p.raw().ev[ch];
                if ch >= channels {
                    if ev.flags != 0 {
                        return Err(format!("frame {f}: events on unplayed channel {ch}"));
                    }
                    continue;
                }
                let (was, now) = (before[ch] & 1, wave[ch] & 1);
                let on = ev.flags as u32 & GT2_EV_GATE_ON != 0;
                let off = ev.flags as u32 & GT2_EV_GATE_OFF != 0;
                if on != (was == 0 && now == 1) || off != (was == 1 && now == 0) || ev.gate != now {
                    return Err(format!(
                        "frame {f} channel {ch}: gate {was}->{now} in the reference, event {ev:?}"
                    ));
                }
                e.gate_ons += usize::from(on);
                if ev.flags as u32 & GT2_EV_NOTE != 0 {
                    e.notes += 1;
                    let row = song
                        .pattern(ev.pattern as u32)
                        .and_then(|r| r.get(ev.row as usize).copied());
                    let ok = ev.flags as u32 & GT2_EV_ROW != 0
                        && row.is_some_and(|r| {
                            let t = ev.note as i32 - (r[0] as i32 - 0x60);
                            (0x60..=0xbc).contains(&r[0]) && (-16..=14).contains(&t)
                        });
                    if !ok {
                        return Err(format!("frame {f} channel {ch}: note {ev:?}, row {row:?}"));
                    }
                }
                if ev.flags as u32 & GT2_EV_ROW != 0 {
                    e.rows += 1;
                    if let Some((pat, row)) = last_row[ch] {
                        let next = pat == ev.pattern && ev.row == row + 1;
                        if !next && ev.row != 0 {
                            return Err(format!(
                                "frame {f} channel {ch}: row {:?} after {pat}/{row}",
                                (ev.pattern, ev.row)
                            ));
                        }
                    }
                    if ev.order_pos != 0xff {
                        let (entries, _) = song.order(sub, ch as u32).unwrap();
                        if entries.get(ev.order_pos as usize) != Some(&ev.pattern) {
                            return Err(format!(
                                "frame {f} channel {ch}: {ev:?} not in {entries:?}"
                            ));
                        }
                    }
                    last_row[ch] = Some((ev.pattern, ev.row));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn the_events_follow_the_reference() {
    let mut e = Events::default();
    let mut failures = Vec::new();
    for (name, sng) in gt2_ref::examples() {
        for sub in 0..gt2_ref::subtunes(&sng) {
            if let Err(err) = check_events(&sng, sub as u8, &default_options(), &mut e) {
                failures.push(format!("{name} {sub}: {err}"));
            }
        }
    }
    for seed in 0..100 {
        let g = gt2_ref::gen::generate(seed);
        let mut o = sidou_options(&g.options);
        o.player_flags &= !(GT2_PLAYER_FULLBUFFERED as u16);
        for sub in 0..g.sng[100] {
            if let Err(err) = check_events(&g.sng, sub, &o, &mut e) {
                failures.push(format!("seed {seed} {sub}: {err}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(e.rows > 0 && e.notes > 0 && e.gate_ons > 0, "{e:?}");
    eprintln!("{e:?}");
}

#[test]
fn the_players_cost_on_the_host() {
    for multispeed in [1u8, 4] {
        let mut o = default_options();
        o.multispeed = multispeed;
        let mut ns: Vec<u128> = Vec::new();
        for (_, sng) in gt2_ref::examples() {
            let song = Song::host_from(&sng);
            for sub in 0..gt2_ref::subtunes(&sng) {
                let mut p = Player::new(&song, sub, &o).unwrap();
                for _ in 0..FRAMES * multispeed as usize {
                    let t = std::time::Instant::now();
                    p.call_silent();
                    ns.push(t.elapsed().as_nanos());
                }
            }
        }
        let mean = ns.iter().sum::<u128>() / ns.len() as u128;
        ns.sort_unstable();
        eprintln!(
            "multispeed {multispeed}: {} calls, mean {mean} ns, 99.9% under {} ns, worst {} ns",
            ns.len(),
            ns[ns.len() * 999 / 1000],
            ns[ns.len() - 1]
        );
    }
    eprintln!("player state: {} bytes", size_of::<gt2_player>());
}

fn repeat_first_song() -> Vec<u8> {
    let p2 = vec![cmd(note(3, 0), 1, 6, 0xc4), REST, REST, REST];
    let p3 = vec![
        row(note(4, 0), 1),
        REST,
        row(note(4, 4), 0),
        REST,
        REST,
        REST,
    ];
    one_subtune_song(
        [
            (vec![0xd2, 3, 2], 0),
            (vec![2, 3], 0),
            (vec![0xd3, 0xf2, 3], 1),
        ],
        vec![ins(0x09, 0x89, 1, 0, 0, 0, 0, 2, 0x09)],
        [vec![(0x41, 0x00), (0xff, 0x00)], vec![], vec![], vec![]],
        vec![rests(4), rests(4), p2, p3],
    )
}

fn before_the_first_note_song() -> Vec<u8> {
    let p0 = vec![
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        row(note(4, 0), 1),
        REST,
        REST,
        REST,
    ];
    let p1 = vec![
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        row(note(3, 7), 2),
        REST,
        REST,
        REST,
        REST,
        REST,
    ];
    one_subtune_song(
        [(vec![0], 0), (vec![1], 0), (vec![0, 1], 0)],
        vec![
            ins(0x09, 0x89, 1, 0, 0, 1, 2, 2, 0x09),
            ins(0x22, 0xa8, 1, 0, 0, 2, 1, 2, 0x09),
        ],
        [
            vec![(0x41, 0x00), (0xff, 0x00)],
            vec![],
            vec![],
            vec![(0x83, 0x02), (0x84, 0x01)],
        ],
        vec![p0, p1],
    )
}

#[test]
fn generated_findings_match() {
    check_every_subtune_exactly("repeat first", &repeat_first_song(), &default_options());
    for (name, flags) in [
        ("unbuffered", 0u16),
        ("buffered", GT2_PLAYER_BUFFERED as u16),
        ("full buffering", GT2_PLAYER_FULLBUFFERED as u16),
        ("zero-page ghosts", GT2_PLAYER_ZPGHOSTREGS as u16),
    ] {
        let mut o = default_options();
        o.player_flags = flags;
        o.zeropage = 0xe0;
        check_every_subtune_exactly(
            &format!("before the first note, {name}"),
            &before_the_first_note_song(),
            &o,
        );
    }
}

fn stereo_song() -> Vec<u8> {
    let mut s = SngSpec::new();
    s.orders = vec![(0..6).map(|k| (vec![k as u8 % 3], 0u8)).collect()];
    s.instr = vec![ins(0x09, 0x89, 1, 0, 0, 0, 0, 2, 0x09)];
    s.tables = vec![vec![(0x41, 0x00), (0xff, 0x00)], vec![], vec![], vec![]];
    s.patterns = (0..3)
        .map(|_| (vec![row(note(3, 0), 1), REST, REST, REST], [0xff, 0, 0, 0]))
        .collect();
    s.encode()
}

#[test]
fn stereo_songs_are_refused_as_gt2reloc_refuses_them() {
    let mut song = Song::host();
    let info = song.read(&stereo_song(), 0).unwrap();
    assert_eq!(info.channels, 6);
    assert_eq!(
        Player::start(&song, 0, &default_options()).err(),
        Some(GT2_PLAY_ERR_STEREO)
    );
}

#[test]
fn a_refused_init_leaves_the_player_zeroed() {
    let cases: [(Vec<u8>, u32, i32); 2] = [
        (stereo_song(), 0, GT2_PLAY_ERR_STEREO),
        (subtunes_song(), 7, GT2_PLAY_ERR_SUBTUNE),
    ];
    for (sng, sub, err) in cases {
        let song = Song::host_from(&sng);
        let mut p = gt2_player::default();
        let bytes = size_of::<gt2_player>();
        unsafe { std::ptr::write_bytes(&mut p as *mut gt2_player as *mut u8, 0xaa, bytes) };
        let mut shape = Box::<gt2_shape>::default();
        let r = unsafe { gt2_play_init(&mut p, &mut *shape, song.ptr(), sub, &default_options()) };
        assert_eq!(r, err);
        let raw =
            unsafe { std::slice::from_raw_parts(&p as *const gt2_player as *const u8, bytes) };
        assert!(raw.iter().all(|&b| b == 0), "not zeroed after {err}");
    }
}

#[test]
fn a_pitch_the_player_does_not_play_is_reported() {
    let song = Song::host_from(&notes_song());
    let mut o = default_options();
    assert_eq!(Player::new(&song, 0, &o).unwrap().shape().warnings, 0);
    o.a4_millihz = 432_000;
    let p = Player::new(&song, 0, &o).unwrap();
    assert_eq!(
        p.shape().warnings as u32 & GT2_PLAY_WARN_PITCH,
        GT2_PLAY_WARN_PITCH
    );
    o.a4_millihz = 0;
    o.clock_hz = 1_000_000;
    assert_eq!(Player::new(&song, 0, &o).unwrap().shape().warnings, 0);
}

fn lab_sounds_song() -> Vec<u8> {
    let mut sounds: Vec<[u8; LAB_SOUND_BYTES as usize]> = Vec::new();
    let base = {
        let mut s = [0u8; LAB_SOUND_BYTES as usize];
        s[LAB_SOUND_CONTROL as usize] = 0x40;
        s[LAB_SOUND_AD as usize] = 0x29;
        s[LAB_SOUND_SR as usize] = 0xa9;
        s[LAB_SOUND_PULSE as usize..][..2].copy_from_slice(&0x812u16.to_le_bytes());
        s[LAB_SOUND_CUTOFF as usize..][..2].copy_from_slice(&1200u16.to_le_bytes());
        s[LAB_SOUND_FILTER_MODE as usize] = 0x10;
        s[(LAB_SOUND_ARP + LIVE_ARP_SPAN) as usize] = 1;
        s[(LAB_SOUND_ARP + LIVE_ARP_RATE) as usize] = 1;
        s
    };
    let with = |f: &dyn Fn(&mut [u8; LAB_SOUND_BYTES as usize])| {
        let mut s = base;
        f(&mut s);
        s
    };
    sounds.push(base);
    sounds.push(with(&|s| s[LAB_SOUND_CONTROL as usize] = 0x14));
    sounds.push(with(&|s| {
        s[LAB_SOUND_CONTROL as usize] = 0x42;
        s[LAB_SOUND_PWM_DEPTH as usize..][..2].copy_from_slice(&700u16.to_le_bytes());
        s[LAB_SOUND_PWM_RATE as usize..][..2].copy_from_slice(&900u16.to_le_bytes());
    }));
    sounds.push(with(&|s| {
        s[LAB_SOUND_CONTROL as usize] = 0x20;
        s[LAB_SOUND_FILTER_VOICES as usize] = 7;
        s[LAB_SOUND_RESONANCE as usize] = 12;
        s[LAB_SOUND_FILTER_LFO as usize] = 12;
        s[LAB_SOUND_PWM_RATE as usize..][..2].copy_from_slice(&300u16.to_le_bytes());
    }));
    sounds.push(with(&|s| {
        s[LAB_SOUND_VIBRATO_RATE as usize] = 55;
        s[LAB_SOUND_VIBRATO_DEPTH as usize] = 90;
        s[LAB_SOUND_VIBRATO_DELAY as usize] = 30;
        s[LAB_SOUND_AD as usize] = 0x0a;
    }));
    sounds.push(with(&|s| {
        s[LAB_SOUND_CONTROL as usize] = 0x10;
        s[(LAB_SOUND_ARP + LIVE_ARP_ON) as usize] = 1;
        s[(LAB_SOUND_ARP + LIVE_ARP_ORDER) as usize] = LIVE_ARP_UP_DOWN as u8;
    }));
    let mut tables: [Vec<(u8, u8)>; 4] = Default::default();
    let mut instr = Vec::new();
    for (k, s) in sounds.iter().enumerate() {
        let first: [u8; 4] = std::array::from_fn(|t| tables[t].len() as u8 + 1);
        let mut o = lab_song_instr::default();
        unsafe {
            lab_song_encode(
                s.as_ptr(),
                c"LAB".as_ptr(),
                k as u32 % 3,
                0,
                first.as_ptr(),
                &mut o,
            )
        };
        for (t, table) in tables.iter_mut().enumerate() {
            table.extend((0..o.rows[t] as usize).map(|i| (o.left[t][i], o.right[t][i])));
        }
        instr.push(o.instr);
    }
    let voice = |a: u8, b: u8| {
        let mut p = Vec::new();
        for k in 0..16u8 {
            p.push(row(note(3, k % 7), if k % 4 < 2 { a } else { b }));
            p.extend(rests(3));
            if k % 5 == 4 {
                p.push(row(KEY_OFF, 0));
            }
        }
        p
    };
    one_subtune_song(
        [(vec![0], 0), (vec![1], 0), (vec![2], 0)],
        instr,
        tables,
        vec![voice(1, 4), voice(2, 5), voice(3, 6)],
    )
}

#[test]
fn lab_sounds_sent_to_the_song_match() {
    check_every_subtune_exactly("lab sounds", &lab_sounds_song(), &default_options());
    let mut o = default_options();
    o.optimize_pulse = 0;
    o.optimize_realtime = 0;
    check_every_subtune_exactly("lab sounds, no optimisations", &lab_sounds_song(), &o);
}
