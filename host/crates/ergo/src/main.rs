// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! `cargo run --release -p ergo -- --out build/ergo` (from host/): the ergonomics harness.
//! Never touches a USB or MIDI device: everything runs in fm1-emu.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use ergo::config::Config;
use ergo::run::{default_build_dir, run, Opts};

const USAGE: &str = "usage: ergo [--out DIR] [--build DIR] [--baseline FILE] [--journeys FILE]
            [--readability FILE] [--config FILE] [--only CHECKS] [--knob ID]
            [--capture full --journey ID] [--fail-on-regression] [--print-config]
            [--check-recipes DIR] [--export-recipes DIR [--journeys FILE]]
            [--compare OLD NEW]
  --out DIR        where the report and files go (default ../build/ergo)
  --build DIR      the fm1 build (default $SIDOU_FM1_BUILD, else ../build/fm1)
  --baseline FILE  a previous run's metrics.json: anything worse is a regression
  --compare OLD NEW
                   compare finding IDs in files or run directories; write compare.json next to NEW
  --only CHECKS    comma list of: journeys readability knobs feedback consistency defaults
  --knob ID        only the knob sweeps whose id contains ID (e.g. K1.live)
  --check-recipes DIR
                   run every JSON recipe in DIR in the emulator; exit 1 if one fails
  --export-recipes DIR
                   write the recipes of tables/recipes.txt (or --journeys FILE) as JSON into DIR
  --capture full --journey ID
                   every LCD frame, the audio and a JSON sidecar of one journey (with
                   --journeys DIR, a recipe of that directory's JSON recipes)";

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
    let mut comparison = None;
    while let Some(a) = args.next() {
        let mut val = |what: &str| {
            args.next()
                .unwrap_or_else(|| die(&format!("{what} needs a value")))
        };
        match a.as_str() {
            "--out" => o.out = PathBuf::from(val("--out")),
            "--build" => o.build = PathBuf::from(val("--build")),
            "--baseline" => o.baseline = Some(PathBuf::from(val("--baseline"))),
            "--compare" => {
                comparison = Some((
                    PathBuf::from(val("--compare OLD")),
                    PathBuf::from(val("--compare NEW")),
                ))
            }
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
    if let Some((old, new)) = comparison {
        match compare(&old, &new) {
            Ok(summary) => print!("{summary}"),
            Err(e) => die(&e),
        }
        return;
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

fn compare(old: &Path, new: &Path) -> Result<String, String> {
    let file = |p: &Path| {
        if p.is_dir() {
            p.join("findings.json")
        } else {
            p.to_path_buf()
        }
    };
    let read = |p: &Path| -> Result<BTreeMap<String, Value>, String> {
        let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        let value: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        let findings = value["findings"]
            .as_array()
            .ok_or_else(|| format!("{}: missing findings array", p.display()))?;
        let mut map = BTreeMap::new();
        for f in findings {
            let id = f["id"].as_str().ok_or("finding missing string id")?;
            if !f["title"].is_string() {
                return Err(format!("{id}: missing string title"));
            }
            if map.insert(id.to_string(), f.clone()).is_some() {
                return Err(format!("duplicate finding id {id}"));
            }
        }
        Ok(map)
    };
    let new = file(new);
    let before = read(&file(old))?;
    let after = read(&new)?;
    let gone: Vec<_> = before
        .iter()
        .filter(|(id, _)| !after.contains_key(*id))
        .map(|(_, f)| f.clone())
        .collect();
    let fresh: Vec<_> = after
        .iter()
        .filter(|(id, _)| !before.contains_key(*id))
        .map(|(_, f)| f.clone())
        .collect();
    let still: Vec<_> = after
        .iter()
        .filter(|(id, _)| before.contains_key(*id))
        .map(|(_, f)| f.clone())
        .collect();
    let mut summary = format!(
        "gone {}\nnew {}\nstill {}\n",
        gone.len(),
        fresh.len(),
        still.len()
    );
    for (name, list) in [("gone", &gone), ("new", &fresh), ("still", &still)] {
        summary.push_str(&format!("\n{name}\n"));
        for f in list {
            summary.push_str(&format!(
                "  {}: {}\n",
                f["id"].as_str().unwrap(),
                f["title"].as_str().unwrap()
            ));
        }
    }
    let output = json!({"gone": gone, "new": fresh, "still": still});
    std::fs::write(
        new.with_file_name("compare.json"),
        serde_json::to_string_pretty(&output).unwrap() + "\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_finding_files_and_run_directories() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../build/scratch")
            .join(format!("compare-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("old.json");
        let new = dir.join("findings.json");
        std::fs::write(
            &old,
            r#"{"findings":[{"id":"A","title":"gone"},{"id":"C","title":"old title"}]}"#,
        )
        .unwrap();
        std::fs::write(
            &new,
            r#"{"findings":[{"id":"B","title":"new"},{"id":"C","title":"still"}]}"#,
        )
        .unwrap();
        let result = compare(&old, &new).unwrap();
        assert_eq!(
            result,
            "gone 1\nnew 1\nstill 1\n\ngone\n  A: gone\n\nnew\n  B: new\n\nstill\n  C: still\n"
        );
        assert_eq!(compare(&old, &dir).unwrap(), result);
        let value: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("compare.json")).unwrap())
                .unwrap();
        assert_eq!(value["gone"][0]["id"], "A");
        assert_eq!(value["new"][0]["id"], "B");
        assert_eq!(value["still"][0]["title"], "still");
        std::fs::remove_file(old).unwrap();
        std::fs::remove_file(new).unwrap();
        std::fs::remove_file(dir.join("compare.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
