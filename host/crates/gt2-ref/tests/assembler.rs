// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};
use std::process::Command;

use gt2_ref::*;
use sha2::{Digest, Sha256};

const URL: &str = "https://sourceforge.net/projects/goattracker2/files/GoatTracker%202/2.77/GoatTracker_2.77.zip/download";
const SHA256: &str = "96c2bd6a6ab3aca2f5bb18b1c764ac6ea69ac245cae14002a72cd87c554561ef";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn sha256(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn run(cmd: &mut Command) {
    let out = cmd.output().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    assert!(
        out.status.success(),
        "{cmd:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn release(dir: &Path) -> PathBuf {
    let zip = dir.join("GoatTracker_2.77.zip");
    if !zip.exists() || sha256(&std::fs::read(&zip).unwrap()) != SHA256 {
        std::fs::create_dir_all(dir).unwrap();
        run(Command::new("curl")
            .args(["-sSL", "-A", "sidou-build", "-o"])
            .arg(&zip)
            .arg(URL));
    }
    assert_eq!(
        sha256(&std::fs::read(&zip).unwrap()),
        SHA256,
        "{}",
        zip.display()
    );
    let out = dir.join("release");
    if !out.join("src/asm/parse.c").exists() {
        let _ = std::fs::remove_dir_all(&out);
        run(Command::new("unzip")
            .arg("-q")
            .arg(&zip)
            .arg("-d")
            .arg(&out));
    }
    out
}

fn exomizer_packer(dir: &Path, rel: &Path) -> PathBuf {
    let exe = dir.join("gt2ref-exomizer");
    let shim = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shim");
    let inc = dir.join("include");
    std::fs::create_dir_all(&inc).unwrap();
    for h in ["bme.h", "gt2ref_shim.h", "gt2ref.h", "gt2ref_rename.h"] {
        std::fs::copy(shim.join(h), inc.join(h)).unwrap();
    }
    let src = rel.join("src");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".into());
    let mut c = Command::new(cc);
    c.arg("-O2")
        .arg("-w")
        .arg("-fno-common")
        .arg("-o")
        .arg(&exe);
    c.arg("-I")
        .arg(&inc)
        .arg("-I")
        .arg(&src)
        .arg("-I")
        .arg(src.join("asm"));
    c.arg("-DGT2REF_EXOMIZER_ASM");
    c.arg(format!("-DGT2REF_SRC_DIR=\"{}\"", src.display()));
    let objs = dir.join("gt-objs");
    std::fs::create_dir_all(&objs).unwrap();
    for f in [
        "gt2reloc", "gsong", "gtable", "ginstr", "gorder", "gpattern", "gplay",
    ] {
        let o = objs.join(format!("{f}.o"));
        run(
            Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()))
                .args(["-O2", "-w", "-fno-common", "-c"])
                .arg("-include")
                .arg(inc.join("gt2ref_rename.h"))
                .arg("-I")
                .arg(&inc)
                .arg("-I")
                .arg(&src)
                .arg("-I")
                .arg(src.join("asm"))
                .arg(src.join(format!("{f}.c")))
                .arg("-o")
                .arg(&o),
        );
        c.arg(o);
    }
    for f in std::fs::read_dir(src.join("asm")).unwrap() {
        let p = f.unwrap().path();
        if p.extension().is_some_and(|x| x == "c") {
            c.arg(p);
        }
    }
    for f in ["gt2ref_shim.c", "gt2ref.c", "gt2ref_driver.c"] {
        c.arg(shim.join(f));
    }
    c.arg("-lm");
    run(&mut c);
    exe
}

fn option_sets() -> Vec<(Vec<&'static str>, Options)> {
    let d = Options::default;
    let mut v: Vec<(Vec<&'static str>, Options)> = vec![(vec![], d())];
    for (a, adsr) in [
        ("A=0000", 0),
        ("A=F800", 0xf800),
        ("A=FF00", 0xff00),
        ("A=0A01", 0x0a01),
    ] {
        v.push((
            vec![a],
            Options {
                hr_adsr: adsr,
                ..d()
            },
        ));
    }
    for (s, m) in [("S=0", 0), ("S=2", 2), ("S=4", 4), ("S=8", 8), ("S=16", 16)] {
        v.push((
            vec![s],
            Options {
                multiplier: m,
                ..d()
            },
        ));
    }
    let bits: [(&'static str, u32); 12] = [
        ("P=8", 8),
        ("P=16", 16),
        ("P=32", 32),
        ("P=64", 64),
        ("P=128", 128),
        ("P=256", 256),
        ("P=512", 512),
        ("P=24", 24),
        ("P=136", 136),
        ("P=520", 520),
        ("P=640", 640),
        ("P=760", 760),
    ];
    for (p, b) in bits {
        v.push((
            vec![p],
            Options {
                player_version: b,
                ..d()
            },
        ));
    }
    v.push((
        vec!["O=0"],
        Options {
            optimize_pulse: 0,
            ..d()
        },
    ));
    v.push((
        vec!["R=0"],
        Options {
            optimize_realtime: 0,
            ..d()
        },
    ));
    v.push((
        vec!["W=20"],
        Options {
            player_address: 0x2000,
            ..d()
        },
    ));
    v.push((
        vec!["Z=40"],
        Options {
            zeropage: 0x40,
            ..d()
        },
    ));
    v.push((
        vec!["G=432.5"],
        Options {
            a4_hz: 432.5,
            ..d()
        },
    ));
    v.push((
        vec!["L=D420"],
        Options {
            sid_address: 0xd420,
            ..d()
        },
    ));
    v.push((vec!["N"], Options { ntsc: 1, ..d() }));
    v.push((
        vec!["A=FF00", "P=136", "S=4"],
        Options {
            hr_adsr: 0xff00,
            player_version: 136,
            multiplier: 4,
            ..d()
        },
    ));
    v
}

#[test]
#[ignore = "fetches GoatTracker 2.77; cargo test --release -p gt2-ref --test assembler -- --ignored"]
fn sidous_assembler_packs_as_goattrackers_own() {
    let dir = root().join("build/goattracker2-2.77");
    let rel = release(&dir);
    let vendored = root().join("third_party/goattracker2");
    let mut files = 0;
    for sub in ["src", "examples", "."] {
        for e in std::fs::read_dir(vendored.join(sub)).unwrap() {
            let p = e.unwrap().path();
            if p.is_file() && p.file_name().unwrap() != "VENDORED.md" {
                let name = p.strip_prefix(&vendored).unwrap();
                assert_eq!(
                    std::fs::read(&p).unwrap(),
                    std::fs::read(rel.join(name)).unwrap(),
                    "{}",
                    name.display()
                );
                files += 1;
            }
        }
    }
    let exe = exomizer_packer(&dir, &rel);
    let (mut packs, mut refused) = (0, 0);
    for (name, sng) in examples() {
        for (args, o) in option_sets() {
            let out = dir.join("out.prg");
            let _ = std::fs::remove_file(&out);
            let status = Command::new(&exe)
                .arg(examples_dir().join(&name))
                .arg(&out)
                .args(&args)
                .output()
                .unwrap()
                .status;
            match pack(&sng, &o) {
                Ok(ours) => {
                    assert!(status.success(), "{name} {args:?}: only SIDOU's packed it");
                    let mut prg = ours.load.to_le_bytes().to_vec();
                    prg.extend_from_slice(&ours.code);
                    let theirs = std::fs::read(&out).unwrap();
                    assert!(prg == theirs, "{name} {args:?}: SIDOU's assembler differs");
                    packs += 1;
                }
                Err(_) => {
                    assert!(
                        !status.success(),
                        "{name} {args:?}: only GoatTracker's packed it"
                    );
                    refused += 1;
                }
            }
        }
    }
    eprintln!(
        "{files} vendored files are the release's; {packs} packs identical, {refused} refused by both"
    );
}
