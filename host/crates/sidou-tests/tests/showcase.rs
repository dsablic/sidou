// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's showcase songs (src/showcase): the files in step with their sources; each read
//! and written by SIDOU's reader and writer within the device's caps; each a canonical
//! GoatTracker file that GoatTracker's own packed player plays as SIDOU's does; each
//! rendering one non-silent, unclipped pass of the expected length on both chip models.
//!
//! SIDOU_SONG_UPDATE=1 rewrites the files; SIDOU_SHOWCASE_WAV=<dir> writes each song's render
//! (and, with SIDOU_SHOWCASE_STEMS=1, one per voice) as WAV files.

use gt2_ref::trace::trace;
use gt2_ref::Options;
use sidou_sys::*;
use sidou_tests::player::{default_options, Player};
use sidou_tests::showcase::{self, Showcase};
use sidou_tests::song::Song;

#[test]
fn the_files_are_up_to_date() {
    let update = std::env::var("SIDOU_SONG_UPDATE").as_deref() == Ok("1");
    let stale = showcase::check_or_write(update);
    assert!(
        stale.is_empty(),
        "{stale:?} is stale; rerun with SIDOU_SONG_UPDATE=1"
    );
}

/// Read with the device's caps into its 64 KiB region, written back the same, with the
/// trailer naming the song's chip; without the trailer, a plain file that reads back as it
/// is.
#[test]
fn each_song_reads_and_writes_on_the_device() {
    for s in showcase::all() {
        let file = s.sng();
        let mut song = Song::device();
        let info = song
            .read(&file, 4)
            .unwrap_or_else(|e| panic!("{}: {}", s.name, e.msg));
        assert_eq!(info.channels, 3, "{}", s.name);
        assert!(info.trailer != 0, "{}: the trailer", s.name);
        assert_eq!(song.write(true), file, "{}: the round trip", s.name);
        let mut chip = song_chip_setup::default();
        let found =
            unsafe { song_ext_get_chip(song.ptr(), sidou_tests::song::TAG_CHPA, &mut chip) };
        assert_eq!(found, 1, "{}: the chip's setup", s.name);
        assert_eq!(chip.model, (s.model == SID_MODEL_8580) as u8, "{}", s.name);
        assert_eq!(chip.envelope, 1, "{}", s.name);
        let plain = song.write(false);
        let (r, msg) = song.check_plain(&plain);
        assert_eq!(r, 0, "{}: {msg}", s.name);
        println!(
            "{}: {} bytes ({} without the trailer), {} needed of the region",
            s.name,
            file.len(),
            plain.len(),
            info.need_bytes
        );
    }
}

/// GoatTracker's load and save leave the plain file as it is.
#[test]
fn each_song_is_canonical_gts5() {
    for s in showcase::all() {
        let plain = s.plain();
        assert_eq!(&plain[..4], b"GTS5");
        let canon = gt2_ref::canonical(&plain).expect("GoatTracker loads it");
        assert!(
            canon == plain,
            "{}: GoatTracker saves it differently",
            s.name
        );
    }
}

/// Every write of every frame of a pass, and a little of the loop after it, equals
/// GoatTracker 2.77's packed player's, run in gt2-ref's 6502 (gt2reloc's defaults).
#[test]
fn each_song_plays_as_goattracker_plays_it() {
    for s in showcase::all() {
        let file = s.sng();
        let frames = s.frames() as usize + 500;
        let t = trace(&file, 0, frames, &Options::default())
            .unwrap_or_else(|e| panic!("{}: trace: {e}", s.name));
        let song = s.song();
        let mut p = Player::new(&song, 0, &default_options()).expect("the player takes it");
        for (f, frame) in t.frames.iter().enumerate() {
            for want in &frame.calls {
                let got = p.call();
                assert_eq!(&got, want, "{}: frame {f}", s.name);
                assert_eq!(p.raw().freq_unknown, 0, "{}: frame {f}", s.name);
            }
        }
    }
}

fn levels(pcm: &[i16]) -> (f64, i32) {
    let sum: f64 = pcm.iter().map(|&x| f64::from(x) * f64::from(x)).sum();
    let rms = (sum / pcm.len() as f64).sqrt() / 32768.0;
    let peak = pcm.iter().map(|&x| i32::from(x).abs()).max().unwrap_or(0);
    (rms, peak)
}

/// One pass: 60-90 s, no second of it silent, and no sample at full scale, on both models.
#[test]
fn each_song_renders_unclipped_on_both_chips() {
    for s in showcase::all() {
        let song = s.song();
        let frames = s.frames();
        let seconds = f64::from(frames) * 19_656.0 / 985_248.0;
        assert!(
            (60.0..=90.0).contains(&seconds),
            "{}: {seconds:.1} s a pass",
            s.name
        );
        for model in [SID_MODEL_6581, SID_MODEL_8580] {
            let pcm = showcase::render(&song, model, frames);
            let want = ((frames + showcase::TAIL_FRAMES) as f64 * 19_656.0 * 44_100.0 / 985_248.0)
                as usize;
            assert!(
                pcm.len().abs_diff(want) <= 1,
                "{}: {} samples",
                s.name,
                pcm.len()
            );
            let (rms, peak) = levels(&pcm);
            println!(
                "{} on the {}: {seconds:.1} s, RMS {:.1} dBFS, peak {:.1} dBFS",
                s.name,
                if model == SID_MODEL_8580 { 8580 } else { 6581 },
                20.0 * rms.log10(),
                20.0 * (f64::from(peak) / 32768.0).log10()
            );
            assert!(peak < 32_767, "{}: clipped", s.name);
            let pass = &pcm[..(seconds * 44_100.0) as usize];
            for (k, second) in pass.as_chunks::<44_100>().0.iter().enumerate() {
                let (rms, _) = levels(second);
                assert!(rms > 0.003, "{}: second {k} silent ({rms})", s.name);
            }
        }
    }
}

fn wanted(s: &Showcase) -> bool {
    let only = std::env::var("SIDOU_SHOWCASE_ONLY").unwrap_or_default();
    only.is_empty() || only == s.name
}

/// SIDOU_SHOWCASE_WAV=<dir>: each song's render on its own chip, for review.
#[test]
fn render_to_dir() {
    let Ok(dir) = std::env::var("SIDOU_SHOWCASE_WAV") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for s in showcase::all().iter().filter(|s| wanted(s)) {
        let song = s.song();
        let frames = s.frames();
        let pcm = showcase::render(&song, s.model, frames);
        let out = format!("{dir}/{}.wav", s.name);
        std::fs::write(out, showcase::wav(&pcm, showcase::RATE)).unwrap();
        if std::env::var("SIDOU_SHOWCASE_STEMS").is_ok() {
            for v in 0..3 {
                let pcm = showcase::render_voices(&song, s.model, frames, 1 << v);
                let out = format!("{dir}/{}-v{}.wav", s.name, v + 1);
                std::fs::write(out, showcase::wav(&pcm, showcase::RATE)).unwrap();
            }
        }
    }
}
