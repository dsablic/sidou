// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::song::Song;
use sidou_tests::song_embed;
use sidou_tests::transport::{Transport, BLOCK};

const TPS: u32 = MIDI_SYNC_TICKS_PER_SAMPLE;
const PASSES: u32 = 4;
const FS: f64 = 24e6 / 544.0;

#[test]
fn loading_a_song_cancels_old_transport_requests_without_resetting_clock_configuration() {
    let mut sync = midi_sync::default();
    unsafe { midi_sync_init(&mut sync) };
    unsafe { midi_sync_set(&mut sync, MIDI_SYNC_FOLLOW, 1) };
    sync.want = (MIDI_SYNC_WANT_START | MIDI_SYNC_WANT_RESUME | MIDI_SYNC_WANT_SEEK) as u8;
    sync.held = 1;
    sync.aligning = 1;
    sync.synced = 1;
    sync.seek_on = 1;
    sync.clock.clocks = 123;
    unsafe { midi_sync_song_changed(&mut sync) };
    assert_eq!(
        (
            sync.want,
            sync.held,
            sync.aligning,
            sync.synced,
            sync.seek_on
        ),
        (0, 0, 0, 0, 0)
    );
    assert_eq!(
        (u32::from(sync.mode), sync.clock_out),
        (MIDI_SYNC_FOLLOW, 1)
    );
    assert_eq!(sync.clock.clocks, 123);
}

#[derive(Clone, Copy, Debug)]
struct At {
    ticks: u32,
    bytes: [u8; 3],
    len: u8,
}

fn rt(ticks: u32, b: u8) -> At {
    At {
        ticks,
        bytes: [b, 0, 0],
        len: 1,
    }
}

fn sc(ctx: *mut std::ffi::c_void) -> *mut song_ctl {
    ctx.cast()
}

unsafe extern "C" fn t_play(ctx: *mut std::ffi::c_void, pos: u32) -> i32 {
    unsafe { song_ctl_play(sc(ctx), 0, pos) }
}

unsafe extern "C" fn t_stop(ctx: *mut std::ffi::c_void) -> i32 {
    unsafe { song_ctl_stop(sc(ctx)) }
}

unsafe extern "C" fn t_resume(ctx: *mut std::ffi::c_void) -> i32 {
    unsafe { song_ctl_continue(sc(ctx)) }
}

unsafe extern "C" fn t_mute(ctx: *mut std::ffi::c_void, mask: u32) -> i32 {
    unsafe { song_ctl_mute(sc(ctx), mask) }
}

unsafe extern "C" fn t_jam(_: *mut std::ffi::c_void, _: u32, _: u32, _: u32) -> i32 {
    1
}

unsafe extern "C" fn t_status(_: *mut std::ffi::c_void, _: *mut song_status) {}

unsafe extern "C" fn t_hold(ctx: *mut std::ffi::c_void, on: u32) -> i32 {
    unsafe { song_ctl_hold(sc(ctx), on as i32) };
    1
}

unsafe extern "C" fn t_align(ctx: *mut std::ffi::c_void, sample: u32) -> i32 {
    unsafe { song_ctl_align(sc(ctx), sample) }
}

unsafe extern "C" fn t_period(ctx: *mut std::ffi::c_void, q16: u32) -> i32 {
    let s = unsafe { &mut *sc(ctx) };
    let p = if q16 == 0 {
        unsafe { song_period_of(u32::from(s.opts.multispeed), u32::from(s.opts.ntsc)) }
    } else {
        song_period {
            q: q16 >> 16,
            r: q16 & 0xffff,
            den: 65536,
        }
    };
    unsafe { song_ctl_set_period(s, p) }
}

unsafe extern "C" fn t_seek(ctx: *mut std::ffi::c_void, call: u32) -> i32 {
    unsafe { song_ctl_seek(sc(ctx), call) }
}

unsafe extern "C" fn t_seek_further(ctx: *mut std::ffi::c_void, call: u32) -> i32 {
    unsafe { song_ctl_seek_further(sc(ctx), call) }
}

unsafe extern "C" fn t_clock(ctx: *mut std::ffi::c_void, out: *mut song_clock) {
    let s = unsafe { &*sc(ctx) };
    let o = unsafe { &mut *out };
    o.state = unsafe { song_ctl_state(s) } as u8;
    o.seeking = if s.ff_on != 0 {
        if s.ff_sent != 0 {
            2
        } else {
            1
        }
    } else {
        0
    };
    let t = s.player.chn[0].tempo;
    o.speed = if s.shapes_used == 0 || s.player.shape.is_null() {
        [0, 0]
    } else if t < 2 {
        s.player.funktempo
    } else {
        [t + 1, t + 1]
    };
    o.calls = unsafe { song_ctl_clock_calls(s) };
    o.from_top = unsafe { song_ctl_from_top(s) } as u8;
    o.call_sample = s.call_sample;
    let p = &s.period;
    o.period_q16 = (p.q << 16) + (p.r as f32 * 65536.0 / p.den.max(1) as f32) as u32;
}

fn transport(sc: *mut song_ctl) -> song_transport {
    song_transport {
        play: Some(t_play),
        stop: Some(t_stop),
        resume: Some(t_resume),
        mute: Some(t_mute),
        jam: Some(t_jam),
        status: Some(t_status),
        ctx: sc.cast(),
        song: unsafe { (*sc).song },
        hold: Some(t_hold),
        align: Some(t_align),
        period: Some(t_period),
        seek: Some(t_seek),
        seek_further: Some(t_seek_further),
        clock: Some(t_clock),
        ..Default::default()
    }
}

struct Rig {
    _song: Box<Song>,
    t: Transport,
    sync: Box<midi_sync>,
    _transport: Box<song_transport>,
    parser: Box<midi_in>,
    midi: Vec<At>,
    next: usize,
    calls: Vec<(u32, u32)>,
    out: Vec<(u32, u32)>,
    stamp: u32,
    ticks_lost_to_holds: u32,
    stamp_late_until: u32,
}

unsafe extern "C" fn on_msg(ctx: *mut std::ffi::c_void, msg: *const midi_msg) {
    let r = unsafe { &mut *ctx.cast::<Rig>() };
    unsafe { midi_sync_msg(&mut *r.sync, msg, r.stamp) };
}

impl Rig {
    fn new() -> Box<Rig> {
        let mut song = Box::new(Song::device());
        song.read(&song_embed::sng(), 4).expect("the song reads");
        let t = Transport::new(&mut song);
        let mut r = Box::new(Rig {
            _song: song,
            t,
            sync: Box::default(),
            _transport: Box::default(),
            parser: Box::default(),
            midi: Vec::new(),
            next: 0,
            calls: Vec::new(),
            out: Vec::new(),
            stamp: 0,
            ticks_lost_to_holds: 0,
            stamp_late_until: 0,
        });
        let rp: *mut Rig = &mut *r;
        let scp: *mut song_ctl = &mut *r.t.sc;
        *r._transport = transport(scp);
        unsafe {
            midi_sync_init(&mut *r.sync);
            midi_sync_bind(&mut *r.sync, &*r._transport);
            midi_in_init(&mut *r.parser, Some(on_msg), None, rp.cast());
        }
        r
    }

    fn samples(&self) -> u32 {
        self.t.sc.samples
    }

    fn run(&mut self, blocks: u32) {
        let next = self.next;
        self.midi[next..].sort_by_key(|m| m.ticks);
        for _ in 0..blocks {
            let s = self.samples();
            let calls_before = self.t.calls();
            let lost = self.ticks_lost_to_holds / TPS;
            if let Some(at) = self.t.block_silent() {
                if self.t.calls() != calls_before {
                    self.calls.push((self.t.calls() - 1, s + at as u32 + lost));
                }
            }
            let rendered = (s.wrapping_sub(BLOCK as u32))
                .wrapping_mul(TPS)
                .wrapping_add(self.ticks_lost_to_holds);
            unsafe { midi_sync_anchor(&mut *self.sync, rendered, s) };
            for j in 0..PASSES {
                let now = rendered.wrapping_add(j * BLOCK as u32 / PASSES * TPS);
                while self.next < self.midi.len() && self.midi[self.next].ticks <= now {
                    let m = self.midi[self.next];
                    self.stamp = m.ticks.max(self.stamp_late_until);
                    unsafe { midi_in_feed(&mut *self.parser, m.bytes.as_ptr(), m.len as usize) };
                    self.next += 1;
                }
                let mut pkt = [0u32; MIDI_SYNC_OUT_MAX as usize];
                let n = unsafe { midi_sync_poll(&mut *self.sync, now, pkt.as_mut_ptr()) };
                for &p in &pkt[..n as usize] {
                    self.out.push((p, now / TPS));
                }
                self.t.pass();
            }
        }
    }

    fn hold(&mut self, ms: u32) {
        let s = self.samples();
        let from = (s.wrapping_sub(BLOCK as u32))
            .wrapping_mul(TPS)
            .wrapping_add(self.ticks_lost_to_holds);
        let d = ms * 24_000;
        self.ticks_lost_to_holds += d;
        self.stamp_late_until = from + d;
        unsafe { midi_sync_held(&mut *self.sync, from, from + d) };
    }

    fn status(&self) -> song_clock {
        let mut st = song_clock::default();
        unsafe { t_clock(self._transport.ctx, &mut st) };
        st
    }

    fn calls_a_row(&self) -> u32 {
        let st = self.status();
        assert_eq!(st.speed[0], st.speed[1], "the first song has no funktempo");
        u32::from(st.speed[0])
    }
}

fn clock(from: u32, bpm: f64, beats: u32, jitter: impl Fn(u32) -> u32) -> Vec<At> {
    let per = 24e6 * 60.0 / bpm / 24.0;
    (0..beats * 24)
        .map(|i| rt(from + (f64::from(i) * per) as u32 + jitter(i), 0xF8))
        .collect()
}

fn lcg(i: u32) -> u32 {
    i.wrapping_mul(1_103_515_245).wrapping_add(12_345) >> 8
}

fn trs_jitter(i: u32) -> u32 {
    lcg(i) % 12_000
}

fn row_errors(r: &Rig, clocks: &[At], skip_beats: u32) -> Vec<f64> {
    let row = r.calls_a_row();
    let mut errs = Vec::new();
    for &(call, sample) in &r.calls {
        if call % row != 0 {
            continue;
        }
        let k = call / row;
        let i = (k * 6) as usize;
        if k < skip_beats * 4 || i >= clocks.len() {
            continue;
        }
        let sent = f64::from(clocks[i].ticks) / f64::from(TPS);
        let err = f64::from(sample) - sent - f64::from(MIDI_SYNC_DELAY);
        errs.push(err / FS * 1e3);
    }
    errs
}

fn start_at(r: &mut Rig, ticks: u32) {
    r.midi.push(rt(ticks, 0xFA));
}

#[test]
fn a_start_on_a_120_bpm_clock_lands_the_rows_on_the_16ths() {
    for bpm in [120.0, 125.3, 90.0] {
        let mut r = Rig::new();
        r.run(20);
        let t0 = r.samples() * TPS;
        let lead = clock(t0, bpm, 2, trs_jitter);
        let beat = (24e6 * 60.0 / bpm) as u32;
        let from = t0 + 2 * beat;
        let mut song_clock: Vec<At> = clock(from, bpm, 12, trs_jitter);
        r.midi.extend(lead);
        start_at(&mut r, from - beat / 48);
        r.midi.append(&mut song_clock.clone());
        let blocks = (14.0 * 60.0 / bpm * FS / BLOCK as f64) as u32;
        r.run(blocks);
        song_clock.truncate(12 * 24);
        let errs = row_errors(&r, &song_clock, 2);
        assert!(errs.len() > 30, "{bpm} BPM: {} rows", errs.len());
        let worst = errs.iter().fold(0.0f64, |a, &e| a.max(e.abs()));
        println!(
            "{bpm} BPM: {} rows from beat 2, worst 16th error {worst:.3} ms, aligns {}, periods {}",
            errs.len(),
            r.sync.aligns,
            r.sync.periods
        );
        assert!(worst <= 1.0, "{bpm} BPM: a row {worst:.3} ms off its 16th");
    }
}

#[test]
fn stop_and_continue_resume_on_the_clock() {
    let mut r = Rig::new();
    r.run(20);
    let t0 = r.samples() * TPS;
    let bpm = 120.0;
    let beat = (24e6 * 60.0 / bpm) as u32;
    let mut all = clock(t0, bpm, 16, trs_jitter);
    let at = |b: u32| t0 + b * beat - beat / 48;
    r.midi
        .extend(all.iter().copied().filter(|m| m.ticks < at(1)));
    start_at(&mut r, at(1));
    r.midi.extend(
        all.iter()
            .copied()
            .filter(|m| m.ticks >= at(1) && m.ticks < at(5)),
    );
    r.midi.push(rt(at(5), 0xFC));
    r.midi.extend(
        all.iter()
            .copied()
            .filter(|m| m.ticks >= at(5) && m.ticks < at(8)),
    );
    r.midi.push(rt(at(8), 0xFB));
    r.midi
        .extend(all.iter().copied().filter(|m| m.ticks >= at(8)));
    let blocks = (16.5 * 0.5 * FS / BLOCK as f64) as u32;
    r.run(blocks);
    let row = r.calls_a_row();
    let stopped: Vec<_> = r
        .calls
        .windows(2)
        .filter(|w| w[1].1 - w[0].1 > 20_000)
        .collect();
    assert_eq!(stopped.len(), 1, "one stop: {stopped:?}");
    let resumed = stopped[0][1];
    assert_eq!(resumed.0, stopped[0][0].0 + 1, "on from the next call");
    all.drain(..24);
    let mut errs = Vec::new();
    for &(call, sample) in &r.calls {
        if call % row != 0 || call < resumed.0 + 8 * row {
            continue;
        }
        let k = call / row + 3 * 4;
        let i = (k * 6) as usize;
        if i < all.len() {
            let sent = f64::from(all[i].ticks) / f64::from(TPS);
            errs.push((f64::from(sample) - sent - f64::from(MIDI_SYNC_DELAY)) / FS * 1e3);
        }
    }
    assert!(errs.len() > 10);
    let worst = errs.iter().fold(0.0f64, |a, &e| a.max(e.abs()));
    println!("after continue: {} rows, worst {worst:.3} ms", errs.len());
    assert!(worst <= 1.0, "a resumed row {worst:.3} ms off");
}

#[test]
fn a_continue_with_no_position_on_a_song_never_played_starts_it_on_the_clock() {
    let mut r = Rig::new();
    r.run(20);
    assert_eq!(r.status().state, MIDI_SYNC_IDLE as u8);
    let t0 = r.samples() * TPS;
    let bpm = 120.0;
    let beat = (24e6 * 60.0 / bpm) as u32;
    let mut song_clock = clock(t0 + beat, bpm, 10, trs_jitter);
    r.midi.extend(clock(t0, bpm, 1, trs_jitter));
    r.midi.push(rt(t0 + beat - beat / 48, 0xFB));
    r.midi.extend(song_clock.iter().copied());
    r.run((11.0 * 60.0 / bpm * FS / BLOCK as f64) as u32);
    assert_eq!(r.status().state, MIDI_SYNC_PLAYING as u8, "it plays");
    assert_eq!(r.calls.first().expect("it played").0, 0, "from the start");
    assert_eq!(r.sync.want, 0, "nothing pending");
    assert!(r.sync.aligns >= 1, "aligned to the clock");
    song_clock.truncate(10 * 24);
    let errs = row_errors(&r, &song_clock, 2);
    assert!(errs.len() > 20, "{} rows", errs.len());
    let worst = errs.iter().fold(0.0f64, |a, &e| a.max(e.abs()));
    assert!(worst <= 1.0, "a row {worst:.3} ms off its 16th");
}

fn fit_residual_ms(c: &midi_clock) -> f64 {
    let pts: Vec<(f64, f64)> = (0..u32::from(c.count))
        .map(|i| {
            let k = ((u32::from(c.head) + i) % MIDI_CLOCK_WINDOW) as usize;
            (
                f64::from(c.number[k].wrapping_sub(c.ref_number) as i32),
                f64::from(c.stamp[k].wrapping_sub(c.ref_stamp) as i32),
            )
        })
        .collect();
    let n = pts.len() as f64;
    let (mx, my) = (
        pts.iter().map(|p| p.0).sum::<f64>() / n,
        pts.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let b = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / sxx;
    let worst = pts
        .iter()
        .map(|p| (p.1 - my - b * (p.0 - mx)).abs())
        .fold(0.0, f64::max);
    worst / FS * 1e3
}

#[test]
fn rows_land_on_the_clock_again_after_flash_holds() {
    let mut r = Rig::new();
    r.run(20);
    let t0 = r.samples() * TPS;
    let bpm = 120.0;
    let beat = (24e6 * 60.0 / bpm) as u32;
    let beats = 40;
    let mut song_clock = clock(t0 + beat, bpm, beats, trs_jitter);
    r.midi.extend(clock(t0, bpm, 1, trs_jitter));
    start_at(&mut r, t0 + beat - beat / 48);
    r.midi.extend(song_clock.iter().copied());
    let blocks_a_beat = 0.5 * FS / BLOCK as f64;
    r.run((7.0 * blocks_a_beat) as u32);
    let aligns = r.sync.aligns;
    let mut worst_fit = 0.0f64;
    for _ in 0..3 {
        r.hold(35);
        for _ in 0..3 {
            r.run((0.25 * blocks_a_beat) as u32);
            worst_fit = worst_fit.max(fit_residual_ms(&r.sync.clock));
        }
    }
    r.run(((beats - 9) as f64 * blocks_a_beat) as u32);
    song_clock.truncate(beats as usize * 24);
    let errs = row_errors(&r, &song_clock, beats - 8);
    assert!(errs.len() > 20, "{} rows", errs.len());
    let worst = errs.iter().fold(0.0f64, |a, &e| a.max(e.abs()));
    println!(
        "the fit across the holds: {worst_fit:.3} ms at most off its line; the last {} rows: \
         worst {worst:.3} ms; aligns since the holds {}",
        errs.len(),
        r.sync.aligns - aligns
    );
    assert!(worst_fit <= 0.6, "the fit {worst_fit:.3} ms off its line");
    assert!(
        r.sync.aligns - aligns <= 3,
        "the song realigned again and again"
    );
    assert!(
        worst <= 1.0,
        "a row {worst:.3} ms off its 16th after the holds"
    );
}

#[test]
fn a_song_position_seeks_and_continue_plays_from_there() {
    let mut r = Rig::new();
    r.run(20);
    let t0 = r.samples() * TPS;
    start_at(&mut r, t0);
    r.midi.push(rt(t0 + 24_000_000 / 2, 0xFC));
    r.run((0.7 * FS / BLOCK as f64) as u32);
    assert_eq!(r.status().state, MIDI_SYNC_STOPPED as u8);
    let row = r.calls_a_row();
    let t1 = r.samples() * TPS;
    r.midi.push(At {
        ticks: t1,
        bytes: [0xF2, 32, 0],
        len: 3,
    });
    let beat = 12_000_000u32;
    r.midi.extend(clock(t1 - beat, 120.0, 1, trs_jitter));
    r.midi.push(rt(t1 + beat / 4, 0xFB));
    r.midi
        .extend(clock(t1 + beat / 4 + beat / 48, 120.0, 6, trs_jitter));
    r.calls.clear();
    r.run((3.0 * FS / BLOCK as f64) as u32);
    let first = r.calls.first().expect("it played on").0;
    assert_eq!(first, 32 * row, "the first call after the continue");
    assert!(r.sync.aligns >= 1);
}

#[test]
fn clock_out_sends_24_a_beat_at_the_songs_tempo_and_the_transport() {
    let mut r = Rig::new();
    unsafe { midi_sync_set(&mut *r.sync, MIDI_SYNC_AUTO, 1) };
    r.run(10);
    assert!(r.t.play(0, 0));
    r.run((3.0 * FS / BLOCK as f64) as u32);
    assert!(r.t.stop());
    r.run(40);
    let realtime = |p: u32| ((p >> 8) & 0xff) as u8;
    assert_eq!(realtime(r.out[0].0), 0xFA, "start first");
    assert_eq!(realtime(r.out.last().unwrap().0), 0xFC, "stop last");
    let clocks: Vec<u32> = r
        .out
        .iter()
        .filter(|p| realtime(p.0) == 0xF8)
        .map(|p| p.1)
        .collect();
    let row = r.calls_a_row();
    let st = r.status();
    let beat = f64::from(4 * row) * f64::from(st.period_q16) / 65536.0;
    let per = beat / 24.0;
    let n = clocks.len();
    let span = f64::from(clocks[n - 1] - clocks[0]) / (n - 1) as f64;
    println!("clock out: {n} clocks, {span:.2} samples apart, the song's {per:.2}");
    assert!((span / per - 1.0).abs() < 0.002, "{span} against {per}");
    for w in clocks.windows(2) {
        let d = f64::from(w[1] - w[0]);
        assert!(
            (d - per).abs() <= 33.0,
            "a clock {d} samples after the last, {per} due"
        );
    }
    let first_call = r.calls[0].1;
    assert!(clocks[0] >= first_call && clocks[0] - first_call <= 33);
    r.out.clear();
    r.sync.out_common_wait = 1;
    assert!(r.t.resume());
    r.run(10);
    assert!(
        r.out.iter().all(|p| p.0 & 0xff != 0x03),
        "a song position inside a SysEx frame: {:x?}",
        r.out
    );
    r.sync.out_common_wait = 0;
    r.run(10);
    assert_eq!(r.out[0].0 & 0xff, 0x03, "a three-byte system common");
    assert_eq!(((r.out[0].0 >> 8) & 0xff) as u8, 0xF2);
    assert_eq!(realtime(r.out[1].0), 0xFB);
}

#[test]
fn clock_out_sends_a_start_for_a_play_from_the_top_after_a_stop() {
    let mut r = Rig::new();
    unsafe { midi_sync_set(&mut *r.sync, MIDI_SYNC_AUTO, 1) };
    r.run(10);
    assert!(r.t.play(0, 0));
    r.run((2.0 * FS / BLOCK as f64) as u32);
    assert!(r.t.stop());
    r.run(40);
    assert!(r.t.calls() > 50, "stopped part way: call {}", r.t.calls());
    r.out.clear();
    r.calls.clear();
    assert!(r.t.play(0, 0));
    r.run((1.0 * FS / BLOCK as f64) as u32);
    let realtime = |p: u32| ((p >> 8) & 0xff) as u8;
    let transport: Vec<u8> = r
        .out
        .iter()
        .map(|p| realtime(p.0))
        .filter(|&b| b != 0xF8)
        .collect();
    assert_eq!(transport, [0xFA], "a start alone: {:x?}", r.out);
    let first_call = r.calls[0];
    assert_eq!(first_call.0, 0, "the song from its top");
    let first_clock = r
        .out
        .iter()
        .find(|p| realtime(p.0) == 0xF8)
        .expect("clocks after the start")
        .1;
    assert!(
        first_clock >= first_call.1 && first_clock - first_call.1 <= 33,
        "the first clock at sample {first_clock}, the first call at {}",
        first_call.1
    );
}

#[test]
fn clock_out_waits_for_room_rather_than_dropping_its_transport() {
    let mut r = Rig::new();
    unsafe { midi_sync_set(&mut *r.sync, MIDI_SYNC_AUTO, 1) };
    r.run(10);
    r.sync.out_room = 0;
    assert!(r.t.play(0, 0));
    r.run(20);
    assert!(r.out.is_empty(), "sent with no room: {:x?}", r.out);
    r.sync.out_room = MIDI_SYNC_OUT_MAX as u8;
    r.run(1);
    let realtime = |p: u32| ((p >> 8) & 0xff) as u8;
    assert_eq!(realtime(r.out[0].0), 0xFA, "the start once there is room");
    r.run((1.0 * FS / BLOCK as f64) as u32);
    assert!(r.t.stop());
    r.run(10);
    assert_eq!(realtime(r.out.last().unwrap().0), 0xFC);
    r.out.clear();
    r.sync.out_room = 1;
    assert!(r.t.resume());
    r.run(10);
    assert!(r.out.iter().all(|p| p.0 & 0xff != 0x03), "{:x?}", r.out);
    assert!(r.out.iter().all(|p| realtime(p.0) != 0xFB), "{:x?}", r.out);
    r.sync.out_room = MIDI_SYNC_OUT_MAX as u8;
    r.run(1);
    assert_eq!(r.out[0].0 & 0xff, 0x03, "{:x?}", r.out);
    assert_eq!(((r.out[0].0 >> 8) & 0xff) as u8, 0xF2);
    assert_eq!(realtime(r.out[1].0), 0xFB);
}

#[test]
fn nothing_goes_out_while_a_clock_is_followed() {
    let mut r = Rig::new();
    unsafe { midi_sync_set(&mut *r.sync, MIDI_SYNC_AUTO, 1) };
    r.run(10);
    let t0 = r.samples() * TPS;
    r.midi.extend(clock(t0, 120.0, 4, |_| 0));
    start_at(&mut r, t0 + 6_000_000);
    r.run((2.0 * FS / BLOCK as f64) as u32);
    let st = r.status();
    assert!(
        r.calls.len() > 10,
        "it played: {} calls, state {}, sync aligning {} want {} aligns {} clock active {} based {} period {}",
        r.calls.len(),
        st.state,
        r.sync.aligning,
        r.sync.want,
        r.sync.aligns,
        r.sync.clock.active,
        r.sync.clock.based,
        r.sync.clock.period
    );
    assert!(r.out.is_empty(), "sent {:x?}", r.out);
}

#[test]
fn a_clock_that_stops_carries_the_song_on_with_auto_and_stops_it_with_follow() {
    for (mode, playing) in [(MIDI_SYNC_AUTO, true), (MIDI_SYNC_FOLLOW, false)] {
        let mut r = Rig::new();
        unsafe { midi_sync_set(&mut *r.sync, mode, 0) };
        r.run(10);
        let t0 = r.samples() * TPS;
        r.midi.extend(clock(t0, 150.0, 4, |_| 0));
        start_at(&mut r, t0 + 4_000_000);
        r.run((1.5 * FS / BLOCK as f64) as u32);
        let period = r.status().period_q16;
        r.run((1.5 * FS / BLOCK as f64) as u32);
        assert_eq!(r.sync.lost, 1, "mode {mode}");
        let st = r.status();
        assert_eq!(
            st.state == MIDI_SYNC_PLAYING as u8,
            playing,
            "mode {mode}: state {}",
            st.state
        );
        if playing {
            let ratio = f64::from(st.period_q16) / f64::from(period);
            assert!((ratio - 1.0).abs() < 1e-3, "the last tempo kept: {ratio}");
        }
    }
}

#[test]
fn clock_off_ignores_the_transport() {
    let mut r = Rig::new();
    unsafe { midi_sync_set(&mut *r.sync, MIDI_SYNC_OFF, 1) };
    r.run(10);
    let t0 = r.samples() * TPS;
    r.midi.extend(clock(t0, 120.0, 2, |_| 0));
    start_at(&mut r, t0 + 1_000_000);
    r.run((1.0 * FS / BLOCK as f64) as u32);
    assert_eq!(r.status().state, MIDI_SYNC_IDLE as u8);
}

#[test]
fn clock_off_releases_a_held_start_before_or_after_loading() {
    for request in [
        MIDI_SYNC_WANT_START,
        MIDI_SYNC_WANT_SEEK | MIDI_SYNC_WANT_RESUME,
    ] {
        for load_blocks in [0, 20] {
            let mut r = Rig::new();
            r.run(10);
            r.sync.clock.active = 1;
            r.sync.want = request as u8;
            r.sync.seek_pos = 32;
            let mut out = [0; MIDI_SYNC_OUT_MAX as usize];
            let now = r.samples() * TPS;
            unsafe { midi_sync_poll(&mut *r.sync, now, out.as_mut_ptr()) };
            assert_eq!(r.status().state, MIDI_SYNC_LOADING as u8);
            r.run(load_blocks);
            assert_eq!(r.t.calls(), 0, "held start");
            unsafe { midi_sync_set(&mut *r.sync, MIDI_SYNC_OFF, 0) };
            r.run(100);
            assert!(
                r.t.calls() > 0,
                "CLOCK OFF left the song held after {load_blocks} blocks"
            );
            assert_eq!(r.t.sc.hold_next, 0);
        }
    }
}

#[test]
fn clock_off_restores_the_native_frame_period() {
    let mut r = Rig::new();
    r.run(10);
    let t0 = r.samples() * TPS;
    r.midi.extend(clock(t0, 150.0, 6, |_| 0));
    start_at(&mut r, t0 + 4_000_000);
    r.run((1.5 * FS / BLOCK as f64) as u32);
    let native = unsafe { song_period_of(r.t.sc.opts.multispeed.into(), r.t.sc.opts.ntsc.into()) };
    assert_ne!(r.t.sc.period.q, native.q, "the external tempo differs");
    unsafe { midi_sync_set(&mut *r.sync, MIDI_SYNC_OFF, 0) };
    r.run(30);
    assert_eq!(r.t.sc.period.q, native.q);
    assert_eq!(r.t.sc.period.r, native.r);
    assert_eq!(r.t.sc.period.den, native.den);
}

#[test]
fn a_stop_during_loading_waits_until_the_transport_can_stop() {
    let mut r = Rig::new();
    r.run(10);
    r.sync.want = MIDI_SYNC_WANT_START as u8;
    let mut out = [0; MIDI_SYNC_OUT_MAX as usize];
    let now = r.samples() * TPS;
    unsafe { midi_sync_poll(&mut *r.sync, now, out.as_mut_ptr()) };
    assert_eq!(r.status().state, MIDI_SYNC_LOADING as u8);
    r.sync.want = MIDI_SYNC_WANT_STOP as u8;
    r.run(100);
    assert_eq!(r.status().state, MIDI_SYNC_STOPPED as u8);
}

#[test]
fn song_select_picks_the_subtune_and_names_fit() {
    let mut r = Rig::new();
    r.midi.push(At {
        ticks: 0,
        bytes: [0xF3, 2, 0],
        len: 2,
    });
    r.run(2);
    assert_eq!(r.sync.subtune, 2);
    for m in 0..MIDI_SYNC_MODES {
        let n = unsafe { std::ffi::CStr::from_ptr(midi_sync_mode_name(m)) };
        assert!(!n.is_empty() && n.to_bytes().len() <= 9);
    }
}

struct TuneHook {
    loaded: bool,
    got: Vec<u32>,
    asked: u32,
}

unsafe extern "C" fn tune_hook(ctx: *mut std::ffi::c_void, b: u32) -> i32 {
    let h = unsafe { &mut *ctx.cast::<TuneHook>() };
    if b == MIDI_SYNC_TUNE_ASK {
        h.asked += 1;
    } else {
        h.got.push(b);
    }
    i32::from(h.loaded)
}

#[test]
fn a_loaded_tune_takes_start_stop_and_continue_and_the_song_gets_nothing() {
    let mut r = Rig::new();
    let mut hook = Box::new(TuneHook {
        loaded: true,
        got: Vec::new(),
        asked: 0,
    });
    let hp: *mut TuneHook = &mut *hook;
    unsafe { midi_sync_tune(&mut *r.sync, Some(tune_hook), hp.cast()) };
    r.run(10);
    let t0 = r.samples() * TPS;
    r.midi.extend(clock(t0, 120.0, 4, |_| 0));
    for (at, b) in [(1u32, 0xFAu8), (2, 0xFC), (3, 0xFB)] {
        r.midi.push(rt(t0 + at * 1_000_000, b));
    }
    r.midi.push(At {
        ticks: t0 + 4_000_000,
        bytes: [0xF2, 0x10, 0x00],
        len: 3,
    });
    r.midi.push(At {
        ticks: t0 + 4_100_000,
        bytes: [0xF3, 0x02, 0x00],
        len: 2,
    });
    r.run((2.5 * FS / BLOCK as f64) as u32);
    assert_eq!(hook.got, [0xFA, 0xFC, 0xFB], "the tune's transport");
    assert!(hook.asked > 96, "every message asked first");
    assert_eq!(
        r.status().state,
        MIDI_SYNC_IDLE as u8,
        "the song never played"
    );
    assert_eq!(r.sync.clock.active, 0, "no clock followed");
    assert_eq!(
        (r.sync.want, r.sync.subtune),
        (0, 0),
        "no request, no subtune"
    );
    unsafe { midi_sync_set(&mut *r.sync, MIDI_SYNC_OFF, 0) };
    let t1 = r.samples() * TPS;
    r.midi.push(rt(t1 + 100_000, 0xFA));
    r.run(20);
    assert_eq!(hook.got, [0xFA, 0xFC, 0xFB]);
    unsafe { midi_sync_set(&mut *r.sync, MIDI_SYNC_AUTO, 0) };
    hook.loaded = false;
    let t2 = r.samples() * TPS;
    r.midi.extend(clock(t2, 120.0, 2, |_| 0));
    start_at(&mut r, t2 + 500_000);
    r.run((1.5 * FS / BLOCK as f64) as u32);
    assert_eq!(hook.got, [0xFA, 0xFC, 0xFB]);
    assert_eq!(r.status().state, MIDI_SYNC_PLAYING as u8, "the song plays");
}
