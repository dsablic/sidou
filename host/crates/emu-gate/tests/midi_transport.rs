// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use fm1_core::Bus;

use common::usb_host::UsbHost;
use common::{firmware, Firmware, Machine};
use sidou_sys::ilp32;
use sidou_tests::song::Song;
use sidou_tests::song_embed;
use sidou_tests::transport::Transport;

const BPM: f64 = 120.0;
const CYCLES_PER_SAMPLE: f64 = 240e6 / (24e6 / 544.0);
const FS: f64 = 24e6 / 544.0;

fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, fw);
    m.run_ms(300);
    m
}

fn booted_on_usb(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    m.usb = Some(UsbHost::plug(&mut m.soc));
    common::run_past_boot(&mut m, fw);
    m.usb.as_mut().unwrap().start();
    for _ in 0..2_000 {
        if UsbHost::configured(&m.soc) {
            break;
        }
        m.usb_frames(1);
    }
    let usb = fw.symbol("usb");
    assert_eq!(m.soc.read8(usb + 1), 1, "not configured");
    m.usb_frames(300);
    m
}

fn usb_midi(m: &mut Machine, bytes: &[u8]) {
    UsbHost::send(&mut m.soc, bytes);
}

fn midi(m: &mut Machine, bytes: &[u8]) {
    if m.usb.is_some() {
        usb_midi(m, bytes);
    } else {
        m.midi(bytes);
    }
}

fn row_calls() -> u32 {
    let mut song = Song::device();
    song.read(&song_embed::sng(), 4).expect("reads");
    let mut t = Transport::new(&mut song);
    assert!(t.play(0, 0));
    assert!(t.run_until(400, |t| t.calls() > 0));
    let tempo = t.sc.player.chn[0].tempo;
    assert!(tempo >= 2, "no funktempo in the first song");
    u32::from(tempo) + 1
}

fn song_field(m: &mut Machine, fw: &Firmware, offset: usize) -> u32 {
    m.soc.read32(fw.symbol("song_transport") + offset as u32)
}

struct Run {
    clocks: Vec<u64>,
    calls: Vec<(u32, u32)>,
}

fn run_clocked(
    m: &mut Machine,
    fw: &Firmware,
    ms: u64,
    mut extra: impl FnMut(&mut Machine, u64) -> bool,
) -> Run {
    let per = 60.0 / BPM / 24.0 * m.soc.periph.hz as f64;
    let start = m.soc.periph.cycles;
    let mut next = start as f64;
    let mut run = Run {
        clocks: Vec::new(),
        calls: Vec::new(),
    };
    let mut last = song_field(m, fw, ilp32::song_ctl::calls);
    for t in 0..ms {
        while (next as u64) <= m.soc.periph.cycles {
            midi(m, &[0xF8]);
            run.clocks.push(m.soc.periph.cycles);
            next += per;
        }
        extra(m, t);
        if m.usb.is_some() {
            m.usb_frames(1);
        } else {
            m.run_ms(1);
        }
        let calls = song_field(m, fw, ilp32::song_ctl::calls);
        if calls != last {
            let s = song_field(m, fw, ilp32::song_ctl::call_sample);
            run.calls.push((calls - 1, s));
            last = calls;
        }
    }
    let _ = start;
    run
}

fn row_deviation(run: &Run, row: u32, clock0: usize, first_row: u32) -> (f64, usize) {
    let mut errs = Vec::new();
    for &(call, sample) in &run.calls {
        if call % row != 0 || call / row < first_row {
            continue;
        }
        let i = clock0 + (call / row * 6) as usize;
        if i >= run.clocks.len() {
            continue;
        }
        let at = run.clocks[i] as f64 / CYCLES_PER_SAMPLE;
        errs.push(f64::from(sample) - at);
    }
    let mean = errs.iter().sum::<f64>() / errs.len() as f64;
    let worst = errs.iter().fold(0.0f64, |a, &e| a.max((e - mean).abs()));
    (worst / FS * 1e3, errs.len())
}

#[test]
fn rows_land_on_a_120_bpm_clock_over_trs() {
    let Some(fw) = firmware("the MIDI transport test") else {
        return;
    };
    let row = row_calls();
    let mut m = booted(&fw);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let beat_ms = (60_000.0 / BPM) as u64;
    let mut start_clock = None;
    let run = run_clocked(&mut m, &fw, beat_ms * 10, |m, t| {
        if t == 2 * beat_ms - 5 {
            m.midi(&[0xFA]);
            start_clock = Some(());
            return true;
        }
        false
    });
    assert!(start_clock.is_some());
    let clock0 = 48;
    assert!(run.calls.len() > 100, "{} calls", run.calls.len());
    assert_eq!(run.calls[0].0, 0, "from the song's start");
    let (worst, n) = row_deviation(&run, row, clock0, 2 * 4);
    let pass = m.passes.iter().copied().max().unwrap_or(0);
    println!(
        "{n} rows from two beats after the start: worst 16th phase error {worst:.3} ms; busiest pass {pass}"
    );
    assert!(n > 20);
    assert!(worst <= 1.0, "a row {worst:.3} ms off its 16th");
    assert!(pass <= common::POLL_INSTRUCTIONS);
}

#[test]
fn stop_song_position_and_continue_over_trs() {
    let Some(fw) = firmware("the MIDI transport test") else {
        return;
    };
    let mut m = booted(&fw);
    stop_song_position_and_continue(&mut m, &fw);
}

#[test]
fn stop_song_position_and_continue_over_usb() {
    let Some(fw) = firmware("the MIDI transport test") else {
        return;
    };
    let mut m = booted_on_usb(&fw);
    stop_song_position_and_continue(&mut m, &fw);
}

fn stop_song_position_and_continue(m: &mut Machine, fw: &Firmware) {
    let row = row_calls();
    let beat_ms = (60_000.0 / BPM) as u64;
    let run = run_clocked(m, fw, beat_ms * 4, |m, t| {
        if t == beat_ms - 5 {
            midi(m, &[0xFA]);
        }
        if t == 3 * beat_ms - 5 {
            midi(m, &[0xFC]);
        }
        false
    });
    let stopped_at = run.calls.last().unwrap().0;
    assert!(
        stopped_at > 0 && stopped_at < 3 * 4 * row,
        "stopped at call {stopped_at}"
    );
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let run = run_clocked(m, fw, beat_ms * 4, |m, t| {
        if t == 10 {
            midi(m, &[0xF2, 32, 0]);
        }
        if t == beat_ms - 5 {
            midi(m, &[0xFB]);
        }
        false
    });
    let first = run.calls.first().expect("it played on").0;
    assert_eq!(first, 32 * row, "the continue's first call: row 32");
    let pass = m.passes.iter().copied().max().unwrap_or(0);
    println!("after SPP to bar 3: first call {first}; busiest pass {pass} (the seek's)");
    assert!(pass <= common::POLL_INSTRUCTIONS);
}

fn follow_for_minutes(fw: &Firmware, bpm: f64) -> (usize, f64, f64, f64) {
    let wall = std::time::Instant::now();
    let seconds: f64 = std::env::var("SIDOU_DRIFT_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600.0);
    let row = row_calls();
    let mut m = booted(fw);
    let hz = m.soc.periph.hz as f64;
    let per = 60.0 / bpm / 24.0 * hz;
    let t0 = m.soc.periph.cycles as f64;
    let clocks = (seconds * bpm / 60.0 * 24.0) as usize + 48;
    let calls_at = fw.symbol("song_transport") + ilp32::song_ctl::calls as u32;
    let sample_at = fw.symbol("song_transport") + ilp32::song_ctl::call_sample as u32;
    let start = t0 + 48.0 * per - m.soc.periph.cycle_of_ms(5) as f64;
    let step = m.soc.periph.cycle_of_ms(4) as f64;
    let mut sent: Vec<u64> = Vec::with_capacity(clocks);
    let mut started = false;
    let mut last = m.soc.read32(calls_at);
    let mut rows: Vec<(u32, u32)> = Vec::new();
    let mut i = 0usize;
    while i < clocks {
        let next = t0 + i as f64 * per;
        let due = if started { next } else { next.min(start) };
        let now = m.soc.periph.cycles as f64;
        if due > now {
            m.run_cycles_jit(((due - now).min(step)).ceil() as u64);
        } else if !started && start <= now {
            m.midi(&[0xFA]);
            started = true;
        } else {
            m.midi(&[0xF8]);
            sent.push(m.soc.periph.cycles);
            i += 1;
        }
        let calls = m.soc.read32(calls_at);
        if calls != last {
            assert_eq!(calls, last + 1, "two calls between two looks");
            if (calls - 1).is_multiple_of(row) {
                rows.push((calls - 1, m.soc.read32(sample_at)));
            }
            last = calls;
        }
    }
    let errs: Vec<(f64, f64)> = rows
        .iter()
        .filter_map(|&(call, sample)| {
            let k = (call / row) as usize;
            let at = sent.get(48 + 6 * k)?;
            let heard = *at as f64 / CYCLES_PER_SAMPLE;
            Some((heard / FS, (f64::from(sample) - heard) / FS * 1e3))
        })
        .collect();
    assert!(errs.len() > 100, "{} rows", errs.len());

    let first_t = errs[0].0;
    let first: Vec<f64> = errs
        .iter()
        .filter(|e| e.0 > first_t + 2.0 && e.0 < first_t + 60.0)
        .map(|e| e.1)
        .collect();
    let mean = first.iter().sum::<f64>() / first.len() as f64;
    let end_t = errs.last().unwrap().0;
    let last_minute = errs
        .iter()
        .filter(|e| e.0 > end_t - 60.0)
        .map(|e| (e.1 - mean).abs())
        .fold(0.0, f64::max);
    let worst = errs
        .iter()
        .filter(|e| e.0 > first_t + 2.0)
        .map(|e| (e.1 - mean).abs())
        .fold(0.0, f64::max);
    let clock_rows = (sent.len() - 48) / 6;
    let song_rows = (last / row) as usize;
    println!(
        "{bpm} BPM for {:.0} s: {} rows; the first minute's mean {mean:.3} ms after its 16th, \
         the last minute at most {last_minute:.3} ms from it, every row from 2 s on at most \
         {worst:.3}; the song at row {song_rows}, the clock at row {clock_rows}; wall {:.1} s",
        (sent.last().unwrap() - sent[0]) as f64 / hz,
        errs.len(),
        wall.elapsed().as_secs_f64()
    );
    assert!(
        song_rows.abs_diff(clock_rows) <= 1,
        "the song at row {song_rows}, the clock at {clock_rows}"
    );
    (errs.len(), last_minute, worst, wall.elapsed().as_secs_f64())
}

#[test]
fn a_song_follows_a_steady_clock_for_ten_minutes_without_drifting() {
    let Some(fw) = firmware("the clock-drift test") else {
        return;
    };
    for bpm in [120.0, 133.0] {
        let (_, last_minute, worst, _) = follow_for_minutes(&fw, bpm);
        assert!(
            worst <= 1.0 && last_minute <= 1.0,
            "{bpm} BPM: a row {worst:.3} ms off its place"
        );
    }
}
