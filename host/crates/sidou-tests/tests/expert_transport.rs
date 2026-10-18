// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::c_void;

use sidou_sys::*;

unsafe extern "C" fn record(ctx: *mut c_void, reg: u8, value: u8) {
    unsafe { &mut *ctx.cast::<Vec<(u8, u8)>>() }.push((reg, value));
}

unsafe extern "C" fn discard(_: *mut c_void, _: u8, _: u8) {}

fn transport_pair() -> (live, live, midi_out) {
    let mut sender = live::default();
    let mut receiver = live::default();
    let mut out = midi_out::default();
    unsafe {
        for l in [&mut sender, &mut receiver] {
            live_init(l, Some(discard), std::ptr::null_mut(), SID_MODEL_6581);
            live_param_set(l, LIVE_PARAM_EXPERT + SID_EXPERT_FILTER_6581_CURVE, 100);
        }
        midi_out_init(&mut out, &sender, 0, 0);
    }
    (sender, receiver, out)
}

fn tick_packets(out: &mut midi_out, sender: &live, room: u32) -> Vec<u32> {
    let mut packets = [0u32; MIDI_OUT_TICK_PACKETS as usize];
    let n = unsafe { midi_out_tick(out, sender, room, packets.as_mut_ptr()) };
    packets[..n as usize].to_vec()
}

fn receive_packets(receiver: &mut live, packets: &[u32], learn: Option<&mut midi_learn>) {
    let mut input = midi_map_port::default();
    unsafe { midi_map_port_init(&mut input, receiver) };
    if let Some(learn) = learn {
        input.learn = learn;
    }
    for packet in packets {
        let [_, _, cc, value] = packet.to_le_bytes();
        unsafe { midi_map_cc(&mut input, 0, cc, value) };
    }
}

#[test]
fn a_canceled_macro_still_releases_overrides_and_restores_learned_127() {
    let (mut sender, mut receiver, mut out) = transport_pair();
    let mut learn = midi_learn::default();
    let retained = LIVE_PARAM_EXPERT + SID_EXPERT_EXTERNAL_HP;
    unsafe {
        midi_learn_init(&mut learn);
        assert_eq!(midi_learn_bind(&mut learn, 14, retained), 1);
        for l in [&mut sender, &mut receiver] {
            live_param_set(l, retained, 127);
        }
        midi_out_init(&mut out, &sender, 0, 0);
        midi_out_learn(&mut out, &learn);
        live_param_set(&mut sender, LIVE_PARAM_AGE, 60);
    }
    assert!(tick_packets(&mut out, &sender, 0).is_empty());
    assert_ne!(out.release_pending, 0);
    unsafe { live_param_set(&mut sender, LIVE_PARAM_AGE, 0) };
    let mut sent = Vec::new();
    for _ in 0..10 {
        let packets = tick_packets(&mut out, &sender, MIDI_OUT_TICK_PACKETS);
        receive_packets(&mut receiver, &packets, Some(&mut learn));
        sent.extend(packets);
    }
    assert_eq!(receiver.expert, sender.expert);
    assert!(sent.iter().any(|p| p.to_le_bytes()[2] == 85));
    assert!(sent.iter().any(|p| p.to_le_bytes()[2..] == [14, 127]));
    assert!(tick_packets(&mut out, &sender, 16).is_empty());
    assert_eq!(out.release_pending, 0);
}

#[test]
fn an_emitted_macro_carries_its_release_without_a_profile_reset() {
    let (mut sender, mut receiver, mut out) = transport_pair();
    unsafe { live_param_set(&mut sender, LIVE_PARAM_AGE, 60) };
    assert!(tick_packets(&mut out, &sender, 0).is_empty());
    assert_ne!(out.release_pending, 0);
    let packets = tick_packets(&mut out, &sender, MIDI_OUT_TICK_PACKETS);
    assert_eq!(out.release_pending, 0);
    assert!(packets.iter().all(|p| p.to_le_bytes()[2] != 85));
    receive_packets(&mut receiver, &packets, None);
    assert_eq!(receiver.character, sender.character);
    assert_eq!(receiver.expert, sender.expert);
}

#[test]
fn a_remote_expert_after_a_queued_macro_is_restored_after_that_macro_is_sent() {
    let (mut sender, mut receiver, mut out) = transport_pair();
    unsafe { live_param_set(&mut sender, LIVE_PARAM_AGE, 60) };
    assert!(tick_packets(&mut out, &sender, 0).is_empty());
    unsafe {
        live_param_set(
            &mut sender,
            LIVE_PARAM_EXPERT + SID_EXPERT_FILTER_6581_CURVE,
            100,
        );
        live_param_set(
            &mut receiver,
            LIVE_PARAM_EXPERT + SID_EXPERT_FILTER_6581_CURVE,
            100,
        );
        midi_out_remote(&mut out, &sender);
    }
    for _ in 0..10 {
        let packets = tick_packets(&mut out, &sender, MIDI_OUT_TICK_PACKETS);
        assert!(packets.iter().all(|p| p.to_le_bytes()[2] != 85));
        receive_packets(&mut receiver, &packets, None);
    }
    assert_eq!(receiver.expert, sender.expert);
    assert_eq!(receiver.character, sender.character);
}

#[test]
fn a_learned_explicit_127_is_not_deduplicated_against_macro_following_255() {
    let (mut sender, mut receiver, mut out) = transport_pair();
    let mut learn = midi_learn::default();
    let param = LIVE_PARAM_EXPERT + SID_EXPERT_EXTERNAL_HP;
    unsafe {
        midi_learn_init(&mut learn);
        assert_eq!(midi_learn_bind(&mut learn, 14, param), 1);
        midi_out_learn(&mut out, &learn);
        live_param_set(&mut sender, param, 127);
    }
    let packets = tick_packets(&mut out, &sender, MIDI_OUT_TICK_PACKETS);
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].to_le_bytes()[2..], [14, 127]);
    receive_packets(&mut receiver, &packets, Some(&mut learn));
    assert_eq!(receiver.expert, sender.expert);
}

#[test]
fn dropping_or_resyncing_forgets_unsent_release_obligations() {
    for resync in [false, true] {
        let (mut sender, _, mut out) = transport_pair();
        unsafe { live_param_set(&mut sender, LIVE_PARAM_AGE, 60) };
        assert!(tick_packets(&mut out, &sender, 0).is_empty());
        assert_ne!(out.release_pending, 0);
        unsafe {
            live_param_set(&mut sender, LIVE_PARAM_AGE, 0);
            if resync {
                midi_out_init(&mut out, &sender, 0, 0);
            } else {
                midi_out_drop(&mut out, &sender);
            }
        }
        assert_eq!(out.release_pending, 0);
        assert!(tick_packets(&mut out, &sender, MIDI_OUT_TICK_PACKETS).is_empty());
    }
}

#[test]
fn expert_nrpn_edits_keep_a_held_gate_and_macros_reclaim_the_path() {
    let mut writes = Vec::<(u8, u8)>::new();
    let mut l = live::default();
    let mut port = midi_map_port::default();
    unsafe {
        live_init(
            &mut l,
            Some(record),
            (&mut writes as *mut Vec<(u8, u8)>).cast(),
            SID_MODEL_6581,
        );
        live_note_on(&mut l, 60, 100);
        writes.clear();
        midi_map_port_init(&mut port, &mut l);
        midi_map_cc(&mut port, 0, 99, 2);
        midi_map_cc(&mut port, 0, 98, 5);
        midi_map_cc(&mut port, 0, 6, 0);
        midi_map_cc(&mut port, 0, 38, 100);
    }
    assert_eq!(
        unsafe { live_param_get(&l, 42) },
        100,
        "expert NRPN reaches the keys' setup"
    );
    assert!(l.voice[0].gate != 0);
    assert!(
        writes.is_empty(),
        "expert edits stage, never reset or rewrite a gate"
    );
    unsafe { live_param_set(&mut l, LIVE_PARAM_AGE, 127) };
    assert_ne!(
        unsafe { live_param_get(&l, 42) },
        100,
        "a macro move reclaims its analogue path"
    );
}

#[test]
fn midi_out_sends_macros_before_explicit_experts_without_freezing_inherited_paths() {
    let mut sender = live::default();
    let mut receiver = live::default();
    let mut writes = Vec::<(u8, u8)>::new();
    let ctx = (&mut writes as *mut Vec<(u8, u8)>).cast();
    let mut out = midi_out::default();
    let mut input = midi_map_port::default();
    unsafe {
        live_init(&mut sender, Some(record), ctx, SID_MODEL_6581);
        live_init(&mut receiver, Some(record), ctx, SID_MODEL_6581);
        for l in [&mut sender, &mut receiver] {
            live_param_set(l, LIVE_PARAM_EXPERT + SID_EXPERT_FILTER_6581_CURVE, 100);
            live_param_set(l, LIVE_PARAM_EXPERT + SID_EXPERT_DAC_KINK, 90);
        }
        midi_out_init(&mut out, &sender, 0, 0);
        midi_map_port_init(&mut input, &mut receiver);
        live_param_set(&mut sender, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN);
        live_param_set(&mut sender, LIVE_PARAM_AGE, 60);
        live_param_set(
            &mut sender,
            LIVE_PARAM_EXPERT + SID_EXPERT_FILTER_6581_CURVE,
            100,
        );
        live_param_set(&mut sender, LIVE_PARAM_EXPERT + SID_EXPERT_DAC_KINK, 90);
    }
    out.next = (LIVE_PARAM_EXPERT + SID_EXPERT_DAC_KINK) as u8;
    let mut packets = [0u32; MIDI_OUT_TICK_PACKETS as usize];
    let mut nrpns = 0;
    for _ in 0..20 {
        let n = unsafe {
            midi_out_tick(
                &mut out,
                &sender,
                MIDI_OUT_TICK_PACKETS,
                packets.as_mut_ptr(),
            )
        };
        for packet in &packets[..n as usize] {
            let [_, status, cc, value] = packet.to_le_bytes();
            assert_eq!(status, 0xb0);
            nrpns += usize::from(cc == 99);
            unsafe { midi_map_cc(&mut input, 0, cc, value) };
        }
    }
    assert_eq!(nrpns, 2, "only explicit edits belong on expert NRPNs");
    assert_eq!(receiver.profile_req, SID_CHIP_6581_WORN as u8);
    assert_eq!(receiver.character, sender.character);
    assert_eq!(
        receiver.expert, sender.expert,
        "a later macro must not erase the transmitted override"
    );
    assert_eq!(
        receiver.expert[SID_EXPERT_FILTER_6581_RANGE as usize],
        SID_EXPERT_DEFAULT as u8
    );
}

#[test]
fn a_user_pick_can_release_an_override_even_when_its_base_and_macros_do_not_change() {
    let mut sender = live::default();
    let mut receiver = live::default();
    let mut writes = Vec::<(u8, u8)>::new();
    let ctx = (&mut writes as *mut Vec<(u8, u8)>).cast();
    let mut out = midi_out::default();
    let mut input = midi_map_port::default();
    unsafe {
        live_init(&mut sender, Some(record), ctx, SID_MODEL_6581);
        live_init(&mut receiver, Some(record), ctx, SID_MODEL_6581);
        for l in [&mut sender, &mut receiver] {
            live_param_set(l, LIVE_PARAM_EXPERT + SID_EXPERT_FILTER_6581_CURVE, 100);
        }
        midi_out_init(&mut out, &sender, 0, 0);
        midi_map_port_init(&mut input, &mut receiver);
        sender.user_valid = 1;
        sender.user_base[0] = SID_CHIP_6581_R4AR as u8;
        sender.user_expert[0][SID_EXPERT_EXTERNAL_HP as usize] = 40;
        live_param_set(&mut sender, LIVE_PARAM_PROFILE, SID_CHIPS);
    }
    let mut packets = [0u32; MIDI_OUT_TICK_PACKETS as usize];
    for _ in 0..20 {
        let n = unsafe {
            midi_out_tick(
                &mut out,
                &sender,
                MIDI_OUT_TICK_PACKETS,
                packets.as_mut_ptr(),
            )
        };
        for packet in &packets[..n as usize] {
            let [_, _, cc, value] = packet.to_le_bytes();
            unsafe { midi_map_cc(&mut input, 0, cc, value) };
        }
    }
    assert_eq!(receiver.expert, sender.expert);
}

#[test]
fn expert_revision_tracks_raw_changes_not_macro_resolution_or_repeated_edits() {
    let mut l = live::default();
    let mut writes = Vec::<(u8, u8)>::new();
    unsafe {
        live_init(
            &mut l,
            Some(record),
            (&mut writes as *mut Vec<(u8, u8)>).cast(),
            SID_MODEL_6581,
        );
        let initial = l.expert_version;
        live_param_set(&mut l, LIVE_PARAM_AGE, 30);
        live_param_set(&mut l, LIVE_PARAM_CUTOFF, 600);
        assert_eq!(l.expert_version, initial);
        live_param_set(&mut l, LIVE_PARAM_EXPERT + SID_EXPERT_DAC_KINK, 90);
        assert_eq!(l.expert_version, initial.wrapping_add(1));
        live_param_set(&mut l, LIVE_PARAM_EXPERT + SID_EXPERT_DAC_KINK, 90);
        live_param_set(&mut l, LIVE_PARAM_AGE, 60);
        assert_eq!(l.expert_version, initial.wrapping_add(1));
        live_param_set(&mut l, LIVE_PARAM_DAC, 60);
        assert_eq!(l.expert_version, initial.wrapping_add(2));
        live_param_set(&mut l, LIVE_PARAM_DAC, 70);
        assert_eq!(l.expert_version, initial.wrapping_add(2));
        l.user_valid = 1;
        l.user_base[0] = SID_CHIP_6581_R4AR as u8;
        l.user_expert[0][SID_EXPERT_EXTERNAL_HP as usize] = 40;
        live_param_set(&mut l, LIVE_PARAM_PROFILE, SID_CHIPS);
        assert_eq!(l.expert_version, initial.wrapping_add(3));
        live_param_set(&mut l, LIVE_PARAM_PROFILE, SID_CHIPS);
        assert_eq!(l.expert_version, initial.wrapping_add(3));
    }
}

#[test]
fn expert_tracking_suppresses_remote_echoes_and_forgets_dropped_or_resynced_edits() {
    let mut l = live::default();
    let mut writes = Vec::<(u8, u8)>::new();
    let mut out = midi_out::default();
    let mut packets = [0u32; MIDI_OUT_TICK_PACKETS as usize];
    let p = LIVE_PARAM_EXPERT + SID_EXPERT_EXTERNAL_HP;
    unsafe {
        live_init(
            &mut l,
            Some(record),
            (&mut writes as *mut Vec<(u8, u8)>).cast(),
            SID_MODEL_6581,
        );
        midi_out_init(&mut out, &l, 0, 0);
        live_param_set(&mut l, p, 40);
        midi_out_local(&mut out, &l);
        assert_eq!(midi_out_tick(&mut out, &l, 3, packets.as_mut_ptr()), 0);
        assert_ne!(out.expert_pending, 0);
        live_param_set(&mut l, p, 50);
        midi_out_remote(&mut out, &l);
        assert_eq!(midi_out_tick(&mut out, &l, 16, packets.as_mut_ptr()), 0);
        assert_eq!(out.expert_pending, 0);

        live_param_set(&mut l, p, 60);
        midi_out_drop(&mut out, &l);
        assert_eq!(out.dropped, 1);
        assert_eq!(out.expert_pending, 0);
        assert_eq!(midi_out_tick(&mut out, &l, 16, packets.as_mut_ptr()), 0);

        live_param_set(&mut l, p, 70);
        midi_out_init(&mut out, &l, 0, 0);
        assert_eq!(midi_out_tick(&mut out, &l, 16, packets.as_mut_ptr()), 0);
        live_param_set(&mut l, p, 80);
        assert_eq!(midi_out_tick(&mut out, &l, 16, packets.as_mut_ptr()), 4);
        assert_eq!(out.expert_pending, 0);

        let mut base = [0xa5a5u16; LIVE_PARAM_COUNT as usize];
        live_params_base(&l, base.as_mut_ptr());
        assert!(base[LIVE_PARAM_EXPERT as usize..]
            .iter()
            .all(|&v| v == 0xa5a5));
        live_params_wire(&l, base.as_mut_ptr());
        assert_eq!(base[p as usize], 80);
        assert_eq!(base[LIVE_PARAM_EXPERT as usize], SID_EXPERT_DEFAULT as u16);
        live_params(&l, base.as_mut_ptr());
        assert_ne!(base[LIVE_PARAM_EXPERT as usize], SID_EXPERT_DEFAULT as u16);
    }
}
