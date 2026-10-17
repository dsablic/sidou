// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The pixel scene in fm1-emu (`bench_scene_main.c`, `bench/workload_scene.h`): what the tile
//! compositor (app/scene_px.h) costs on the device in a frame like RED MOON RITE's (13
//! slots), a tile, a cell sent, a run, a scan and a frame's send, with the log checked against
//! the host build's (the same runs, and every tile's hash the same: the device composites the
//! host's pixels). Instructions are a lower bound on cycles.
//!
//! The bounds are about 10 % over what M2f Task 3 measured; RED MOON RITE's own frames (M2f
//! Task 4) are measured with the view. Measured (instructions), against the compositor as
//! first written, before the run checkpoints (app/scene_art.h), the run-at-a-time sprite
//! blend, the list of slots in use and the cap on a run's painted cells:
//!
//! | | Before | Now |
//! |---|---|---|
//! | A tile, average / worst | 4.8k / 8.7k | 1,959 / 4,826 |
//! | A cell sent (alone), average / worst | ~5.6k | 2,779 / 5,646 |
//! | A run, worst (30 cells before; 8 now) | 230k | 35.5k |
//! | The first frame's send, 900 cells | 5.00M | 2.49M |
//! | The fighters' move: scan of 132 + send of 123 | 1.66M | 0.89M |
//! | The sky's change: scan of 480 + send of 247 | 4.44M | 1.99M |
//! | A scan, per candidate | | 2,829 (42 a 120k pass) |
//! | A slice of 16 candidates, worst | | 51.9k |
//! | The refresh sweep's two tiles, marked and sent | | 4.4k |

mod common;

use sidou_sys::{SCENE_PX_RUN_CELLS, SCENE_PX_SCAN_SLICE};

/// A tile composited (scene_px_tile): on average, and the worst of the 900.
const TILE_AVERAGE: f64 = 2_150.0;
const TILE_WORST: u64 = 5_300;
/// A cell rendered alone, as a run of one: the painter (the tile and its hash), the panel's
/// pixels and the glyph pass. The worst of the 900.
const CELL_WORST: u64 = 6_200;
/// grid_next_run finding a run, and finding none.
const NEXT_RUN: u64 = 1_400;
/// A run: grid_next_run and grid_render_run, at most SCENE_PX_RUN_CELLS cells of the scene.
const RUN_WORST: u64 = SCENE_PX_RUN_CELLS as u64 * CELL_WORST + NEXT_RUN;
/// What a ui_poll that renders two runs (platform/fm1/ui.c: one before the send, one while it
/// transfers) spends besides them: its own bookkeeping and the window it starts (lcd_send).
/// Allowed, not measured here: the view's gate measures the whole ui_poll (M2f Task 4).
const POLL_REST: u64 = 10_000;
/// A song's analysis or derivation slice, which a main-loop pass may run beside ui_poll
/// (core/player/song_ctl.h SONG_SLICE_INSTRUCTIONS).
const SONG_SLICE: u64 = sidou_sys::SONG_SLICE_INSTRUCTIONS as u64;
// Two runs of the scene's worst cells fit a ui_poll's allowance, with a song's slice in the
// pass: SCENE_PX_RUN_CELLS and CELL_WORST cannot grow past it.
const _: () = assert!(2 * RUN_WORST + POLL_REST + SONG_SLICE <= common::POLL_INSTRUCTIONS);
/// A scan's cost per candidate composited (the tile and its hash).
const SCAN_PER_TILE: f64 = 3_100.0;
/// A slice of the scan (SCENE_PX_SCAN_SLICE candidates): a tile at TILE_WORST each, with its
/// hash, and the walk over the candidate bits round the screen.
const SCAN_HASH: u64 = 500;
const SCAN_WALK: u64 = 8_000;
const SLICE_WORST: u64 = SCENE_PX_SCAN_SLICE as u64 * (TILE_WORST + SCAN_HASH) + SCAN_WALK;
// A pass that scans a slice fits the allowance with the rest of a ui_poll and a song's slice.
const _: () = assert!(SLICE_WORST + POLL_REST + SONG_SLICE <= common::POLL_INSTRUCTIONS);

/// A scan: the candidates before it, those composited, the cells marked after it, and its
/// cost.
type Scan = (u32, u32, u32, u64);

/// The log's sections and the measured steps, read in order.
struct Log<'a> {
    log: &'a [i16],
    at: usize,
    phases: &'a [u64],
    step: usize,
}

impl Log<'_> {
    fn word(&mut self) -> u32 {
        let v = self.log[self.at] as u16;
        self.at += 1;
        u32::from(v)
    }

    /// The next measured step's instructions (0 while only counting the steps).
    fn step(&mut self) -> u64 {
        let v = self.phases.get(2 * self.step).copied().unwrap_or(0);
        self.step += 1;
        v
    }

    /// A send: each run's cost (grid_next_run and grid_render_run), the empty grid_next_run,
    /// the cells sent and the windows.
    fn send(&mut self) -> Send {
        let runs = self.word();
        let mut s = Send::default();
        for _ in 0..runs {
            let _at = self.word();
            s.cells += self.word();
            s.runs.push(self.step());
        }
        s.empty = self.step();
        s
    }

    fn scan(&mut self) -> Scan {
        let (ncand, scanned, marked) = (self.word(), self.word(), self.word());
        (ncand, scanned, marked, self.step())
    }
}

#[derive(Default)]
struct Send {
    runs: Vec<u64>,
    empty: u64,
    cells: u32,
}

impl Send {
    fn cost(&self) -> u64 {
        self.runs.iter().sum::<u64>() + self.empty
    }

    /// Its SPI time: each cell's 128 bytes, and each window's 11 bytes of commands
    /// (platform/fm1/lcd.c: CASET and RASET with 4 bytes each, RAMWR).
    fn spi_ms(&self) -> f64 {
        spi_ms(self.cells, self.runs.len() as u32)
    }
}

fn spi_ms(cells: u32, windows: u32) -> f64 {
    f64::from(cells * 128 + windows * 11) * 8.0 / SPI_HZ * 1e3
}

/// platform/fm1/lcd.c: SPI1 at 60 MHz / (4 + 1).
const SPI_HZ: f64 = 12e6;
/// docs/art/README.md: at most about 20 ms of SPI per 40 ms frame.
const SPI_BUDGET_MS: f64 = 20.0;

struct Measured {
    setup: u64,
    tiles: Vec<u64>,
    cells: Vec<u64>,
    first: Send,
    unchanged: Scan,
    frames: Vec<(&'static str, u64, Scan, Send)>,
    refresh: (u64, Send),
    sliced: (u32, Vec<u64>, Send),
    steps: usize,
}

fn parse(log: &[i16], phases: &[u64]) -> Measured {
    let mut l = Log {
        log,
        at: 0,
        phases,
        step: 0,
    };
    let setup = l.step();
    let tiles = (0..900).map(|_| l.step()).collect();
    let cells = (0..900).map(|_| l.step()).collect();
    let first = l.send();
    let unchanged = l.scan();
    let frames = ["fighters_move", "sky_change", "flames_tick"]
        .into_iter()
        .map(|name| {
            let set = l.step();
            let scan = l.scan();
            (name, set, scan, l.send())
        })
        .collect();
    let refresh = (l.step(), l.send());
    let ncand = l.word();
    let slices = (0..l.word()).map(|_| l.step()).collect();
    let sliced = (ncand, slices, l.send());
    assert_eq!(
        l.at + 1800,
        log.len(),
        "the log's sections, then the hashes"
    );
    Measured {
        setup,
        tiles,
        cells,
        first,
        unchanged,
        frames,
        refresh,
        sliced,
        steps: l.step,
    }
}

#[test]
fn the_pixel_scene_fits_its_budgets_and_matches_host() {
    let Some(build) = common::build_dir() else {
        common::skip_message("the pixel scene's emulator gate");
        return;
    };
    let host = emu_gate::render_host_scene();
    let steps = parse(&host, &[]).steps;
    let r = common::run(&build, "sid_bench_scene", 2 * steps);
    let log = &r.samples;
    assert_eq!(log.len(), host.len(), "the log's length");
    if let Some(i) = log.iter().zip(&host).position(|(a, b)| a != b) {
        panic!(
            "the target's log differs from the host's at {i} (of {}): target {} host {}",
            log.len(),
            log[i],
            host[i]
        );
    }
    let m = parse(log, &r.phases);

    let tile_average = m.tiles.iter().sum::<u64>() as f64 / 900.0;
    let tile_worst = *m.tiles.iter().max().unwrap();
    let cell_average = m.cells.iter().sum::<u64>() as f64 / 900.0;
    let cell_worst = *m.cells.iter().max().unwrap();
    let mut runs: Vec<u64> = m.first.runs.clone();
    let mut empties = vec![m.first.empty];
    let mut scan_tiles = 0u64;
    let mut scan_cost = 0u64;
    let mut frames = serde_json::Map::new();
    for (name, set, (ncand, scanned, marked, scan), send) in &m.frames {
        scan_tiles += u64::from(*scanned);
        scan_cost += scan;
        frames.insert(
            name.to_string(),
            serde_json::json!({
                "set": set, "candidates": ncand, "scanned": scanned, "marked": marked,
                "scan": scan, "runs": send.runs.len(), "cells_sent": send.cells,
                "send": send.cost(), "send_spi_ms": send.spi_ms(),
                "total": set + scan + send.cost(),
            }),
        );
        assert_eq!(
            *marked, send.cells,
            "{name}: the cells marked are the cells sent"
        );
        runs.extend(&send.runs);
        empties.push(send.empty);
    }
    let (refresh, refresh_send) = &m.refresh;
    let (slice_cand, slices, slice_send) = &m.sliced;
    for s in [refresh_send, slice_send] {
        runs.extend(&s.runs);
        empties.push(s.empty);
    }
    let scan_per_tile = scan_cost as f64 / scan_tiles as f64;
    let run_worst = *runs.iter().max().unwrap();
    let empty_worst = *empties.iter().max().unwrap();
    let slice_worst = *slices.iter().max().unwrap();
    // The worst frame SCENE can send: SCENE_VIEW_FRAME_CELLS cells, each a window of its own.
    let cells = sidou_sys::SCENE_VIEW_FRAME_CELLS;
    let worst_spi = spi_ms(cells, cells);
    let report = serde_json::json!({
        "metric": "instructions (lower bound on cycles; assumes 1 IPC)",
        "slots": 13,
        "set_up": m.setup,
        "tile_average": tile_average,
        "tile_worst": tile_worst,
        "cell_average": cell_average,
        "cell_worst": cell_worst,
        "run_cells_at_most": SCENE_PX_RUN_CELLS,
        "run_worst": run_worst,
        "two_runs_worst": 2 * run_worst,
        "next_run_empty_worst": empty_worst,
        "first_frame": {"runs": m.first.runs.len(), "cells": m.first.cells,
                        "send": m.first.cost(),
                        "per_cell": m.first.cost() as f64 / f64::from(m.first.cells)},
        "unchanged_scan": {"candidates": m.unchanged.0, "scanned": m.unchanged.1,
                           "marked": m.unchanged.2, "scan": m.unchanged.3},
        "frames": frames,
        "refresh_two_tiles": {"cells": refresh_send.cells,
                              "total": refresh + refresh_send.cost()},
        "scan_per_tile": scan_per_tile,
        "tiles_a_120k_pass": (common::POLL_INSTRUCTIONS as f64 / scan_per_tile).floor(),
        "sliced_scan": {"candidates": slice_cand, "slice": SCENE_PX_SCAN_SLICE,
                        "slices": slices.len(), "slice_worst": slice_worst,
                        "total": slices.iter().sum::<u64>()},
        "worst_frame_spi_ms": worst_spi,
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    let out = build.join("scene_budget.json");
    std::fs::write(&out, report.to_string())
        .unwrap_or_else(|e| panic!("write report {}: {e}", out.display()));

    assert_eq!(m.first.cells, 900, "the first frame sends the screen");
    assert_eq!(m.unchanged.2, 0, "an unchanged scan marks nothing");
    assert_eq!(refresh_send.cells, 2, "the sweep's two tiles");
    assert_eq!(
        slices.len(),
        (*slice_cand as usize).div_ceil(SCENE_PX_SCAN_SLICE as usize),
        "the slices"
    );
    assert!(
        tile_average <= TILE_AVERAGE,
        "a tile costs {tile_average:.0} on average, over {TILE_AVERAGE}"
    );
    assert!(
        tile_worst <= TILE_WORST,
        "a tile costs {tile_worst}, over {TILE_WORST}"
    );
    assert!(
        cell_worst <= CELL_WORST,
        "a cell costs {cell_worst}, over {CELL_WORST}"
    );
    assert!(
        empty_worst <= NEXT_RUN,
        "grid_next_run finding nothing costs {empty_worst}, over {NEXT_RUN}"
    );
    assert!(
        run_worst <= RUN_WORST,
        "a run costs {run_worst}, over {RUN_WORST}"
    );
    assert!(
        scan_per_tile <= SCAN_PER_TILE,
        "a scan costs {scan_per_tile:.0} a tile, over {SCAN_PER_TILE}"
    );
    assert!(
        slice_worst <= SLICE_WORST,
        "a slice of the scan costs {slice_worst}, over {SLICE_WORST}"
    );
    assert!(
        worst_spi <= SPI_BUDGET_MS,
        "{cells} cells in as many windows take {worst_spi:.2} ms of SPI"
    );
}
