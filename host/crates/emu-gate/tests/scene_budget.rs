// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use sidou_sys::{SCENE_PX_RUN_CELLS, SCENE_PX_SCAN_SLICE};

const TILE_AVERAGE: f64 = 2_150.0;
const TILE_WORST: u64 = 5_300;
const CELL_WORST: u64 = 6_200;
const NEXT_RUN: u64 = 1_400;
const RUN_WORST: u64 = SCENE_PX_RUN_CELLS as u64 * CELL_WORST + NEXT_RUN;
const POLL_REST: u64 = 10_000;
const SONG_SLICE: u64 = sidou_sys::SONG_SLICE_INSTRUCTIONS as u64;
const _: () = assert!(2 * RUN_WORST + POLL_REST + SONG_SLICE <= common::POLL_INSTRUCTIONS);
const SCAN_PER_TILE: f64 = 3_100.0;
const SCAN_HASH: u64 = 500;
const SCAN_WALK: u64 = 8_000;
const SLICE_WORST: u64 = SCENE_PX_SCAN_SLICE as u64 * (TILE_WORST + SCAN_HASH) + SCAN_WALK;
const _: () = assert!(SLICE_WORST + POLL_REST + SONG_SLICE <= common::POLL_INSTRUCTIONS);

type Scan = (u32, u32, u32, u64);

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

    fn step(&mut self) -> u64 {
        let v = self.phases.get(2 * self.step).copied().unwrap_or(0);
        self.step += 1;
        v
    }

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

    fn spi_ms(&self) -> f64 {
        spi_ms(self.cells, self.runs.len() as u32)
    }
}

fn spi_ms(cells: u32, windows: u32) -> f64 {
    f64::from(cells * CELL_BYTES + windows * WINDOW_COMMAND_BYTES) * 8.0 / SPI_HZ * 1e3
}

const SPI_HZ: f64 = 12e6;
const CELL_BYTES: u32 = 128;
const WINDOW_COMMAND_BYTES: u32 = 11;
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
