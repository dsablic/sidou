// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The fixtures for the site's wasm core (`tools/wasm-core`, `web/core.js`): what this host build
//! of the same C makes of SIDOU's songs and GoatTracker's examples, so `web/core.test.mjs` can
//! check the wasm build gives the same bytes and samples. For each file: the read's result, the
//! `.sng` written back with and without SIDOU's trailer, the plain file's check and whether
//! GoatTracker 2.77 (gt2-ref) loads and saves it unchanged, every instrument's GTI5 export, and
//! the device's player through song_ctl for two seconds of the song and an instrument at a note
//! (each output as its length and a SHA-256 prefix). Also cut files (refusals) and the two readings of an ambiguous file
//! (`the_trailer_records_the_channel_count` in tests/song.rs), whose plain export warns. The
//! test checks the committed fixture is what this build makes now;
//! `SIDOU_WEB_CORE_FIXTURE_UPDATE=1` rewrites it.
//!
//!   SIDOU_WEB_CORE_FIXTURE_UPDATE=1 cargo nextest run --release -p sidou-tests -E 'test(web_core)'

use std::fmt::Write as _;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use sidou_sys::*;
use sidou_tests::song::*;
use sidou_tests::transport::{Transport, BLOCK, SAMPLE_RATE};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// A file's or a render's fingerprint: SHA-256's first 8 bytes, in hex. (Not the CRC: a file
/// with SIDOU's trailer ends in a CRC of what it covers, and two such files that differ only in
/// what the trailer records have the same CRC-32 as a whole.)
fn sum(b: &[u8]) -> String {
    format!("\"{}\"", hex(&Sha256::digest(b)[..8]))
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// web/core.js's block count for `seconds` at the device's rate.
fn blocks(seconds: f64) -> usize {
    (seconds * SAMPLE_RATE as f64 / BLOCK as f64).ceil() as usize
}

/// platform/web/core_api.c's chip: CHPA's model, else OPTS's.
fn model(song: &Song) -> sid_model {
    let mut c = song_chip_setup::default();
    if unsafe { song_ext_get_chip(song.ptr(), TAG_CHPA, &mut c) } != 0 {
        return if c.model != 0 {
            SID_MODEL_8580
        } else {
            SID_MODEL_6581
        };
    }
    let mut o = gt2_options::default();
    unsafe { song_ext_get_options(song.ptr(), &mut o) };
    if o.sid_model != 0 {
        SID_MODEL_8580
    } else {
        SID_MODEL_6581
    }
}

fn transport(song: &mut Song) -> Transport {
    let m = model(song);
    let mut t = Transport::new(song);
    unsafe { sid_init(&mut *t.chip, m, SID_CLOCK_PAL, SAMPLE_RATE) };
    t
}

fn render(t: &mut Transport, n: usize, out: &mut Vec<u8>) {
    let mut b = [0i16; BLOCK];
    for _ in 0..n {
        t.pass();
        t.block(&mut b);
        out.extend(b.iter().flat_map(|s| s.to_le_bytes()));
    }
}

/// sc_play and two seconds of sc_render.
fn song_pcm(song: &mut Song) -> Option<Vec<u8>> {
    let mut t = transport(song);
    if !t.play(0, 0) {
        return None;
    }
    let mut out = Vec::new();
    render(&mut t, blocks(2.0), &mut out);
    Some(out)
}

/// renderInstrument(1, { note 48, hold 0.5, seconds 1.5 }): sc_audition, sc_jam and renders.
fn instrument_pcm(song: &mut Song) -> Option<Vec<u8>> {
    let mut t = transport(song);
    if !t.audition() {
        return None;
    }
    let mut b = [0i16; BLOCK];
    for _ in 0..1000 {
        if t.jam_ready() {
            break;
        }
        t.pass();
        t.block(&mut b);
    }
    if !t.jam_ready() || !t.jam(0, 48, 1) {
        return None;
    }
    t.pass();
    let mut out = Vec::new();
    render(&mut t, blocks(0.5), &mut out);
    t.jam(0, 0xff, 0);
    t.pass();
    render(&mut t, blocks(1.0), &mut out);
    Some(out)
}

fn gti5(song: &Song, i: u32) -> Result<Vec<u8>, u32> {
    let mut ex = gt2_ins_export::default();
    let e = unsafe { gt2_ins_export_begin(&mut ex, song.ptr(), i) };
    if e != GT2_OK {
        return Err(e as u32);
    }
    let mut out = vec![0u8; ex.length as usize];
    let n = unsafe {
        gt2_ins_export_read(
            (&mut ex as *mut gt2_ins_export).cast(),
            0,
            out.as_mut_ptr(),
            ex.length,
        )
    };
    assert_eq!(n, ex.length);
    Ok(out)
}

fn entry(name: &str, file: &[u8], embed: bool) -> String {
    let mut o = String::new();
    let _ = write!(o, "{{\"name\": {}", json_str(name));
    if embed {
        let _ = write!(o, ", \"hex\": \"{}\"", hex(file));
    }
    let mut s = Song::device();
    let r = s.read(file, 4);
    let (result, info) = match &r {
        Ok(i) => (0, i),
        Err(i) => (-1, i),
    };
    let _ = write!(
        o,
        ", \"read\": {{\"ok\": {}, \"message\": {}, \"version\": {}, \"channels\": {}, \"trailer\": {}}}",
        result == 0,
        json_str(&info.msg),
        info.version,
        info.channels,
        info.trailer
    );
    if r.is_err() {
        o.push('}');
        return o;
    }
    let sng = s.write(true);
    let plain = s.write(false);
    let (check, warning) = s.check_plain(&plain);
    let goattracker = gt2_ref::canonical(&plain).is_ok_and(|c| c == plain);
    let _ = write!(
        o,
        ",\n   \"sng\": [{}, {}], \"plain\": [{}, {}], \"check\": [{}, {}], \"goattracker\": {}",
        sng.len(),
        sum(&sng),
        plain.len(),
        sum(&plain),
        check,
        json_str(&warning),
        goattracker
    );
    let n = unsafe { gt2_saved_instruments(s.ptr()) };
    let ins: Vec<String> = (1..=n)
        .map(|i| match gti5(&s, i) {
            Ok(b) => format!("[{}, {}]", b.len(), sum(&b)),
            Err(e) => format!("[-{e}]"),
        })
        .collect();
    let _ = write!(o, ",\n   \"gti5\": [{}]", ins.join(", "));
    let pcm =
        |p: Option<Vec<u8>>| p.map_or("null".into(), |b| format!("[{}, {}]", b.len() / 2, sum(&b)));
    let song_audio = pcm(song_pcm(&mut s));
    let ins_audio = if n >= 1 {
        pcm(instrument_pcm(&mut s))
    } else {
        "null".into()
    };
    let _ = write!(
        o,
        ",\n   \"song_pcm\": {song_audio}, \"instrument_pcm\": {ins_audio}}}"
    );
    o
}

/// The mono and the stereo song of an ambiguous file, each saved with its trailer
/// (tests/song.rs `the_trailer_records_the_channel_count`'s first generator state).
fn ambiguous() -> (Vec<u8>, Vec<u8>) {
    let lookalike = [0xff, 0x00, 0x01, 0xff, 0x00, 0x01, 0xff, 0x00];
    let mut rng = Rng(0xdf75_5aa4_46c7_9a0d);
    let mut spec = random_canonical(&mut rng, false);
    let mut i = [0u8; 25];
    i[..8].copy_from_slice(&lookalike);
    for b in i.iter_mut().skip(8) {
        *b = if rng.chance(50) { 0 } else { rng.byte() };
    }
    i[0] |= 1;
    spec.orders.truncate(1);
    spec.instr = vec![i];
    for p in spec.patterns.iter_mut() {
        for r in p.0.iter_mut() {
            r[1] = r[1].min(1);
        }
    }
    let file = spec.encode();
    let mut s = Song::device();
    let info = s.read(&file, 4).unwrap();
    assert_eq!(info.channels, 6);
    let mut m = Song::device();
    set_mono_from_spec(&mut m, &spec);
    assert_eq!(m.write(false), file);
    (m.write(true), s.write(true))
}

#[test]
fn web_core_fixture() {
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(root().join("assets/songs"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sng"))
        .map(|p| {
            (
                format!("assets/songs/{}", p.file_name().unwrap().to_string_lossy()),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect();
    files.sort();
    let mut examples = gt2_ref::examples();
    examples.sort();
    for (name, b) in examples {
        files.push((format!("third_party/goattracker2/examples/{name}"), b));
    }
    let mut entries: Vec<String> = files.iter().map(|(n, b)| entry(n, b, false)).collect();
    // Refusals: cut files, and a file that is not a song.
    let comet = std::fs::read(root().join("assets/songs/chrome-comet.sng")).unwrap();
    for cut in [3usize, 300, 2000] {
        entries.push(entry(
            &format!("chrome-comet cut to {cut}"),
            &comet[..cut],
            true,
        ));
    }
    entries.push(entry(
        "not a song",
        b"PSID not a song at all, just text",
        true,
    ));
    let (mono, stereo) = ambiguous();
    entries.push(entry("ambiguous, mono", &mono, true));
    entries.push(entry("ambiguous, stereo", &stereo, true));
    let json = format!("{{\"songs\": [\n  {}\n]}}\n", entries.join(",\n  "));
    let path = root().join("web/core-fixtures.json");
    if std::env::var_os("SIDOU_WEB_CORE_FIXTURE_UPDATE").is_some() {
        std::fs::write(&path, &json).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == json,
        "web/core-fixtures.json is not what this build makes: rerun with SIDOU_WEB_CORE_FIXTURE_UPDATE=1"
    );
}
