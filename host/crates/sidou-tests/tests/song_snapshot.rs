// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{fs, path::Path, process::Command};

#[test]
fn the_device_song_snapshot_keeps_wide_values_and_raw_expert_defaults() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let main = fs::read_to_string(root.join("platform/fm1/main.c")).unwrap();
    let start = main
        .find("typedef struct {\n    song_chip_setup chip;")
        .unwrap();
    let end = main[start..]
        .find("__attribute__((noinline, unused)) static void song_collect")
        .unwrap()
        + start;
    let source = format!(
        r#"
#include <assert.h>
#include <string.h>
#include "midi_route.h"
#include "song_io.h"
{}
int main(void) {{
    song_save_data data;
    uint16_t before[LIVE_PARAM_COUNT], after[LIVE_PARAM_COUNT];
    for (uint32_t trial = 0; trial < 128u; trial++) {{
        memset(&data, 0xa5, sizeof data);
        memset(after, 0, sizeof after);
        for (uint32_t p = 0; p < LIVE_PARAM_COUNT; p++) {{
            before[p] = (uint16_t)((p * 13u + trial) % 128u);
        }}
        before[LIVE_PARAM_CUTOFF] = (uint16_t)(trial * 2047u / 127u);
        before[LIVE_PARAM_PULSE_WIDTH] = (uint16_t)(trial * 4095u / 127u);
        before[LIVE_PARAM_VOLUME] = 0u;
        for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {{
            before[LIVE_PARAM_EXPERT + f] = f % 3u == 0u ? SID_EXPERT_DEFAULT : trial;
        }}
        song_save_params(&data, before);
        song_saved_params(&data, after);
        assert(memcmp(before, after, sizeof before) == 0);
    }}
    return 0;
}}
"#,
        &main[start..end]
    );
    let scratch = root.join("build/scratch");
    fs::create_dir_all(&scratch).unwrap();
    let file = scratch.join(format!("schema5-song-snapshot-{}.c", std::process::id()));
    let exe = file.with_extension("test");
    fs::write(&file, source).unwrap();
    let build = Command::new("cc")
        .args(["-std=c11", "-Werror", "-I"])
        .arg(root.join("app"))
        .arg("-I")
        .arg(root.join("core"))
        .arg(&file)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    let run = build
        .status
        .success()
        .then(|| Command::new(&exe).output().unwrap());
    fs::remove_file(&file).unwrap();
    if exe.exists() {
        fs::remove_file(&exe).unwrap();
    }
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = run.unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
