// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use psid_ref::compare::{check_timing, compare, Outcome, Rule};
use psid_ref::{header, sidou};
use sha2::{Digest, Sha256};
use sidou_tests::psid_fixtures::*;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn sha256(p: &Path) -> String {
    let b = std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    Sha256::digest(&b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

const BEGIN: &str = "/* psid-ref: begin */";
const END: &str = "/* psid-ref: end */";

const ADDED: [&str; 4] = [
    "/* an I/O-page read: cycle = cycles since the start of this call, as a write's */\n\
     typedef void (*psid_read_fn)(void *user, uint32_t cycle, uint32_t addr, uint32_t val);\n",
    "    psid_read_fn rd;                      /* the I/O page's reads, logged (psid_set_read_log) */\n\
     \x20   void *rd_user;\n",
    "#define bus_read bus_read_unlogged\n",
    "#undef bus_read\n\
     /* the original bus_read, each read of the I/O page (while $01 maps it in) reported */\n\
     static uint8_t bus_read(psid_t *p, uint16_t ad)\n\
     {\n\
     \x20   uint8_t v = bus_read_unlogged(p, ad);\n\
     \x20   if (ad >= 0xD000 && ad < 0xE000 && io_on(p) && p->rd)\n\
     \x20       p->rd(p->rd_user, p->cycle, ad, v);\n\
     \x20   return v;\n\
     }\n\
     \n\
     /* after psid_load, which clears it */\n\
     void psid_set_read_log(psid_t *p, psid_read_fn rd, void *user)\n\
     {\n\
     \x20   p->rd = rd;\n\
     \x20   p->rd_user = user;\n\
     }\n\
     \n\
     uint8_t psid_peek(const psid_t *p, uint16_t ad) { return p->mem[ad]; }\n",
];

#[test]
fn the_vendored_files_are_unmodified_and_the_copy_only_adds_its_marked_lines() {
    let tp = root().join("third_party");
    for (f, want) in [
        (
            "fm1-sid-player/psid.c",
            "ca816da41a354a850da432449c2c988f11d4e33a3a83f9aa4b09d5b525850f0d",
        ),
        (
            "fm1-sid-player/psid.h",
            "531d09cb20e1435e29b4ba98e640a0ac601ce5996c3597bada257f6d0c0ad898",
        ),
        (
            "fm1-sid-player/psidlog.c",
            "2c944a23293f1f2542d136d7f81eb38fd9d65c988be387c2b05be8e24204350d",
        ),
        (
            "chips/m6502.h",
            "b3fc32a01f1f7cdc62f919ccba06cc20c2a333e7ca30c38189e31aa65868d46e",
        ),
    ] {
        assert_eq!(sha256(&tp.join(f)), want, "{f}");
    }
    let original = std::fs::read_to_string(tp.join("fm1-sid-player/psid.c")).unwrap();
    let copy =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("c/psid_oracle.c"))
            .unwrap();
    let (mut kept, mut blocks) = (Vec::new(), Vec::new());
    let mut block: Option<String> = None;
    for line in copy.split('\n') {
        match (line, block.as_mut()) {
            (BEGIN, None) => block = Some(String::new()),
            (END, Some(_)) => blocks.push(block.take().unwrap()),
            (BEGIN | END, _) => panic!("unbalanced psid-ref markers"),
            (_, Some(b)) => {
                b.push_str(line);
                b.push('\n');
            }
            (_, None) => kept.push(line),
        }
    }
    assert!(block.is_none(), "an unclosed psid-ref block");
    assert_eq!(
        kept.join("\n"),
        original,
        "the copy changes the original's lines"
    );
    assert_eq!(blocks.len(), 1 + ADDED.len());
    let notice = &blocks[0];
    for must in [
        "SPDX-FileCopyrightText: 2026 Joakim Eriksson",
        "Copyright (c) 2026 Joakim Eriksson",
        "Redistribution and use in source and binary forms",
        "EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.",
    ] {
        assert!(notice.contains(must), "the notice lacks {must:?}");
    }
    assert!(notice.trim_end().ends_with("*/") && !notice[..notice.len() - 3].contains("*/"));
    for (got, want) in blocks[1..].iter().zip(ADDED) {
        assert_eq!(got, want);
    }
}

struct Job {
    name: String,
    file: Vec<u8>,
    song: u32,
}

fn check(job: &Job, seconds: u32) -> Result<Outcome, String> {
    let h = header(&job.file).map_err(|c| format!("refused ({c})"))?;
    let s = sidou::run(&job.file, job.song, seconds)?;
    let o = psid_ref::run(&job.file, job.song, seconds)?;
    check_timing(&h, &s, job.song, seconds).map_err(|e| format!("E8: {e}"))?;
    compare(&h, &s, &o)
}

fn run_all(jobs: &[Job], seconds: u32) -> Vec<Result<Outcome, String>> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<Result<Outcome, String>>>> =
        Mutex::new((0..jobs.len()).map(|_| None).collect());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= jobs.len() {
                    break;
                }
                let r = check(&jobs[i], seconds);
                out.lock().unwrap()[i] = Some(r);
            });
        }
    });
    out.into_inner()
        .unwrap()
        .into_iter()
        .map(|r| r.unwrap())
        .collect()
}

fn packed(sng: &[u8], sid: u32) -> Vec<u8> {
    let o = gt2_ref::Options {
        sid_address: sid,
        ..gt2_ref::Options::default()
    };
    let p = gt2_ref::pack(sng, &o).expect("pack");
    let mut psid = Psid {
        version: 2,
        load: p.load,
        init: p.init(),
        play: p.play(),
        songs: gt2_ref::subtunes(sng).max(1) as u16,
        start: 1,
        speed: 0,
        flags: CLOCK_PAL,
        sid2: 0,
        sid3: 0,
        data: p.code.clone(),
    };
    if sid != 0xD400 {
        psid = psid.sid2(((sid >> 4) & 0xFF) as u8);
    }
    psid.bytes()
}

fn songs() -> Vec<(String, Vec<u8>)> {
    let mut v = gt2_ref::examples();
    let dir = root().join("assets/songs");
    let mut own: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .expect("assets/songs")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sng"))
        .map(|p| {
            let n = p.file_name().unwrap().to_string_lossy().into_owned();
            (n, std::fs::read(&p).unwrap())
        })
        .collect();
    own.sort();
    v.extend(own);
    v
}

#[test]
fn goattracker_songs_play_exactly_as_the_oracle_plays_them() {
    let seconds = if cfg!(debug_assertions) { 10 } else { 60 };
    let mut jobs = Vec::new();
    for (name, sng) in songs() {
        for sid in [0xD400, 0xD420] {
            let file = packed(&sng, sid);
            let songs = header(&file).unwrap().songs as u32;
            for song in 1..=songs {
                jobs.push(Job {
                    name: format!("{name} at ${sid:04X}, song {song}"),
                    file: file.clone(),
                    song,
                });
            }
        }
    }
    let results = run_all(&jobs, seconds);
    let (mut calls, mut writes) = (0, 0);
    for (job, r) in jobs.iter().zip(&results) {
        let o = r.as_ref().unwrap_or_else(|e| panic!("{}: {e}", job.name));
        assert_eq!(o.first, None, "{}", job.name);
        assert!(o.writes > 0, "{}", job.name);
        calls += o.calls;
        writes += o.writes;
    }
    eprintln!(
        "{} songs and subtunes ({seconds} s each): {calls} calls and {writes} writes equal",
        jobs.len()
    );
}

type Expected = Option<(Rule, usize)>;

fn fixtures() -> Vec<(&'static str, Psid, u32, Expected)> {
    use Rule::*;
    let e1 = Some((E1Bank, 2));
    let plain = |end: &str| {
        let play = format!("{COUNT}cmp #$03\nbne done\n{end}done: rts\n");
        Psid::new(&[(INIT, "rts\n"), (PLAY, &play)])
    };
    let mut v: Vec<(&'static str, Psid, u32, Expected)> = vec![
        ("vbi", vbi(), 1, e1),
        ("vbi, NTSC", vbi().flags(CLOCK_NTSC), 1, e1),
        ("cia $4000", cia(0x4000), 1, e1),
        ("cia 999", cia(999), 1, e1),
        ("cia 100", cia(100), 1, e1),
        ("cia, the KERNAL's latch", cia_default(), 1, e1),
        ("cia, NTSC", cia_default().flags(CLOCK_NTSC), 1, e1),
        ("cia, changing latch", cia_changing(), 1, e1),
        (
            "irq through $0314, JMP $EA31",
            irq_kernal(false),
            1,
            Some((E11IrqStack, 1)),
        ),
        (
            "irq through $0314, RTI",
            irq_kernal(true),
            1,
            Some((E11IrqStack, 1)),
        ),
        ("irq through $FFFE", irq_hardware(), 1, None),
        (
            "init $A000, play $E000",
            banks(0xA000, 0xE000),
            1,
            Some((E1Bank, 0)),
        ),
        (
            "init $1000, play $D000",
            banks(0x1000, 0xD000),
            1,
            Some((E1Bank, 1)),
        ),
        ("init $9F00, play $FF00", banks(0x9F00, 0xFF00), 1, None),
        ("init of 2 s", slow_init(), 1, e1),
        ("init forever", init_forever(), 1, None),
        ("play overruns", play_overrun(23), 1, e1),
        ("play forever", play_forever(), 1, e1),
        (
            "play forever, $01 alone",
            plain("forever: jmp forever\n"),
            1,
            Some((E7Cut, 3)),
        ),
        ("jam", jam(), 1, e1),
        (
            "jam, $01 alone",
            plain(".BYTE ($02)\n"),
            1,
            Some((E10Jam, 3)),
        ),
        ("busy init", busy_init(), 1, e1),
        ("osc3 noise", osc3_noise(), 1, Some((E2SidRead, 0))),
        ("env3 wait", env3_wait(), 1, Some((E2SidRead, 0))),
        ("raster wait", raster_wait(0x80), 1, e1),
        (
            "raster wait, NTSC",
            raster_wait(0x60).flags(CLOCK_NTSC),
            1,
            Some((E3Raster, 0)),
        ),
        ("raster bit 8 wait", raster_high_wait(), 1, e1),
        (
            "raster in play",
            raster_in_play(0x3000),
            1,
            Some((E3Raster, 1)),
        ),
        ("cia counter", cia_counter(), 1, Some((E4CiaTimer, 0))),
        ("irq flags, vbi", irq_flags(false), 1, Some((E5IrqFlag, 1))),
        ("irq flags, cia", irq_flags(true), 1, Some((E5IrqFlag, 1))),
        ("bus read", bus_read(4), 1, Some((E2SidRead, 0))),
        ("bus read, 0x1d00", bus_read(0x1d00), 1, e1),
        (
            "mirrors",
            stores(&[
                0xD400, 0xD41F, 0xD480, 0xD7E0, 0xD7FF, 0xD3FF, 0xD800, 0xDC00,
            ]),
            1,
            e1,
        ),
        (
            "2SID at $D420",
            stores(&[0xD400, 0xD420, 0xD43F, 0xD440]).sid2(0x42),
            1,
            e1,
        ),
        (
            "2SID at $DE00",
            stores(&[0xD400, 0xDE00, 0xDE1F]).sid2(0xE0),
            1,
            Some((E12SecondSidOutside, 0)),
        ),
        (
            "3SID at $D440",
            stores(&[0xD400, 0xD420, 0xD440, 0xD441])
                .sid2(0x42)
                .sid3(0x44),
            1,
            Some((E9ThirdSid, 0)),
        ),
        (
            "3SID at $DE00",
            stores(&[0xD400, 0xD420, 0xDE00, 0xD440])
                .sid2(0x42)
                .sid3(0xE0),
            1,
            e1,
        ),
        ("two SIDs' buses", two_sid_buses(), 1, Some((E2SidRead, 0))),
        ("I/O read back", io_readback(), 1, Some((E6OtherRead, 0))),
        ("I/O as RAM", io_ram(), 1, Some((E1Bank, 1))),
        ("read-modify-write", rmw(), 1, Some((E2SidRead, 0))),
    ];
    for song in [1, 32, 33, 40] {
        v.push(("forty songs", forty_songs(), song, e1));
    }
    v
}

#[test]
fn task_6s_fixtures_differ_only_where_their_rule_says() {
    let seconds = 3;
    let fx = fixtures();
    let jobs: Vec<Job> = fx
        .iter()
        .map(|(name, p, song, _)| Job {
            name: name.to_string(),
            file: p.bytes(),
            song: *song,
        })
        .collect();
    let results = run_all(&jobs, seconds);
    let mut wrong = Vec::new();
    for ((name, _, song, want), r) in fx.iter().zip(&results) {
        match r {
            Ok(o) => {
                let got = o.first.as_ref().map(|d| (d.rule, d.call));
                if got != *want {
                    wrong.push(format!(
                        "{name} (song {song}): {:?}, want {want:?}",
                        o.first
                    ));
                } else {
                    eprintln!(
                        "{name} (song {song}): {} calls equal, {:?}",
                        o.calls, o.first
                    );
                }
            }
            Err(e) => wrong.push(format!("{name} (song {song}): {e}")),
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_crafted_headers_are_sidous_alone() {
    let mut b = vec![0u8; 124];
    b[..4].copy_from_slice(b"PSID");
    b[5] = 2;
    b[7] = 0x7C;
    b[15] = 1;
    b[17] = 1;
    let mut c = b.clone();
    c[6] = 0x20;
    c[7] = 0;
    c[11] = 0x10;
    c[13] = 0x10;
    for f in [b, c] {
        assert!(header(&f).is_err());
        assert!(psid_ref::run(&f, 1, 1).is_err());
        assert!(sidou::run(&f, 1, 1).is_err());
    }
}

fn sid_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            sid_files(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("sid")) {
            out.push(p);
        }
    }
}

#[test]
#[ignore]
fn the_users_tunes_against_the_oracle() {
    let dir = PathBuf::from(std::env::var("SIDOU_PSID_DIR").expect("SIDOU_PSID_DIR"));
    let seconds: u32 = std::env::var("SIDOU_PSID_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let mut files = Vec::new();
    sid_files(&dir, &mut files);
    assert!(!files.is_empty(), "no .sid files under {}", dir.display());
    let mut jobs = Vec::new();
    let mut refused = Vec::new();
    for p in &files {
        let file = std::fs::read(p).unwrap();
        let name = p
            .strip_prefix(&dir)
            .unwrap_or(p)
            .to_string_lossy()
            .into_owned();
        match header(&file) {
            Ok(h) => {
                for song in 1..=u32::from(h.songs) {
                    jobs.push(Job {
                        name: name.clone(),
                        file: file.clone(),
                        song,
                    });
                }
            }
            Err(code) => refused.push((name, code)),
        }
    }
    let results = run_all(&jobs, seconds);
    let mut report = format!(
        "SIDOU's PSID environment against m6502.h + psid.c, {seconds} s a song, {} files\n\
         (host/crates/psid-ref/tests/oracle.rs; TESTING.md, \"The PSID oracle\")\n\n\
         {:<48} {:>4} {:>7} {:>9}  ended by\n",
        files.len(),
        "tune",
        "song",
        "calls",
        "writes"
    );
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut unexplained = Vec::new();
    for (job, r) in jobs.iter().zip(&results) {
        let (calls, writes, ended) = match r {
            Ok(o) => {
                let ended = match &o.first {
                    Some(d) => format!(
                        "{} (call {}, cycle {}: {})",
                        d.rule.name(),
                        d.call,
                        d.cycle,
                        d.detail
                    ),
                    None => "nothing: equal".into(),
                };
                let key = o.first.as_ref().map_or("equal", |d| d.rule.name());
                *counts.entry(key.into()).or_default() += 1;
                (o.calls.to_string(), o.writes.to_string(), ended)
            }
            Err(e) => {
                *counts.entry("UNEXPLAINED".into()).or_default() += 1;
                unexplained.push(format!("{} song {}: {e}", job.name, job.song));
                ("-".into(), "-".into(), format!("UNEXPLAINED: {e}"))
            }
        };
        report += &format!(
            "{:<48} {:>4} {:>7} {:>9}  {ended}\n",
            job.name, job.song, calls, writes
        );
    }
    for (name, code) in &refused {
        let why = match *code as u32 {
            sidou_sys::PSID_E_RSID => "RSID (M3 plays PSID only)",
            sidou_sys::PSID_E_MUS => "MUS",
            sidou_sys::PSID_E_NOT_SID => "not a SID file",
            _ => "a header SIDOU refuses",
        };
        report += &format!(
            "{name:<48}    -       -         -  refused by SIDOU's parser: {why} ({code})\n"
        );
        *counts.entry("refused".into()).or_default() += 1;
    }
    report += "\nSongs by what ended their comparison:\n";
    for (k, n) in &counts {
        report += &format!("  {k}: {n}\n");
    }
    let out = root().join("build/psid-ref");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("report.txt"), &report).unwrap();
    eprintln!("{report}");
    assert!(unexplained.is_empty(), "{}", unexplained.join("\n"));
}
