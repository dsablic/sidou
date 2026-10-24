// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The digi drums (decision record 0009): the DIGI chunk in SIDOU's trailer (GoatTracker
//! keeps the GT2 part and drops the chunk), the built-in samples, and their playback in the
//! song transport: the codes as the 6581's volume DAC makes them, from the hit's sample, the
//! voice's own sound held off on chip A.

use sidou_sys::*;
use sidou_tests::song::*;
use sidou_tests::transport::*;

const TAG_DIGI: u32 = tag(b"DIGI");
const R: [u8; 4] = [0xbd, 0, 0, 0];

fn kit_instr(d: u32) -> [u8; 25] {
    // SAFETY: a kit drum is 25 bytes, an instrument's.
    unsafe { std::mem::transmute::<gt2_kit_drum, [u8; 25]>(gt2_kit_instr[d as usize]) }
}

/// Instrument 1 the kit's kick and 2 its snare, the kit's rows; voice 1 plays instrument 1 at
/// row 0 and `second` at row 2 (240 ms on), the other voices rest.
fn spec(second: u8) -> SngSpec {
    let mut s = SngSpec::new();
    s.instr = vec![kit_instr(GT2_KIT_KICK), kit_instr(GT2_KIT_SNARE)];
    // SAFETY: plain constant data.
    let (wave, pulse) = unsafe {
        (
            (&raw const gt2_kit_wave).read(),
            (&raw const gt2_kit_pulse).read(),
        )
    };
    let rows = |t: &[[u8; 2]]| t.iter().map(|r| (r[0], r[1])).collect::<Vec<_>>();
    let pair = |l: &[u8], r: &[u8]| l.iter().zip(r).map(|(&a, &b)| [a, b]).collect::<Vec<_>>();
    s.tables = vec![
        rows(&pair(&wave[0], &wave[1])),
        rows(&pair(&pulse[0], &pulse[1])),
        vec![],
        vec![],
    ];
    let mut p0 = vec![R; 64];
    p0[0] = [0x60 + 36, 1, 0, 0];
    p0[2] = [0x60 + 36, second, 0, 0];
    s.patterns[0].0 = p0;
    s
}

fn digi_chunk(entries: &[(u8, u8)]) -> Vec<u8> {
    let mut d = vec![1, entries.len() as u8];
    for &(i, k) in entries {
        d.extend([i, k]);
    }
    d
}

fn song_with(entries: &[(u8, u8)], second: u8) -> Song {
    let mut song = Song::host();
    song.read(&spec(second).encode(), 0).unwrap();
    assert_eq!(song.ext_set(TAG_DIGI, &digi_chunk(entries)), 0);
    song
}

fn sample(kind: u32) -> Vec<u8> {
    let n = unsafe { song_digi_sample(kind, std::ptr::null_mut()) };
    assert!(n > 0, "sample {kind}");
    let mut codes = vec![0xffu8; n as usize];
    assert_eq!(unsafe { song_digi_sample(kind, codes.as_mut_ptr()) }, n);
    codes
}

fn level(code: u8) -> i32 {
    (i32::from(code) - 8) * SONG_DIGI_LEVEL as i32
}

/// What a block did: the chip's samples, the digi's added to them, and the sample a digi's
/// hit started at in it.
struct Block {
    chip: Vec<i16>,
    out: Vec<i16>,
    hit: Option<usize>,
}

/// One device block (song_ctl's block, chip A's render split at the call, the digi, the
/// block's end), keeping the chip's samples apart from the digi's.
fn block(t: &mut Transport) -> Block {
    let n = BLOCK as u32;
    let mut chip = vec![0i16; BLOCK];
    unsafe {
        song_ctl_block(&mut *t.sc, &mut *t.chip);
        let at = song_ctl_due(&*t.sc, n);
        song_ctl_render(&mut *t.sc, &mut *t.chip, chip.as_mut_ptr(), n);
        let before = t.sc.digi_at;
        let mut out = chip.clone();
        song_ctl_block_end(&mut *t.sc, out.as_mut_ptr(), n);
        // a hit in this block's call: no digi before, one after
        let hit = (at < n && before == 0 && t.sc.digi_at != 0).then_some(at as usize);
        Block { chip, out, hit }
    }
}

/// The transport playing the song from its start: the blocks until `blocks` have passed
/// since the first call.
fn play(song: &mut Song, model: sid_model, mute: u32, blocks: usize) -> (Vec<Block>, Transport) {
    let mut t = Transport::new(song);
    unsafe { sid_init(&mut *t.chip, model, SID_CLOCK_PAL, SAMPLE_RATE) };
    if mute != 0 {
        assert!(t.mute(mute));
    }
    assert!(t.play(0, 0));
    let mut out = Vec::new();
    let mut started = false;
    for _ in 0..blocks * 4 {
        if out.len() == blocks {
            break;
        }
        t.pass();
        let b = block(&mut t);
        started |= b.hit.is_some();
        if started {
            out.push(b);
        }
    }
    (out, t)
}

#[test]
fn the_chunk_round_trips_and_goattracker_keeps_only_the_gt2_part() {
    let song = song_with(&[(1, SONG_DIGI_KICK as u8), (2, SONG_DIGI_CLAP as u8)], 2);
    let mut v = song_digi::default();
    assert_eq!(unsafe { song_ext_get_digi(song.ptr(), &mut v) }, 1);
    assert_eq!(v.count, 2);
    assert_eq!(&v.entry[..2], &[[1, 1], [2, 3]]);
    let file = song.write(true);
    let plain = song.write(false);
    assert_eq!(
        &file[..plain.len()],
        &plain[..],
        "the GT2 part is the plain song's"
    );
    let mut back = Song::host();
    back.read(&file, 0).unwrap();
    assert_eq!(back.ext_find(TAG_DIGI), Some(digi_chunk(&[(1, 1), (2, 3)])));
    assert_eq!(back.write(true), file, "SIDOU writes it back byte for byte");
    // GoatTracker reads the file, the trailer skipped, and saves the GT2 part as it was.
    assert_eq!(
        gt2_ref::canonical(&file).expect("GoatTracker loads it"),
        plain
    );
    // set and get agree
    let mut w = song_digi {
        count: 1,
        ..Default::default()
    };
    w.entry[0] = [5, SONG_DIGI_HAT as u8];
    let mut s2 = Song::host();
    s2.read(&plain, 0).unwrap();
    assert_eq!(unsafe { song_ext_set_digi(s2.ptr_mut(), &w) }, 0);
    assert_eq!(s2.ext_find(TAG_DIGI), Some(digi_chunk(&[(5, 4)])));
}

#[test]
fn the_map_takes_known_entries_and_the_last_for_an_instrument() {
    let song = song_with(&[(0, 1), (1, 9), (2, 1), (2, 2), (64, 1), (63, 4)], 2);
    let mut map = [0u8; GT2_MAX_INSTR as usize + 1];
    unsafe { song_ext_digi_map(song.ptr(), map.as_mut_ptr()) };
    let marked: Vec<(usize, u8)> = map
        .iter()
        .enumerate()
        .filter(|e| *e.1 != 0)
        .map(|(i, &k)| (i, k))
        .collect();
    assert_eq!(marked, vec![(2, 2), (63, 4)]);
    // a chunk of another version, or a short one, marks nothing
    let mut s = song_with(&[], 2);
    assert_eq!(s.ext_set(TAG_DIGI, &[2, 1, 1, 1]), 0);
    unsafe { song_ext_digi_map(s.ptr(), map.as_mut_ptr()) };
    assert!(map.iter().all(|&k| k == 0));
    assert_eq!(s.ext_set(TAG_DIGI, &[1, 2, 1, 1]), 0);
    unsafe { song_ext_digi_map(s.ptr(), map.as_mut_ptr()) };
    assert!(map.iter().all(|&k| k == 0));
}

/// The built-in samples: 4-bit codes (two a byte in the firmware), rest at 8 at their end, each
/// about as long as decision record 0009 says, under 5,000 codes together.
#[test]
fn the_samples_are_4_bit_codes_ending_at_rest() {
    let mut bytes = 0;
    for (kind, ms) in [
        (SONG_DIGI_KICK, 220.0),
        (SONG_DIGI_SNARE, 180.0),
        (SONG_DIGI_CLAP, 200.0),
        (SONG_DIGI_HAT, 60.0),
    ] {
        let c = sample(kind);
        bytes += c.len();
        let len_ms = c.len() as f64 * SONG_DIGI_SPAN as f64 / f64::from(SAMPLE_RATE) * 1000.0;
        println!("sample {kind}: {} codes, {len_ms:.0} ms", c.len());
        assert!(
            len_ms > ms * 0.15 && len_ms <= ms + 1.0,
            "sample {kind}: {len_ms:.0} ms"
        );
        assert_eq!(c.last(), Some(&8), "sample {kind} ends at rest");
        assert!(
            c.iter().any(|&v| v == 0 || v == 15),
            "sample {kind} reaches full scale"
        );
    }
    assert_eq!(unsafe { song_digi_sample(0, std::ptr::null_mut()) }, 0);
    assert_eq!(
        unsafe { song_digi_sample(SONG_DIGI_KINDS, std::ptr::null_mut()) },
        0
    );
    // The firmware's codes, unpacked from gen/digi.h's two a byte, are sidou-tablegen's.
    for (k, (name, codes)) in sidou_tablegen::digi::samples().into_iter().enumerate() {
        assert_eq!(sample(k as u32 + 1), codes, "{name}");
    }
    assert!(bytes < 5000, "{bytes} bytes");
}

/// A digi instrument's note plays its sample from the call's sample on, each code held
/// SONG_DIGI_SPAN samples at (code - 8) SONG_DIGI_LEVEL over the chip's output; its voice's
/// gate stays off on chip A meanwhile, the chip's volume register the song's; a note of
/// another instrument ends the hold. On the 8580 the digi is the same (the 6581's level).
#[test]
fn a_digi_note_plays_its_sample_and_holds_its_voice_off() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut song = song_with(&[(1, SONG_DIGI_KICK as u8)], 2);
        let (blocks, t) = play(&mut song, model, 0, 200);
        let codes = sample(SONG_DIGI_KICK);
        assert_eq!(blocks.len(), 200, "the kick's digi started");
        let first = blocks[0].hit.unwrap();
        let mut k = 0usize; // samples since the hit
        let mut checked = 0;
        for (b, blk) in blocks.iter().enumerate() {
            for i in 0..BLOCK {
                if b == 0 && i < first {
                    assert_eq!(blk.out[i], blk.chip[i], "nothing before the hit");
                    continue;
                }
                let code = codes.get(k / SONG_DIGI_SPAN as usize).copied();
                let want = match code {
                    Some(c) => (i32::from(blk.chip[i]) + level(c)).clamp(-32768, 32767),
                    None => i32::from(blk.chip[i]),
                };
                assert_eq!(
                    i32::from(blk.out[i]),
                    want,
                    "block {b} sample {i} ({k} after the hit)"
                );
                k += 1;
                checked += 1;
            }
        }
        assert!(
            checked > codes.len() * SONG_DIGI_SPAN as usize,
            "the whole sample was heard"
        );
        // the kick's own sound held off: voice 1 silent until row 2's snare
        let quiet = blocks[..75]
            .iter()
            .all(|b| b.chip.iter().all(|&v| v.abs() < 200));
        assert!(quiet, "the fallback sounded under the digi");
        assert_eq!(
            t.chip.mode_vol & 15,
            15,
            "the volume register is the song's"
        );
        assert!(
            blocks[90..]
                .iter()
                .any(|b| b.chip.iter().any(|&v| v.abs() > 2000)),
            "the snare (not a digi) sounds on the chip"
        );
        assert_eq!(
            (t.sc.digi_hold, t.sc.mute_sent),
            (0, 0),
            "the snare ended the hold"
        );
    }
}

/// A muted voice plays no digi; a digi on another voice plays.
#[test]
fn a_muted_voice_plays_no_digi() {
    let mut song = song_with(&[(1, SONG_DIGI_KICK as u8)], 2);
    let (blocks, t) = play(&mut song, SID_MODEL_6581, 1, 40);
    assert!(blocks.is_empty(), "a muted voice's digi started");
    assert_eq!(t.sc.digi_at, 0);
}

/// NEW's song holds the digi drums from SONG_IO_DIGI, copies of the chip drums, and a NEW load
/// marks them with the DIGI chunk (app tests: song_io).
#[test]
fn the_new_song_holds_the_digi_drums_as_copies_of_the_chip_drums() {
    let mut sng = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        let n = unsafe {
            song_io_template(
                std::ptr::null_mut(),
                sng.len() as u32,
                buf.as_mut_ptr(),
                256,
            )
        };
        if n == 0 {
            break;
        }
        sng.extend_from_slice(&buf[..n as usize]);
    }
    let mut song = Song::host();
    song.read(&sng, 0).unwrap();
    let raw = song.raw();
    let names = ["DG KICK", "DG SNARE", "DG CLAP", "DG HAT"];
    let drums = [
        GT2_KIT_KICK,
        GT2_KIT_SNARE,
        GT2_KIT_CLAP,
        GT2_KIT_CLOSED_HAT,
    ];
    for (d, (name, drum)) in names.iter().zip(drums).enumerate() {
        let i = raw.instr[SONG_IO_DIGI as usize - 1 + d];
        let kit = kit_instr(drum);
        assert_eq!(
            i[..GT2_INS_NAME as usize],
            kit[..GT2_INS_NAME as usize],
            "{name}"
        );
        let n = &i[GT2_INS_NAME as usize..];
        assert_eq!(&n[..name.len()], name.as_bytes());
        assert!(n[name.len()..].iter().all(|&b| b == 0));
    }
    assert_eq!(
        unsafe { gt2_saved_instruments(song.ptr()) },
        SONG_IO_DIGI - 1 + SONG_DIGI_KINDS - 1
    );
    assert_eq!(gt2_ref::canonical(&sng).expect("GoatTracker loads it"), sng);
}

/// STEP's audition (a jam on the stopped song) plays a digi drum too.
#[test]
fn an_audition_of_a_digi_instrument_plays_its_sample() {
    let mut song = song_with(&[(1, SONG_DIGI_SNARE as u8)], 2);
    let mut t = Transport::new(&mut song);
    assert!(t.audition());
    assert!(t.run_until(2000, |t| t.jam_ready()));
    assert!(t.jam(0, 36, 1));
    let mut heard = false;
    for _ in 0..200 {
        t.pass();
        let b = block(&mut t);
        heard |= b.out != b.chip;
    }
    assert!(heard, "the audition's digi did not sound");
}
