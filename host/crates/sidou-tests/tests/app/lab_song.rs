// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! LAB's song bridge (`app/lab_song.h`, docs/plans/2026-10-30-1.5-design.md section 2): a LAB
//! sound as a GoatTracker 2 instrument and its rows, played by SIDOU's player as LIVE plays the
//! sound; and the instrument back as a sound.

use sidou_sys::*;
use sidou_tests::player::{default_options, Player};
use sidou_tests::song::{SngSpec, Song};

use crate::lab_sound::Sound;
use crate::Rig;

const BYTES: usize = LAB_SOUND_BYTES as usize;

fn put16(s: &mut Sound, at: u32, v: u32) {
    s[at as usize] = v as u8;
    s[at as usize + 1] = (v >> 8) as u8;
}

/// A plain pulse sound: AD $29, SR $A9, a width of $812, no filter, no modulation.
pub(crate) fn plain() -> Sound {
    let mut s = [0u8; BYTES];
    s[LAB_SOUND_CONTROL as usize] = 0x40;
    s[LAB_SOUND_AD as usize] = 0x29;
    s[LAB_SOUND_SR as usize] = 0xa9;
    put16(&mut s, LAB_SOUND_PULSE, 0x812);
    put16(&mut s, LAB_SOUND_CUTOFF, 1000);
    s[LAB_SOUND_FILTER_MODE as usize] = 0x10;
    s[(LAB_SOUND_ARP + LIVE_ARP_SPAN) as usize] = 1;
    s[(LAB_SOUND_ARP + LIVE_ARP_RATE) as usize] = 2;
    assert!(unsafe { lab_sound_valid(s.as_ptr()) } != 0);
    s
}

pub(crate) fn encode(s: &Sound, voice: u32, first: [u8; 4]) -> lab_song_instr {
    let mut o = lab_song_instr::default();
    unsafe {
        lab_song_encode(
            s.as_ptr(),
            c"LAB 03".as_ptr(),
            voice,
            0,
            first.as_ptr(),
            &mut o,
        )
    };
    o
}

pub(crate) fn rows(o: &lab_song_instr, t: usize) -> Vec<(u8, u8)> {
    (0..o.rows[t] as usize)
        .map(|i| (o.left[t][i], o.right[t][i]))
        .collect()
}

fn words(notes: u32, dropped: bool) -> String {
    let mut out = [0 as std::ffi::c_char; 40];
    unsafe { lab_song_words(notes, i32::from(dropped), out.as_mut_ptr(), 31) };
    unsafe { std::ffi::CStr::from_ptr(out.as_ptr()) }
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn each_exact_field_has_its_encoding() {
    let s = plain();
    let o = encode(&s, 0, [1, 1, 1, 1]);
    // AD, SR, the wave and pulse programs, no filter or vibrato, gate timer 2 with the hard
    // restart, the waveform from the first frame, the name.
    assert_eq!(&o.instr[..9], &[0x29, 0xa9, 1, 1, 0, 0, 0, 2, 0x41]);
    assert_eq!(&o.instr[9..16], b"LAB 03\0");
    assert_eq!(rows(&o, 0), [(0x41, 0), (0xff, 0)]);
    assert_eq!(rows(&o, 1), [(0x88, 0x12), (0xff, 0)]);
    assert_eq!(o.rows[2] + o.rows[3], 0);
    assert_eq!(o.notes, 0, "nothing approximated");
    // Ring and sync ride on the waveform row; a sound without the pulse has no pulse program.
    for (control, wave) in [(0x14u8, 0x15u8), (0x22, 0x23)] {
        let mut s = plain();
        s[LAB_SOUND_CONTROL as usize] = control;
        let o = encode(&s, 0, [1, 1, 1, 1]);
        assert_eq!(rows(&o, 0), [(wave, 0), (0xff, 0)]);
        assert_eq!((o.instr[3], o.rows[1], o.instr[8]), (0, 0, wave));
    }
    // An attack of 0 is raised to 1, and said.
    let mut s = plain();
    s[LAB_SOUND_AD as usize] = 0x09;
    let o = encode(&s, 0, [1, 1, 1, 1]);
    assert_eq!(o.instr[0], 0x19);
    assert_eq!(u32::from(o.notes), LAB_SONG_ATTACK);
    // The programs start where they are placed, and the pointers say so.
    let o = encode(&plain(), 0, [7, 20, 1, 1]);
    assert_eq!((o.instr[2], o.instr[3]), (7, 20));
}

#[test]
fn the_filter_goes_to_the_songs_voice_in_eight_bits_with_reso_link_baked() {
    let mut s = plain();
    s[LAB_SOUND_FILTER_VOICES as usize] = 7;
    s[LAB_SOUND_RESONANCE as usize] = 9;
    let o = encode(&s, 1, [1, 1, 1, 1]);
    assert_eq!(rows(&o, 2), [(0x90, 0x92), (0, 125), (0xff, 0)]);
    assert_eq!(o.instr[4], 1);
    assert_eq!(u32::from(o.notes), LAB_SONG_FILTER);
    // RESO LINK +8 at the cutoff: the resonance LIVE writes there, clamped at 15.
    s[LAB_SOUND_RESONANCE as usize] = 2;
    s[LAB_SOUND_RESO_LINK as usize] = 2;
    let steps = unsafe { &live_cutoff_steps[0] };
    let place = steps.iter().rposition(|&c| c <= 1000).unwrap() as i32;
    let want = (2 + 2 * (32 - place) / 8).clamp(0, 15) as u8;
    let o = encode(&s, 2, [1, 1, 1, 1]);
    assert_eq!(o.right[2][0], want << 4 | 4);
}

#[test]
fn the_pwm_is_a_pulse_loop_and_the_vibrato_a_calculated_speed() {
    let mut s = plain();
    put16(&mut s, LAB_SOUND_PULSE, 2048);
    put16(&mut s, LAB_SOUND_PWM_DEPTH, 512);
    put16(&mut s, LAB_SOUND_PWM_RATE, 655); // about 1 Hz: 50 frames a cycle
    let o = encode(&s, 0, [1, 5, 1, 9]);
    // set the centre; climb to the top; then down and up the whole band, round and round: 40
    // frames that run of 48 at tempo 6 (the pulse rests on each row's gate-off tick)
    assert_eq!(
        rows(&o, 1),
        [(0x88, 0), (10, 51), (20, 205), (20, 51), (0xff, 7)]
    );
    assert_eq!(u32::from(o.notes), LAB_SONG_PWM);
    // PWM LINK baked: +4 at a width 1024 from the square adds 512.
    put16(&mut s, LAB_SOUND_PWM_DEPTH, 0);
    put16(&mut s, LAB_SOUND_PULSE, 1024);
    s[LAB_SOUND_PWM_LINK as usize] = 4;
    let linked = encode(&s, 0, [1, 5, 1, 9]);
    assert_eq!(linked.rows[1], 5);
    assert_eq!(linked.right[1][2], 205);
    // The vibrato: 6 Hz, depth 64 (half a semitone), 20 ticks' delay.
    let mut s = plain();
    s[LAB_SOUND_VIBRATO_RATE as usize] = 60;
    s[LAB_SOUND_VIBRATO_DEPTH as usize] = 64;
    s[LAB_SOUND_VIBRATO_DELAY as usize] = 20;
    let o = encode(&s, 0, [1, 1, 1, 9]);
    assert_eq!(rows(&o, 3), [(0x81, 2)]);
    assert_eq!(
        (o.instr[5], o.instr[6]),
        (9, 11),
        "1 + 20 ticks / 2: 0 would be none"
    );
    assert_eq!(u32::from(o.notes), LAB_SONG_VIB);
}

#[test]
fn the_filter_lfo_is_a_filter_loop_and_the_arpeggio_the_helpers_chord() {
    let mut s = plain();
    s[LAB_SOUND_FILTER_VOICES as usize] = 7;
    s[LAB_SOUND_FILTER_LFO as usize] = 8;
    put16(&mut s, LAB_SOUND_PWM_RATE, 655);
    let o = encode(&s, 0, [1, 1, 3, 1]);
    let f = rows(&o, 2);
    assert_eq!(&f[..2], &[(0x90, 0x01), (0, 125)]);
    assert_eq!(f.last(), Some(&(0xff, 3 + 3)), "{f:?}");
    assert!(f.len() <= 8);
    assert_eq!(u32::from(o.notes), LAB_SONG_FILTER | LAB_SONG_LFO);
    // ARP on: the helper's default chord on the sound's waveform, at the arpeggio's rate.
    let mut s = plain();
    s[(LAB_SOUND_ARP + LIVE_ARP_ON) as usize] = 1;
    let o = encode(&s, 0, [4, 1, 1, 1]);
    let mut a = song_pages_arp::default();
    let mut used = 0;
    let (l, r) = (o.left[0], o.right[0]);
    assert_eq!(
        unsafe {
            song_pages_arp_decode(
                l.as_ptr(),
                r.as_ptr(),
                4,
                o.rows[0] as u32,
                &mut a,
                &mut used,
            )
        },
        1
    );
    assert_eq!((a.steps, a.speed, a.wave), (4, 3, 0x41));
    assert_eq!(&a.off[..4], &[0, 4, 7, 12]);
    assert_eq!(u32::from(o.notes), LAB_SONG_ARP);
}

#[test]
fn what_a_send_drops_or_approximates_is_named() {
    let mut s = plain();
    s[LAB_SOUND_MONO_SYNC as usize] = 1;
    s[LAB_SOUND_TUNE as usize] = 5;
    s[LAB_SOUND_VELOCITY as usize] = 1;
    s[LAB_SOUND_SLOTS as usize] = 1;
    s[LAB_SOUND_AD as usize] = 0;
    let o = encode(&s, 0, [1, 1, 1, 1]);
    let n = u32::from(o.notes);
    assert_eq!(
        n & LAB_SONG_DROPPED,
        LAB_SONG_SYNC | LAB_SONG_TUNE | LAB_SONG_PLAY | LAB_SONG_FX
    );
    assert_eq!(words(n, true), "SYNC TUNE PLAY FX");
    assert_eq!(words(n, false), "ATTACK");
    assert_eq!(
        words(LAB_SONG_PWM | LAB_SONG_FILTER | LAB_SONG_VIB, false),
        "PWM FILTER VIB"
    );
}

/// The sound as a one-instrument song: C-4 on voice 1.
pub(crate) fn song_of(o: &lab_song_instr) -> Vec<u8> {
    let mut spec = SngSpec::new();
    spec.instr = vec![o.instr];
    for t in 0..4 {
        spec.tables[t] = rows(o, t);
    }
    spec.patterns[0].0[0] = [0x60 + 48, 1, 0, 0];
    spec.encode()
}

/// The SID's registers after `frames` calls of SIDOU's player on the sent instrument.
fn played(o: &lab_song_instr, frames: usize) -> Vec<[u8; 25]> {
    let mut song = Song::host();
    song.read(&song_of(o), 0).map_err(|i| i.msg).unwrap();
    let mut p = Player::new(&song, 0, &default_options()).unwrap();
    let mut regs = [0u8; 25];
    (0..frames)
        .map(|_| {
            for (r, v) in p.call() {
                regs[r as usize] = v;
            }
            regs
        })
        .collect()
}

/// The exact fields as SIDOU's player plays the sent instrument and as LIVE plays the sound:
/// the waveform with its gate, AD, SR and the pulse width equal; the cutoff its top 8 bits, the
/// resonance and the mode equal, the routing the song's voice.
#[test]
fn the_sent_instrument_plays_the_lab_sounds_registers() {
    let mut s = plain();
    s[LAB_SOUND_CONTROL as usize] = 0x42; // with sync
    s[LAB_SOUND_FILTER_VOICES as usize] = 7;
    s[LAB_SOUND_RESONANCE as usize] = 9;
    let mut rig = Rig::new(SID_MODEL_6581);
    unsafe { live_sound_apply(&mut *rig.live, s.as_ptr()) };
    rig.note_on(60);
    let regs = played(&encode(&s, 0, [1, 1, 1, 1]), 12);
    let at = regs
        .iter()
        .position(|r| r[4] & 1 != 0)
        .expect("the gate opens");
    let r = regs[at + 3];
    assert_eq!(r[4], rig.control(0), "the waveform, sync and the gate");
    assert_eq!((r[5], r[6]), (rig.reg(5), rig.reg(6)), "AD and SR");
    assert_eq!(
        u16::from(r[2]) | u16::from(r[3] & 15) << 8,
        rig.voice_pw(0),
        "the pulse width"
    );
    assert_eq!(r[0x16], rig.reg(0x16), "the cutoff's top 8 bits");
    assert_eq!(r[0x17] >> 4, rig.reg(0x17) >> 4, "the resonance");
    assert_eq!(r[0x17] & 7, 1, "the song's voice into the filter");
    assert_eq!(r[0x18] & 0x70, rig.reg(0x18) & 0x70, "the mode");
}

/// The approximate fields played: the PWM's band and period, the vibrato's rate.
#[test]
fn the_pwm_loop_and_the_vibrato_play_within_their_tolerances() {
    let mut s = plain();
    put16(&mut s, LAB_SOUND_PULSE, 2048);
    put16(&mut s, LAB_SOUND_PWM_DEPTH, 512);
    put16(&mut s, LAB_SOUND_PWM_RATE, 655);
    s[LAB_SOUND_VIBRATO_RATE as usize] = 60;
    s[LAB_SOUND_VIBRATO_DEPTH as usize] = 64;
    let regs = played(&encode(&s, 0, [1, 1, 1, 1]), 400);
    let pw: Vec<i32> = regs[100..]
        .iter()
        .map(|r| i32::from(r[2]) | i32::from(r[3] & 15) << 8)
        .collect();
    let (lo, hi) = (*pw.iter().min().unwrap(), *pw.iter().max().unwrap());
    assert!(
        (hi - 2048 - 512).abs() <= 51 && (2048 - 512 - lo).abs() <= 51,
        "{lo}..{hi}"
    );
    // 50 frames a cycle (1 Hz), within two: the frames between two tops
    let tops: Vec<usize> = (1..pw.len() - 1)
        .filter(|&i| pw[i] == hi && pw[i - 1] < hi)
        .collect();
    assert!(tops.len() >= 3);
    assert!(
        tops.windows(2).all(|w| (w[1] - w[0]).abs_diff(50) <= 2),
        "{tops:?}"
    );
    // the vibrato: LIVE's 6.0 Hz is 8.3 frames a cycle; the nearest is 6 frames that run, 7.2
    // on average with the tick 0s it rests on (6.9 Hz)
    let f: Vec<i32> = regs[100..380] // before the pattern's end, where the song starts over
        .iter()
        .map(|r| i32::from(r[0]) | i32::from(r[1]) << 8)
        .collect();
    let fmax = *f.iter().max().unwrap();
    let peaks: Vec<usize> = (1..f.len())
        .filter(|&i| f[i] == fmax && f[i - 1] < fmax)
        .collect();
    assert!(peaks.len() >= 30, "{peaks:?}");
    assert!(
        peaks.windows(2).all(|w| (6..=8).contains(&(w[1] - w[0]))),
        "{peaks:?}"
    );
    let mean = (peaks[peaks.len() - 1] - peaks[0]) as f64 / (peaks.len() - 1) as f64;
    assert!((mean - 7.2).abs() < 0.3, "{mean}");
}

// ---- the exact round trip: the LBnn chunk ----

/// The sent instrument written into a song (as the batch writer would), its sound kept.
fn sent_song(s: &Sound, instr: usize, first: [u8; 4]) -> (Song, lab_song_instr) {
    let mut song = Song::device();
    let o = encode(s, 0, first);
    let raw = unsafe { &mut *song.ptr_mut() };
    raw.instr[instr - 1] = o.instr;
    for t in 0..4 {
        for i in 0..o.rows[t] as usize {
            raw.ltable[t][first[t] as usize - 1 + i] = o.left[t][i];
            raw.rtable[t][first[t] as usize - 1 + i] = o.right[t][i];
        }
    }
    unsafe { gt2_saved_recount(song.ptr_mut()) }; // written here directly, not as edits
    assert_eq!(
        unsafe { lab_song_keep(song.ptr_mut(), instr as u32, &o, s.as_ptr()) },
        0
    );
    (song, o)
}

fn kept(song: &Song, instr: u32) -> Option<Sound> {
    let (mut rows, mut s) = ([0u8; 4], [0u8; BYTES]);
    (unsafe { lab_song_kept(song.ptr(), instr, rows.as_mut_ptr(), s.as_mut_ptr()) } != 0)
        .then_some(s)
}

/// `LB0F` keeps the sound and the CRC of the instrument's bytes and its programs' rows: the
/// sound comes back while they are unedited, and not once a byte, a row or the name changes.
#[test]
fn the_chunk_keeps_the_sound_while_the_instrument_is_unedited() {
    let mut s = plain();
    s[LAB_SOUND_FILTER_VOICES as usize] = 7;
    s[LAB_SOUND_SLOTS as usize] = 2; // dropped by the send, kept by the chunk
    s[LAB_SOUND_TUNE as usize] = 7;
    let (mut song, o) = sent_song(&s, 15, [3, 1, 9, 1]);
    assert_eq!(unsafe { lab_song_tag(15) }, u32::from_le_bytes(*b"LB0F"));
    let chunk = song
        .ext_find(u32::from_le_bytes(*b"LB0F"))
        .expect("the chunk");
    assert_eq!(chunk.len(), 9 + BYTES);
    assert_eq!(chunk[0], 1);
    assert_eq!(&chunk[1..5], &o.rows);
    assert_eq!(&chunk[9..], &s);
    assert_eq!(kept(&song, 15), Some(s));
    assert_eq!(kept(&song, 14), None);
    let edits: [(&str, fn(&mut gt2_song)); 4] = [
        ("a byte", |r| r.instr[14][1] ^= 0x10),
        ("a row", |r| r.rtable[2][9] ^= 1),
        ("the name", |r| r.instr[14][20] = b'X'),
        ("the pulse's jump", |r| r.ltable[1][1] = 0x7f),
    ];
    for (what, edit) in edits {
        let mut copy = Song::device();
        copy.copy_from(&song);
        edit(unsafe { &mut *copy.ptr_mut() });
        assert_eq!(kept(&copy, 15), None, "{what}");
    }
    // a row past the programs is not theirs
    unsafe { (*song.ptr_mut()).ltable[0][10] = 0x11 };
    assert_eq!(kept(&song, 15), Some(s));
}

/// The chunk goes through a save and a load with the song; GoatTracker's own player reads the
/// file and plays the instrument (the trailer is after the GT2 part, which it skips).
#[test]
fn the_chunk_survives_a_save_and_goattracker_skips_it() {
    let s = plain();
    let (song, _) = sent_song(&s, 1, [1, 1, 1, 1]);
    let file = song.write(true);
    let mut back = Song::device();
    back.read(&file, 4).map_err(|i| i.msg).unwrap();
    assert_eq!(kept(&back, 1), Some(s));
    let plain_file = song.write(false);
    assert!(file.len() > plain_file.len() + 9 + BYTES);
    let t = gt2_ref::trace::trace(&file, 0, 4, &gt2_ref::Options::default());
    assert!(t.is_ok(), "GoatTracker reads the file");
}

/// At most 16 chunks: past them a stale one makes room, and with none stale no chunk is kept.
#[test]
fn at_most_sixteen_chunks_are_kept() {
    let s = plain();
    let (mut song, _) = sent_song(&s, 1, [1, 1, 1, 1]); // the rows every one shares
    for i in 1..=17u32 {
        let mut o = encode(&s, 0, [1, 1, 1, 1]);
        o.instr[0] = i as u8;
        unsafe { (*song.ptr_mut()).instr[i as usize - 1] = o.instr };
        let r = unsafe { lab_song_keep(song.ptr_mut(), i, &o, s.as_ptr()) };
        assert_eq!(r, if i <= 16 { 0 } else { -1 }, "instrument {i}");
    }
    unsafe { (*song.ptr_mut()).instr[4][0] = 0x99 }; // instrument 5 edited: stale
    let mut o = encode(&s, 0, [1, 1, 1, 1]);
    o.instr[0] = 17;
    assert_eq!(
        unsafe { lab_song_keep(song.ptr_mut(), 17, &o, s.as_ptr()) },
        0
    );
    assert!(song.ext_find(u32::from_le_bytes(*b"LB05")).is_none());
    assert!(song.ext_find(u32::from_le_bytes(*b"LB11")).is_some());
    assert!(song.raw().ext_used <= 16 * (8 + 9 + BYTES as u32));
}
