// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use gt2_ref::trace::trace;
use gt2_ref::Options;
use sidou_sys::*;
use sidou_tests::song::SngSpec;

const FRAMES: usize = 160;
type WaveRow = (u8, u8);

const WAVE_PROGRAM_ROW: u32 = 1;
const PATTERN_NOTE_C3: i32 = 0x60 + 36;
const VOICE_REGISTERS: usize = 7;
const FREQ_LO: usize = 0;
const FREQ_HI: usize = 1;
const CONTROL: usize = 4;

fn arp(offsets: &[i8], speed: u8, loop_step: u8) -> song_pages_arp {
    let mut a = song_pages_arp {
        steps: offsets.len() as u8,
        speed,
        loop_: loop_step,
        wave: SONG_PAGES_ARP_WAVE as u8,
        ..Default::default()
    };
    a.off[..offsets.len()].copy_from_slice(offsets);
    a
}

fn rows(a: &song_pages_arp, first: u32) -> Vec<(u8, u8)> {
    let mut left = [0u8; SONG_PAGES_ARP_ROWS as usize];
    let mut right = [0u8; SONG_PAGES_ARP_ROWS as usize];
    let n = unsafe { song_pages_arp_encode(a, first, left.as_mut_ptr(), right.as_mut_ptr()) };
    (0..n as usize).map(|i| (left[i], right[i])).collect()
}

fn song_playing_c3_plus(wave_rows: Vec<(u8, u8)>, semis: i32) -> Vec<u8> {
    let mut s = SngSpec::new();
    let mut i = [0u8; 25];
    i[..9].copy_from_slice(&[0x09, 0x00, WAVE_PROGRAM_ROW as u8, 0, 0, 0, 0, 2, 0x09]);
    s.instr = vec![i];
    s.tables[0] = wave_rows;
    s.patterns[0].0[0] = [(PATTERN_NOTE_C3 + semis) as u8, 1, 0, 0];
    s.encode()
}

fn voice1_registers_per_frame(sng: &[u8]) -> Vec<[u8; VOICE_REGISTERS]> {
    let t = trace(sng, 0, FRAMES, &Options::default()).expect("the reference plays the song");
    let mut regs = [0u8; VOICE_REGISTERS];
    t.frames
        .iter()
        .map(|f| {
            for &(reg, value) in f.calls.iter().flatten() {
                if let Some(r) = regs.get_mut(usize::from(reg)) {
                    *r = value;
                }
            }
            regs
        })
        .collect()
}

fn voice1_freqs(sng: &[u8]) -> Vec<u16> {
    voice1_registers_per_frame(sng)
        .iter()
        .map(|r| u16::from(r[FREQ_HI]) << 8 | u16::from(r[FREQ_LO]))
        .collect()
}

fn voice1_controls(sng: &[u8]) -> Vec<u8> {
    voice1_registers_per_frame(sng)
        .iter()
        .map(|r| r[CONTROL])
        .collect()
}

fn held_note_pitch(semis: i32) -> u16 {
    let f = voice1_freqs(&song_playing_c3_plus(
        vec![(0x41, 0x00), (0xff, 0x00)],
        semis,
    ));
    *f.last().unwrap()
}

fn freqs_from_first_sound(a: &song_pages_arp) -> Vec<u16> {
    let f = voice1_freqs(&song_playing_c3_plus(rows(a, WAVE_PROGRAM_ROW), 0));
    let at = f.iter().position(|&x| x != 0).expect("a note sounds");
    f[at..].to_vec()
}

fn freqs_the_helper_promises(a: &song_pages_arp, frames: usize) -> Vec<u16> {
    let mut steps: Vec<usize> = Vec::new();
    let n = a.steps as usize;
    let mut k = 0;
    while steps.len() < frames {
        steps.extend(std::iter::repeat_n(k, a.speed as usize));
        k += 1;
        if k == n {
            if a.loop_ == 0 {
                k = n - 1;
            } else {
                k = a.loop_ as usize - 1;
            }
        }
    }
    steps.truncate(frames);
    steps
        .into_iter()
        .map(|k| held_note_pitch(i32::from(a.off[k])))
        .collect()
}

fn check(name: &str, a: &song_pages_arp) {
    let got = freqs_from_first_sound(a);
    let want = freqs_the_helper_promises(a, 100);
    let distinct: std::collections::BTreeSet<u16> = want.iter().copied().collect();
    assert!(!distinct.contains(&0), "{name}: a step is silent");
    let offsets: std::collections::BTreeSet<i8> =
        a.off[..a.steps as usize].iter().copied().collect();
    assert!(
        distinct.len() >= offsets.len().min(if a.loop_ == 0 { 1 } else { 2 }),
        "{name}: the steps sound alike"
    );
    assert!(got.len() >= want.len(), "{name}: only {} frames", got.len());
    assert_eq!(
        &got[..want.len()],
        &want[..],
        "{name}: {:?}",
        rows(a, WAVE_PROGRAM_ROW)
    );
}

#[test]
fn the_reference_player_plays_what_the_helper_wrote() {
    let cases: [(&str, &[i8], u8, u8); 10] = [
        ("major, 1 tick, looping", &[0, 4, 7, 12], 1, 1),
        (
            "major, 3 ticks, looping at the first (the extra row)",
            &[0, 4, 7, 12],
            3,
            1,
        ),
        (
            "minor, 2 ticks, looping at the third",
            &[0, 3, 7, 12, 15],
            2,
            3,
        ),
        ("held last note", &[0, 4, 7], 4, 0),
        ("held last note at 1 tick", &[0, 4, 7], 1, 0),
        ("one step", &[7], 5, 1),
        ("below the note", &[-12, -5, 0, 7], 2, 1),
        (
            "eight steps, the slowest",
            &[0, 2, 4, 5, 7, 9, 11, 12],
            16,
            8,
        ),
        (
            "eight steps looping at the first",
            &[0, 3, 7, 10, 12, 15, 19, 22],
            2,
            1,
        ),
        ("a loop at the last step", &[0, 5, 12], 3, 3),
    ];
    for (name, offsets, speed, loop_step) in cases {
        check(name, &arp(offsets, speed, loop_step));
    }
}

#[test]
fn the_rows_are_plain_gt2() {
    let cases: [(&str, song_pages_arp, u32, &[WaveRow]); 4] = [
        (
            "1 tick: the waveform in every row, a jump to the loop step",
            arp(&[0, 4, 7], 1, 1),
            5,
            &[(0x41, 0), (0x41, 4), (0x41, 7), (0xff, 5)],
        ),
        (
            "3 ticks looping at the first: delays of two and one more row",
            arp(&[0, 4, 7], 3, 1),
            1,
            &[(0x41, 0), (2, 4), (2, 7), (2, 0), (0xff, 2)],
        ),
        (
            "3 ticks: a delay of two in the later rows",
            arp(&[0, 4, 7], 3, 2),
            1,
            &[(0x41, 0), (2, 4), (2, 7), (0xff, 2)],
        ),
        (
            "below the note: 7-bit two's complement offsets",
            arp(&[0, -1, -12], 2, 0),
            1,
            &[(0x41, 0), (1, 0x7f), (1, 0x74), (0xff, 0)],
        ),
    ];
    for (name, a, first, want) in cases {
        assert_eq!(rows(&a, first), want, "{name}");
    }
}

fn decode(rows: &[(u8, u8)], first: u32) -> Option<song_pages_arp> {
    let left: Vec<u8> = rows.iter().map(|r| r.0).collect();
    let right: Vec<u8> = rows.iter().map(|r| r.1).collect();
    let mut a = song_pages_arp::default();
    let mut used = 0;
    let ok = unsafe {
        song_pages_arp_decode(
            left.as_ptr(),
            right.as_ptr(),
            first,
            rows.len() as u32,
            &mut a,
            &mut used,
        )
    };
    (ok != 0).then(|| {
        assert_eq!(used as usize, rows.len(), "{rows:?}: the rows it took");
        a
    })
}

#[test]
fn every_arpeggio_reads_back_as_the_rows_it_wrote() {
    let mut rng = sidou_tests::song::Rng(0x5eed);
    for steps in 1..=SONG_PAGES_ARP_STEPS as u8 {
        for speed in 1..=SONG_PAGES_ARP_SPEED_MAX as u8 {
            for loop_step in 0..=steps {
                let offsets: Vec<i8> = (0..steps)
                    .map(|_| (rng.below(128) as i32 - 64) as i8)
                    .collect();
                let a = arp(&offsets, speed, loop_step);
                let first = 1 + rng.below(240);
                let r = rows(&a, first);
                let back = decode(&r, first).unwrap_or_else(|| panic!("{r:?} is an arpeggio"));
                assert_eq!(
                    rows(&back, first),
                    r,
                    "{offsets:?} speed {speed} loop {loop_step}"
                );
                assert_eq!(
                    back.off[..back.steps as usize],
                    offsets[..back.steps as usize]
                );
                if steps > 1 {
                    assert_eq!(
                        (back.steps, back.speed, back.loop_),
                        (steps, speed.max(1), loop_step)
                    );
                }
            }
        }
    }
}

#[test]
fn hand_made_and_other_rows_are_told_apart() {
    let a = decode(&[(0x41, 0), (0x41, 4), (0x41, 7), (0xff, 3)], 3).unwrap();
    assert_eq!((a.steps, a.speed, a.loop_, a.wave), (3, 1, 1, 0x41));
    assert_eq!(a.off[..3], [0, 4, 7]);
    let nine: Vec<(u8, u8)> = (0..9).map(|_| (0x41, 0)).chain([(0xff, 0)]).collect();
    let not_arpeggios: [(&str, &[(u8, u8)]); 12] = [
        ("an absolute note", &[(0x41, 0x81), (0xff, 0)]),
        ("keep", &[(0x41, 0), (0x41, 0x80), (0xff, 0)]),
        ("a delay first", &[(2, 0), (2, 4), (0xff, 0)]),
        ("mixed delays", &[(0x41, 0), (2, 4), (3, 7), (0xff, 0)]),
        ("a jump out of the rows", &[(0x41, 0), (2, 4), (0xff, 7)]),
        (
            "a jump onto a delayed program's waveform row",
            &[(0x41, 0), (2, 4), (0xff, 1)],
        ),
        ("no end", &[(0x41, 0), (2, 4)]),
        ("a command", &[(0xf1, 0), (0xff, 0)]),
        (
            "a waveform change the helper does not write",
            &[(0x41, 0), (0x21, 4), (0xff, 0)],
        ),
        ("too many steps", &nine),
        ("a blank row", &[(0, 0), (0, 0)]),
        ("a stop", &[(0xff, 0), (0xff, 0)]),
    ];
    for (name, rows) in not_arpeggios {
        assert!(decode(rows, 1).is_none(), "{name}: {rows:?}");
    }
}

#[test]
fn the_reference_player_plays_each_waveform_the_helper_chooses() {
    for wave in [0x11u8, 0x21, 0x41, 0x81, 0x31, 0x51, 0x61, 0x71, 0x45, 0x23] {
        let mut a = arp(&[0, 4, 7, 12], 2, 1);
        a.wave = wave;
        let r = rows(&a, WAVE_PROGRAM_ROW);
        assert!(
            r.iter().all(|&(l, _)| l == wave || l < 0x10 || l == 0xff),
            "{wave:02X}: the rows carry the waveform: {r:?}"
        );
        let ctl = voice1_controls(&song_playing_c3_plus(r, 0));
        let at = ctl
            .iter()
            .position(|&c| c & 0xf0 != 0)
            .expect("a waveform sounds");
        assert_eq!(ctl[at], wave, "{wave:02X}: the control register");
        assert!(
            ctl[at..at + 60].iter().all(|&c| c == wave),
            "{wave:02X}: it holds through the steps"
        );
        let got = freqs_from_first_sound(&a);
        let want = freqs_the_helper_promises(&a, 60);
        assert_eq!(&got[..60], &want[..], "{wave:02X}: the steps");
    }
}
