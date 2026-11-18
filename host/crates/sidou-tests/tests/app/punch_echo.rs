// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::Rig;

const FX: u32 = SID_SLOT_FX;

struct Throw {
    rig: Rig,
    e: Box<punch_echo>,
    slots: Box<sid_slots>,
    _pool: Box<sid_slot_pool>,
    fx: Box<fx_live>,
    applied: usize,
}

impl Throw {
    fn new(own: Option<(u32, [u8; 4])>) -> Throw {
        let mut rig = Rig::new(SID_MODEL_6581);
        if let Some((alg, values)) = own {
            unsafe {
                live_slot_pick(&mut *rig.live, FX, alg);
                for (f, v) in values.into_iter().enumerate() {
                    live_slot_set(&mut *rig.live, FX, LIVE_SLOT_AMOUNT + f as u32, i32::from(v));
                }
            }
        }
        let mut pool = Box::<sid_slot_pool>::default();
        let mut slots = Box::<sid_slots>::default();
        let mut fx = Box::<fx_live>::default();
        let mut e = Box::<punch_echo>::default();
        unsafe {
            sid_slots_init(&mut *slots, 44_118, 1, &mut *pool);
            fx_live_init(&mut *fx, &mut *slots);
            punch_echo_init(&mut *e, &mut *rig.live);
        }
        let mut t = Throw {
            rig,
            e,
            slots,
            _pool: pool,
            fx,
            applied: 0,
        };
        t.apply();
        t
    }

    fn apply(&mut self) {
        for &(reg, value) in &self.rig.log()[self.applied..] {
            unsafe { fx_live_apply(&mut *self.fx, reg, value) };
        }
        self.applied = self.rig.log().len();
    }

    fn hold(&mut self, on: bool, ms: u32) {
        unsafe { punch_echo_hold(&mut *self.e, i32::from(on), ms) };
        self.apply();
    }

    fn tick(&mut self, ms: u32) {
        unsafe { punch_echo_tick(&mut *self.e, ms) };
        self.apply();
    }

    fn heard(&self) -> (u32, [u8; 4], f32) {
        let p = self.fx.p[FX as usize];
        let byte = |v: f32| (v * LIVE_SLOT_MAX as f32).round() as u8;
        (
            p.algorithm,
            [byte(p.amount), byte(p.a), byte(p.b), byte(p.mix)],
            self.slots.send,
        )
    }

    fn own(&self) -> (u32, [u8; 4], f32) {
        let s = self.rig.live.slot[FX as usize];
        (u32::from(s[0]), [s[1], s[2], s[3], s[4]], 1.0)
    }
}

#[test]
fn echo_throw_sends_into_a_delay_lets_its_tail_ring_and_then_gives_the_fx_slot_back() {
    for (what, own, alg) in [
        ("the slot off", None, SID_FXSLOT_DELAY),
        ("a reverb", Some((SID_FXSLOT_REVERB_LUSH, [80, 48, 96, 40])), SID_FXSLOT_DELAY),
        ("a ping-pong", Some((SID_FXSLOT_PINGPONG, [20, 100, 10, 30])), SID_FXSLOT_PINGPONG),
    ] {
        let mut t = Throw::new(own);
        let mine = t.own();
        assert_eq!(t.heard().0, mine.0, "{what}: the slot before");
        t.hold(true, 1000);
        let (heard_alg, values, send) = t.heard();
        assert_eq!((heard_alg, send), (alg, 1.0), "{what}: thrown into a delay");
        assert_eq!(values[0], PUNCH_ECHO_FEEDBACK as u8, "{what}: the feedback");
        assert_eq!(values[3], PUNCH_ECHO_MIX as u8, "{what}: the mix");
        if alg == mine.0 {
            assert_eq!(values[1..3], mine.1[1..3], "{what}: the player's own time and damping");
        }
        t.hold(false, 2000);
        assert_eq!(t.heard().2, 0.0, "{what}: the send closes at release");
        assert_eq!(t.heard().0, alg, "{what}: the tail rings in the delay");
        t.tick(2000 + PUNCH_ECHO_TAIL_MS - 1);
        assert_eq!(t.heard().0, alg, "{what}: still ringing");
        t.tick(2000 + PUNCH_ECHO_TAIL_MS);
        assert_eq!(t.heard(), mine, "{what}: the FX slot back as it was");
        assert_eq!(t.own(), mine, "{what}: nothing written into the sound");
    }
}

#[test]
fn a_second_throw_in_the_tail_opens_the_send_again_and_starts_a_new_tail() {
    let mut t = Throw::new(None);
    t.hold(true, 0);
    t.hold(false, 100);
    t.tick(100 + PUNCH_ECHO_TAIL_MS / 2);
    t.hold(true, 100 + PUNCH_ECHO_TAIL_MS / 2);
    assert_eq!(t.heard().2, 1.0, "the send opens again");
    t.tick(100 + PUNCH_ECHO_TAIL_MS);
    assert_eq!(t.heard().0, SID_FXSLOT_DELAY, "a held throw has no tail to end");
    t.hold(false, 200 + PUNCH_ECHO_TAIL_MS);
    t.tick(200 + 2 * PUNCH_ECHO_TAIL_MS - 1);
    assert_eq!(t.heard().0, SID_FXSLOT_DELAY, "the new tail rings its whole length");
    t.tick(200 + 2 * PUNCH_ECHO_TAIL_MS);
    assert_eq!(t.heard().0, SID_FXSLOT_NONE);
}
