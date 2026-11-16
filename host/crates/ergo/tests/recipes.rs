// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
    let reports = validate(
        &recipes_dir(),
        &build,
        &Config::default(),
        &out,
        ergo::Shard::ALL,
    )
    .unwrap();
    for r in &reports {
        assert!(r.problems.is_empty(), "{}: {:?}", r.id, r.problems);
    }
    std::fs::remove_dir_all(&out).ok();
}

#[test]
fn the_text_song_the_videos_store_is_the_docs_example() {
    let doc = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/prompt-to-patch-song.md"),
    )
    .unwrap();
    let start = doc
        .find("{\"format\": \"sidou-song/1\", \"name\": \"THREE PARTS\"")
        .unwrap();
    let block = &doc[start..start + doc[start..].find("```").unwrap()];
    let file = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/text-songs/three-parts.json"),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(block).unwrap(),
        serde_json::from_str::<serde_json::Value>(&file).unwrap()
    );
    let sng = ergo::device::showcase_song("three-parts").unwrap();
    assert!(sng.starts_with(b"GTS5"));
}

#[test]
fn a_recipes_art_pack_is_in_the_store_as_the_library_sends_it() {
    let Some(build) = ergo::common::build_dir() else {
        ergo::common::skip_message("the recipes' art pack");
        return;
    };
    let d = ergo::device::Dev::power_on_with(&build, &[], &[], &["red-moon-rite".to_string()]);
    let (kind, pack) = ergo::common::stored_file(&d.m.soc.flash.mem, sidou_sysex::art::NAME)
        .expect("the art pack in the store");
    assert_eq!(u32::from(kind), sidou_sys::STORE_FILE_ART);
    assert_eq!(pack, ergo::common::shipped_art());
}
