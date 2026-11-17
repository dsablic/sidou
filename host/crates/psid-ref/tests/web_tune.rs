// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Write as _;
use std::path::PathBuf;

use psid_ref::{sidou, Call};
use sha2::{Digest, Sha256};

const SECONDS: u32 = 10;
const CALLS_A_DIGEST: usize = 50;
const DIGEST_HEX: usize = 16;
const TUNES: [(&str, u32); 3] = [("sidou-loop", 1), ("2xtest", 1), ("dojo", 2)];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn call_bytes(c: &Call, out: &mut Vec<u8>) {
    out.extend_from_slice(&c.start.to_le_bytes());
    for w in &c.writes {
        out.extend_from_slice(&w.cycle.to_le_bytes());
        out.extend_from_slice(&[w.chip, w.reg, w.value]);
    }
}

fn digests(calls: &[Call]) -> Vec<String> {
    calls
        .chunks(CALLS_A_DIGEST)
        .map(|chunk| {
            let mut bytes = Vec::new();
            chunk.iter().for_each(|c| call_bytes(c, &mut bytes));
            Sha256::digest(&bytes).iter().fold(String::new(), |mut s, x| {
                let _ = write!(s, "{x:02x}");
                s
            })[..DIGEST_HEX]
                .to_string()
        })
        .collect()
}

#[test]
fn the_site_captures_the_writes_sidous_psid_environment_makes() {
    let mut json = format!(
        "{{\n  \"seconds\": {SECONDS},\n  \"callsADigest\": {CALLS_A_DIGEST},\n  \"tunes\": [\n"
    );
    for (k, (name, song)) in TUNES.iter().enumerate() {
        let file = std::fs::read(root().join(format!("assets/starter/{name}.sid"))).unwrap();
        let run = sidou::run(&file, *song, SECONDS).unwrap();
        let writes: usize = run.calls.iter().map(|c| c.writes.len()).sum();
        let list = digests(&run.calls)
            .iter()
            .map(|d| format!("\"{d}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let comma = if k + 1 < TUNES.len() { "," } else { "" };
        let _ = writeln!(
            json,
            "    {{ \"file\": \"assets/starter/{name}.sid\", \"song\": {song}, \"calls\": {}, \"writes\": {writes}, \"digests\": [{list}] }}{comma}",
            run.calls.len()
        );
    }
    json.push_str("  ]\n}\n");
    let path = root().join("web/tune-fixtures.json");
    if std::env::var_os("SIDOU_WEB_TUNE_FIXTURE_UPDATE").is_some() {
        std::fs::write(&path, &json).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == json,
        "web/tune-fixtures.json is not what this build makes: rerun with SIDOU_WEB_TUNE_FIXTURE_UPDATE=1"
    );
}
