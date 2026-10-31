// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::panel::{
    chord, holding, press, select_menu_item, A4_KEY, ALGORITHM, ARP, GLO, HOME, K1, K4, KEY0, PLAY,
    REC, SELECT, SEQ,
};
use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::song::Song;
use sidou_tests::song_embed;

const K2: u8 = K1 + 1;
const PLAY_MS: u64 = 10_000;
const MAX_SILENT_HALVES: usize = 4;
const AUDIBLE_PEAK: i32 = 1000;
const PRESS_MS: u64 = 30;
const DURING_STEP_MS: u64 = 100;
const SAMPLES_A_CALL: usize = 880;
const CALLS_A_ROW: usize = 6;
const CUTOFF_MIDDLE: u16 = 1024;
const CUTOFF_DETENT: u16 = 32;
const CUTOFF_DOWN_TO_64: i32 = -30;
const CUTOFF_64: u16 = CUTOFF_MIDDLE - 30 * CUTOFF_DETENT;
const BAR_ROWS: u32 = 16;
const LEAD_FIRST_BAR_PATTERN: u32 = 4;
const SPECTRUM_WINDOW: usize = 8192;
const BRIGHTNESS_HZ: f64 = 1500.0;
const DARKER_THAN: f64 = 0.85;
const SAME_WITHIN: f64 = 0.05;
const SONG_HOOK_WRITE_ROW: u32 = 1;
const SONG_HOOK_INSERT_ROW: u32 = 2;
const SONG_HOOK_DELETE_ROW: u32 = 3;
const SONG_HOOK_REQUEST_BYTES: u32 = 20;
const SONG_HOOK_A: u32 = 4;
const SONG_HOOK_B: u32 = 8;
const SONG_HOOK_CELL: u32 = 12;
const A1: u8 = 12 + 9;
const C3: u8 = 36;
const A4: u8 = 4 * 12 + 9;
const PAGES_TO_TABLE: usize = 5;
const PAGES_TO_INSTR: usize = 4;
const SNG_SUBTUNES: usize = 100;
const BUDGET_CALLS: u32 = 3000;
const BUDGET_CALLS_MIN: u32 = 2900;
const GENERATED_SEEDS: u64 = 40;
const DIGI_KICK_INSTRUMENT: u8 = GT2_KIT_DRUMS as u8 + 1;

fn press_up_at_once(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(PRESS_MS);
    m.key(id, false);
}

fn transport(fw: &Firmware) -> u32 {
    fw.symbol("song_transport")
}

fn field32(m: &mut Machine, fw: &Firmware, offset: usize) -> u32 {
    m.soc.read32(transport(fw) + offset as u32)
}

fn inject(m: &mut Machine, fw: &Firmware, song: &Song) {
    let base = fw.symbol("__song_region");
    for (i, &b) in song.bytes().iter().enumerate() {
        m.soc.write8(base + i as u32, b);
    }
}

fn region_song(m: &mut Machine, fw: &Firmware) -> Song {
    let mut song = Song::device();
    let base = fw.symbol("__song_region");
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(song.ptr_mut().cast::<u8>(), SONG_REGION_BYTES as usize)
    };
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = m.soc.read8(base + i as u32);
    }
    song
}

fn set_cell(song: &mut Song, pattern: u32, row: u32, cell: [u8; 4]) {
    let mut edit = gt2_edit::default();
    unsafe {
        assert_eq!(
            gt2_set_row(song.ptr_mut(), pattern, row, cell.as_ptr(), &mut edit),
            GT2_OK
        );
        gt2_edit_commit(song.ptr_mut(), &edit);
    }
}

fn cell_of(song: &Song, pattern: u32, row: u32) -> [u8; 4] {
    let d = unsafe { gt2_pattern_data(song.ptr(), pattern).add(4 * row as usize) };
    unsafe { std::slice::from_raw_parts(d, 4) }
        .try_into()
        .unwrap()
}

fn key_note(key: u8) -> u8 {
    (GT2_FIRSTNOTE + PANEL_BASE_NOTE + u32::from(key) - 12) as u8
}

fn first_pattern_of_voice(song: &Song, voice: u32) -> u32 {
    u32::from(unsafe { *gt2_order_data(song.ptr(), 0, voice) })
}

fn adopt_the_auditions_setup_chunks(actual: &Song, expected: &mut Song) {
    let setup = [
        sidou_tests::song::TAG_CHPA,
        sidou_tests::song::TAG_CHPB,
        sidou_tests::song::TAG_OPTS,
        sidou_tests::song::TAG_KEYS,
        sidou_tests::song::TAG_SLOT,
        sidou_tests::song::TAG_MIDI,
    ];
    for (tag, data) in actual.ext_chunks() {
        if tag != sidou_tests::song::TAG_LOCK && expected.ext_find(tag).is_none() {
            assert!(
                setup.contains(&tag),
                "unexpected audition metadata tag {tag:#x}"
            );
            assert_eq!(expected.ext_set(tag, &data), 0);
        }
    }
}

fn open_song_pages(m: &mut Machine, pages: usize, each_ms: u64) {
    for _ in 0..pages {
        press(m, SEQ);
        m.run_ms(each_ms);
    }
}

fn plain_play(
    song: &Song,
    play_block: usize,
    b_use: Option<usize>,
    rewrite: Option<usize>,
) -> SongPlay<'_> {
    SongPlay {
        song,
        play_block,
        ccs: &[],
        ticks: &[],
        edits: &[],
        stop_block: None,
        b_use_block: b_use,
        b_rewrite_block: rewrite,
    }
}

fn assert_the_locked_bar_is_darker_and_the_next_as_it_was(p: &Played, locked: &Song, plain: &Song) {
    let frames = (p.play_block + 400) * HALF_FRAMES + 40 * CALLS_A_ROW * SAMPLES_A_CALL;
    let host = |s: &Song| render_host_song(frames, &plain_play(s, p.play_block, p.b_use, p.b_use));
    let (locked, unlocked) = (host(locked), host(plain));
    let brightness = |v: &[i16], from: usize| {
        let left: Vec<f32> = v
            .iter()
            .step_by(2)
            .skip(from)
            .take(SPECTRUM_WINDOW)
            .map(|&s| f32::from(s))
            .collect();
        power_above(&left, BRIGHTNESS_HZ)
    };
    let from = p.play_block * HALF_FRAMES + 20 * SAMPLES_A_CALL;
    let dark = brightness(&locked, from);
    let bright = brightness(&unlocked, from);
    let two_bars_on = from + 2 * BAR_ROWS as usize * CALLS_A_ROW * SAMPLES_A_CALL;
    let (l2, p2) = (
        brightness(&locked, two_bars_on),
        brightness(&unlocked, two_bars_on),
    );
    println!(
        "the power above 1.5 kHz (the whole mix: the bass and drums are not filtered): the \
         locked bar {dark:.4}, unlocked {bright:.4}; two bars on {l2:.4} and {p2:.4}"
    );
    assert!(
        dark < DARKER_THAN * bright,
        "the locked bar is not darker: {dark} against {bright}"
    );
    assert!(
        (l2 - p2).abs() <= SAME_WITHIN * p2,
        "the song's cutoff did not return: {l2} against {p2}"
    );
}

struct Played {
    audio: Vec<i16>,
    play_block: usize,
    command_blocks: Vec<usize>,
    b_use: Option<usize>,
}

fn play(
    fw: &Firmware,
    song: Option<&Song>,
    ms: u64,
    during: impl FnMut(&mut Machine, u64),
) -> Played {
    play_after(fw, song, ms, |_| {}, during)
}

fn play_after(
    fw: &Firmware,
    song: Option<&Song>,
    ms: u64,
    before: impl FnOnce(&mut Machine),
    during: impl FnMut(&mut Machine, u64),
) -> Played {
    play_at_pot(fw, None, song, ms, before, during)
}

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
    press_up_at_once(&mut m, PLAY);
    let mut elapsed = PRESS_MS;
    while elapsed < ms {
        during(&mut m, elapsed);
        m.run_ms(DURING_STEP_MS);
        elapsed += DURING_STEP_MS;
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
        render_host_song(frames, &plain_play(&s, p.play_block, p.b_use, rewrite))
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
        peak > AUDIBLE_PEAK,
        "{name}: the song is silent (peak {peak}): the comparison would be vacuous"
    );
    assert!(
        r.is_ok(),
        "{name}: the firmware's song differs from the host's: closest {r:?} LSB"
    );
    p
}

#[test]
fn play_plays_the_first_song_bit_exact_with_the_host() {
    let Some((_, fw)) = build_and_firmware("the song test") else {
        return;
    };
    let p = check_song("sidou-loop", &fw, None);
    println!("{} blocks from power-on to the play", p.play_block);
}

#[test]
fn a_four_note_tracker_phrase_is_the_expected_song_and_plays_bit_exact() {
    let Some((_, fw)) = build_and_firmware("the TRACKER phrase test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    let mut expected = demo();
    let pattern = first_pattern_of_voice(&expected, 0);
    press(&mut m, SEQ);
    m.run_ms(300);
    press(&mut m, SEQ);
    m.run_ms(500);
    press(&mut m, REC);
    for (row, key) in [16u8, 18, 20, 23].into_iter().enumerate() {
        press(&mut m, KEY0 + key);
        m.run_ms(300);
        let mut cell = cell_of(&expected, pattern, row as u32);
        cell[0] = key_note(key);
        cell[1] = 1;
        set_cell(&mut expected, pattern, row as u32, cell);
    }
    let actual = region_song(&mut m, &fw);
    assert_eq!(
        actual.write(false),
        expected.write(false),
        "the panel programmed the expected GT2 data"
    );
    adopt_the_auditions_setup_chunks(&actual, &mut expected);
    assert_eq!(
        actual.write(true),
        expected.write(true),
        "the panel programmed the expected phrase"
    );
    check_song("TRACKER four-note phrase", &fw, Some(&actual));
}

fn step_toggle(song: &mut Song, pattern: u32, row: u32) {
    let rows = unsafe { gt2_pattern_rows(song.ptr(), pattern) } - 1;
    let cell = |s: &Song, r: u32| -> [u8; 4] {
        let d = unsafe {
            std::slice::from_raw_parts(gt2_pattern_data(s.ptr(), pattern), 4 * rows as usize)
        };
        d[4 * r as usize..4 * r as usize + 4].try_into().unwrap()
    };
    let is_note = |n: u8| (GT2_FIRSTNOTE as u8..=GT2_LASTNOTE as u8).contains(&n);
    let write = |s: &mut Song, r: u32, c: [u8; 4]| set_cell(s, pattern, r, c);
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
        c[0] = GT2_FIRSTNOTE as u8 + C3;
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

#[test]
fn a_pattern_programmed_on_step_is_the_expected_song_and_plays_bit_exact() {
    let Some((_, fw)) = build_and_firmware("the STEP programming test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    let mut expected = demo();
    let pattern = first_pattern_of_voice(&expected, 1);
    let rows = unsafe { gt2_pattern_rows(expected.ptr(), pattern) } - 1;
    press(&mut m, SEQ);
    m.run_ms(500);
    m.turn(SELECT, 1);
    m.run_ms(300);
    let step_key = |k: u32| KEY0 + k as u8;
    for step in [2u32, 9, 12, 5, 2] {
        press(&mut m, step_key(step));
        m.run_ms(400);
        step_toggle(&mut expected, pattern, step);
    }
    assert!(rows >= BAR_ROWS);
    const LOCKED_STEP: u32 = 7;
    const CUTOFF_UP_CLICKS: i32 = 4;
    const SET_SAWTOOTH_CLICKS: i32 = 2;
    holding(&mut m, step_key(LOCKED_STEP), |m| {
        m.turn(K1, CUTOFF_UP_CLICKS);
        m.run_ms(300);
    });
    m.run_ms(300);
    let free = (0..BAR_ROWS)
        .find(|&r| r != LOCKED_STEP && cell_of(&expected, pattern, r)[2] == 0)
        .expect("a row with no command");
    holding(&mut m, step_key(free), |m| {
        m.turn(K4, SET_SAWTOOTH_CLICKS);
        m.run_ms(300);
    });
    m.run_ms(300);
    let mut c = cell_of(&expected, pattern, free);
    c[2] = 7;
    c[3] = 0x21;
    set_cell(&mut expected, pattern, free, c);
    let actual = region_song(&mut m, &fw);
    assert_eq!(
        actual.write(false),
        expected.write(false),
        "the keys programmed the expected GT2 data"
    );
    adopt_the_auditions_setup_chunks(&actual, &mut expected);
    assert_eq!(
        unsafe {
            song_lock_put(
                expected.ptr_mut(),
                pattern,
                LOCKED_STEP,
                SONG_LOCK_CUTOFF,
                u32::from(CUTOFF_MIDDLE) + CUTOFF_UP_CLICKS as u32 * u32::from(CUTOFF_DETENT),
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

#[test]
fn a_held_step_and_k1_darkens_that_step_bit_exact_with_the_host() {
    let Some((_, fw)) = build_and_firmware("the STEP lock spectrum test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    press(&mut m, SEQ);
    m.run_ms(500);
    for step in 0..BAR_ROWS as u8 {
        holding(&mut m, KEY0 + step, |m| {
            m.turn(K1, CUTOFF_DOWN_TO_64);
            m.run_ms(300);
        });
        m.run_ms(100);
    }
    let song = region_song(&mut m, &fw);
    let mut got = vec![song_lock::default(); SONG_LOCKS_MAX as usize];
    let n = unsafe { song_ext_get_locks(song.ptr(), got.as_mut_ptr(), SONG_LOCKS_MAX) } as usize;
    let got: Vec<_> = got[..n]
        .iter()
        .map(|l| (l.pattern, l.row, l.param, l.value))
        .collect();
    let want: Vec<_> = (0..BAR_ROWS as u8)
        .map(|r| {
            (
                LEAD_FIRST_BAR_PATTERN as u8,
                r,
                SONG_LOCK_CUTOFF as u8,
                CUTOFF_64,
            )
        })
        .collect();
    assert_eq!(got, want, "a lock on each of the bar's 16 steps");
    let plain = demo();
    assert_eq!(song.write(false), plain.write(false), "no step was toggled");
    let p = check_song("STEP-locked lead", &fw, Some(&song));
    assert_the_locked_bar_is_darker_and_the_next_as_it_was(&p, &song, &plain);
}

#[test]
fn an_instrument_page_edit_changes_the_next_notes_bit_exact_with_the_host() {
    let Some((_, fw)) = build_and_firmware("the INSTR edit audio test") else {
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
    open_song_pages(&mut m, PAGES_TO_INSTR, 400);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("song_pages_instr_view"));
    press_up_at_once(&mut m, PLAY);
    m.run_ms(1000);
    m.turn(K1, 1);
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
    let actual = region_song(&mut m, &fw);
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

#[test]
fn the_first_song_is_bit_exact_while_red_moon_rite_plays_it() {
    let Some((_, fw)) = build_and_firmware("the song test under RED MOON RITE") else {
        return;
    };
    let f = &fw;
    check_song_after("sidou-loop under RED MOON RITE", &fw, None, |m| {
        press(m, HOME);
        m.run_ms(700);
        select_menu_item(m, VIEWS_MENU_SCENE as i32);
        m.turn(ALGORITHM, 1);
        m.run_ms(4000);
        assert_eq!(
            top_view(m, f),
            f.symbol("scene_view_view"),
            "SCENE did not open"
        );
        m.turn(SELECT, -1);
        m.run_ms(1000);
        let which = f.symbol("ui") + (ilp32::views::scene + ilp32::scene_view::which) as u32;
        assert_eq!(
            u32::from(m.soc.read8(which)),
            SCENE_RMR,
            "not RED MOON RITE"
        );
    });
}

#[test]
fn the_first_song_at_multispeed_4_is_bit_exact_with_the_host() {
    let Some((_, fw)) = build_and_firmware("the song test") else {
        return;
    };
    let mut o = sidou_tests::player::default_options();
    o.multispeed = 4;
    let song = device_song(&song_embed::sng(), Some(&o));
    check_song("sidou-loop x4", &fw, Some(&song));
}

#[test]
fn the_song_region_is_64_kib_at_pools_start_and_ram_keeps_its_free_minimum() {
    let Some((build, fw)) = build_and_firmware("the map test") else {
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
    assert_eq!(
        (fw.symbol("_ustack_lo"), fw.symbol("_ustack_top")),
        (USER_STACK.start, USER_STACK.end)
    );
    assert_eq!(
        (fw.symbol("_sstack_lo"), fw.symbol("_sstack_top")),
        (SYSTEM_STACK.start, SYSTEM_STACK.end)
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

const BENCH_NAME_CELLS: u32 = 0;
const BENCH_SWAPS: u32 = 1;
const BENCH_NO_EDITS: u32 = 2;
const BENCH_ROW_CELLS: u32 = 3;
const SWAPS_EVERY: usize = 4;
const ROW_DELTA_BLOCKS: u32 = 192;

struct Bench {
    blocks: Vec<u64>,
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
    let baseline = bench(build, &song, 0, ROW_DELTA_BLOCKS, BENCH_NO_EDITS);
    let rows = bench(build, &song, 0, ROW_DELTA_BLOCKS, BENCH_ROW_CELLS);
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
    let Some((build, _)) = build_and_firmware("the row-delta apply microcase") else {
        return;
    };
    let additional = row_delta_apply_cost(&build);
    assert_within_budget(
        "eight isolated row applies a sample",
        additional as f64 / HALF_FRAMES as f64,
        PLAYER_WORST_BLOCK,
    );
}

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
                v.push((format!("{name} {sub} chance"), chanced(&sng), sub));
            }
        }
    }
    v
}

fn chanced(sng: &[u8]) -> Song {
    let mut song = device_song(sng, None);
    let mut n = 0;
    for p in 0..GT2_MAX_PATTERNS {
        let rows = unsafe { gt2_pattern_rows(song.ptr(), p) };
        for row in 0..rows.saturating_sub(1) {
            if n == SONG_LOCKS_MAX {
                return song;
            }
            if unsafe { song_lock_put(song.ptr_mut(), p, row, SONG_LOCK_CHANCE, 50) } == 0 {
                n += 1;
            }
        }
    }
    song
}

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

#[test]
fn the_players_worst_block_fits_its_budget() {
    let Some((build, _)) = build_and_firmware("the player's budget") else {
        return;
    };
    let mut worst = (0u64, String::new());
    let mut worst_edits_without_a_call = 0u64;
    let mut worst_call_beyond_the_edits = 0u64;
    let mut edits_main = 0u64;
    let (mut analysis, mut derive) = (0u64, 0u64);
    let mut songs = budget_songs();
    for seed in 0..GENERATED_SEEDS {
        let g = gt2_ref::gen::generate(seed);
        if g.options.multiplier == 0 || g.options.multiplier > SONG_MULTISPEED_MAX {
            continue;
        }
        let mut o = sidou_tests::player::default_options();
        o.multispeed = g.options.multiplier as u8;
        o.hr_adsr = g.options.hr_adsr as u16;
        o.optimize_pulse = g.options.optimize_pulse as u8;
        o.optimize_realtime = g.options.optimize_realtime as u8;
        for sub in 0..u32::from(g.sng[SNG_SUBTUNES]) {
            songs.push((
                format!("seed {seed} {sub}"),
                device_song(&g.sng, Some(&o)),
                sub,
            ));
        }
    }
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
        let blocks = (BUDGET_CALLS * SAMPLES_A_CALL as u32 / ms / HALF_FRAMES as u32) + 8;
        for kind in [BENCH_NAME_CELLS, BENCH_SWAPS] {
            if kind == BENCH_SWAPS && !jobs.len().is_multiple_of(SWAPS_EVERY) {
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
        let a_subtune_the_packed_song_does_not_hold = b.error != 0;
        if a_subtune_the_packed_song_does_not_hold {
            continue;
        }
        assert!(b.calls >= BUDGET_CALLS_MIN, "{name}: {} calls", b.calls);
        analysis = analysis.max(b.analysis);
        derive = derive.max(b.derive);
        let max = *b.blocks.iter().max().unwrap();
        let min = *b.blocks.iter().skip(1).min().unwrap();
        worst_edits_without_a_call = worst_edits_without_a_call.max(min);
        worst_call_beyond_the_edits = worst_call_beyond_the_edits.max(max - min);
        edits_main = edits_main.max(b.edits_main);
        if max > worst.0 {
            worst = (
                max,
                format!(
                    "{name} ({})",
                    if *kind == BENCH_NAME_CELLS {
                        "cells"
                    } else {
                        "swaps"
                    }
                ),
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
         {PLAYER_WORST_BLOCK}) in {}; a block of 8 edits and no call {worst_edits_without_a_call}, a call at most \
         {worst_call_beyond_the_edits} besides; over {} runs of {} songs; the main loop's sending of 8 edits at \
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
    assert!(
        u64::from(SONG_FF_CALLS) * charged <= u64::from(SONG_SLICE_INSTRUCTIONS),
        "{SONG_FF_CALLS} calls of {worst_call_beyond_the_edits} are over the slice of {SONG_SLICE_INSTRUCTIONS}"
    );
}

#[test]
fn edits_while_playing_are_bit_exact_with_the_host() {
    let Some((_, fw)) = build_and_firmware("the song edit test") else {
        return;
    };
    let hook = fw.symbol("song_test_hook");
    const EDITS_AT_MS: u64 = 2_000 + PRESS_MS;
    let note = |semitone: u8, instrument: u8| [GT2_FIRSTNOTE as u8 + semitone, instrument, 0, 0];
    let reqs: [(u32, u32, u32, [u8; 4]); 4] = [
        (SONG_HOOK_WRITE_ROW, 4, 2, note(A4, 2)),
        (SONG_HOOK_INSERT_ROW, 8, 3, [0; 4]),
        (SONG_HOOK_WRITE_ROW, 0, 5, note(A1, 1)),
        (SONG_HOOK_DELETE_ROW, 9, 1, [0; 4]),
    ];
    let p = play(&fw, None, 6_000, |m, ms| {
        if ms == EDITS_AT_MS {
            for (i, &(op, a, b, cell)) in reqs.iter().enumerate() {
                let r = hook + SONG_HOOK_REQUEST_BYTES * i as u32;
                m.soc.write32(r + SONG_HOOK_A, a);
                m.soc.write32(r + SONG_HOOK_B, b);
                for (k, &c) in cell.iter().enumerate() {
                    m.soc.write8(r + SONG_HOOK_CELL + k as u32, c);
                }
                m.soc.write32(r, op);
            }
        }
    });
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
                    SONG_HOOK_WRITE_ROW => gt2_set_row(s, a, b, cell.as_ptr(), e),
                    SONG_HOOK_INSERT_ROW => gt2_pattern_insert_row(s, a, b, e),
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
                edits: &prims,
                ..plain_play(&song, p.play_block, p.b_use, rewrite)
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

#[test]
fn multispeed_8_is_refused_with_its_toast() {
    let Some((_, fw)) = build_and_firmware("the song refusal test") else {
        return;
    };
    let mut o = sidou_tests::player::default_options();
    o.multispeed = 8;
    let song = device_song(&song_embed::sng(), Some(&o));
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    inject(&mut m, &fw, &song);
    press_up_at_once(&mut m, PLAY);
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

#[test]
fn play_again_stops_the_song_releases_every_voice_and_hands_chip_a_back() {
    let Some((_, fw)) = build_and_firmware("the song stop test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    press_up_at_once(&mut m, PLAY);
    m.run_ms(3_000);
    press_up_at_once(&mut m, PLAY);
    m.run_ms(100);
    let chip = rendered_sid(&mut m, &fw, 0);
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

#[test]
fn the_analysis_runs_in_slices_within_the_poll_allowance() {
    let Some((_, fw)) = build_and_firmware("the song pass test") else {
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
    press_up_at_once(&mut m, PLAY);
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
    assert_within_budget("song_ctl_poll", slice, u64::from(SONG_SLICE_INSTRUCTIONS));
    assert_within_budget("the busiest pass", pass, POLL_INSTRUCTIONS);
}

#[test]
fn a_locked_cutoff_plays_a_darker_bar_bit_exact_with_the_host() {
    let Some((_, fw)) = build_and_firmware("the song lock test") else {
        return;
    };
    const LOW_CUTOFF: u32 = 40;
    let mut song = demo();
    for row in 0..BAR_ROWS {
        assert_eq!(
            unsafe {
                song_lock_put(
                    song.ptr_mut(),
                    LEAD_FIRST_BAR_PATTERN,
                    row,
                    SONG_LOCK_CUTOFF,
                    LOW_CUTOFF,
                )
            },
            0
        );
    }
    let p = check_song("sidou-loop locked", &fw, Some(&song));
    assert_the_locked_bar_is_darker_and_the_next_as_it_was(&p, &song, &demo());
}

fn chip_a_voice(m: &mut Machine, fw: &Firmware, v: usize) -> (u32, u8, u8) {
    let chip = rendered_sid(m, fw, 0);
    let o = chip + (ilp32::sid::osc + v * ilp32::sid_osc::SIZE) as u32;
    let e = chip + (ilp32::sid::env + v * ilp32::sid_env::SIZE) as u32;
    (
        m.soc.read32(o + ilp32::sid_osc::freq as u32),
        m.soc.read8(o + ilp32::sid_osc::control as u32),
        m.soc.read8(e + ilp32::sid_env::gate as u32),
    )
}

#[test]
fn glo_and_the_first_key_mute_voice_1_within_a_frame_and_again_give_it_back() {
    let Some((_, fw)) = build_and_firmware("the song mute test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    press_up_at_once(&mut m, PLAY);
    m.run_ms(2_000);
    let t = transport(&fw);
    let mut before = std::collections::HashSet::new();
    for _ in 0..100 {
        m.run_ms(20);
        before.insert(chip_a_voice(&mut m, &fw, 0).0);
    }
    assert!(before.len() > 2, "voice 1 plays: {before:?}");
    holding(&mut m, GLO, |m| {
        m.key(KEY0, true);
        let from = m.ms();
        while m.soc.read8(t + ilp32::song_ctl::mute as u32) != 1 {
            m.run_ms(1);
            assert!(
                m.ms() - from <= UI_FRAME_MS,
                "the mute took more than a frame"
            );
        }
        println!(
            "the mute reached the interrupt {} ms after the key",
            m.ms() - from
        );
        m.key(KEY0, false);
    });
    m.run_ms(UI_FRAME_MS);
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
    chord(&mut m, GLO, KEY0);
    let mut after = std::collections::HashSet::new();
    for _ in 0..100 {
        m.run_ms(20);
        after.insert(chip_a_voice(&mut m, &fw, 0).0);
    }
    assert_eq!(m.soc.read8(t + ilp32::song_ctl::mute as u32), 0);
    assert!(after.len() > 2, "voice 1 plays again: {after:?}");
}

#[test]
fn keys_a_over_a_playing_song_keeps_every_midi_notes_pitch_and_gate() {
    let Some((_, fw)) = build_and_firmware("the KEYS A song test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    let l = ui_live(&mut m, &fw);
    m.soc
        .write8(l + ilp32::live::keys_mode as u32, LIVE_KEYS_A as u8);
    m.run_ms(100);
    press_up_at_once(&mut m, PLAY);
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
    let mut heard = std::collections::HashSet::new();
    for _ in 0..100 {
        m.run_ms(20);
        heard.insert(chip_a_voice(&mut m, &fw, 0).0);
    }
    assert!(heard.len() > 2, "voice 1 plays the song again: {heard:?}");
}

#[test]
fn a_key_in_tracker_auditions_on_chip_a_with_the_song_stopped_and_hands_chip_a_back() {
    let Some((_, fw)) = build_and_firmware("the song audition test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    press(&mut m, SEQ);
    press(&mut m, SEQ);
    m.run_ms(500);
    let t = transport(&fw);
    let from = m.ms();
    m.key(A4_KEY, true);
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
    m.key(A4_KEY, false);
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

#[test]
fn master_scales_a_playing_song_and_full_master_leaves_it_alone() {
    let Some((_, fw)) = build_and_firmware("the MASTER song test") else {
        return;
    };
    let s = demo();
    let peak = |a: &[i16]| a.iter().map(|&v| i32::from(v).abs()).max().unwrap_or(0);
    let mut full_peak = 0;
    const FULL_MASTER: u16 = 1023;
    for pot in [FULL_MASTER, 256, 64] {
        let p = play_at_pot(&fw, Some(pot), None, PLAY_MS, |_| {}, |_, _| {});
        let r = matches(&p, |frames, rewrite| {
            render_host_song_master(frames, &plain_play(&s, p.play_block, p.b_use, rewrite), pot)
        });
        let loudest = peak(&p.audio);
        println!("MASTER {pot}: peak {loudest}: {r:?}");
        assert!(
            r.is_ok(),
            "pot {pot}: the firmware differs from the host: {r:?}"
        );
        if pot == FULL_MASTER {
            full_peak = loudest;
            assert!(
                full_peak > AUDIBLE_PEAK,
                "the song is silent: the scaling would be vacuous"
            );
        } else {
            let gain = unsafe { master_gain_of(u32::from(pot), LIVE_MASTER_MAX) };
            let want = f64::from(full_peak) * f64::from(gain) / f64::from(MASTER_UNITY);
            assert!(
                (f64::from(loudest) - want).abs() <= want * 0.02 + 2.0,
                "pot {pot}: peak {loudest}, wanted {want:.0} of {full_peak}"
            );
        }
    }
}

#[test]
fn rec_records_a_timed_key_script_on_the_expected_rows_and_plays_back_bit_exact() {
    let Some((_, fw)) = build_and_firmware("the REC recording test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    let mut expected = demo();
    let pattern = first_pattern_of_voice(&expected, 0);
    let t = transport(&fw);
    press(&mut m, SEQ);
    m.run_ms(300);
    press(&mut m, SEQ);
    m.run_ms(500);
    press(&mut m, REC);
    m.run_ms(100);
    press_up_at_once(&mut m, PLAY);
    let calls = |m: &mut Machine| m.soc.read32(t + ilp32::song_ctl::calls as u32);
    let row_of = |m: &mut Machine| m.soc.read8(t + ilp32::song_ctl::row_row as u32);
    let from = m.ms();
    let first = row_of(&mut m);
    while row_of(&mut m) == first {
        m.run_ms(1);
        assert!(m.ms() - from < 3_000, "the song never started");
    }
    let (first_row_call, row0) = (calls(&mut m) - 1, u32::from(row_of(&mut m)));
    const HALF_A_CALL_MS: u64 = 10;
    const ROWS_BETWEEN_KEYS: u32 = 3;
    let call_in_row_and_key: [(u32, u8); 4] = [(1, 16), (4, 18), (5, 20), (0, 23)];
    let mut landed_rows_and_keys: Vec<(u32, u8)> = Vec::new();
    let mut knob_rows: Vec<u32> = Vec::new();
    let mut key_row = row0 + 2;
    for (i, &(call_in_row, key)) in call_in_row_and_key.iter().enumerate() {
        let want = first_row_call + CALLS_A_ROW as u32 * (key_row - row0) + call_in_row;
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
        m.run_ms(HALF_A_CALL_MS);
        let row_now = u32::from(row_of(&mut m));
        assert_eq!(row_now, key_row, "voice 1's row at call {want}");
        press_up_at_once(&mut m, KEY0 + key);
        let past_halfway = call_in_row >= CALLS_A_ROW as u32 / 2;
        landed_rows_and_keys.push((key_row + u32::from(past_halfway), key));
        let turns_k1_after = i == 1 || i == 3;
        if turns_k1_after {
            m.run_ms(20);
            let r = u32::from(row_of(&mut m));
            m.turn(K1, if i == 1 { 2 } else { 1 });
            knob_rows.push(r);
        }
        key_row += ROWS_BETWEEN_KEYS;
    }
    m.run_ms(400);
    press_up_at_once(&mut m, PLAY);
    m.run_ms(500);
    for &(row, key) in &landed_rows_and_keys {
        let mut cell = cell_of(&expected, pattern, row);
        cell[0] = key_note(key);
        cell[1] = 1;
        set_cell(&mut expected, pattern, row, cell);
    }
    let actual = region_song(&mut m, &fw);
    assert_eq!(
        actual.write(false),
        expected.write(false),
        "the keys, timed, wrote the expected rows ({landed_rows_and_keys:?})"
    );
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
        u32::from(recs[0].value) + u32::from(CUTOFF_DETENT),
        "K1's detents are 32 apart"
    );
    adopt_the_auditions_setup_chunks(&actual, &mut expected);
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

#[test]
fn the_play_led_is_dim_between_beats_and_lit_on_each_beat() {
    let Some((_, fw)) = build_and_firmware("the PLAY LED test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    m.run_ms(100);
    m.soc.periph.led_take();
    m.run_ms(100);
    assert_eq!(
        m.soc.periph.led_take()[usize::from(PLAY)],
        0.0,
        "stopped: dark"
    );
    press_up_at_once(&mut m, PLAY);
    m.run_ms(1500);
    m.soc.periph.led_take();
    let mut levels = Vec::new();
    for _ in 0..400 {
        m.run_ms(10);
        levels.push(m.soc.periph.led_take()[usize::from(PLAY)]);
    }
    let max = levels.iter().copied().fold(0.0f32, f32::max);
    let min = levels.iter().copied().fold(f32::MAX, f32::min);
    let lit = levels.iter().filter(|&&v| v > max / 2.0).count();
    println!("PLAY's LED over 4 s: min {min:e}, max {max:e}, {lit} of 400 windows lit");
    assert!(min > 0.0, "dim between the beats, not dark");
    assert!(max > 5.0 * min, "lit on a beat: {max} against dim {min}");
    assert!(
        (40..=200).contains(&lit),
        "lit for about a quarter of the time: {lit} windows of 400"
    );
}

fn kit_digi_song(every_row: bool) -> Song {
    use sidou_tests::song::SngSpec;
    let kit = |d: u32| unsafe {
        std::mem::transmute::<gt2_kit_drum, [u8; 25]>(gt2_kit_instr[d as usize])
    };
    let mut s = SngSpec::new();
    s.instr = (0..GT2_KIT_DRUMS).map(kit).collect();
    let mut dg = kit(GT2_KIT_KICK);
    dg[GT2_INS_NAME as usize..].fill(0);
    dg[GT2_INS_NAME as usize..][..7].copy_from_slice(b"DG KICK");
    s.instr.push(dg);
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
    let r = [GT2_REST as u8, 0, 0, 0];
    let note = |i: u8| [GT2_FIRSTNOTE as u8 + C3, i, 0, 0];
    let kit_instrument = |drum: u32| drum as u8 + 1;
    let mut v1 = vec![r; 16];
    let mut v2 = vec![r; 16];
    let mut v3 = vec![r; 16];
    for k in 0..16 {
        if every_row || k % 4 == 0 {
            v1[k] = note(DIGI_KICK_INSTRUMENT);
        }
        v3[k] = note(kit_instrument(if k == 14 {
            GT2_KIT_OPEN_HAT
        } else {
            GT2_KIT_CLOSED_HAT
        }));
    }
    v2[4] = note(kit_instrument(GT2_KIT_SNARE));
    v2[12] = note(kit_instrument(GT2_KIT_SNARE));
    v2[10] = note(kit_instrument(GT2_KIT_CLAP));
    s.patterns = vec![
        (v1, [0xff, 0, 0, 0]),
        (v2, [0xff, 0, 0, 0]),
        (v3, [0xff, 0, 0, 0]),
    ];
    let mut song = device_song(&s.encode(), None);
    assert_eq!(
        song.ext_set(
            sidou_tests::song::tag(b"DIGI"),
            &[1, 1, DIGI_KICK_INSTRUMENT, SONG_DIGI_KICK as u8]
        ),
        0
    );
    song
}

#[test]
fn the_drum_kit_and_a_digi_drum_play_bit_exact_with_the_host() {
    let Some((_, fw)) = build_and_firmware("the drum kit test") else {
        return;
    };
    check_song("kit and digi", &fw, Some(&kit_digi_song(false)));
}

#[test]
fn a_playing_digi_drum_costs_the_interrupt_within_digi_per_sample() {
    let Some((_, fw)) = build_and_firmware("the digi cost test") else {
        return;
    };
    let range = fw.function_range("song_ctl_block_end");
    let mut window = None;
    let song = kit_digi_song(true);
    play_after(
        &fw,
        Some(&song),
        3000,
        |_| {},
        |m, elapsed| {
            if elapsed == 1000 + PRESS_MS {
                m.watch_alnk_pcs(range.clone());
                m.alnk_pcs_inside = 0;
                window = Some((m.alnk_blocks.len(), 0));
            } else if elapsed == 2000 + PRESS_MS {
                let (from, _) = window.unwrap();
                window = Some((from, m.alnk_blocks.len()));
                let n = (m.alnk_blocks.len() - from) as f64 * HALF_FRAMES as f64;
                let digi = m.alnk_pcs_inside as f64 / n;
                println!(
                    "{}",
                    serde_json::json!({ "digi_instructions_per_sample": digi })
                );
                assert!(digi > 3.0, "no digi played: {digi:.2} a sample");
                assert_within_budget("the digi drums a sample", digi, DIGI_PER_SAMPLE);
            }
        },
    );
    assert!(
        matches!(window, Some((_, to)) if to > 0),
        "the window never closed"
    );
}

fn draft_of(m: &mut Machine, fw: &Firmware) -> (u8, u8, u8) {
    let d = transport(fw) + ilp32::song_ctl::draft as u32;
    (
        m.soc.read8(d + ilp32::gt2_draft::channel as u32),
        m.soc.read8(d + ilp32::gt2_draft::first as u32),
        m.soc.read8(d + ilp32::gt2_draft::len as u32),
    )
}

fn freqs_for(m: &mut Machine, fw: &Firmware, ms: u64) -> Vec<u32> {
    (0..ms / 5)
        .map(|_| {
            m.run_ms(5);
            chip_a_voice(m, fw, 0).0
        })
        .collect()
}

#[test]
fn the_arp_helpers_draft_plays_on_chip_a_before_save_and_home_takes_it_off_unwritten() {
    let Some((_, fw)) = build_and_firmware("the ARP helper's audition") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_song_pages(&mut m, PAGES_TO_TABLE, 300);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("song_pages_table_view"));
    let region = fw.symbol("__song_region");
    let before: Vec<u8> = (0..SONG_REGION_BYTES)
        .map(|i| m.soc.read8(region + i))
        .collect();
    press(&mut m, ARP);
    m.run_ms(200);
    assert_eq!(
        draft_of(&mut m, &fw),
        (0, 1, 2),
        "the draft on voice 1 from row 01"
    );
    let (two_steps, to_step_2, an_octave, looping_at_step_1) = (1, 1, 1, 1);
    m.turn(ALGORITHM, two_steps);
    m.run_ms(100);
    m.turn(SELECT, to_step_2);
    m.run_ms(100);
    m.turn(K1, an_octave);
    m.run_ms(100);
    m.turn(K4, looping_at_step_1);
    m.run_ms(200);
    assert_eq!(draft_of(&mut m, &fw), (0, 1, 3), "each change heard");
    m.key(KEY0 + 12, true);
    m.run_ms(200);
    let f = freqs_for(&mut m, &fw, 400);
    let mut pitches: Vec<u32> = f.clone();
    pitches.sort_unstable();
    pitches.dedup();
    println!("the draft's pitches {pitches:?}");
    assert_eq!(pitches.len(), 2, "two steps: {f:?}");
    let ratio = f64::from(pitches[1]) / f64::from(pitches[0]);
    assert!(
        (ratio - 2.0).abs() < 0.02,
        "an octave over the lowest: {ratio}"
    );
    let a_semitone = 1;
    m.turn(K2, a_semitone);
    m.run_ms(100);
    let g = freqs_for(&mut m, &fw, 400);
    let top = *g.iter().max().unwrap();
    let up = f64::from(top) / f64::from(pitches[1]);
    assert!(
        (up - 2f64.powf(1.0 / 12.0)).abs() < 0.01,
        "a semitone up at once: {g:?}"
    );
    m.key(KEY0 + 12, false);
    m.run_ms(200);
    press(&mut m, HOME);
    m.run_ms(200);
    assert_eq!(
        draft_of(&mut m, &fw).0,
        GT2_DRAFT_OFF as u8,
        "HOME takes it off"
    );
    let after: Vec<u8> = (0..SONG_REGION_BYTES)
        .map(|i| m.soc.read8(region + i))
        .collect();
    assert!(before == after, "nothing written");
}

#[test]
fn a_held_step_sounds_its_cutoff_lock_on_chip_a_only_while_held() {
    let Some((_, fw)) = build_and_firmware("the held step's audition") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    press(&mut m, SEQ);
    m.run_ms(500);
    let t = transport(&fw);
    let pin = t + ilp32::song_ctl::lock_pin as u32;
    let chip_fc = |m: &mut Machine| {
        let chip = rendered_sid(m, &fw, 0);
        m.soc.read32(chip + ilp32::sid::fc as u32)
    };
    holding(&mut m, KEY0, |m| {
        m.turn(K1, CUTOFF_DOWN_TO_64);
        m.run_ms(300);
        assert_eq!(chip_fc(m), u32::from(CUTOFF_64), "the turn heard at once");
        assert_eq!(m.soc.read8(pin) & 1, 1, "pinned while held");
    });
    m.run_ms(300);
    assert_eq!(m.soc.read8(pin), 0, "let go");
    const AUDITION_ENDS_MS: u64 = 2500;
    m.run_ms(AUDITION_ENDS_MS);
    m.key(KEY0, true);
    m.run_ms(200);
    assert_eq!(
        chip_fc(&mut m),
        u32::from(CUTOFF_64),
        "held again: its lock"
    );
    m.key(KEY0, false);
    m.run_ms(300);
    assert_eq!(m.soc.read8(pin), 0);
    assert_ne!(
        chip_fc(&mut m),
        u32::from(CUTOFF_64),
        "the song's own cutoff after it"
    );
}

#[test]
fn the_arp_helper_on_an_empty_row_of_the_built_in_loop_does_not_crash() {
    let Some((_, fw)) = build_and_firmware("the ARP helper on a blank row") else {
        return;
    };
    let song = transport(&fw) + ilp32::song_ctl::song as u32;
    let wave = |m: &mut Machine, row: u32| {
        let base = m.soc.read32(song);
        (
            m.soc.read8(base + ilp32::gt2_song::ltable as u32 + row),
            m.soc.read8(base + ilp32::gt2_song::rtable as u32 + row),
        )
    };
    let mut probe = Machine::boot(&fw.image);
    run_past_boot(&mut probe, &fw);
    let used = (0..GT2_TABLE_ROWS)
        .rev()
        .find(|&r| wave(&mut probe, r) != (0, 0))
        .map_or(0, |r| r + 1);
    println!(
        "the built-in loop at {:#x}: wave rows 00..{used:02X} used",
        probe.soc.read32(song)
    );
    for row in [used, 0x80, 0xf5, 0xfe] {
        let mut m = Machine::boot(&fw.image);
        run_past_boot(&mut m, &fw);
        open_song_pages(&mut m, PAGES_TO_TABLE, 300);
        assert_eq!(top_view(&mut m, &fw), fw.symbol("song_pages_table_view"));
        const TABLE_PAGE: u32 = 8;
        for _ in 0..row / TABLE_PAGE {
            m.turn(SELECT, TABLE_PAGE as i32);
            m.run_ms(100);
        }
        m.turn(SELECT, (row % TABLE_PAGE) as i32);
        m.run_ms(200);
        assert_eq!(wave(&mut m, row), (0, 0), "row {row:02X} blank");
        press(&mut m, ARP);
        m.run_ms(1000);
        let d = draft_of(&mut m, &fw);
        println!("row {row:02X}: draft {d:?}");
        if row + 6 <= GT2_TABLE_ROWS {
            assert_eq!(
                d,
                (0, row as u8 + 1, 6),
                "row {row:02X}: the draft from its row"
            );
        }
        m.key(KEY0 + 12, true);
        m.run_ms(500);
        m.key(KEY0 + 12, false);
        m.run_ms(500);
        assert!(m.resets.is_empty(), "row {row:02X}: resets {:?}", m.resets);
        assert_eq!(top_view(&mut m, &fw), fw.symbol("song_pages_table_view"));
    }
}
