// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use residfp_sys::Model;
use sidou_sys::{sid, sid_init, sid_model, SID_CLOCK_PAL, SID_MODEL_6581, SID_MODEL_8580};
use sidou_tablegen::filter_fit::{direct_spectra, fit, measure_modes, CurveSetting};
use sidou_tests::alignment::{align, overlapping};
use sidou_tests::band_energy::{
    band_centre_hz, band_frames, distance, fitted_output_stage, Bands, Distance, OutputStage, BANDS,
};
use sidou_tests::harmonics::{probe, Probe, HIGHEST};
use sidou_tests::psid::{self, Write, BLOCK};
use sidou_tests::render::REF_SAMPLE_RATE;
use sidou_tests::residfp_engine::ResidfpEngine;
use sidou_tests::signal_metrics::write_wav16;

const SECONDS: f64 = 30.0;
const MAX_LAG_SECONDS: f64 = 2.0;
const SONG: u32 = 1;
const PLAYER_VOLUME: u8 = 0x0f;
const THREADS: usize = 4;
const RECORDING: &str = "rec.f32";
const PROBE_FCS: [u32; 5] = [256, 384, 512, 768, 1024];
const PROBE_MIN_HZ: f64 = 800.0;
const LP_TONE_DIVISOR: f64 = 5.0;
const LP: u8 = 0x10;
const BP: u8 = 0x20;
const LEVELS: [(u8, u8); 4] = [(1, 0x4), (1, 0x8), (1, 0xf), (3, 0xf)];

fn model_name(m: sid_model) -> &'static str {
    if m == SID_MODEL_6581 {
        "6581"
    } else {
        "8580"
    }
}

fn residfp_model(m: sid_model) -> Model {
    if m == SID_MODEL_6581 {
        Model::Mos6581
    } else {
        Model::Csg8580
    }
}

fn residfp_f0(m: sid_model, fc: u32) -> f64 {
    let setting = CurveSetting::default_for(residfp_model(m));
    let direct = direct_spectra(&setting);
    fit(&measure_modes(&setting, &direct, fc, 0), None).f0
}

fn harmonics() {
    println!("harmonic level against the fundamental (dB), H2..H{HIGHEST}: SIDOU | reSIDfp | SIDOU minus reSIDfp");
    for m in [SID_MODEL_6581, SID_MODEL_8580] {
        let (fc, f0) = PROBE_FCS
            .iter()
            .map(|&fc| (fc, residfp_f0(m, fc)))
            .find(|&(_, f0)| f0 >= PROBE_MIN_HZ)
            .expect("a cutoff in reach");
        println!("{} fc {fc}: reSIDfp f0 {f0:.0} Hz", model_name(m));
        let lp_tone = f0 / LP_TONE_DIVISOR;
        let probes = [
            (None, "direct", lp_tone),
            (Some(LP), "LP", lp_tone),
            (None, "direct", f0),
            (Some(BP), "BP", f0),
        ];
        for (mode, name, tone_hz) in probes {
            for (voices, sustain) in LEVELS {
                let p = Probe {
                    mode,
                    fc,
                    tone_hz,
                    voices,
                    sustain,
                };
                let r = probe(m, &p);
                let diff: Vec<f64> = r.sidou.iter().zip(&r.residfp).map(|(a, b)| a - b).collect();
                println!(
                    "{} {name:<6} tone {tone_hz:6.0} Hz voices {voices} sustain {sustain:#x}: {} | {} | {}",
                    model_name(m),
                    row(&r.sidou),
                    row(&r.residfp),
                    row(&diff)
                );
            }
        }
    }
}

fn row(x: &[f64]) -> String {
    x.iter()
        .map(|v| format!("{v:6.1}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn samples() -> usize {
    (SECONDS * REF_SAMPLE_RATE) as usize
}

fn render_sidou(m: sid_model, writes: &[Write]) -> Vec<f32> {
    let mut chips = Box::new([sid::default(), sid::default()]);
    for c in chips.iter_mut() {
        unsafe { sid_init(c, m, SID_CLOCK_PAL, REF_SAMPLE_RATE as u32) };
    }
    let (a, _) = psid::reference(&mut chips, writes, samples().div_ceil(BLOCK));
    a.iter().map(|&v| f32::from(v) / 32768.0).collect()
}

fn render_residfp(m: sid_model, writes: &[Write]) -> Vec<f32> {
    let mut e = ResidfpEngine::new(m, REF_SAMPLE_RATE, false);
    let mut out = Vec::new();
    let mut now = 0u32;
    for w in writes.iter().filter(|w| w.chip == 0) {
        if w.cycle > now {
            e.render_cycles(w.cycle - now, &mut out);
            now = w.cycle;
        }
        e.write(w.reg, w.value);
    }
    while out.len() < samples() {
        e.render_cycles(SID_CLOCK_PAL / 50, &mut out);
    }
    out.truncate(samples());
    out.iter().map(|&v| f32::from(v) / 32768.0).collect()
}

struct FilterUse {
    on: f64,
    modes: [f64; 3],
    fc: (u32, u32),
}

fn filter_use(writes: &[Write]) -> FilterUse {
    let end = (SECONDS * f64::from(SID_CLOCK_PAL)) as u32;
    let mut regs = [0u8; 0x20];
    let (mut on, mut modes, mut fc) = (0u64, [0u64; 3], (u32::MAX, 0));
    let mut last = 0u32;
    let mut account = |regs: &[u8; 0x20], until: u32, last: &mut u32| {
        let span = u64::from(until.saturating_sub(*last));
        *last = until;
        let mode = regs[0x18] >> 4 & 7;
        if regs[0x17] & 7 == 0 || mode == 0 {
            return;
        }
        on += span;
        for (i, m) in modes.iter_mut().enumerate() {
            if mode >> i & 1 != 0 {
                *m += span;
            }
        }
        let cutoff = u32::from(regs[0x16]) << 3 | u32::from(regs[0x15] & 7);
        fc = (fc.0.min(cutoff), fc.1.max(cutoff));
    };
    for w in writes.iter().filter(|w| w.chip == 0 && w.cycle < end) {
        account(&regs, w.cycle, &mut last);
        regs[usize::from(w.reg & 0x1f)] = w.value;
    }
    account(&regs, end, &mut last);
    let total = f64::from(end);
    FilterUse {
        on: on as f64 / total,
        modes: modes.map(|m| m as f64 / total),
        fc,
    }
}

fn read_f32(path: &Path) -> std::io::Result<Vec<f32>> {
    let bytes = std::fs::read(path)?;
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&b| f32::from_le_bytes(b))
        .collect())
}

struct Match {
    distance: Distance,
    stage: OutputStage,
    lag: isize,
}

fn matched(recording: &[f32], test: &[f32]) -> Match {
    let a = align(
        recording,
        test,
        (MAX_LAG_SECONDS * REF_SAMPLE_RATE) as usize,
        REF_SAMPLE_RATE,
    );
    let (r, t) = overlapping(recording, test, a);
    let (stage, distance) = fitted_output_stage(
        &band_frames(r, REF_SAMPLE_RATE),
        &band_frames(t, REF_SAMPLE_RATE),
    );
    Match {
        distance,
        stage,
        lag: a.lag,
    }
}

struct Source {
    name: String,
    model: sid_model,
    is_chip: bool,
    audio: Vec<f32>,
}

fn chip_model(name: &str) -> sid_model {
    if name.starts_with("CSG8580") {
        SID_MODEL_8580
    } else {
        SID_MODEL_6581
    }
}

struct TuneReport {
    text: String,
    filter_on: f64,
    rows: Vec<(String, String, sid_model, Distance)>,
}

fn sources(dir: &Path, writes: &[Write]) -> std::io::Result<Vec<Source>> {
    let mut v = Vec::new();
    for m in [SID_MODEL_6581, SID_MODEL_8580] {
        v.push(Source {
            name: format!("SIDOU {}", model_name(m)),
            model: m,
            is_chip: false,
            audio: render_sidou(m, writes),
        });
        v.push(Source {
            name: format!("reSIDfp {}", model_name(m)),
            model: m,
            is_chip: false,
            audio: render_residfp(m, writes),
        });
    }
    let mut chips: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            std::fs::metadata(p.join(RECORDING))
                .is_ok_and(|m| m.len() as usize >= samples() * size_of::<f32>())
        })
        .collect();
    chips.sort();
    for p in chips {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        v.push(Source {
            model: chip_model(&name),
            name,
            is_chip: true,
            audio: read_f32(&p.join(RECORDING))?,
        });
    }
    Ok(v)
}

fn tune(dir: &Path) -> std::io::Result<TuneReport> {
    let writes = tune_writes(&dir.join("sid/tune.sid"))?;
    let u = filter_use(&writes);
    let all = sources(dir, &writes)?;
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    let mut text = format!(
        "## {name}: filter on {:.0}% of the time (LP {:.0}%, BP {:.0}%, HP {:.0}%), FC {}..{}, {} writes\n",
        100.0 * u.on,
        100.0 * u.modes[0],
        100.0 * u.modes[1],
        100.0 * u.modes[2],
        u.fc.0,
        u.fc.1,
        writes.len()
    );
    let chips: Vec<&Source> = all.iter().filter(|s| s.is_chip).collect();
    text += &format!(
        "{:<14}{}\n",
        "shape dB",
        chips
            .iter()
            .map(|c| format!("{:>32}", c.name))
            .collect::<String>()
    );
    let mut rows = Vec::new();
    for s in &all {
        let mut line = format!("{:<14}", s.name);
        for c in &chips {
            if c.name == s.name {
                line += &format!("{:>32}", "-");
                continue;
            }
            let test = &s.audio[..samples().min(s.audio.len())];
            let m = matched(&c.audio, test);
            line += &format!(
                "{:>6.2} lvl {:4.2} {:>3}/{:<5} @{:<6.2}",
                m.distance.shape_db,
                m.distance.level_db,
                m.stage.high_pass_hz,
                if m.stage.low_pass_hz.is_finite() {
                    format!("{:.0}k", m.stage.low_pass_hz / 1000.0)
                } else {
                    "-".into()
                },
                m.lag as f64 / REF_SAMPLE_RATE
            );
            if s.model == c.model {
                rows.push((s.name.clone(), c.name.clone(), s.model, m.distance));
            }
        }
        text += &line;
        text.push('\n');
    }
    let sidou = &all[0].audio;
    let residfp = &all[1].audio;
    let direct = distance(
        &band_frames(residfp, REF_SAMPLE_RATE),
        &band_frames(sidou, REF_SAMPLE_RATE),
    );
    text += &format!(
        "SIDOU 6581 against reSIDfp 6581, no alignment: shape {:.2} lvl {:.2}\n",
        direct.shape_db, direct.level_db
    );
    Ok(TuneReport {
        text,
        filter_on: u.on,
        rows,
    })
}

fn mean(x: &[f64]) -> f64 {
    x.iter().sum::<f64>() / x.len() as f64
}

const FILTER_TUNE: f64 = 0.25;
const COLUMNS: [(&str, &str, sid_model); 5] = [
    ("SIDOU 6581", "SIDOU", SID_MODEL_6581),
    ("reSIDfp 6581", "reSIDfp", SID_MODEL_6581),
    ("6581 chips", "chip", SID_MODEL_6581),
    ("SIDOU 8580", "SIDOU", SID_MODEL_8580),
    ("reSIDfp 8580", "reSIDfp", SID_MODEL_8580),
];
const CHIPS_6581: usize = 2;

fn columns(r: &TuneReport) -> Vec<Vec<Distance>> {
    COLUMNS
        .iter()
        .map(|&(_, source, model)| {
            r.rows
                .iter()
                .filter(|(s, _, m, _)| *m == model && source_matches(s, source))
                .map(|(_, _, _, d)| *d)
                .collect()
        })
        .collect()
}

fn mean_of(d: &[Distance], f: impl Fn(&Distance) -> f64) -> f64 {
    mean(&d.iter().map(f).collect::<Vec<_>>())
}

fn per_band(d: &[Distance]) -> Bands {
    std::array::from_fn(|b| mean_of(d, |x| x.per_band_db[b]))
}

fn summary(reports: &[(String, TuneReport)]) {
    println!("\n## summary: mean shape distance (dB) against the same model's chips; lvl: level-contour RMS (dB)");
    print!("{:<16}{:>7}", "tune", "filter");
    for (name, _, _) in COLUMNS {
        print!("{name:>14}");
    }
    println!("{:>12}{:>16}", "6581 max", "SIDOU 6581 is");
    let mut groups: [Vec<Vec<Vec<Distance>>>; 2] = [Vec::new(), Vec::new()];
    for (name, r) in reports {
        let cols = columns(r);
        let chip_max = cols[CHIPS_6581]
            .iter()
            .map(|d| d.shape_db)
            .fold(f64::NAN, f64::max);
        print!("{name:<16}{:>6.0}%", 100.0 * r.filter_on);
        for c in &cols {
            print!(
                "{:>6.2} lvl{:>4.1}",
                mean_of(c, |d| d.shape_db),
                mean_of(c, |d| d.level_db)
            );
        }
        let ours = mean_of(&cols[0], |d| d.shape_db);
        let verdict = if ours <= chip_max {
            "inside".to_string()
        } else {
            format!("out by {:.2}", ours - chip_max)
        };
        println!("{chip_max:>12.2}{verdict:>16}");
        groups[usize::from(r.filter_on >= FILTER_TUNE)].push(cols);
    }
    for (label, g) in [
        ("unfiltered controls", &groups[0]),
        ("filter tunes", &groups[1]),
    ] {
        print!("{:<23}", format!("mean, {} {label}", g.len()));
        for i in 0..COLUMNS.len() {
            let tune_means: Vec<f64> = g.iter().map(|c| mean_of(&c[i], |d| d.shape_db)).collect();
            let levels: Vec<f64> = g.iter().map(|c| mean_of(&c[i], |d| d.level_db)).collect();
            print!("{:>6.2} lvl{:>4.1}", mean(&tune_means), mean(&levels));
        }
        println!();
    }
    println!("\n## per band, filter tunes: mean signed difference, source minus chip (dB)");
    print!("{:<8}", "band Hz");
    for (name, _, _) in COLUMNS {
        print!("{name:>14}");
    }
    println!("{:>14}", "|6581 chips|");
    let filtered = &groups[1];
    let all = |i: usize| -> Vec<Distance> { filtered.iter().flat_map(|c| c[i].clone()).collect() };
    let bands: Vec<Bands> = (0..COLUMNS.len()).map(|i| per_band(&all(i))).collect();
    let chip_abs: Bands = {
        let chips = all(CHIPS_6581);
        std::array::from_fn(|b| mean_of(&chips, |x| x.per_band_db[b].abs()))
    };
    for b in 0..BANDS {
        print!("{:<8.0}", band_centre_hz(b));
        for v in &bands {
            print!("{:>14.2}", v[b]);
        }
        println!("{:>14.2}", chip_abs[b]);
    }
    println!("\n## per band and tune: SIDOU 6581 minus the 6581 chips, then reSIDfp 6581 (dB), bands 50 Hz..16 kHz");
    for (name, r) in reports.iter().filter(|(_, r)| r.filter_on >= FILTER_TUNE) {
        let cols = columns(r);
        for (label, i) in [("SIDOU", 0), ("reSIDfp", 1)] {
            let row: String = per_band(&cols[i])
                .iter()
                .map(|v| format!("{v:5.1}"))
                .collect();
            println!("{name:<16}{label:<8}{row}");
        }
    }
}

fn source_matches(source: &str, kind: &str) -> bool {
    match kind {
        "chip" => !source.starts_with("SIDOU") && !source.starts_with("reSIDfp"),
        k => source.starts_with(k),
    }
}

fn chips(root: &Path) -> std::io::Result<()> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("sid/tune.sid").is_file())
        .collect();
    dirs.sort();
    let mut reports = Vec::new();
    for batch in dirs.chunks(THREADS) {
        let done: Vec<std::io::Result<TuneReport>> = std::thread::scope(|s| {
            let handles: Vec<_> = batch.iter().map(|d| s.spawn(|| tune(d))).collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("tune thread"))
                .collect()
        });
        for (d, r) in batch.iter().zip(done) {
            let r = r?;
            println!("{}", r.text);
            reports.push((d.file_name().unwrap().to_string_lossy().into_owned(), r));
        }
    }
    summary(&reports);
    Ok(())
}

fn tune_writes(sid_file: &Path) -> std::io::Result<Vec<Write>> {
    let file = std::fs::read(sid_file)?;
    if !file.starts_with(b"PSID") {
        return Err(std::io::Error::other(format!(
            "{}: not a PSID file",
            sid_file.display()
        )));
    }
    let cycles = (SECONDS * f64::from(SID_CLOCK_PAL)) as u32 + SID_CLOCK_PAL;
    let volume = Write {
        cycle: 0,
        chip: 0,
        reg: 0x18,
        value: PLAYER_VOLUME,
    };
    Ok(std::iter::once(volume)
        .chain(psid::stream(&file, SONG, cycles))
        .collect())
}

fn usage(files: &[String]) -> std::io::Result<()> {
    for f in files {
        let writes = match tune_writes(Path::new(f)) {
            Ok(w) => w,
            Err(e) => {
                println!("{e}");
                continue;
            }
        };
        let u = filter_use(&writes);
        println!(
            "{f}: filter on {:.0}% (LP {:.0}%, BP {:.0}%, HP {:.0}%), FC {}..{}",
            100.0 * u.on,
            100.0 * u.modes[0],
            100.0 * u.modes[1],
            100.0 * u.modes[2],
            u.fc.0,
            u.fc.1
        );
    }
    Ok(())
}

fn render(sid_file: &Path, out: &Path) -> std::io::Result<()> {
    let writes = tune_writes(sid_file)?;
    for m in [SID_MODEL_6581, SID_MODEL_8580] {
        for (engine, audio) in [
            ("sidou", render_sidou(m, &writes)),
            ("residfp", render_residfp(m, &writes)),
        ] {
            let path = out.join(format!("{engine}_{}.wav", model_name(m)));
            write_wav16(&path, &audio, REF_SAMPLE_RATE as u32)?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [cmd] if cmd == "harmonics" => {
            harmonics();
            Ok(())
        }
        [cmd, dir] if cmd == "chips" => chips(Path::new(dir)),
        [cmd, files @ ..] if cmd == "usage" => usage(files),
        [cmd, sid_file, out] if cmd == "render" => render(Path::new(sid_file), Path::new(out)),
        _ => {
            eprintln!(
                "usage: filter-metrics harmonics | chips <dir> | usage <sid>... | render <sid> <out dir>"
            );
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
