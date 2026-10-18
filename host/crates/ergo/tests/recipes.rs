// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The example recipes in docs/recipes: they parse, lint clean, are what the table exports, and
//! (with the fm1 build, SIDOU_FM1_BUILD) run in the emulator and meet their expectations.

use std::path::{Path, PathBuf};

use ergo::config::Config;
use ergo::recipe::{export, lint, load_dir, parse_recipe, validate, DEFAULT_RECIPES};

fn recipes_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/recipes")
}

#[test]
fn the_committed_recipes_are_valid_and_current() {
    let loaded = load_dir(&recipes_dir()).unwrap();
    assert!(loaded.len() >= 3, "{} recipes", loaded.len());
    let journeys: Vec<_> = loaded
        .iter()
        .map(|(p, v)| {
            (
                p,
                parse_recipe(v).unwrap_or_else(|e| panic!("{}: {e}", p.display())),
            )
        })
        .collect();
    let ids: Vec<String> = journeys.iter().map(|(_, j)| j.id.clone()).collect();
    for (p, j) in &journeys {
        assert_eq!(p.file_stem().unwrap().to_string_lossy(), j.id);
        let problems = lint(j, &ids);
        assert!(problems.is_empty(), "{}: {problems:?}", j.id);
    }
    // What `ergo --export-recipes` writes now is what is committed.
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ergo-recipes-export");
    let names = export(DEFAULT_RECIPES, &dir).unwrap();
    assert_eq!(names.len(), loaded.len());
    for n in names {
        let want = std::fs::read_to_string(dir.join(&n)).unwrap();
        let have = std::fs::read_to_string(recipes_dir().join(&n)).unwrap_or_default();
        assert!(want == have, "docs/recipes/{n} is stale: run `cargo run --release -p ergo -- --export-recipes ../docs/recipes`");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_recipes_run_in_the_emulator_and_meet_their_expectations() {
    let Some(build) = ergo::common::build_dir() else {
        ergo::common::skip_message("the ergo recipe check");
        return;
    };
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ergo-recipes-run");
    let reports = validate(&recipes_dir(), &build, &Config::default(), &out).unwrap();
    for r in &reports {
        assert!(r.problems.is_empty(), "{}: {:?}", r.id, r.problems);
    }
    std::fs::remove_dir_all(&out).ok();
}
