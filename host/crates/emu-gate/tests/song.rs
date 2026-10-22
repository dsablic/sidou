// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The song player in the audio interrupt (M2c Task 6), end to end in fm1-emu: PLAY plays
//! SIDOU's first song and the firmware's audio equals the host build's render of the same
//! song, with the same calls at the same samples, sample for sample (at multispeed 1 and 4,
//! through edits while it plays and a profile change of chip A); the player's worst block
//! against PLAYER_WORST_BLOCK on every example song and generated songs (a bench image, the
//! songs written into its RAM one at a time); the main loop's passes while a song loads;
//! the memory map (the song's region, the free RAM).

mod common;

use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::song::Song;
use sidou_tests::song_embed;

/// app/panel.h PANEL_BTN_PLAY (Felucca's id for button 12).
const PLAY: u8 = PANEL_BTN_PLAY as u8;
/// How long a test plays a song.
const PLAY_MS: u64 = 10_000;
/// The DMA plays the zeroed buffer before the first half the interrupt filled.
const MAX_SILENT_HALVES: usize = 4;

fn fixture(what: &str) -> Option<(std::path::PathBuf, Firmware)> {
    let Some(build) = build_dir() else {
        skip_message(what);
        return None;
    };
    let fw = Firmware::load(&build);
    Some((build, fw))
}

fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
}

/// The firmware's song_ctl (platform/fm1/audio.c `song_transport`) and one of its fields.
fn transport(fw: &Firmware) -> u32 {
    fw.symbol("song_transport")
}

fn field32(m: &mut Machine, fw: &Firmware, offset: usize) -> u32 {
    m.soc.read32(transport(fw) + offset as u32)
}

/// A song written over the device's region (its main loop leaves the region alone until
/// PLAY).
fn inject(m: &mut Machine, fw: &Firmware, song: &Song) {
    let base = fw.symbol("__song_region");
    for (i, &b) in song.bytes().iter().enumerate() {
        m.soc.write8(base + i as u32, b);
    }
}

/// What the firmware did: its audio, the block PLAY applied at (0: the first the interrupt
/// rendered), the blocks each later command applied at.
struct Played {
    audio: Vec<i16>,
    play_block: usize,
    command_blocks: Vec<usize>,
    /// The block chip B went in use (the keys' move at PLAY: app/keys_route.h).
    b_use: Option<usize>,
}

/// Boots past the boot, writes `song` into the region if given, presses PLAY, runs `ms`
/// with `during` called every 100 ms (the time since PLAY).
fn play(
    fw: &Firmware,
    song: Option<&Song>,
    ms: u64,
    during: impl FnMut(&mut Machine, u64),
) -> Played {
    play_after(fw, song, ms, |_| {}, during)
}

/// `play`, with `before` run between the boot and PLAY (a view opened, say).
fn play_after(
    fw: &Firmware,
    song: Option<&Song>,
    ms: u64,
    before: impl FnOnce(&mut Machine),
    during: impl FnMut(&mut Machine, u64),
) -> Played {
    play_at_pot(fw, None, song, ms, before, during)
}

/// `play_after` with the MASTER pot at `pot` from power-on (None: fm1-emu's POT).
fn play_at_pot(
    fw: &Firmware,
    pot: Option<u16>,
    song: Option<&Song>,
    ms: u64,
    before: impl FnOnce(&mut Machine),
    mut during: impl FnMut(&mut Machine, u64),
) -> Played {
    let mut m = Machine::boot(&fw.image);
    if let Some(v) = pot {
        m.pot(v);
    }
    let chips = fw.chips();
    let t = transport(fw);
    m.snapshot_bytes = vec![
        chips + ilp32::reg_ring_chips::ticks as u32,
        t + ilp32::song_ctl::applied as u32,
        fw.symbol("b_on"),
    ];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 1)));
    run_past_boot(&mut m, fw);
    if let Some(s) = song {
        inject(&mut m, fw, s);
    }
    before(&mut m);
    press(&mut m, PLAY);
    let mut elapsed = 30;
    while elapsed < ms {
        during(&mut m, elapsed);
        m.run_ms(100);
        elapsed += 100;
    }
    let play_block = field32(&mut m, fw, ilp32::song_ctl::play_block) as usize;
    assert!(play_block > 0, "PLAY never reached the interrupt");
    let mut command_blocks = Vec::new();
    let mut applied = None;
    let mut b_use = None;
    for (i, snap) in m.buffer_snapshots.iter().enumerate() {
        if b_use.is_none() && snap.bytes[2] != 0 {
            b_use = Some(i);
        }
        if let Some(a) = applied {
            for _ in 0..snap.bytes[1].wrapping_sub(a) {
                command_blocks.push(i);
            }
        }
        applied = Some(snap.bytes[1]);
    }
    Played {
        audio: m.soc.periph.audio.clone(),
        play_block: play_block - 1,
        command_blocks,
        b_use,
    }
}

/// The device's audio against a host render, over the silent halves the DMA may lead with
/// and the block the keys' registers reach chip B in (around the one it goes in use: the
/// device sets the use at once and its writes go at the pass's end): the halves found, or the
/// closest difference.
fn matches(p: &Played, host: impl Fn(usize, Option<usize>) -> Vec<i16>) -> Result<usize, i32> {
    let mut closest = i32::MAX;
    let rewrites: Vec<Option<usize>> = match p.b_use {
        Some(b) => (b.saturating_sub(1)..=b + 1).map(Some).collect(),
        None => vec![None],
    };
    for silent in 0..=MAX_SILENT_HALVES {
        if p.audio[..2 * silent * HALF_FRAMES].iter().any(|&v| v != 0) {
            break;
        }
        let played = &p.audio[2 * silent * HALF_FRAMES..];
        for &r in &rewrites {
            let want = host(played.len() / 2, r);
            let d = max_diff(played, &want);
            closest = closest.min(d);
            if d == 0 {
                return Ok(silent);
            }
        }
    }
    Err(closest)
}

fn demo() -> Song {
    device_song(&song_embed::sng(), None)
}

fn check_song(name: &str, fw: &Firmware, song: Option<&Song>) -> Played {
    check_song_after(name, fw, song, |_| {})
}

fn check_song_after(
    name: &str,
    fw: &Firmware,
    song: Option<&Song>,
    before: impl FnOnce(&mut Machine),
) -> Played {
    let p = play_after(fw, song, PLAY_MS, before, |_, _| {});
    let s = song.map_or_else(demo, |s| {
        let mut c = Song::device();
        c.copy_from(s);
        c
    });
    let r = matches(&p, |frames, rewrite| {
        render_host_song(
            frames,
            &SongPlay {
                song: &s,
                play_block: p.play_block,
                ccs: &[],
                ticks: &[],
                edits: &[],
                stop_block: None,
                b_use_block: p.b_use,
                b_rewrite_block: rewrite,
            },
        )
    });
    let peak = p
        .audio
        .iter()
        .map(|&v| i32::from(v).abs())
        .max()
        .unwrap_or(0);
    println!(
        "{name}: PLAY at block {}, {} frames, peak {peak}: {r:?}",
        p.play_block,
        p.audio.len() / 2
    );
    assert!(
        peak > 1000,
        "{name}: the song is silent (peak {peak}): the comparison would be vacuous"
    );
    assert!(
        r.is_ok(),
        "{name}: the firmware's song differs from the host's: closest {r:?} LSB"
    );
    p
}

/// PLAY plays SIDOU's first song, sample for sample as the host renders it (10 s, PAL).
#[test]
fn play_plays_the_first_song_bit_exact_with_the_host() {
    let Some((_, fw)) = fixture("the song test") else {
        return;
    };
    let p = check_song("sidou-loop", &fw, None);
    // The start: PLAY's press to the block it applied at (the analysis's slices, chip A ready
    // at once without M2b Task 4's chip_ctl).
    println!("{} blocks from power-on to the play", p.play_block);
}

#[test]
fn a_four_note_tracker_phrase_is_the_expected_song_and_plays_bit_exact() {
    let Some((_, fw)) = fixture("the TRACKER phrase test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    let mut expected = demo();
    let order = unsafe { gt2_order_data(expected.ptr(), 0, 0) };
    let pattern = u32::from(unsafe { *order });
    press(&mut m, PANEL_BTN_SEQ as u8); // STEP, then TRACKER
    m.run_ms(300);
    press(&mut m, PANEL_BTN_SEQ as u8);
    m.run_ms(500);
    press(&mut m, PANEL_BTN_REC as u8);
    for (row, key) in [16u8, 18, 20, 23].into_iter().enumerate() {
        press(&mut m, PANEL_KEY_ID0 as u8 + key);
        m.run_ms(300);
        let old = unsafe { gt2_pattern_data(expected.ptr(), pattern).add(row * 4) };
        let mut cell: [u8; 4] = unsafe { std::slice::from_raw_parts(old, 4) }
            .try_into()
            .unwrap();
        cell[0] = (GT2_FIRSTNOTE + PANEL_BASE_NOTE + u32::from(key) - 12) as u8;
        cell[1] = 1;
        let mut edit = gt2_edit::default();
        unsafe {
            assert_eq!(
                gt2_set_row(
                    expected.ptr_mut(),
                    pattern,
                    row as u32,
                    cell.as_ptr(),
                    &mut edit
                ),
                GT2_OK
            );
            gt2_edit_commit(expected.ptr_mut(), &edit);
        }
    }
    let mut actual = Song::device();
    let base = fw.symbol("__song_region");
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(actual.ptr_mut().cast::<u8>(), SONG_REGION_BYTES as usize)
    };
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = m.soc.read8(base + i as u32);
    }
    // M2c captures runtime setup on first audition. Keep pre-existing chunks and the
    // independently expected GT2 data exact; only known new setup chunks are admitted.
    assert_eq!(
        actual.write(false),
        expected.write(false),
        "the panel programmed the expected GT2 data"
    );
    for (tag, data) in actual.ext_chunks() {
        if expected.ext_find(tag).is_none() {
            assert!(
                [
                    sidou_tests::song::TAG_CHPA,
                    sidou_tests::song::TAG_CHPB,
                    sidou_tests::song::TAG_OPTS,
                    sidou_tests::song::TAG_KEYS,
                    sidou_tests::song::TAG_SLOT,
                    sidou_tests::song::TAG_MIDI
                ]
                .contains(&tag),
                "unexpected audition metadata tag {tag:#x}"
            );
            assert_eq!(expected.ext_set(tag, &data), 0);
        }
    }
    assert_eq!(
        actual.write(true),
        expected.write(true),
        "the panel programmed the expected phrase"
    );
    // A fresh playback starts from the same SID state on both builds, independent of the
    // audition's oscillator/envelope history while entering the notes.
    check_song("TRACKER four-note phrase", &fw, Some(&actual));
}

/// What STEP's toggle does to a pattern (app/step_view.h): a rest or key off becomes the lane's
/// note C-3 with instrument 1, an existing note a rest, and on a staccato lane the key off after
/// a note (on the next row if that is a rest; where the row before holds a note, the removed
/// note's row is a key off).
fn step_toggle(song: &mut Song, pattern: u32, row: u32) {
    let rows = unsafe { gt2_pattern_rows(song.ptr(), pattern) } - 1;
    let cell = |s: &Song, r: u32| -> [u8; 4] {
        let d = unsafe {
            std::slice::from_raw_parts(gt2_pattern_data(s.ptr(), pattern), 4 * rows as usize)
        };
        d[4 * r as usize..4 * r as usize + 4].try_into().unwrap()
    };
    let is_note = |n: u8| (GT2_FIRSTNOTE as u8..=GT2_LASTNOTE as u8).contains(&n);
    let write = |s: &mut Song, r: u32, c: [u8; 4]| {
        let mut edit = gt2_edit::default();
        unsafe {
            assert_eq!(
                gt2_set_row(s.ptr_mut(), pattern, r, c.as_ptr(), &mut edit),
                GT2_OK
            );
            gt2_edit_commit(s.ptr_mut(), &edit);
        }
    };
    let mut c = cell(song, row);
    let next = (row + 1 < rows).then(|| cell(song, row + 1));
    if is_note(c[0]) {
        let after_note = row > 0 && is_note(cell(song, row - 1)[0]);
        c[0] = if after_note {
            GT2_KEYOFF as u8
        } else {
            GT2_REST as u8
        };
        c[1] = 0;
        write(song, row, c);
        if let Some(mut n) = next {
            if n[0] == GT2_KEYOFF as u8 && n[1] == 0 && n[2] == 0 {
                n[0] = GT2_REST as u8;
                write(song, row + 1, n);
            }
        }
    } else {
        c[0] = GT2_FIRSTNOTE as u8 + 36;
        c[1] = 1;
        write(song, row, c);
        if let Some(mut n) = next {
            if n[0] == GT2_REST as u8 {
                n[0] = GT2_KEYOFF as u8;
                write(song, row + 1, n);
            }
        }
    }
}

/// STEP (M2d Task 5): steps toggled from the keys, a lock on one held step and a SET on another,
/// make the expected song (the model's own edits, independently applied), and it plays bit-exact
/// with the host, the lock in the host's render too.
#[test]
fn a_pattern_programmed_on_step_is_the_expected_song_and_plays_bit_exact() {
    let Some((_, fw)) = fixture("the STEP programming test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    let mut expected = demo();
    // V2's first pattern.
    let order = unsafe { gt2_order_data(expected.ptr(), 0, 1) };
    let pattern = u32::from(unsafe { *order });
    let rows = unsafe { gt2_pattern_rows(expected.ptr(), pattern) } - 1;
    press(&mut m, PANEL_BTN_SEQ as u8);
    m.run_ms(500);
    m.turn(PANEL_ENC_SELECT as u8, 1);
    m.run_ms(300);
    let key = |m: &mut Machine, k: u32, down: bool| m.key(PANEL_KEY_ID0 as u8 + k as u8, down);
    for step in [2u32, 9, 12, 5, 2] {
        key(&mut m, step, true);
        m.run_ms(30);
        key(&mut m, step, false);
        m.run_ms(400);
        step_toggle(&mut expected, pattern, step);
    }
    // A lock on step 7 (K1 four detents up from the middle), a SET on a step whose command
    // column is free (K4 two detents: sawtooth).
    assert!(rows >= 16);
    key(&mut m, 7, true);
    m.run_ms(60);
    m.turn(PANEL_ENC_K1 as u8, 4);
    m.run_ms(300);
    key(&mut m, 7, false);
    m.run_ms(300);
    let free = (0..16u32)
        .find(|&r| {
            let d = unsafe { gt2_pattern_data(expected.ptr(), pattern) };
            let c = unsafe { std::slice::from_raw_parts(d.add(4 * r as usize), 4) };
            r != 7 && c[2] == 0
        })
        .expect("a row with no command");
    key(&mut m, free, true);
    m.run_ms(60);
    m.turn(PANEL_ENC_K1 as u8 + 3, 2);
    m.run_ms(300);
    key(&mut m, free, false);
    m.run_ms(300);
    {
        let d = unsafe { gt2_pattern_data(expected.ptr(), pattern) };
        let mut c: [u8; 4] = unsafe { std::slice::from_raw_parts(d.add(4 * free as usize), 4) }
            .try_into()
            .unwrap();
        c[2] = 7;
        c[3] = 0x21;
        let mut edit = gt2_edit::default();
        unsafe {
            assert_eq!(
                gt2_set_row(expected.ptr_mut(), pattern, free, c.as_ptr(), &mut edit),
                GT2_OK
            );
            gt2_edit_commit(expected.ptr_mut(), &edit);
        }
    }
    let mut actual = Song::device();
    let base = fw.symbol("__song_region");
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(actual.ptr_mut().cast::<u8>(), SONG_REGION_BYTES as usize)
    };
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = m.soc.read8(base + i as u32);
    }
    assert_eq!(
        actual.write(false),
        expected.write(false),
        "the keys programmed the expected GT2 data"
    );
    for (tag, data) in actual.ext_chunks() {
        if tag != sidou_tests::song::TAG_LOCK && expected.ext_find(tag).is_none() {
            assert!(
                [
                    sidou_tests::song::TAG_CHPA,
                    sidou_tests::song::TAG_CHPB,
                    sidou_tests::song::TAG_OPTS,
                    sidou_tests::song::TAG_KEYS,
                    sidou_tests::song::TAG_SLOT,
                    sidou_tests::song::TAG_MIDI
                ]
                .contains(&tag),
                "unexpected audition metadata tag {tag:#x}"
            );
            assert_eq!(expected.ext_set(tag, &data), 0);
        }
    }
    // The lock went in after the audition's chunks, as the chunks stand in the file.
    assert_eq!(
        unsafe {
            song_lock_put(
                expected.ptr_mut(),
                pattern,
                7,
                SONG_LOCK_CUTOFF,
                1024 + 4 * 32,
            )
        },
        0
    );
    assert_eq!(
        actual.write(true),
        expected.write(true),
        "the keys programmed the expected song, lock included"
    );
    check_song("STEP-programmed pattern", &fw, Some(&actual));
}

/// STEP (M2d Task 5): a step held and K1 turned down locks that step's cutoff; SIDOU LOOP's
/// lead bar locked step by step from the keys plays bit-exact with the host and is darker than
/// the song without the locks (less of its power above 1.5 kHz), the bar after it as it was.
#[test]
fn a_held_step_and_k1_darkens_that_step_bit_exact_with_the_host() {
    let Some((_, fw)) = fixture("the STEP lock spectrum test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    let key = |m: &mut Machine, k: u32, down: bool| m.key(PANEL_KEY_ID0 as u8 + k as u8, down);
    press(&mut m, PANEL_BTN_SEQ as u8);
    m.run_ms(500); // STEP, V1: the lead's first bar (pattern 4), 16 steps
    for step in 0..16u32 {
        key(&mut m, step, true);
        m.run_ms(60);
        m.turn(PANEL_ENC_K1 as u8, -30); // 1024 - 30 * 32; the emulator turns a detent every 8 ms
        m.run_ms(300);
        key(&mut m, step, false);
        m.run_ms(100);
    }
    let mut song = Song::device();
    let base = fw.symbol("__song_region");
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(song.ptr_mut().cast::<u8>(), SONG_REGION_BYTES as usize)
    };
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = m.soc.read8(base + i as u32);
    }
    let mut got = vec![song_lock::default(); SONG_LOCKS_MAX as usize];
    let n = unsafe { song_ext_get_locks(song.ptr(), got.as_mut_ptr(), SONG_LOCKS_MAX) } as usize;
    let got: Vec<_> = got[..n]
        .iter()
        .map(|l| (l.pattern, l.row, l.param, l.value))
        .collect();
    let want: Vec<_> = (0..16u8)
        .map(|r| (4u8, r, SONG_LOCK_CUTOFF as u8, 64u16))
        .collect();
    assert_eq!(got, want, "a lock on each of the bar's 16 steps");
    let plain = demo();
    assert_eq!(song.write(false), plain.write(false), "no step was toggled");
    let p = check_song("STEP-locked lead", &fw, Some(&song));
    let host = |s: &Song| {
        render_host_song(
            (p.play_block + 400) * HALF_FRAMES + 40 * 6 * 880,
            &SongPlay {
                song: s,
                play_block: p.play_block,
                ccs: &[],
                ticks: &[],
                edits: &[],
                stop_block: None,
                b_use_block: p.b_use,
                b_rewrite_block: p.b_use,
            },
        )
    };
    let (locked, unlocked) = (host(&song), host(&plain));
    let left = |v: &[i16], from: usize, n: usize| -> Vec<f32> {
        v.iter()
            .step_by(2)
            .skip(from)
            .take(n)
            .map(|&s| f32::from(s))
            .collect()
    };
    let from = p.play_block * HALF_FRAMES + 20 * 880;
    let dark = power_above(&left(&locked, from, 8192), 1500.0);
    let bright = power_above(&left(&unlocked, from, 8192), 1500.0);
    let later = from + 32 * 6 * 880;
    let (l2, p2) = (
        power_above(&left(&locked, later, 8192), 1500.0),
        power_above(&left(&unlocked, later, 8192), 1500.0),
    );
    println!(
        "STEP's locks: the power above 1.5 kHz of the locked bar {dark:.4}, unlocked {bright:.4}; \
         two bars on {l2:.4} and {p2:.4}"
    );
    assert!(
        dark < 0.85 * bright,
        "the locked bar is not darker: {dark} against {bright}"
    );
    assert!(
        (l2 - p2).abs() <= 0.05 * p2,
        "the song's cutoff did not return: {l2} against {p2}"
    );
}

#[test]
fn an_instrument_page_edit_changes_the_next_notes_bit_exact_with_the_host() {
    let Some((_, fw)) = fixture("the INSTR edit audio test") else {
        return;
    };
    let source = demo();
    let old = source.raw().instr[0][GT2_INS_AD as usize];
    let value = old.saturating_add(16);
    assert_ne!(old, value);
    let mut m = Machine::boot(&fw.image);
    let t = transport(&fw);
    let ad = fw.symbol("__song_region") + ilp32::gt2_song::instr as u32 + GT2_INS_AD;
    m.snapshot_bytes = vec![
        fw.chips() + ilp32::reg_ring_chips::ticks as u32,
        t + ilp32::song_ctl::applied as u32,
        fw.symbol("b_on"),
        ad,
    ];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 1)));
    run_past_boot(&mut m, &fw);
    for _ in 0..4 {
        press(&mut m, PANEL_BTN_SEQ as u8);
        m.run_ms(400);
    }
    assert_eq!(top_view(&mut m, &fw), fw.symbol("song_pages_instr_view"));
    press(&mut m, PLAY);
    m.run_ms(1000);
    m.turn(PANEL_ENC_K1 as u8, 1);
    m.run_ms(2000);
    assert_eq!(
        m.soc.read8(ad),
        value,
        "INSTR changed the selected instrument's AD byte"
    );
    let play_block = field32(&mut m, &fw, ilp32::song_ctl::play_block) as usize - 1;
    let edit_block = m
        .buffer_snapshots
        .iter()
        .enumerate()
        .find(|(i, snap)| *i >= play_block && snap.bytes[3] == value)
        .map(|(i, _)| i)
        .expect("the instrument edit reached a rendered block");
    let b_use = m
        .buffer_snapshots
        .iter()
        .position(|snap| snap.bytes[2] != 0);
    let mut actual = Song::device();
    let region = fw.symbol("__song_region");
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(actual.ptr_mut().cast::<u8>(), SONG_REGION_BYTES as usize)
    };
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = m.soc.read8(region + i as u32);
    }
    let mut expected = demo();
    let mut expected_edit = gt2_edit::default();
    unsafe {
        assert_eq!(
            gt2_set_instr(expected.ptr_mut(), 1, GT2_INS_AD, value, &mut expected_edit),
            GT2_OK
        );
        gt2_edit_commit(expected.ptr_mut(), &expected_edit);
    }
    assert_eq!(actual.write(false), expected.write(false));
    let played = Played {
        audio: m.soc.periph.audio.clone(),
        play_block,
        command_blocks: Vec::new(),
        b_use,
    };
    let primitive: SongPrim =
        Box::new(move |s, edit| unsafe { gt2_set_instr(s, 1, GT2_INS_AD, value, edit) });
    let edits = vec![(edit_block, primitive)];
    let make = |rewrite, with_edit| SongPlay {
        song: &source,
        play_block,
        ccs: &[],
        ticks: &[],
        edits: if with_edit { &edits } else { &[] },
        stop_block: None,
        b_use_block: b_use,
        b_rewrite_block: rewrite,
    };
    let before_edit = Played {
        audio: played.audio[..2 * (edit_block - 2) * HALF_FRAMES].to_vec(),
        play_block,
        command_blocks: Vec::new(),
        b_use,
    };
    let baseline = matches(&before_edit, |frames, rewrite| {
        render_host_song(frames, &make(rewrite, false))
    });
    assert!(
        baseline.is_ok(),
        "INSTR navigation changed audio before its edit: {baseline:?}"
    );
    let result = matches(&played, |frames, rewrite| {
        render_host_song(frames, &make(rewrite, true))
    });
    println!("INSTR edit AD {old:02X}->{value:02X}, block {edit_block}, bit-exact: {result:?}");
    assert!(
        result.is_ok(),
        "INSTR audio differs from the independently edited host: {result:?}"
    );
    let frames = played.audio.len() / 2;
    let edited = render_host_song(frames, &make(b_use, true));
    let clean = render_host_song(frames, &make(b_use, false));
    assert!(
        max_diff(&edited, &clean) > 0,
        "AD edit must actually change the next notes' sound"
    );
}

/// The first song while SCENE shows RED MOON RITE, which the song's events drive
/// (app/scene_song.h): the audio is the host's, sample for sample, the vignette touching none
/// of it.
#[test]
fn the_first_song_is_bit_exact_while_red_moon_rite_plays_it() {
    let Some((_, fw)) = fixture("the song test under RED MOON RITE") else {
        return;
    };
    let f = &fw;
    check_song_after("sidou-loop under RED MOON RITE", &fw, None, |m| {
        // HOME, SELECT to SCENE in the menu, ALGORITHM: THE SCENE's card, then SCENE on FIRE;
        // SELECT back once: RED MOON RITE.
        const HOME: u8 = PANEL_BTN_HOME as u8;
        let items = VIEWS_MENU_ITEMS as i32;
        press(m, HOME);
        m.run_ms(700);
        m.turn(PANEL_ENC_SELECT as u8, -items);
        m.run_ms(10 * items as u64);
        m.turn(PANEL_ENC_SELECT as u8, VIEWS_MENU_SCENE as i32);
        m.run_ms(10 * VIEWS_MENU_SCENE as u64);
        m.turn(PANEL_ENC_ALGORITHM as u8, 1);
        m.run_ms(4000);
        assert_eq!(
            top_view(m, f),
            f.symbol("scene_view_view"),
            "SCENE did not open"
        );
        m.turn(PANEL_ENC_SELECT as u8, -1);
        m.run_ms(1000);
        let which = f.symbol("ui") + (ilp32::views::scene + ilp32::scene_view::which) as u32;
        assert_eq!(
            u32::from(m.soc.read8(which)),
            SCENE_RMR,
            "not RED MOON RITE"
        );
    });
}

/// The same song at multispeed 4 (four calls a frame, one in a block at most).
#[test]
fn the_first_song_at_multispeed_4_is_bit_exact_with_the_host() {
    let Some((_, fw)) = fixture("the song test") else {
        return;
    };
    let mut o = sidou_tests::player::default_options();
    o.multispeed = 4;
    let song = device_song(&song_embed::sng(), Some(&o));
    check_song("sidou-loop x4", &fw, Some(&song));
}

/// app.ld's map: the song's region is 64 KiB, 4 KiB aligned, at POOL's start; RAM (the RAM
/// region and POOL) keeps at least FREE_RAM_MIN bytes free (common/mod.rs), each region at
/// least REGION_FREE_MIN (5,120).
#[test]
fn the_song_region_and_the_free_ram() {
    let Some((build, fw)) = fixture("the map test") else {
        return;
    };
    let region = fw.symbol("__song_region");
    let end = fw.symbol("__song_region_end");
    assert_eq!(end - region, 65_536);
    assert_eq!(region % 4096, 0);
    let elf = read(&build.join("platform/fm1/sidou_app.elf"), "firmware ELF");
    use object::{Object, ObjectSection};
    let file = object::File::parse(&*elf).unwrap();
    let size = |name: &str| file.section_by_name(name).map_or(0, |s| s.size());
    const RAM_START: u32 = 0x01C0_E000;
    const POOL_START: u32 = 0x01C1_E000;
    const POOL_BYTES: u64 = 0x59F00;
    let ram_bytes = u64::from(POOL_START - RAM_START);
    let ram_used = size(".data") + size(".bss") + size(".song_ram");
    let ram_free = ram_bytes - ram_used;
    let pool_free = POOL_BYTES - size(".pool");
    assert_eq!(region, POOL_START, "the region starts POOL");
    assert_eq!(
        u64::from(POOL_START) + POOL_BYTES + 256,
        u64::from(fw.symbol("_ustack_lo")),
        "POOL ends at the user stack's guard band"
    );
    // The stacks every machine checks when it goes (common::Machine's Drop).
    assert_eq!(
        (fw.symbol("_ustack_lo"), fw.symbol("_ustack_top")),
        (USTACK.start, USTACK.end)
    );
    assert_eq!(
        (fw.symbol("_sstack_lo"), fw.symbol("_sstack_top")),
        (SSTACK.start, SSTACK.end)
    );
    println!(
        "RAM region: {} used of {ram_bytes}, {ram_free} free; POOL: {} used of {POOL_BYTES}, \
         {pool_free} free; together {} free; RAMTEXT {} bytes",
        ram_used,
        size(".pool"),
        ram_free + pool_free,
        size(".ram_text")
    );
    assert!(
        ram_free >= REGION_FREE_MIN && pool_free >= REGION_FREE_MIN,
        "each region keeps {REGION_FREE_MIN} free: RAM region {ram_free}, POOL {pool_free}"
    );
    assert!(
        ram_free + pool_free >= FREE_RAM_MIN,
        "free RAM under {FREE_RAM_MIN}"
    );
}

/// One bench run (bench/fm1/bench_song_main.c): the song in the bench's RAM, `blocks`
/// blocks with SONG_EDITS_PER_BLOCK edits of `kind` (0 name cells, 1 swaps, 2 none,
/// 3 isolated real row cells) each after the first.
struct Bench {
    /// Instructions of each block.
    blocks: Vec<u64>,
    /// The main loop's sending of a block's edits (song_ctl_edit eight times), at most.
    edits_main: u64,
    analysis: u64,
    derive: u64,
    calls: u32,
    commands: u32,
    error: i32,
}

const SONG_ADDR: u32 = 0x01CA_0000;
const PARAM_ADDR: u32 = 0x01CB_0000;

fn bench(build: &std::path::Path, song: &Song, sub: u32, blocks: u32, kind: u32) -> Bench {
    bench_bytes(build, song.bytes(), sub, blocks, kind)
}

/// `bench` on a song's packed bytes (`Song::bytes`), which a thread may take.
fn bench_bytes(build: &std::path::Path, song: &[u8], sub: u32, blocks: u32, kind: u32) -> Bench {
    let r = run_with(build, "sid_bench_song", 3 + 2 * blocks as usize, |soc| {
        for (i, &b) in song.iter().enumerate() {
            soc.write8(SONG_ADDR + i as u32, b);
        }
        for (i, v) in [sub, blocks, kind].into_iter().enumerate() {
            soc.write32(PARAM_ADDR + 4 * i as u32, v);
        }
    });
    let error = r.words[2] as i32;
    let calls = r.words[3];
    Bench {
        blocks: (0..blocks as usize).map(|b| r.phases[3 + 2 * b]).collect(),
        edits_main: (1..blocks as usize)
            .map(|b| r.phases[2 + 2 * b])
            .max()
            .unwrap_or(0),
        analysis: r.phases[0],
        derive: r.phases[1],
        calls,
        commands: r.words[4],
        error,
    }
}

fn row_delta_apply_cost(build: &std::path::Path) -> u64 {
    let song = demo();
    let baseline = bench(build, &song, 0, 192, 2);
    let rows = bench(build, &song, 0, 192, 3);
    assert_eq!(baseline.error, 0);
    assert_eq!(rows.error, 0);
    assert!(
        baseline.commands > 0,
        "the fixture must have sent its play command"
    );
    assert_eq!(
        baseline.calls, rows.calls,
        "the player's clock is unchanged"
    );
    assert_eq!(
        baseline.commands, rows.commands,
        "kind 3 adds no player commands"
    );
    let additional = baseline
        .blocks
        .iter()
        .zip(&rows.blocks)
        .skip(1)
        .map(|(&before, &after)| {
            after
                .checked_sub(before)
                .expect("isolated row applies cannot make the player's block cheaper")
        })
        .max()
        .unwrap();
    assert!(
        additional > 0,
        "kind 3 must execute real row-delta accounting"
    );
    println!(
        "eight isolated row applies: maximum block {} -> {}, additional {additional} instructions \
         including loop/call overhead; player calls {} in both runs",
        baseline.blocks.iter().skip(1).max().unwrap(),
        rows.blocks.iter().skip(1).max().unwrap(),
        rows.calls
    );
    additional
}

#[test]
fn real_row_delta_accounting_is_included_in_the_interrupt_cost() {
    let Some((build, _)) = fixture("the row-delta apply microcase") else {
        return;
    };
    let additional = row_delta_apply_cost(&build);
    assert!(additional as f64 / HALF_FRAMES as f64 <= PLAYER_WORST_BLOCK);
}

/// The songs the budget is measured over: every example, every subtune, and generated
/// songs within the device's caps, with the options to play them with.
fn budget_songs() -> Vec<(String, Song, u32)> {
    let mut v = Vec::new();
    for (name, sng) in gt2_ref::examples() {
        for sub in 0..gt2_ref::subtunes(&sng) {
            for ms in [1u8, 4] {
                let mut o = sidou_tests::player::default_options();
                o.multispeed = ms;
                v.push((
                    format!("{name} {sub} x{ms}"),
                    device_song(&sng, Some(&o)),
                    sub,
                ));
            }
            if sub == 0 {
                v.push((format!("{name} {sub} locked"), locked(&sng), sub));
            }
        }
    }
    v
}

/// A song with locks on as many of its rows as SONG_LOCKS_MAX allows: every filter and pulse
/// parameter (five locks a row), on the rows of the patterns its first subtune names, in order.
fn locked(sng: &[u8]) -> Song {
    let mut song = device_song(sng, None);
    let mut pats: Vec<u32> = Vec::new();
    for ch in 0..3 {
        if let Some((entries, _)) = song.order(0, ch) {
            for e in entries {
                if e < 0xd0 && !pats.contains(&u32::from(e)) {
                    pats.push(u32::from(e));
                }
            }
        }
    }
    let values = [
        (SONG_LOCK_CUTOFF, 300),
        (SONG_LOCK_RESONANCE, 9),
        (SONG_LOCK_FILTER_MODE, 1),
        (SONG_LOCK_ROUTE, 1),
        (SONG_LOCK_PULSE, 0x400),
    ];
    let mut n = 0;
    'fill: for row in 0..128u32 {
        for &p in &pats {
            if row + 1 >= unsafe { gt2_pattern_rows(song.ptr(), p) } {
                continue;
            }
            for &(param, v) in &values {
                if n == SONG_LOCKS_MAX {
                    break 'fill;
                }
                assert_eq!(
                    unsafe { song_lock_put(song.ptr_mut(), p, row, param, v) },
                    0
                );
                n += 1;
            }
        }
    }
    song
}

/// PLAYER_WORST_BLOCK: the player's call and SONG_EDITS_PER_BLOCK of the costliest edit in
/// one block, at most 60 instructions a sample, over every example song and subtune at
/// multispeed 1 and 4 (3,000 calls each) and generated songs. Then the whole interrupt with
/// two chips at their worst, the slots' heaviest and the player within the audio target, and
/// the 6502's headroom (M3: the song player and the 6502 never run together). The runs (193,
/// each a bench in its own fm1-emu) go to every core, and their maxima are taken in their
/// order, so the numbers are a sequential run's: 14 minutes on one core, under 4 on ten.
#[test]
fn the_players_worst_block_fits_its_budget() {
    let Some((build, _)) = fixture("the player's budget") else {
        return;
    };
    let mut worst = (0u64, String::new());
    let mut worst_edits = 0u64; // a block with the edits and no call
    let mut worst_call = 0u64; // a call and its block's handling, no edits besides
    let mut edits_main = 0u64;
    let (mut analysis, mut derive) = (0u64, 0u64);
    let mut songs = budget_songs();
    for seed in 0..40u64 {
        let g = gt2_ref::gen::generate(seed);
        if g.options.multiplier == 0 || g.options.multiplier > SONG_MULTISPEED_MAX {
            continue;
        }
        let mut o = sidou_tests::player::default_options();
        o.multispeed = g.options.multiplier as u8;
        o.hr_adsr = g.options.hr_adsr as u16;
        o.optimize_pulse = g.options.optimize_pulse as u8;
        o.optimize_realtime = g.options.optimize_realtime as u8;
        for sub in 0..u32::from(g.sng[100]) {
            songs.push((
                format!("seed {seed} {sub}"),
                device_song(&g.sng, Some(&o)),
                sub,
            ));
        }
    }
    // The runs, in order (swaps on a quarter of them: they cost less, reported), each an
    // independent bench in its own fm1-emu, spread over the cores; their results are then taken
    // in this order, so the maxima and the worst's name are a sequential run's.
    struct Job<'a> {
        name: &'a str,
        song: Vec<u8>,
        sub: u32,
        blocks: u32,
        kind: u32,
    }
    let mut jobs = Vec::new();
    for (name, song, sub) in &songs {
        let ms = unsafe {
            let mut o = gt2_options::default();
            song_ext_get_options(song.ptr(), &mut o);
            u32::from(o.multispeed.max(1))
        };
        // 3,000 calls: a call every 880 / ms samples
        let blocks = (3000 * 880 / ms / HALF_FRAMES as u32) + 8;
        for kind in [0u32, 1] {
            if kind == 1 && jobs.len() % 4 != 0 {
                continue;
            }
            jobs.push(Job {
                name,
                song: song.bytes().to_vec(),
                sub: *sub,
                blocks,
                kind,
            });
        }
    }
    let runs = jobs.len();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut results: Vec<(usize, Bench)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(j) = jobs.get(i) else {
                            return done;
                        };
                        done.push((i, bench_bytes(&build, &j.song, j.sub, j.blocks, j.kind)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("a bench thread"))
            .collect()
    });
    results.sort_by_key(|&(i, _)| i);
    for (i, b) in &results {
        let Job { name, kind, .. } = &jobs[*i];
        if b.error != 0 {
            continue; // a subtune the packed song does not hold
        }
        assert!(b.calls >= 2900, "{name}: {} calls", b.calls);
        analysis = analysis.max(b.analysis);
        derive = derive.max(b.derive);
        let max = *b.blocks.iter().max().unwrap();
        let min = *b.blocks.iter().skip(1).min().unwrap();
        worst_edits = worst_edits.max(min);
        worst_call = worst_call.max(max - min);
        edits_main = edits_main.max(b.edits_main);
        if max > worst.0 {
            worst = (
                max,
                format!("{name} ({})", if *kind == 0 { "cells" } else { "swaps" }),
            );
        }
    }
    let row_delta = row_delta_apply_cost(&build);
    let charged = worst.0 + row_delta;
    let per_sample = charged as f64 / HALF_FRAMES as f64;
    let headroom = AUDIO_TARGET
        - (SID_WORST_BLOCK_TWO_CHIPS + TAP_PER_SAMPLE + ISR_OVERHEAD)
        - SLOTS_SWITCH_WORST
        - per_sample;
    println!(
        "the player's worst block: {} + {row_delta} isolated row-apply instructions = {charged} \
         instructions ({per_sample:.1} a sample, budget \
         {PLAYER_WORST_BLOCK}) in {}; a block of 8 edits and no call {worst_edits}, a call at most \
         {worst_call} besides; over {} runs of {} songs; the main loop's sending of 8 edits at \
         most {edits_main}. The play's analysis at most {analysis} instructions (all its slices), a \
         whole derivation {derive}. With two chips at their worst ({SID_WORST_BLOCK_TWO_CHIPS}), \
         the tap, the ISR and the slots' worst switching block ({SLOTS_SWITCH_WORST}): \
         {headroom:.1} instructions a sample left of {AUDIO_TARGET}",
        worst.0,
        worst.1,
        runs,
        songs.len()
    );
    assert!(
        per_sample <= PLAYER_WORST_BLOCK,
        "the player's worst block costs {per_sample:.1} a sample (in {}), over {PLAYER_WORST_BLOCK}",
        worst.1
    );
    // a fast-forward's slice: SONG_FF_CALLS calls at the worst (and its bookkeeping)
    assert!(
        u64::from(SONG_FF_CALLS) * charged <= u64::from(SONG_SLICE_INSTRUCTIONS),
        "{SONG_FF_CALLS} calls of {worst_call} are over the slice of {SONG_SLICE_INSTRUCTIONS}"
    );
}

/// platform/fm1/main.c `song_test_hook`: four requests of 20 bytes (op, a, b, cell, block).
const HOOK_REQ: u32 = 20;

/// Edits while the song plays: row writes and rebuilds (swaps), made by the device's main
/// loop with the model's primitives as M2d's editor will. The audio equals the host's render
/// with the same edits applied at the same blocks (each command's block from the interrupt's
/// applied count, read after every block).
#[test]
fn edits_while_playing_are_bit_exact_with_the_host() {
    let Some((_, fw)) = fixture("the song edit test") else {
        return;
    };
    let hook = fw.symbol("song_test_hook");
    // (op, pattern, row, cell): notes the song already has, so the shape stays as it is
    let reqs: [(u32, u32, u32, [u8; 4]); 4] = [
        (1, 4, 2, [0x60 + 4 * 12 + 9, 2, 0, 0]),
        (2, 8, 3, [0; 4]),
        (1, 0, 5, [0x60 + 12 + 9, 1, 0, 0]),
        (3, 9, 1, [0; 4]),
    ];
    let p = play(&fw, None, 6_000, |m, ms| {
        if ms == 2_030 {
            for (i, &(op, a, b, cell)) in reqs.iter().enumerate() {
                let r = hook + HOOK_REQ * i as u32;
                m.soc.write32(r + 4, a);
                m.soc.write32(r + 8, b);
                for (k, &c) in cell.iter().enumerate() {
                    m.soc.write8(r + 12 + k as u32, c);
                }
                m.soc.write32(r, op);
            }
        }
    });
    // the play's command, then the four edits
    assert_eq!(
        p.command_blocks.len(),
        1 + reqs.len(),
        "{:?}",
        p.command_blocks
    );
    assert_eq!(p.command_blocks[0], p.play_block);
    let prims: Vec<(usize, SongPrim)> = reqs
        .iter()
        .zip(&p.command_blocks[1..])
        .map(|(&(op, a, b, cell), &block)| {
            let f: SongPrim = Box::new(move |s, e| unsafe {
                match op {
                    1 => gt2_set_row(s, a, b, cell.as_ptr(), e),
                    2 => gt2_pattern_insert_row(s, a, b, e),
                    _ => gt2_pattern_delete_row(s, a, b, e),
                }
            });
            (block, f)
        })
        .collect();
    let song = demo();
    let r = matches(&p, |frames, rewrite| {
        render_host_song(
            frames,
            &SongPlay {
                song: &song,
                play_block: p.play_block,
                ccs: &[],
                ticks: &[],
                edits: &prims,
                stop_block: None,
                b_use_block: p.b_use,
                b_rewrite_block: rewrite,
            },
        )
    });
    println!(
        "edits applied at blocks {:?}: {r:?}",
        &p.command_blocks[1..]
    );
    assert!(
        r.is_ok(),
        "the edited song differs from the host's: closest {r:?} LSB"
    );
}

/// A song the device cannot play in one call a block (multispeed 8) is refused with a toast,
/// and nothing plays.
#[test]
fn multispeed_8_is_refused_with_its_toast() {
    let Some((_, fw)) = fixture("the song refusal test") else {
        return;
    };
    let mut o = sidou_tests::player::default_options();
    o.multispeed = 8;
    let song = device_song(&song_embed::sng(), Some(&o));
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    inject(&mut m, &fw, &song);
    press(&mut m, PLAY);
    m.run_ms(200);
    let toast = fw.symbol("ui") + ilp32::screen::toast as u32;
    let text: String = (0..SCREEN_TOAST_CHARS)
        .map(|i| m.soc.read8(toast + i) as char)
        .take_while(|&c| c != '\0')
        .collect();
    assert_eq!(text, "SNG: MULTISPEED 8 (MAX 4)");
    assert_eq!(
        field32(&mut m, &fw, ilp32::song_ctl::play_block),
        0,
        "nothing played"
    );
}

/// PLAY again stops the song: every voice's gate goes off and its envelope releases (as on
/// a C64), and chip A goes back to the keys (keys_route_set_song: keys_route takes it back).
#[test]
fn play_again_stops_and_releases() {
    let Some((_, fw)) = fixture("the song stop test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    press(&mut m, PLAY);
    m.run_ms(3_000);
    press(&mut m, PLAY);
    m.run_ms(100);
    let chips = fw.chips();
    let active = m.soc.read8(chips + ilp32::reg_ring_chips::active as u32);
    let chip =
        chips + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32;
    for v in 0..3u32 {
        let env = chip + (ilp32::sid::env + v as usize * ilp32::sid_env::SIZE) as u32;
        assert_eq!(
            m.soc.read8(env + ilp32::sid_env::gate as u32),
            0,
            "voice {v}'s gate"
        );
    }
    assert_eq!(
        field32(&mut m, &fw, ilp32::song_ctl::handed_back),
        1,
        "chip A handed back"
    );
    assert_eq!(
        m.soc
            .read8(transport(&fw) + ilp32::song_ctl::isr_playing as u32),
        0
    );
}

/// The main loop while a song loads: PLAY on the largest example, its analysis a slice a
/// pass; each slice (song_ctl_poll) within SONG_SLICE_INSTRUCTIONS, each pass within
/// POLL_INSTRUCTIONS. The start's delay reported.
#[test]
fn the_analysis_runs_in_slices_within_the_poll_allowance() {
    let Some((_, fw)) = fixture("the song pass test") else {
        return;
    };
    let (name, sng) = gt2_ref::examples()
        .into_iter()
        .max_by_key(|(_, s)| s.len())
        .unwrap();
    let song = device_song(&sng, None);
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    inject(&mut m, &fw, &song);
    m.watch_calls(fw.symbol("song_ctl_poll"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let blocks0 = field32(&mut m, &fw, ilp32::song_ctl::blocks);
    press(&mut m, PLAY);
    m.run_ms(1_000);
    let play_block = field32(&mut m, &fw, ilp32::song_ctl::play_block);
    assert!(play_block > 0, "{name} never played");
    let slice = m.watched_calls.iter().copied().max().unwrap();
    let pass = m.passes.iter().copied().max().unwrap();
    let delay = play_block - blocks0;
    println!(
        "{name} ({} bytes): the worst song_ctl_poll {slice} instructions (slice budget \
         {SONG_SLICE_INSTRUCTIONS}), the worst pass {pass} ({POLL_INSTRUCTIONS}); PLAY's press \
         to the play: {delay} blocks ({:.0} ms), {} slices",
        sng.len(),
        f64::from(delay) * HALF_FRAMES as f64 * 1000.0 / FS,
        field32(&mut m, &fw, ilp32::song_ctl::slices)
    );
    assert!(slice <= u64::from(SONG_SLICE_INSTRUCTIONS));
    assert!(pass <= POLL_INSTRUCTIONS);
}

/// Parameter locks (M2c Task 7): the first song with the lead's first bar's cutoff locked low
/// plays bit-exact with the host's render, and that bar is darker than without the locks (less
/// of its power above 1.5 kHz), the rest as it was.
#[test]
fn a_locked_cutoff_plays_a_darker_bar_bit_exact_with_the_host() {
    let Some((_, fw)) = fixture("the song lock test") else {
        return;
    };
    let mut song = demo();
    for row in 0..16 {
        assert_eq!(
            unsafe { song_lock_put(song.ptr_mut(), 4, row, SONG_LOCK_CUTOFF, 40) },
            0
        );
    }
    let p = check_song("sidou-loop locked", &fw, Some(&song));
    let host = |s: &Song| {
        render_host_song(
            (p.play_block + 400) * HALF_FRAMES + 40 * 6 * 880,
            &SongPlay {
                song: s,
                play_block: p.play_block,
                ccs: &[],
                ticks: &[],
                edits: &[],
                stop_block: None,
                b_use_block: p.b_use,
                b_rewrite_block: p.b_use,
            },
        )
    };
    // the device's equals the locked host render (check_song); the two host renders compared
    let (locked, plain) = (host(&song), host(&demo()));
    let left = |v: &[i16], from: usize, n: usize| -> Vec<f32> {
        v.iter()
            .step_by(2)
            .skip(from)
            .take(n)
            .map(|&s| f32::from(s))
            .collect()
    };
    // the bar: 16 rows of 6 calls, 880 samples a call; its middle, from the play's block
    let from = p.play_block * HALF_FRAMES + 20 * 880;
    let dark = power_above(&left(&locked, from, 8192), 1500.0);
    let bright = power_above(&left(&plain, from, 8192), 1500.0);
    // the bar after next (rows 32-47): the song's own cutoff again
    let later = from + 32 * 6 * 880;
    let (l2, p2) = (
        power_above(&left(&locked, later, 8192), 1500.0),
        power_above(&left(&plain, later, 8192), 1500.0),
    );
    println!(
        "the power above 1.5 kHz (the whole mix: the bass and drums are not filtered): the \
         locked bar {dark:.4}, unlocked {bright:.4}; two bars on {l2:.4} and {p2:.4}"
    );
    assert!(
        dark < 0.85 * bright,
        "the locked bar is not darker: {dark} against {bright}"
    );
    assert!(
        (l2 - p2).abs() <= 0.05 * p2,
        "the song's cutoff did not return: {l2} against {p2}"
    );
}

/// app/panel.h: GLO, SEQ and the first note key's id (Felucca's ids).
const GLO: u8 = PANEL_BTN_GLO as u8;
const SEQ: u8 = PANEL_BTN_SEQ as u8;
const KEY0: u8 = 14;

/// Chip A's voice v as the interrupt renders it: its frequency, its control register's
/// waveform and its envelope's gate.
fn chip_a_voice(m: &mut Machine, fw: &Firmware, v: usize) -> (u32, u8, u8) {
    let chips = fw.chips();
    let active = m.soc.read8(chips + ilp32::reg_ring_chips::active as u32);
    let chip =
        chips + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32;
    let o = chip + (ilp32::sid::osc + v * ilp32::sid_osc::SIZE) as u32;
    let e = chip + (ilp32::sid::env + v * ilp32::sid_env::SIZE) as u32;
    (
        m.soc.read32(o + ilp32::sid_osc::freq as u32),
        m.soc.read8(o + ilp32::sid_osc::control as u32),
        m.soc.read8(e + ilp32::sid_env::gate as u32),
    )
}

/// The song views' mute (M2d Task 1): GLO held and the leftmost key while SIDOU LOOP plays.
/// The mute reaches the interrupt within a frame, voice 1's gate goes off and its writes
/// stop (chip A's voice stays as it was while the song plays on), the other voices play on;
/// GLO and the key again give it back.
#[test]
fn glo_and_the_first_key_mute_voice_1_and_its_writes_stop() {
    let Some((_, fw)) = fixture("the song mute test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    press(&mut m, PLAY);
    m.run_ms(2_000);
    let t = transport(&fw);
    let mut before = std::collections::HashSet::new();
    for _ in 0..100 {
        m.run_ms(20);
        before.insert(chip_a_voice(&mut m, &fw, 0).0);
    }
    assert!(before.len() > 2, "voice 1 plays: {before:?}");
    m.key(GLO, true);
    m.run_ms(30);
    m.key(KEY0, true);
    let from = m.ms();
    while m.soc.read8(t + ilp32::song_ctl::mute as u32) != 1 {
        m.run_ms(1);
        assert!(m.ms() - from <= 40, "the mute took more than a frame");
    }
    println!(
        "the mute reached the interrupt {} ms after the key",
        m.ms() - from
    );
    m.key(KEY0, false);
    m.key(GLO, false);
    m.run_ms(40);
    let held = chip_a_voice(&mut m, &fw, 0);
    assert_eq!(held.2, 0, "voice 1's gate off");
    let mut others = std::collections::HashSet::new();
    for _ in 0..150 {
        m.run_ms(20);
        assert_eq!(
            chip_a_voice(&mut m, &fw, 0),
            held,
            "voice 1 was written while muted"
        );
        others.insert((
            chip_a_voice(&mut m, &fw, 1).0,
            chip_a_voice(&mut m, &fw, 2).0,
        ));
    }
    assert!(others.len() > 2, "voices 2 and 3 play on");
    m.key(GLO, true);
    m.run_ms(30);
    m.key(KEY0, true);
    m.run_ms(30);
    m.key(KEY0, false);
    m.key(GLO, false);
    let mut after = std::collections::HashSet::new();
    for _ in 0..100 {
        m.run_ms(20);
        after.insert(chip_a_voice(&mut m, &fw, 0).0);
    }
    assert_eq!(m.soc.read8(t + ilp32::song_ctl::mute as u32), 0);
    assert!(after.len() > 2, "voice 1 plays again: {after:?}");
}

/// KEYS A over a playing song (the M2 review's 2.1): a MIDI note takes its voice from the song
/// before the note's first register reaches chip A, so the song never writes over the note.
/// SIDOU LOOP plays on chip A with the keys on it; notes come in over TRS at phases spread
/// over the song's 20 ms calls, and every held note's voice keeps the frequency the keys
/// wrote (live's freq_out) and its gate until its note-off; the voice then plays the song
/// again.
#[test]
fn keys_a_over_a_playing_song_keeps_every_midi_notes_pitch_and_gate() {
    let Some((_, fw)) = fixture("the KEYS A song test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    let l = m.soc.read32(fw.symbol("ui") + ilp32::views::l as u32);
    m.soc
        .write8(l + ilp32::live::keys_mode as u32, LIVE_KEYS_A as u8);
    m.run_ms(100);
    press(&mut m, PLAY);
    m.run_ms(2_000);
    let t = transport(&fw);
    assert_eq!(m.soc.read8(t + ilp32::song_ctl::isr_playing as u32), 1);
    assert_eq!(
        m.soc.read8(l + ilp32::live::keys_chip as u32),
        0,
        "KEYS A: the keys stay on chip A"
    );
    let gated = |m: &mut Machine, v: usize| {
        let at = l + (ilp32::live::voice + v * ilp32::live_voice::SIZE) as u32;
        (
            m.soc.read8(at + ilp32::live_voice::gate as u32),
            u32::from(m.soc.read16(at + ilp32::live_voice::freq_out as u32)),
        )
    };
    let step = m.soc.periph.cycle_of_ms(1) / 7;
    let mut overwritten = Vec::new();
    for i in 0..60u64 {
        let note = 48 + (i % 24) as u8;
        m.midi(&[0x90, note, 100]);
        let from = m.ms();
        let v = loop {
            if let Some(v) = (0..3).find(|&v| gated(&mut m, v).0 == 1) {
                break v;
            }
            assert!(m.ms() - from < 30, "attempt {i}: the note took no voice");
            m.run_ms(1);
        };
        // The keys' registers reach chip A within the main loop's latency, and stay.
        let mut held = 0;
        for ms in 0..60 {
            let (gate, freq) = gated(&mut m, v);
            let (f, control, env) = chip_a_voice(&mut m, &fw, v);
            let keys = gate == 1 && f == freq && control & 1 == 1 && env == 1;
            if keys {
                held += 1;
            } else if held > 0 || ms >= 30 {
                overwritten.push((i, v, held, freq, f, control, env));
                break;
            }
            m.run_ms(1);
        }
        m.midi(&[0x80, note, 0]);
        m.run_ms(30 + i % 20);
        m.run_cycles(step * (i % 7));
    }
    assert!(
        overwritten.is_empty(),
        "the song wrote over the keys' note (attempt, voice, ms it held, the keys' freq, \
         chip A's freq, control, envelope gate): {overwritten:?}"
    );
    // Given back: the voices play the song again.
    let mut heard = std::collections::HashSet::new();
    for _ in 0..100 {
        m.run_ms(20);
        heard.insert(chip_a_voice(&mut m, &fw, 0).0);
    }
    assert!(heard.len() > 2, "voice 1 plays the song again: {heard:?}");
}

/// The audition (M2d Task 1): in TRACKER with the song stopped, a key sounds the song's
/// instrument on the cursor's voice (voice 1) on chip A, its release lets it go, no row is
/// read (the song stays where it was: never played), and two seconds later chip A goes back
/// to the keys.
#[test]
fn a_key_in_tracker_auditions_on_chip_a_with_the_song_stopped() {
    let Some((_, fw)) = fixture("the song audition test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    press(&mut m, SEQ);
    press(&mut m, SEQ);
    m.run_ms(500);
    let t = transport(&fw);
    let from = m.ms();
    m.key(KEY0 + 16, true); // A4
    while chip_a_voice(&mut m, &fw, 0).2 == 0 {
        m.run_ms(1);
        assert!(m.ms() - from <= 400, "the audition did not sound");
    }
    println!("the audition sounds {} ms after the key", m.ms() - from);
    assert_eq!(m.soc.read8(t + ilp32::song_ctl::isr_playing as u32), 0);
    assert_eq!(m.soc.read8(t + ilp32::song_ctl::audition as u32), 1);
    let note = chip_a_voice(&mut m, &fw, 0).0;
    m.run_ms(300);
    assert_ne!(chip_a_voice(&mut m, &fw, 0).0, 0, "at {note}");
    for v in 1..3 {
        assert_eq!(chip_a_voice(&mut m, &fw, v).2, 0, "voice {} silent", v + 1);
    }
    m.key(KEY0 + 16, false);
    m.run_ms(100);
    assert_eq!(chip_a_voice(&mut m, &fw, 0).2, 0, "released");
    assert_eq!(field32(&mut m, &fw, ilp32::song_ctl::handed_back), 0);
    m.run_ms(2_500);
    assert_eq!(
        m.soc.read8(t + ilp32::song_ctl::audition as u32),
        0,
        "the audition ended"
    );
    assert_eq!(
        field32(&mut m, &fw, ilp32::song_ctl::handed_back),
        1,
        "chip A back to the keys"
    );
}

/// MASTER scales a playing song (the hardware test's first find: a song played at full volume
/// whatever the pot said, its own $D418 writes, 15, overriding the chip volume MASTER used to
/// set). With the pot at 1023, 256 and 64 the firmware's audio equals the host's render of the
/// same song through the same MASTER gain stage, sample for sample; the loudest sample scales
/// as the pot does; and at full MASTER the output is the song's, as before the fix (the host
/// render at unity, which is what the bit-exact tests above compare with at POT).
#[test]
fn master_scales_a_playing_song_and_full_master_leaves_it_alone() {
    let Some((_, fw)) = fixture("the MASTER song test") else {
        return;
    };
    let s = demo();
    let peak = |a: &[i16]| a.iter().map(|&v| i32::from(v).abs()).max().unwrap_or(0);
    let mut full_peak = 0;
    for pot in [1023u16, 256, 64] {
        let p = play_at_pot(&fw, Some(pot), None, PLAY_MS, |_| {}, |_, _| {});
        let r = matches(&p, |frames, rewrite| {
            render_host_song_master(
                frames,
                &SongPlay {
                    song: &s,
                    play_block: p.play_block,
                    ccs: &[],
                    ticks: &[],
                    edits: &[],
                    stop_block: None,
                    b_use_block: p.b_use,
                    b_rewrite_block: rewrite,
                },
                pot,
            )
        });
        let loudest = peak(&p.audio);
        println!("MASTER {pot}: peak {loudest}: {r:?}");
        assert!(
            r.is_ok(),
            "pot {pot}: the firmware differs from the host: {r:?}"
        );
        if pot == 1023 {
            full_peak = loudest;
            assert!(
                full_peak > 1000,
                "the song is silent: the scaling would be vacuous"
            );
        } else {
            // the audio taper (app/master_gain.h): the pot's gain as a share of unity
            let gain = unsafe { master_gain_of(u32::from(pot), LIVE_MASTER_MAX) };
            let want = f64::from(full_peak) * f64::from(gain) / f64::from(MASTER_UNITY);
            assert!(
                (f64::from(loudest) - want).abs() <= want * 0.02 + 2.0,
                "pot {pot}: peak {loudest}, wanted {want:.0} of {full_peak}"
            );
        }
    }
}

/// REC while the song plays (M2d Task 6): with SIDOU LOOP playing at tempo 6 (six calls a row)
/// in TRACKER and REC on, a key script timed against the player's own call count lands on the
/// expected rows (a key a quarter or less into a row on that row, three quarters or more on the
/// next: the nearest), the K1 turns become cutoff locks on the rows they were turned in, the
/// pattern the firmware holds is the expected one (cell writes and the lock table only), and
/// playing that song again is bit-exact with the host's render.
#[test]
fn rec_records_a_timed_key_script_on_the_expected_rows_and_plays_back_bit_exact() {
    let Some((_, fw)) = fixture("the REC recording test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    let mut expected = demo();
    let order = unsafe { gt2_order_data(expected.ptr(), 0, 0) };
    let pattern = u32::from(unsafe { *order });
    let t = transport(&fw);
    press(&mut m, SEQ);
    m.run_ms(300);
    press(&mut m, SEQ);
    m.run_ms(500);
    press(&mut m, PANEL_BTN_REC as u8);
    m.run_ms(100);
    press(&mut m, PLAY);
    let calls = |m: &mut Machine| m.soc.read32(t + ilp32::song_ctl::calls as u32);
    // The play starts from a fast-forwarded hand-over, so its call count does not begin at
    // the first row: the calls are counted from the first row the player starts after PLAY.
    let row_of = |m: &mut Machine| m.soc.read8(t + ilp32::song_ctl::row_row as u32);
    let from = m.ms();
    let first = row_of(&mut m);
    while row_of(&mut m) == first {
        m.run_ms(1);
        assert!(m.ms() - from < 3_000, "the song never started");
    }
    let (base, row0) = (calls(&mut m) - 1, u32::from(row_of(&mut m)));
    // (call of the row a key is pressed in, which key): in calls from the play's start,
    // the row's own call 0, 1, 4 or 5 (the key goes 10 ms into the call: 0.5 of a call).
    let script: [(u32, u8); 4] = [(1, 16), (4, 18), (5, 20), (0, 23)];
    let mut landed: Vec<(u32, u8)> = Vec::new(); // (row, key)
    let mut knob_rows: Vec<u32> = Vec::new();
    let mut at_row = row0 + 2; // the row the first key's call is on
    for (i, &(phase, key)) in script.iter().enumerate() {
        // Wait for call `phase` of row `at_row` (calls from the play's start), then half a call.
        let want = base + 6 * (at_row - row0) + phase;
        let from = m.ms();
        loop {
            let n = calls(&mut m).wrapping_sub(1);
            if n == want {
                break;
            }
            assert!(n < want, "missed call {want}: at {n}");
            m.run_ms(1);
            assert!(m.ms() - from < 6_000, "call {want} never came");
        }
        m.run_ms(10);
        // The premise: the player is on the row the calls say (tempo 6, from row 0).
        let row_now = u32::from(row_of(&mut m));
        assert_eq!(row_now, at_row, "voice 1's row at call {want}");
        m.key(PANEL_KEY_ID0 as u8 + key, true);
        m.run_ms(30);
        m.key(PANEL_KEY_ID0 as u8 + key, false);
        // phase 0 and 1 are before halfway into the row: that row; 4 and 5: the next.
        let row = at_row + u32::from(phase >= 3);
        landed.push((row, key));
        if i == 1 || i == 3 {
            // A K1 turn on the row the player is on now.
            m.run_ms(20);
            let r = u32::from(row_of(&mut m));
            m.turn(PANEL_ENC_K1 as u8, if i == 1 { 2 } else { 1 });
            knob_rows.push(r);
        }
        at_row += 3;
    }
    m.run_ms(400);
    press(&mut m, PLAY); // stop
    m.run_ms(500);
    for &(row, key) in &landed {
        let d = unsafe { gt2_pattern_data(expected.ptr(), pattern).add(row as usize * 4) };
        let mut cell: [u8; 4] = unsafe { std::slice::from_raw_parts(d, 4) }
            .try_into()
            .unwrap();
        cell[0] = (GT2_FIRSTNOTE + PANEL_BASE_NOTE + u32::from(key) - 12) as u8;
        cell[1] = 1;
        let mut edit = gt2_edit::default();
        unsafe {
            assert_eq!(
                gt2_set_row(expected.ptr_mut(), pattern, row, cell.as_ptr(), &mut edit),
                GT2_OK
            );
            gt2_edit_commit(expected.ptr_mut(), &edit);
        }
    }
    let mut actual = Song::device();
    let base = fw.symbol("__song_region");
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(actual.ptr_mut().cast::<u8>(), SONG_REGION_BYTES as usize)
    };
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = m.soc.read8(base + i as u32);
    }
    assert_eq!(
        actual.write(false),
        expected.write(false),
        "the keys, timed, wrote the expected rows ({landed:?})"
    );
    // The locks: two K1 turns, the second 32 above the first (1 detent after 2), on the rows
    // the player was on.
    let mut recs = vec![song_lock::default(); SONG_LOCKS_MAX as usize];
    let n = unsafe { song_ext_get_locks(actual.ptr(), recs.as_mut_ptr(), SONG_LOCKS_MAX) } as usize;
    let got: Vec<(u32, u32, u32)> = recs[..n]
        .iter()
        .map(|l| (u32::from(l.pattern), u32::from(l.row), u32::from(l.param)))
        .collect();
    let want_rows: Vec<(u32, u32, u32)> = knob_rows
        .iter()
        .map(|&r| (pattern, r, SONG_LOCK_CUTOFF))
        .collect();
    assert_eq!(
        got, want_rows,
        "cutoff locks on the rows the knob was turned in"
    );
    assert_eq!(
        u32::from(recs[1].value),
        u32::from(recs[0].value) + 32,
        "K1's detents are 32 apart"
    );
    for (tag, data) in actual.ext_chunks() {
        if tag != sidou_tests::song::TAG_LOCK && expected.ext_find(tag).is_none() {
            assert_eq!(expected.ext_set(tag, &data), 0, "setup chunk {tag:#x}");
        }
    }
    for l in &recs[..n] {
        assert_eq!(
            unsafe {
                song_lock_put(
                    expected.ptr_mut(),
                    u32::from(l.pattern),
                    u32::from(l.row),
                    u32::from(l.param),
                    u32::from(l.value),
                )
            },
            0
        );
    }
    assert_eq!(
        actual.write(true),
        expected.write(true),
        "the recording is the expected song, locks included"
    );
    check_song("REC-recorded pattern", &fw, Some(&actual));
}
