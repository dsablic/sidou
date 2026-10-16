// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Which chip the keys play (`app/keys_route.c`) and each chip's setup (`app/chip_ctl.c`),
//! through the very ring the device runs: live on keys_route, chips A and B on one ring.

use std::ffi::c_void;

use sidou_sys::*;

const RATE: u32 = 44_118;

unsafe extern "C" fn drain_now(ctx: *mut c_void) {
    let o = unsafe { &mut *ctx.cast::<reg_ring_out>() };
    unsafe { reg_ring_drain(o.ring, o.chips) };
}

unsafe extern "C" fn moved(ctx: *mut c_void, chip: u32) {
    let k = unsafe { &mut *ctx.cast::<Keys>() };
    unsafe { reg_ring_set_keys(&mut *k.chips, chip) };
    k.moves.push(chip);
}

/// The device's instrument (platform/fm1/main.c): live on keys_route over two chips.
struct Keys {
    ring: Box<reg_ring>,
    chips: Box<reg_ring_chips>,
    outs: [Box<reg_ring_out>; 2],
    kr: Box<keys_route>,
    live: Box<live>,
    moves: Vec<u32>,
}

impl Keys {
    fn new() -> Box<Self> {
        let mut ring = Box::<reg_ring>::default();
        let mut chips = Box::<reg_ring_chips>::default();
        unsafe {
            reg_ring_init(&mut *ring);
            reg_ring_chips_init(&mut *chips, SID_MODEL_6581, SID_CLOCK_PAL, RATE);
        }
        let mut out = |chip: u8| {
            let mut o = Box::new(reg_ring_out {
                ring: &mut *ring,
                chips: &mut *chips,
                chip,
                wait: Some(drain_now),
                ..Default::default()
            });
            let p: *mut reg_ring_out = &mut *o;
            o.wait_ctx = p.cast();
            o
        };
        let outs = [out(0), out(1)];
        let mut k = Box::new(Keys {
            ring,
            chips,
            outs,
            kr: Box::default(),
            live: Box::default(),
            moves: Vec::new(),
        });
        let kp: *mut Keys = &mut *k;
        let a: *mut reg_ring_out = &mut *k.outs[0];
        let b: *mut reg_ring_out = &mut *k.outs[1];
        let krp: *mut keys_route = &mut *k.kr;
        unsafe {
            keys_route_init(
                &mut *k.kr,
                &mut *k.live,
                Some(reg_ring_sink),
                Some(reg_ring_prepare_step),
                a.cast(),
                b.cast(),
            );
            k.kr.moved = Some(moved);
            k.kr.moved_ctx = kp.cast();
            live_init(
                &mut *k.live,
                Some(keys_route_sink),
                krp.cast(),
                SID_MODEL_6581,
            );
            live_set_preparer(&mut *k.live, Some(keys_route_prepare), krp.cast());
            keys_route_start(&mut *k.kr);
        }
        k.block();
        k
    }

    fn block(&mut self) {
        unsafe {
            reg_ring_commit(&mut *self.ring);
            reg_ring_pass_end(&mut *self.chips);
            reg_ring_drain(&mut *self.ring, &mut *self.chips);
        }
    }

    /// A main-loop pass with a tick (platform/fm1/main.c): live's, then keys_route's.
    fn tick(&mut self) {
        unsafe {
            live_tick(&mut *self.live);
            keys_route_tick(&mut *self.kr);
        }
        self.block();
    }

    fn ticks(&mut self, n: u32) {
        for _ in 0..n {
            self.tick();
        }
    }

    fn chip(&mut self, x: u32) -> &mut sid {
        unsafe { &mut *reg_ring_chip_of(&mut *self.chips, x) }
    }

    fn song(&mut self, on: bool) {
        unsafe { keys_route_set_song(&mut *self.kr, i32::from(on)) };
    }
}

fn render(s: &mut sid, n: usize) -> Vec<i16> {
    let mut out = vec![0i16; n];
    unsafe { sid_render(s, out.as_mut_ptr(), n as u32) };
    out
}

fn gates(s: &sid) -> [u8; 3] {
    s.env.map(|e| e.gate)
}

/// The setup chip x has, as its ring sees it: the profile's filter rows and the ramp's
/// character.
fn setup(k: &Keys, x: usize) -> (u8, u8, [u8; 5]) {
    let c = &*k.chips;
    let s = &c.chip[usize::from(c.active[x])];
    let n = c.ramp[x].now;
    (
        s.filter_curve,
        s.model as u8,
        [n.profile, n.age, n.dac, n.thump, n.bleed],
    )
}

#[test]
fn chip_ctl_follows_a_setup_and_records_what_it_saw() {
    let mut k = Keys::new();
    let want = chip_setup {
        profile: SID_CHIP_6581_WORN as u8,
        controls: [40, 127, 64, 10],
        gate_reset: 0,
    };
    let mut ctl = Box::<chip_ctl>::default();
    let b: *mut reg_ring_out = &mut *k.outs[1];
    unsafe {
        chip_ctl_init(
            &mut *ctl,
            Some(reg_ring_sink),
            b.cast(),
            Some(reg_ring_prepare_step),
            b.cast(),
        );
    }
    assert_eq!(unsafe { chip_ctl_has(&*ctl, &want) }, 0);
    let mut n = 0;
    while unsafe { chip_ctl_follow(&mut *ctl, &want) } == 0 {
        k.block();
        n += 1;
        assert!(n < 40, "never got there");
    }
    k.block();
    assert_eq!(unsafe { chip_ctl_has(&*ctl, &want) }, 1);
    assert_eq!(k.chip(1).filter_curve, 3, "WORN");
    assert!(k.chip(1).env.iter().all(|e| e.gate_reset == 0));
    assert_eq!(k.outs[1].unprepared_resets, 0, "a reset made at once");
    unsafe { chip_ctl_saw(&mut *ctl, LIVE_REG_CHAR_FIRST as u8 + 2, 3) };
    assert_eq!(ctl.has.controls[1], 3);
    assert_eq!(unsafe { chip_ctl_has(&*ctl, &want) }, 0);
}

/// With no song the keys play chip A as in M1; chip B is made with the keys' setup in the
/// background (prepared, never reset at once), and holds it after every change: a profile, the
/// FX button, the controls, the envelope mode.
#[test]
fn chip_b_holds_the_keys_setup_after_every_change() {
    let mut k = Keys::new();
    k.ticks(20);
    assert_eq!(k.live.keys_chip, 0);
    assert_eq!(
        setup(&k, 1),
        setup(&k, 0),
        "chip B made with the keys' setup"
    );
    assert_eq!(
        k.outs[1].unprepared_resets, 0,
        "chip B's first reset made at once"
    );
    for change in 0..4 {
        unsafe {
            match change {
                0 => live_param_set(&mut *k.live, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN),
                1 => live_button(&mut *k.live, LIVE_BUTTON_FX),
                2 => {
                    live_param_set(&mut *k.live, LIVE_PARAM_AGE, 90);
                    live_param_set(&mut *k.live, LIVE_PARAM_DAC, 60);
                }
                _ => live_param_set(
                    &mut *k.live,
                    LIVE_PARAM_ENVELOPE_MODE,
                    LIVE_ENVELOPE_AUTHENTIC,
                ),
            }
        }
        k.ticks(400);
        assert_eq!(setup(&k, 1), setup(&k, 0), "change {change}");
        let gr = |s: &sid| s.env[0].gate_reset;
        let (a, b) = (gr(k.chip(0)), gr(k.chip(1)));
        assert_eq!(a, b, "change {change}: the envelope mode");
    }
    assert!(k.moves.is_empty(), "the keys moved with no song");
    assert_eq!(k.outs[1].unprepared_resets, 0);
    assert_eq!(k.chips.lost_resets, 0);
}

/// A song starts on chip A: the keys move to chip B (same setup), a key held across the start
/// is released on A and sounds again on B; new notes sound on B. The song stops with a key
/// held: the keys stay on B until it is released, then go back to A.
#[test]
fn a_song_moves_the_keys_to_chip_b_and_back_when_no_key_is_held() {
    let mut k = Keys::new();
    k.ticks(20);
    unsafe { live_note_on(&mut *k.live, 60, 100) };
    k.tick();
    assert_eq!(gates(k.chip(0))[0], 1, "the key sounds on A");
    k.song(true);
    k.tick();
    assert_eq!(k.moves, [1]);
    assert_eq!(k.live.keys_chip, 1);
    assert_eq!(gates(k.chip(0))[0], 0, "released on A, the song's now");
    assert_eq!(gates(k.chip(1))[0], 1, "the held key sounds again on B");
    assert!(render(k.chip(1), 4096).iter().any(|&s| s != 0));
    unsafe { live_note_on(&mut *k.live, 64, 100) };
    k.tick();
    assert_eq!(
        gates(k.chip(1)).iter().filter(|&&g| g == 1).count(),
        2,
        "a new note on B"
    );
    assert_eq!(gates(k.chip(0)), [0, 0, 0]);
    // The song stops: two keys held, so the keys stay on B.
    k.song(false);
    k.ticks(30);
    assert_eq!(k.live.keys_chip, 1, "moved with keys held");
    unsafe {
        live_note_off(&mut *k.live, 60);
        live_note_off(&mut *k.live, 64);
    }
    k.ticks(2);
    assert_eq!(k.moves, [1, 0], "back to A once no key is held");
    unsafe { live_note_on(&mut *k.live, 67, 100) };
    k.tick();
    assert!(gates(k.chip(0)).contains(&1), "the keys play A again");
    assert_eq!(gates(k.chip(1)), [0, 0, 0], "and not B");
    assert_eq!(k.chips.lost_resets, 0);
}

/// While a song plays on A the keys' setup changes act on chip B (the keys'), not on A (the
/// song's); when the song stops A is prepared to the keys' setup, and only then do the keys go
/// back to it.
#[test]
fn during_a_song_the_keys_setup_changes_chip_b_and_a_takes_it_after() {
    let mut k = Keys::new();
    k.ticks(20);
    k.song(true);
    k.tick();
    assert_eq!(k.live.keys_chip, 1);
    let a_before = setup(&k, 0);
    unsafe { live_button(&mut *k.live, LIVE_BUTTON_FX) };
    k.ticks(30);
    assert_eq!(
        k.chip(1).model,
        SID_MODEL_8580,
        "the FX button on the keys' chip"
    );
    assert_eq!(setup(&k, 0), a_before, "the song's chip changed");
    k.song(false);
    let mut back = None;
    for t in 1..=40 {
        k.tick();
        if back.is_none() && k.live.keys_chip == 0 {
            back = Some(t);
            assert_eq!(
                k.chip(0).model,
                SID_MODEL_8580,
                "back before A had the keys' setup"
            );
        }
    }
    let back = back.expect("the keys never went back");
    assert!(
        back > 7,
        "back after {back} ticks: A was not prepared first"
    );
    assert_eq!(k.chips.lost_resets, 0);
}

/// KEYS A: during a song the keys stay on chip A; a key takes its voice from the song
/// (keys_route_voices_taken) while it sounds and gives it back at the release; the song keeps
/// the filter and the volume (the instrument's writes to $15-$18 do not reach the chip) and its
/// setup (the FX button does not reset the chip).
#[test]
fn keys_a_takes_a_voice_from_the_song_and_gives_it_back() {
    let mut k = Keys::new();
    k.ticks(20);
    unsafe { live_set_keys_mode(&mut *k.live, LIVE_KEYS_A) };
    k.song(true);
    k.tick();
    assert!(k.moves.is_empty(), "KEYS A moved the keys");
    assert_eq!(unsafe { keys_route_voices_taken(&*k.kr) }, 0);
    unsafe { live_note_on(&mut *k.live, 60, 100) };
    k.tick();
    let taken = unsafe { keys_route_voices_taken(&*k.kr) };
    assert_eq!(taken.count_ones(), 1, "the key's voice: {taken:03b}");
    let v = taken.trailing_zeros() as usize;
    assert_eq!(gates(k.chip(0))[v], 1);
    let (fc, model) = (k.chip(0).fc, k.chip(0).model);
    unsafe {
        live_knob(&mut *k.live, LIVE_KNOB_CUTOFF, -20);
        live_button(&mut *k.live, LIVE_BUTTON_FX);
    }
    k.ticks(20);
    assert_eq!(k.chip(0).fc, fc, "the song's filter changed");
    assert_eq!(k.chip(0).model, model, "the song's chip was reset");
    unsafe { live_note_off(&mut *k.live, 60) };
    k.tick();
    assert_eq!(
        unsafe { keys_route_voices_taken(&*k.kr) },
        0,
        "the voice given back"
    );
    // The song stops: chip A takes the keys' setup (the FX button's model) and filter.
    k.song(false);
    k.ticks(30);
    assert_eq!(k.chip(0).model, SID_MODEL_8580);
    let want = u32::from(k.live.cutoff_out);
    assert_eq!(k.chip(0).fc, want);
    assert_eq!(k.chips.lost_resets, 0);
}

/// The keys sound the same on either chip: the same key on chip A with no song and on chip B
/// during one, each chip fresh from the same setup, gives the same samples.
#[test]
fn a_key_sounds_the_same_on_either_chip() {
    let play = |song: bool| {
        let mut k = Keys::new();
        k.ticks(20);
        k.song(song);
        k.tick();
        unsafe { live_note_on(&mut *k.live, 57, 100) };
        k.tick();
        let x = u32::from(k.live.keys_chip);
        assert_eq!(x, u32::from(song));
        // Each chip's voice 1 from its own last state: compare the envelopes and the
        // frequencies the chip got, then the sound of a fresh chip with those registers.
        let s = k.chip(x);
        (
            s.osc[0].freq,
            s.env[0].attack,
            s.env[0].sustain,
            s.filter_curve,
            s.model,
            gates(s),
        )
    };
    assert_eq!(play(false), play(true));
}

/// A pass at its heaviest with the keys moving in it (reg_ring.c, REG_RING_WORST_PASS): a
/// preset (every register), the FX button's request, every key down and up, then the move to
/// chip B (gate-offs and every register again), with no wait for room.
#[test]
fn a_move_in_a_heavy_pass_fits_the_ring_without_waiting() {
    let mut k = Keys::new();
    k.ticks(20);
    for n in [48u8, 55, 60] {
        unsafe { live_note_on(&mut *k.live, n, 100) };
    }
    k.block();
    k.song(true);
    unsafe {
        live_set_preset(&mut *k.live, 1);
        for n in 36..63u8 {
            live_note_on(&mut *k.live, n, 100);
            live_note_off(&mut *k.live, n);
        }
        live_tick(&mut *k.live);
        keys_route_tick(&mut *k.kr);
    }
    let used = unsafe { reg_ring_used(&*k.ring) };
    println!("a heavy pass with a move: {used} entries");
    assert_eq!(k.live.keys_chip, 1, "the keys moved");
    assert!(used <= REG_RING_WORST_PASS, "{used} entries");
    assert_eq!(k.outs[0].full_waits + k.outs[1].full_waits, 0);
    k.block();
    assert_eq!(k.chips.lost_resets, 0);
}
