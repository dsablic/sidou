// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::{Mutex, MutexGuard};

use sidou_pack::harness::{
    felucca_uac_test_main, uac_h_desc, uac_h_direct_start, uac_h_direct_tap, uac_h_drain,
    uac_h_flowing, uac_h_get, uac_h_packet, uac_h_rate, uac_h_reset, uac_h_skip, uac_h_stream,
    uac_h_take, uac_h_tick, UacState,
};

static HARNESS: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    HARNESS.lock().unwrap_or_else(|e| e.into_inner())
}

const BLOCK: usize = 128;
const CHUNK: usize = 32;
const FS_DEV: f64 = 24.0e6 / 544.0;
const TICK_NS: u64 = 100_000;
const MS: u64 = 1_000_000;

fn state() -> UacState {
    // SAFETY: the harness's state, read under `exclusive`.
    unsafe { uac_h_get() }
}

fn desc(wvalue: u32) -> Vec<u8> {
    let mut p = std::ptr::null();
    // SAFETY: usb.c's static descriptor and its length.
    unsafe {
        let n = uac_h_desc(wvalue, &mut p);
        assert!(n > 0, "descriptor {wvalue:#06x} stalls");
        std::slice::from_raw_parts(p, n as usize).to_vec()
    }
}

fn le16(b: &[u8]) -> u32 {
    u32::from(b[0]) | u32::from(b[1]) << 8
}

fn string(i: u32) -> String {
    let d = desc(0x0300 | i);
    assert_eq!((usize::from(d[0]), d[1]), (d.len(), 3));
    let units: Vec<u16> = d[2..]
        .chunks(2)
        .map(|c| c[0] as u16 | (c[1] as u16) << 8)
        .collect();
    String::from_utf16(&units).unwrap()
}

#[test]
fn feluccas_own_usb_audio_test_passes_on_sidous_usb_c() {
    let _g = exclusive();
    // SAFETY: the test's own main; its state is its own translation unit's.
    let fails = unsafe { felucca_uac_test_main() };
    assert_eq!(fails, 0, "Felucca's uac_test failed (its output above)");
}

#[test]
fn the_device_is_one_composite_usb_midi_and_class_compliant_audio_input() {
    let _g = exclusive();
    let dev = desc(0x0100);
    assert_eq!(dev.len(), 18);
    assert_eq!((dev[0], dev[1]), (18, 1));
    assert_eq!(
        (dev[4], dev[5], dev[6]),
        (0, 0, 0),
        "class per interface (no IAD)"
    );
    assert_eq!((le16(&dev[8..]), le16(&dev[10..])), (0x1209, 0x0001));
    assert_eq!(le16(&dev[12..]), 0x0328);
    assert_eq!(string(dev[15] as u32), "SIDOU FM-1");
    assert_eq!(string(dev[14] as u32), "SIDOU contributors");

    let cfg = desc(0x0200);
    assert_eq!((cfg[0], cfg[1]), (9, 2));
    assert_eq!(le16(&cfg[2..]) as usize, cfg.len(), "wTotalLength");
    assert_eq!(
        cfg[4], 3,
        "bNumInterfaces: audio control, MIDI streaming, audio streaming"
    );
    assert_eq!(cfg[7] & 0x80, 0x80, "bus powered");

    let mut off = 0;
    let (mut iface, mut alt) = (u8::MAX, u8::MAX);
    let mut settings: Vec<(u8, u8, u8, u8, u8)> = Vec::new();
    let mut eps: Vec<(u8, u8, u8, u32, u8)> = Vec::new();
    let mut ac_total = 0;
    let mut ac_header: Option<Vec<u8>> = None;
    let mut fmt: Option<Vec<u8>> = None;
    let mut it: Option<Vec<u8>> = None;
    let mut ot: Option<Vec<u8>> = None;
    let mut as_general: Option<Vec<u8>> = None;
    let mut cs_ep: Option<Vec<u8>> = None;
    let mut eps_detail = Vec::new();
    while off < cfg.len() {
        let d = &cfg[off..off + usize::from(cfg[off])];
        assert!(
            d.len() >= 2 && off + d.len() <= cfg.len(),
            "a descriptor overruns at {off}"
        );
        match d[1] {
            2 => {}
            4 => {
                (iface, alt) = (d[2], d[3]);
                settings.push((d[2], d[3], d[4], d[5], d[6]));
            }
            5 => {
                eps.push((iface, alt, d[2], 0, 0));
                eps_detail.push((d[2], d[3], le16(&d[4..]), d[6]));
            }
            0x24 => {
                let (class, sub) = settings.last().map(|s| (s.3, s.4)).unwrap();
                if (class, sub) == (1, 1) {
                    ac_total += d.len();
                    match d[2] {
                        1 => ac_header = Some(d.to_vec()),
                        2 => it = Some(d.to_vec()),
                        3 => ot = Some(d.to_vec()),
                        _ => panic!("an unexpected audio control descriptor {d:02x?}"),
                    }
                } else if (class, sub) == (1, 2) {
                    match d[2] {
                        1 => as_general = Some(d.to_vec()),
                        2 => fmt = Some(d.to_vec()),
                        _ => panic!("an unexpected audio streaming descriptor {d:02x?}"),
                    }
                }
            }
            0x25 => {
                if settings.last().map(|s| (s.3, s.4)) == Some((1, 2)) {
                    cs_ep = Some(d.to_vec());
                }
            }
            t => panic!("descriptor type {t:#x} at {off}"),
        }
        off += d.len();
    }
    assert_eq!(off, cfg.len());
    assert_eq!(
        settings,
        vec![
            (0, 0, 0, 1, 1),
            (1, 0, 2, 1, 3),
            (2, 0, 0, 1, 2),
            (2, 1, 1, 1, 2)
        ],
        "interfaces: AC 0, MIDI streaming 1, audio streaming 2 (alt 0 without bandwidth, alt 1)"
    );
    for (i, a, n, _, _) in &settings {
        let found = eps.iter().filter(|e| e.0 == *i && e.1 == *a).count();
        assert_eq!(
            found,
            usize::from(*n),
            "interface {i} alt {a}: bNumEndpoints"
        );
    }
    assert_eq!(
        eps_detail,
        vec![(0x01, 2, 64, 0), (0x81, 2, 64, 0), (0x84, 0x05, 196, 1)]
    );
    let h = ac_header.expect("no AC header");
    assert_eq!(le16(&h[3..]), 0x0100, "UAC 1.0");
    assert_eq!(
        le16(&h[5..]) as usize,
        ac_total,
        "the AC header's wTotalLength"
    );
    assert_eq!(
        &h[7..],
        &[2, 2, 1],
        "the collection: audio streaming first, then MIDI"
    );
    let it = it.expect("no input terminal");
    assert_eq!(
        (it[3], le16(&it[4..]), it[7], le16(&it[8..])),
        (1, 0x0603, 2, 3)
    );
    let ot = ot.expect("no output terminal");
    assert_eq!(
        (ot[3], le16(&ot[4..]), ot[7]),
        (2, 0x0101, 1),
        "USB streaming, from 1"
    );
    let g = as_general.expect("no AS general");
    assert_eq!((g[3], le16(&g[5..])), (2, 1), "terminal 2, PCM");
    let f = fmt.expect("no format type I");
    assert_eq!(
        &f[3..8],
        &[1, 2, 2, 16, 2],
        "type I, 2 ch, 2-byte subframes, 16 bit, 2 rates"
    );
    let rate = |i: usize| u32::from(f[i]) | u32::from(f[i + 1]) << 8 | u32::from(f[i + 2]) << 16;
    assert_eq!((rate(8), rate(11)), (44_100, 48_000));
    assert_eq!(
        cs_ep.expect("no CS endpoint")[3] & 1,
        1,
        "sampling frequency control"
    );
}

#[derive(Clone, Copy, PartialEq)]
enum Signal {
    Count,
    Extremes,
}

fn frame(sig: Signal, n: u32) -> (i16, i16) {
    match sig {
        Signal::Count => (n as u16 as i16, !(n as u16) as i16),
        Signal::Extremes => {
            const X: [i16; 4] = [i16::MAX, i16::MIN, -1, 0];
            (X[n as usize % 4], X[(n as usize + 1) % 4])
        }
    }
}

fn pack(l: i16, r: i16) -> u32 {
    u32::from(l as u16) | u32::from(r as u16) << 16
}

fn start(hz: u32) {
    // SAFETY: the harness, under `exclusive`.
    unsafe {
        uac_h_reset();
        uac_h_rate(hz);
        uac_h_stream(1);
        uac_h_flowing(1);
    }
}

fn reference(hz: u32, sig: Signal, frames: u32) -> Vec<u32> {
    start(hz);
    let mut out = Vec::new();
    let mut d = vec![0u32; 1024];
    // SAFETY: the harness, under the caller's `exclusive`.
    unsafe {
        uac_h_direct_start();
        let primed = uac_h_drain(d.as_mut_ptr(), 1024);
        assert!(d[..primed as usize].iter().all(|&f| f == 0));
        let mut n = 0;
        while n < frames {
            let mut blk = [0i32; 2 * CHUNK];
            for i in 0..CHUNK {
                let (l, r) = frame(sig, n + i as u32);
                blk[2 * i] = i32::from(l);
                blk[2 * i + 1] = i32::from(r);
            }
            uac_h_direct_tap(blk.as_ptr(), CHUNK as u32);
            let k = uac_h_drain(d.as_mut_ptr(), 1024);
            out.extend_from_slice(&d[..k as usize]);
            n += CHUNK as u32;
        }
    }
    out
}

#[derive(Clone, Copy)]
enum Render {
    Random,
    Budget,
    Steady,
}

struct Run {
    recorded: Vec<u32>,
    sizes: [u32; 64],
    produced: u32,
    state: UacState,
}

fn record(hz: u32, sig: Signal, render: Render, ms: u64, starve: Option<(u64, u64)>) -> Run {
    start(hz);
    let half = (BLOCK as f64 / FS_DEV * 1e9) as u64;
    let (mut now, mut next_half, mut next_tick, mut next_pkt) = (0u64, 300_000u64, 37_000u64, MS);
    let mut render_end: Option<u64> = None;
    let mut pending: Option<u64> = None;
    let mut lcg = 12_345u32;
    let mut rnd = |m: u64| {
        lcg = lcg.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        u64::from(lcg >> 8) % m
    };
    let mut run = Run {
        recorded: Vec::new(),
        sizes: [0; 64],
        produced: 0,
        state: state(),
    };
    let mut half_buf = [0i32; 2 * BLOCK];
    let mut d = [0u32; 64];
    let end = ms * MS;
    while now < end {
        now = [
            next_half,
            render_end.unwrap_or(u64::MAX),
            next_tick,
            next_pkt,
        ]
        .into_iter()
        .min()
        .unwrap();
        if render_end == Some(now) {
            for (i, f) in half_buf.chunks_mut(2).enumerate() {
                let (l, r) = frame(sig, run.produced + i as u32);
                f[0] = i32::from(l) * 256;
                f[1] = i32::from(r) * 256;
            }
            pending = Some(next_half);
            render_end = None;
        } else if now == next_half {
            let share = match render {
                Render::Random => 70 + rnd(780),
                Render::Budget => 70 + rnd(430),
                Render::Steady => 450 + rnd(100),
            };
            render_end = Some(now + half * share / 1000);
            next_half += half;
        } else if now == next_tick {
            if !starve.is_some_and(|(from, to)| now >= from * MS && now < to * MS) {
                // SAFETY: TIMER5's calls, under the caller's `exclusive`.
                unsafe {
                    if let Some(moved_on) = pending.take() {
                        if now < moved_on {
                            uac_h_take(half_buf.as_ptr());
                            if state().blocks as u64 * BLOCK as u64 > u64::from(run.produced) {
                                run.produced += BLOCK as u32;
                            }
                        } else {
                            uac_h_skip();
                        }
                    }
                    uac_h_tick();
                }
            }
            next_tick += TICK_NS;
        } else if now == next_pkt {
            // SAFETY: d holds the largest packet.
            let n = unsafe { uac_h_packet(d.as_mut_ptr()) } as usize;
            run.sizes[n] += 1;
            run.recorded.extend_from_slice(&d[..n]);
            next_pkt = (now / MS + 1) * MS + rnd(MS / 2);
        }
    }
    run.state = state();
    run
}

fn in_order(recorded: &[u32], want: &[u32]) -> (usize, usize) {
    let probe = &want[..64];
    let at = (0..recorded.len() - 64)
        .find(|&i| &recorded[i..i + 64] == probe)
        .expect("the producer's frames never reached the host");
    let same = recorded[at..]
        .iter()
        .zip(want)
        .take_while(|(a, b)| a == b)
        .count();
    (at, same)
}

fn check_recording(hz: u32, render: Render, label: &str) {
    let run = record(hz, Signal::Count, render, 10_000, None);
    let want = reference(hz, Signal::Count, run.produced);
    let s = run.state;
    let (at, same) = in_order(&run.recorded, &want);
    let after = run.recorded.len() - at;
    let packets: u32 = run.sizes.iter().sum();
    let mean = run.recorded.len() as f64 / f64::from(packets);
    let (lo, hi) = if hz == 48_000 { (47, 49) } else { (43, 46) };
    println!(
        "{label}: {} frames recorded in {packets} packets (mean {mean:.4}), {} produced; \
         fill at the hand-over {}..{}; adj up {} down {}; underruns {} overruns {} skipped {}",
        run.recorded.len(),
        run.produced,
        s.fill_lo,
        s.fill_hi,
        s.adj_up,
        s.adj_down,
        s.underruns,
        s.overruns,
        s.skipped
    );
    assert_eq!(
        same, after,
        "{label}: frame {same} after the start is not the producer's (a lost, repeated or \
         reordered frame)"
    );
    assert!(
        after as f64 > 9.9 * f64::from(hz),
        "{label}: only {after} frames in 10 s"
    );
    assert_eq!(
        (s.underruns, s.overruns, s.skipped),
        (0, 0, 0),
        "{label}: a glitch"
    );
    let other: u32 = run
        .sizes
        .iter()
        .enumerate()
        .filter(|(n, _)| *n < lo || *n > hi)
        .map(|(_, c)| c)
        .sum();
    assert_eq!(
        other, 0,
        "{label}: packets outside {lo}..{hi} frames: {:?}",
        run.sizes
    );
    let nominal = f64::from(hz) / 1000.0;
    assert!((mean - nominal).abs() < 0.1, "{label}: mean packet {mean}");
    match render {
        Render::Steady => assert!(s.fill_lo >= s.maxf, "{label}: within a packet of empty"),
        Render::Budget => assert!(
            s.fill_lo >= 24,
            "{label}: the ring's lowest fill {}",
            s.fill_lo
        ),
        Render::Random => {}
    }
}

#[test]
fn a_host_records_every_frame_in_order_at_44_1_khz_bit_for_bit() {
    let _g = exclusive();
    let want = reference(44_100, Signal::Count, 4096);
    let input: Vec<u32> = (0..4096)
        .map(|n| {
            let (l, r) = frame(Signal::Count, n);
            pack(l, r)
        })
        .collect();
    assert_eq!(want, input);
    check_recording(44_100, Render::Steady, "44.1 kHz, steady renders");
    check_recording(44_100, Render::Budget, "44.1 kHz, renders of 7..50 %");
    check_recording(44_100, Render::Random, "44.1 kHz, renders of 7..85 %");
}

#[test]
fn a_host_records_every_resampled_frame_in_order_at_48_khz() {
    let _g = exclusive();
    check_recording(48_000, Render::Steady, "48 kHz, steady renders");
    check_recording(48_000, Render::Budget, "48 kHz, renders of 7..50 %");
    check_recording(48_000, Render::Random, "48 kHz, renders of 7..85 %");
}

#[test]
fn full_scale_reaches_the_host_unchanged() {
    let _g = exclusive();
    let run = record(44_100, Signal::Extremes, Render::Steady, 1_000, None);
    let want: Vec<u32> = (0..run.produced)
        .map(|n| {
            let (l, r) = frame(Signal::Extremes, n);
            pack(l, r)
        })
        .collect();
    let (at, same) = in_order(&run.recorded, &want);
    assert_eq!(same, run.recorded.len() - at);
    assert!(run.recorded[at..]
        .iter()
        .any(|&f| f == pack(i16::MAX, i16::MIN)));
}

#[test]
fn a_held_block_is_recorded_as_silence() {
    let _g = exclusive();
    start(44_100);
    let mut d = [0u32; 1024];
    let half = [0x1234i32 * 256; 2 * BLOCK];
    let held = [0i32; 2 * BLOCK];
    // SAFETY: the harness, under `exclusive`.
    unsafe {
        uac_h_tick();
        assert_eq!(state().want, 1);
        uac_h_take(half.as_ptr());
        for _ in 0..4 {
            uac_h_tick();
        }
        let primed = state().prime;
        let n = uac_h_drain(d.as_mut_ptr(), 1024);
        assert_eq!(n, primed + BLOCK as u32);
        assert!(d[primed as usize..n as usize]
            .iter()
            .all(|&f| f == pack(0x1234, 0x1234)));
        uac_h_tick();
        uac_h_take(held.as_ptr());
        for _ in 0..4 {
            uac_h_tick();
        }
        let n = uac_h_drain(d.as_mut_ptr(), 1024);
        assert_eq!(n, BLOCK as u32);
        assert!(d[..n as usize].iter().all(|&f| f == 0));
    }
}

#[test]
fn nothing_is_copied_while_no_host_records() {
    let _g = exclusive();
    let lr = [0x1234i32 * 256; 2 * BLOCK];
    // SAFETY: the harness, under `exclusive`.
    unsafe {
        uac_h_reset();
        for _ in 0..100 {
            uac_h_tick();
            uac_h_take(lr.as_ptr());
        }
        let s = state();
        assert_eq!((s.want, s.left, s.blocks, s.ring_w), (0, 0, 0, 0));
        uac_h_stream(1);
        for _ in 0..100 {
            uac_h_tick();
            uac_h_take(lr.as_ptr());
        }
        let s = state();
        assert_eq!((s.want, s.blocks, s.ring_w), (0, 0, 0));
    }
}

#[test]
fn a_starved_timer5_skips_a_block_and_the_stream_recovers() {
    let _g = exclusive();
    let run = record(
        44_100,
        Signal::Count,
        Render::Steady,
        2_000,
        Some((1_000, 1_006)),
    );
    let s = run.state;
    println!(
        "starved 6 ms: skipped {} underruns {} overruns {} fill {}..{}",
        s.skipped, s.underruns, s.overruns, s.fill_lo, s.fill_hi
    );
    assert!(s.skipped >= 1, "no block skipped while TIMER5 was held off");
    assert!(s.underruns > 0, "the ring did not run dry in 6 ms");
    let tail = &run.recorded[run.recorded.len() - 22_050..];
    let first = tail[0] as u16;
    for (i, &f) in tail.iter().enumerate() {
        let n = first.wrapping_add(i as u16);
        assert_eq!(
            f,
            pack(n as i16, !n as i16),
            "frame {i} of the last half second"
        );
    }
}
