// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sidou_sysex::import::{bank_files, default_name, Model, SongFile};
use sidou_sysex::patch::{bank_file, patch_file, song_file, step_file, PatchFile};
use sidou_sysex::send::{file_type_of, sent_name, Client, Port, Sent, Timing, DEFAULT_PORT};
use sidou_sysex::sid;

const USAGE: &str = "usage: sidou-sysex sid FILE.sid|DIR... [--out DIR] [--trs] [--pack]\n       sidou-sysex import BANK.syx [--out DIR] [--8580] [--name NAME]\n       sidou-sysex patch FILE.json [--out DIR] [--8580]\n       sidou-sysex bank FILE.json [--out DIR] [--8580]\n       sidou-sysex step FILE.json [--out DIR] [--8580]\n       sidou-sysex song FILE.json [--out DIR] [--8580]\n       sidou-sysex send FILE.sng|.sid|.ins [--port NAME] [--name NAME] [--replace]";
const IMPORT_NAME_MAX: usize = 13;

const PACK_FILE_NAME: &str = "pack.syx";

struct Args {
    files: Vec<PathBuf>,
    out: PathBuf,
    trs: bool,
    pack: bool,
}

fn args(rest: Vec<String>) -> Result<Args, String> {
    let mut it = rest.into_iter();
    let mut a = Args {
        files: Vec::new(),
        out: PathBuf::from("."),
        trs: false,
        pack: false,
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => a.out = it.next().ok_or(USAGE)?.into(),
            "--trs" => a.trs = true,
            "--pack" => a.pack = true,
            "-h" | "--help" => return Err(USAGE.to_string()),
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}\n{USAGE}")),
            _ => a.files.push(arg.into()),
        }
    }
    if a.files.is_empty() {
        return Err(USAGE.to_string());
    }
    Ok(a)
}

fn expand(args: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for arg in args {
        if !arg.is_dir() {
            files.push(arg);
            continue;
        }
        let mut found: Vec<PathBuf> = std::fs::read_dir(&arg)
            .map_err(|e| format!("{}: {e}", arg.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("sid")))
            .collect();
        found.sort();
        files.extend(found);
    }
    Ok(files)
}

fn target(out: &Path, file: &Path, used: &mut HashSet<PathBuf>) -> PathBuf {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "tune".to_string());
    let mut path = out.join(format!("{stem}.syx"));
    let mut k = 2;
    while used.contains(&path) {
        path = out.join(format!("{stem}-{k}.syx"));
        k += 1;
    }
    used.insert(path.clone());
    path
}

struct Import {
    bank: PathBuf,
    out: PathBuf,
    model: Model,
    name: Option<String>,
}

fn import_args(rest: Vec<String>) -> Result<Import, String> {
    let mut it = rest.into_iter();
    let mut bank = None;
    let mut i = Import {
        bank: PathBuf::new(),
        out: PathBuf::from("."),
        model: Model::Mos6581,
        name: None,
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => i.out = it.next().ok_or(USAGE)?.into(),
            "--8580" => i.model = Model::Csg8580,
            "--name" => i.name = Some(it.next().ok_or(USAGE)?.to_ascii_uppercase()),
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}\n{USAGE}")),
            _ if bank.is_none() => bank = Some(PathBuf::from(arg)),
            _ => return Err(USAGE.to_string()),
        }
    }
    i.bank = bank.ok_or(USAGE)?;
    if i.name
        .as_ref()
        .is_some_and(|n| n.is_empty() || n.len() > IMPORT_NAME_MAX)
    {
        return Err(format!("--name takes 1 to {IMPORT_NAME_MAX} characters"));
    }
    Ok(i)
}

fn make_out(out: &Path) -> Result<(), ExitCode> {
    std::fs::create_dir_all(out).map_err(|e| {
        eprintln!("{}: {e}", out.display());
        ExitCode::FAILURE
    })
}

fn stem(name: &str) -> String {
    name.replace(' ', "-")
}

fn write_song(out: &Path, song: &SongFile) -> Result<(), ExitCode> {
    for (ext, bytes) in [("sng", &song.sng), ("syx", &song.syx)] {
        let path = out.join(format!("{}.{ext}", stem(&song.name)));
        std::fs::write(&path, bytes).map_err(|e| {
            eprintln!("{}: {e}", path.display());
            ExitCode::FAILURE
        })?;
    }
    println!(
        "{} -> {}.syx",
        song.name,
        out.join(stem(&song.name)).display()
    );
    Ok(())
}

fn patch_main(rest: Vec<String>, read: fn(&str, Model) -> Result<PatchFile, String>) -> ExitCode {
    let i = match import_args(rest) {
        Ok(i) if i.name.is_none() => i,
        Ok(_) => {
            eprintln!("--name is the patch's own \"name\"\n{USAGE}");
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let text = match std::fs::read_to_string(&i.bank) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}: {e}", i.bank.display());
            return ExitCode::FAILURE;
        }
    };
    let file = match read(&text, i.model) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}: {e}", i.bank.display());
            return ExitCode::FAILURE;
        }
    };
    if let Some(warning) = &file.quiet {
        eprintln!("{}: warning: {warning}", i.bank.display());
    }
    if let Some(tempo) = &file.tempo {
        println!("{}: {tempo} BPM", i.bank.display());
    }
    if let Err(code) = make_out(&i.out).and_then(|()| write_song(&i.out, &file.song)) {
        return code;
    }
    println!("On the FM-1: SONG, LOAD {}, SEQ to INSTR, an instrument, ENV opens it in LAB, SAVE keeps it", file.song.name);
    ExitCode::SUCCESS
}

struct Send {
    file: PathBuf,
    port: String,
    name: Option<String>,
    replace: bool,
}

fn send_args(rest: Vec<String>) -> Result<Send, String> {
    let mut it = rest.into_iter();
    let mut file = None;
    let mut a = Send {
        file: PathBuf::new(),
        port: DEFAULT_PORT.to_string(),
        name: None,
        replace: false,
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--port" => a.port = it.next().ok_or(USAGE)?,
            "--name" => a.name = Some(it.next().ok_or(USAGE)?.to_ascii_uppercase()),
            "--replace" => a.replace = true,
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}\n{USAGE}")),
            _ if file.is_none() => file = Some(PathBuf::from(arg)),
            _ => return Err(USAGE.to_string()),
        }
    }
    a.file = file.ok_or(USAGE)?;
    Ok(a)
}

fn send_to(
    port: impl Port,
    a: &Send,
    bytes: &[u8],
    file_type: u8,
    name: &str,
) -> Result<Sent, String> {
    let mut c = Client::new(port, Timing::default());
    let d = c.inquire()?;
    println!(
        "{} is SIDOU {}.{}.{}, {} bytes free",
        c.port.name(),
        d.version[0],
        d.version[1],
        d.version[2],
        d.free
    );
    c.send_file(name, file_type, bytes, a.replace, &mut |w| {
        eprintln!("the FM-1 is busy: sending again in {} s", w.as_secs());
    })
}

#[cfg(target_os = "macos")]
fn open_port(name: &str) -> Result<sidou_sysex::coremidi::MidiPort, String> {
    sidou_sysex::coremidi::MidiPort::open(name)
}

#[cfg(not(target_os = "macos"))]
fn open_port(_name: &str) -> Result<sidou_sysex::send::NoPort, String> {
    Err("send talks to MIDI ports on macOS only: elsewhere use the Library page".to_string())
}

fn send_main(rest: Vec<String>) -> ExitCode {
    let a = match send_args(rest) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let bytes = match std::fs::read(&a.file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{}: {e}", a.file.display());
            return ExitCode::FAILURE;
        }
    };
    let Some(file_type) = file_type_of(&bytes) else {
        eprintln!("{}: not a song (.sng), a .sid tune or an instrument (.ins): send sends nothing else, never a firmware package or update", a.file.display());
        return ExitCode::FAILURE;
    };
    let name = a
        .name
        .clone()
        .unwrap_or_else(|| sent_name(&a.file.display().to_string(), &bytes));
    println!(
        "sending {} as {name} to the MIDI port \"{}\"",
        a.file.display(),
        a.port
    );
    let done = open_port(&a.port).and_then(|p| send_to(p, &a, &bytes, file_type, &name));
    match done {
        Ok(Sent::Same) => println!("{name} is already on the FM-1, identical: left alone"),
        Ok(Sent::Sent) => println!("{name} sent ({} bytes)", bytes.len()),
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn import_main(rest: Vec<String>) -> ExitCode {
    let i = match import_args(rest) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if let Err(code) = make_out(&i.out) {
        return code;
    }
    let bank = match std::fs::read(&i.bank) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{}: {e}", i.bank.display());
            return ExitCode::FAILURE;
        }
    };
    let name = i.name.unwrap_or_else(|| default_name(&bank).to_string());
    let files = match bank_files(&bank, i.model, &name) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}: {e}", i.bank.display());
            return ExitCode::FAILURE;
        }
    };
    for song in &files.songs {
        if let Err(code) = write_song(&i.out, song) {
            return code;
        }
    }
    let report = i.out.join(format!("{}.txt", stem(&name)));
    if let Err(e) = std::fs::write(&report, &files.report) {
        eprintln!("{}: {e}", report.display());
        return ExitCode::FAILURE;
    }
    println!(
        "{}: what each patch keeps and loses. On the FM-1: load a song, pull an instrument into LAB \
         (INSTR, ENV), then SAVE",
        report.display()
    );
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let mut all = std::env::args().skip(1);
    let command = all.next();
    let rest: Vec<String> = all.collect();
    match command.as_deref() {
        Some("import") => import_main(rest),
        Some("patch") => patch_main(rest, patch_file),
        Some("bank") => patch_main(rest, bank_file),
        Some("step") => patch_main(rest, step_file),
        Some("song") => patch_main(rest, song_file),
        Some("send") => send_main(rest),
        Some("sid") => sid_main(rest),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn sid_main(rest: Vec<String>) -> ExitCode {
    let a = match args(rest) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let files = match expand(a.files.clone()) {
        Ok(f) if !f.is_empty() => f,
        Ok(_) => {
            eprintln!("no .sid files\n{USAGE}");
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if let Err(code) = make_out(&a.out) {
        return code;
    }
    let mut pack = Vec::new();
    let mut names = HashSet::new();
    let mut paths = HashSet::new();
    let mut failed = 0;
    let mut frames = 0;
    for file in &files {
        let shown = file.display();
        let bytes = match std::fs::read(file) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{shown}: {e}");
                failed += 1;
                continue;
            }
        };
        let h = match sid::check(&bytes) {
            Ok(h) => h,
            Err(why) => {
                eprintln!("{shown}: refused: {why}");
                failed += 1;
                continue;
            }
        };
        let stem = file.file_stem().unwrap_or_default().to_string_lossy();
        let name = sid::store_name(&sid::title(&h), &stem, &mut names);
        let syx = sid::syx(&name, &bytes).expect("a checked tune's frames");
        let path = if a.pack {
            a.out.join(PACK_FILE_NAME)
        } else {
            target(&a.out, file, &mut paths)
        };
        if a.pack {
            pack.extend_from_slice(&syx);
        } else if let Err(e) = std::fs::write(&path, &syx) {
            eprintln!("{}: {e}", path.display());
            failed += 1;
            continue;
        }
        let packets = bytes.len().div_ceil(sidou_sysex::MAX_DATA_LEN) + 2;
        frames += packets;
        println!(
            "{shown} -> {} as {name}: {} by {}, {} song{}, {} chip{}; {packets} messages",
            path.display(),
            sid::title(&h),
            sid::author(&h),
            h.songs,
            if h.songs == 1 { "" } else { "s" },
            if h.sid2 != 0 { 2 } else { 1 },
            if h.sid2 != 0 { "s" } else { "" },
        );
        for w in sid::warnings(&h) {
            println!("  note: {w}");
        }
    }
    if a.pack && !pack.is_empty() {
        let path = a.out.join(PACK_FILE_NAME);
        if let Err(e) = std::fs::write(&path, &pack) {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
        println!(
            "{}: {} tunes, {} bytes, {frames} messages",
            path.display(),
            names.len(),
            pack.len()
        );
    }
    if frames > 0 {
        if a.trs {
            println!(
                "TRS: pause one second after every message (about {} s for these)",
                frames
            );
        } else {
            println!(
                "USB: send each message once the device has answered the last (ACK), or pause \
                 one second after each; over TRS, run with --trs"
            );
        }
    }
    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
