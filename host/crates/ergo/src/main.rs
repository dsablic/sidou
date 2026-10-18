// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! `cargo run --release -p ergo -- --out build/ergo` (from host/): the ergonomics harness.
//! Never touches a USB or MIDI device: everything runs in fm1-emu.

use std::path::PathBuf;

use ergo::config::Config;
use ergo::run::{default_build_dir, run, Opts};

const USAGE: &str = "usage: ergo [--out DIR] [--build DIR] [--baseline FILE] [--journeys FILE]
            [--readability FILE] [--config FILE] [--only CHECKS] [--knob ID]
            [--capture full --journey ID] [--fail-on-regression] [--print-config]
            [--check-recipes DIR] [--export-recipes DIR [--journeys FILE]]
  --out DIR        where the report and files go (default ../build/ergo)
  --build DIR      the fm1 build (default $SIDOU_FM1_BUILD, else ../build/fm1)
  --baseline FILE  a previous run's metrics.json: anything worse is a regression
  --only CHECKS    comma list of: journeys readability knobs feedback consistency defaults
  --knob ID        only the knob sweeps whose id contains ID (e.g. K1.live)
  --check-recipes DIR
                   run every JSON recipe in DIR in the emulator; exit 1 if one fails
  --export-recipes DIR
                   write the recipes of tables/recipes.txt (or --journeys FILE) as JSON into DIR
  --capture full --journey ID
                   every LCD frame, the audio and a JSON sidecar of one journey";

fn main() {
    let mut args = std::env::args().skip(1);
    let mut o = Opts {
        out: PathBuf::from("../build/ergo"),
        build: std::env::var_os("SIDOU_FM1_BUILD")
            .filter(|d| !d.is_empty())
            .map_or_else(default_build_dir, PathBuf::from),
        baseline: None,
        journeys_file: None,
        readability_file: None,
        config_file: None,
        only: None,
        capture: None,
        fail_on_regression: false,
        knob_filter: None,
    };
    let mut capture_mode = None;
    let mut check_recipes: Option<PathBuf> = None;
    let mut export_recipes: Option<PathBuf> = None;
    let mut journey = None;
    while let Some(a) = args.next() {
        let mut val = |what: &str| {
            args.next()
                .unwrap_or_else(|| die(&format!("{what} needs a value")))
        };
        match a.as_str() {
            "--out" => o.out = PathBuf::from(val("--out")),
            "--build" => o.build = PathBuf::from(val("--build")),
            "--baseline" => o.baseline = Some(PathBuf::from(val("--baseline"))),
            "--journeys" => o.journeys_file = Some(PathBuf::from(val("--journeys"))),
            "--readability" => o.readability_file = Some(PathBuf::from(val("--readability"))),
            "--config" => o.config_file = Some(PathBuf::from(val("--config"))),
            "--only" => o.only = Some(val("--only").split(',').map(str::to_string).collect()),
            "--knob" => o.knob_filter = Some(val("--knob")),
            "--capture" => capture_mode = Some(val("--capture")),
            "--journey" => journey = Some(val("--journey")),
            "--check-recipes" => check_recipes = Some(PathBuf::from(val("--check-recipes"))),
            "--export-recipes" => export_recipes = Some(PathBuf::from(val("--export-recipes"))),
            "--fail-on-regression" => o.fail_on_regression = true,
            "--print-config" => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&Config::default().to_json()).unwrap()
                );
                return;
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            x => die(&format!("unknown argument {x}\n{USAGE}")),
        }
    }
    if let Some(dir) = export_recipes {
        let table = match &o.journeys_file {
            Some(p) => {
                std::fs::read_to_string(p).unwrap_or_else(|e| die(&format!("{}: {e}", p.display())))
            }
            None => ergo::recipe::DEFAULT_RECIPES.to_string(),
        };
        match ergo::recipe::export(&table, &dir) {
            Ok(n) => println!(
                "wrote {} recipe(s) to {}: {}",
                n.len(),
                dir.display(),
                n.join(", ")
            ),
            Err(e) => die(&e),
        }
        return;
    }
    if let Some(dir) = check_recipes {
        if !o.build.join("platform/fm1/sidou_app.bin").exists() {
            die(&format!("no fm1 build at {}", o.build.display()));
        }
        let cfg = Config::default();
        match ergo::recipe::validate(&dir, &o.build, &cfg, &o.out) {
            Ok(reports) => {
                let mut failed = 0;
                for r in &reports {
                    if r.problems.is_empty() {
                        println!("ok    {}", r.id);
                    } else {
                        failed += 1;
                        println!("FAIL  {}", r.id);
                        for p in &r.problems {
                            println!("      {p}");
                        }
                    }
                }
                println!("{} recipe(s), {failed} failed", reports.len());
                std::process::exit(i32::from(failed > 0));
            }
            Err(e) => die(&e),
        }
    }
    if let Some(m) = capture_mode {
        if m != "full" {
            die("--capture takes `full`");
        }
        o.capture = Some(journey.unwrap_or_else(|| die("--capture full needs --journey ID")));
    }
    if !o.build.join("platform/fm1/sidou_app.bin").exists() {
        die(&format!(
            "no fm1 build at {}: build the fm1 preset (BUILDING.md) or set SIDOU_FM1_BUILD",
            o.build.display()
        ));
    }
    match run(&o) {
        Ok(code) => std::process::exit(code),
        Err(e) => die(&e),
    }
}

fn die(msg: &str) -> ! {
    eprintln!("ergo: {msg}");
    std::process::exit(2)
}
