// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::player::default_options;
use sidou_tests::song::{SngSpec, Song};
use sidou_tests::transport::Transport;

unsafe extern "C" fn discard(_: *mut std::ffi::c_void, _: u8, _: u8) {}

fn fixture(include_audition: bool, empty_third: bool, note: u8) -> Song {
    let mut spec = SngSpec::new();
    spec.orders = vec![vec![(vec![0], 0), (vec![1], 0), (vec![2], 0)]];
    let mut plain = [0u8; 25];
    plain[..9].copy_from_slice(&[0x09, 0x80, 1, 0, 0, 0, 0, 2, 0x09]);
    let mut filtered = plain;
    filtered[GT2_INS_FILTER as usize] = 1;
    spec.instr = vec![plain, filtered];
    spec.tables = vec![
        vec![(0x21, 0), (0xff, 0)],
        vec![],
        vec![(0x90, 0xf4), (0, 0x56), (0xff, 0)],
        vec![],
    ];
    spec.patterns = (0..3)
        .map(|voice| {
            let mut rows = vec![[0xbd, 0, 0, 0]; 8];
            if voice != 2 || !empty_third {
                rows[0] = [0x60 + 48, 1, 0, 0];
            }
            if include_audition {
                rows[1] = [0x60 + note, 2, 0, 0];
            }
            (rows, [0xff, 0, 0, 0])
        })
        .collect();
    Song::host_from(&spec.encode())
}

fn compare_audition(voice: u32, empty_third: bool, note: u8, already_used: bool) {
    let mut song = fixture(already_used, empty_third, 48);
    let expected_song = fixture(true, empty_third, note);
    let mut transport = Transport::new(&mut song);
    assert!(transport.audition());
    assert!(transport.run_until(300, |t| t.jam_ready() && t.calls() > 0));
    assert_eq!(
        u32::from(transport.shape().flags) & GT2_PF_NOFILTER != 0,
        !already_used
    );
    assert_eq!(transport.shape().channels, if empty_third { 2 } else { 3 });

    let mut analysis = Box::<gt2_analysis>::default();
    let mut shape = Box::<gt2_shape>::default();
    let mut expected = Box::<gt2_player>::default();
    unsafe {
        assert_eq!(
            gt2_shape_analyse(
                &mut *analysis,
                expected_song.ptr(),
                0,
                &default_options(),
                &mut *shape,
            ),
            0
        );
        assert_eq!(u32::from(shape.flags) & GT2_PF_NOFILTER, 0);
        assert_eq!(shape.channels, 3);
        gt2_play_start(&mut *expected, song.ptr(), &*shape, std::ptr::null());
        expected.frozen = 1;
        gt2_play_frame(&mut *expected, Some(discard), std::ptr::null_mut());
        gt2_play_jam(
            &mut *expected,
            voice,
            note,
            2,
            Some(discard),
            std::ptr::null_mut(),
        );
        expected.chn[voice as usize].counter = 1;
        gt2_play_frame(&mut *expected, Some(discard), std::ptr::null_mut());
    }
    assert_eq!(
        expected.filtstep, 1,
        "the independently used instrument starts its filter"
    );

    assert!(transport.jam(voice, u32::from(note), 2));
    assert!(
        transport.run_until(500, |t| {
            t.sc.player.ev[voice as usize].flags & GT2_EV_NOTE as u8 != 0
        }),
        "the jam never starts on the requested voice"
    );
    assert_eq!(
        transport.sc.player.filtstep, expected.filtstep,
        "jam must initialise the filter"
    );
    for frame in 0..8 {
        unsafe {
            gt2_play_frame(&mut *expected, Some(discard), std::ptr::null_mut());
            song_ctl_call(&mut *transport.sc);
        }
        assert_eq!(
            transport.sc.player.regs, expected.regs,
            "audition registers, frame {frame}"
        );
    }
    assert_eq!(transport.sc.player.chn[voice as usize].note, note);
    assert_eq!(
        transport.sc.player.freq_unknown, 0,
        "audition note must be in the widened range"
    );
    assert_eq!(transport.sc.edit_cells, 0);
    assert_eq!(transport.sc.edit_swaps, 0);
}

#[test]
fn unused_filter_instrument_auditions_like_a_used_instrument() {
    compare_audition(0, false, 48, false);
}

#[test]
fn unused_third_voice_auditions_like_a_packed_voice() {
    compare_audition(2, true, 48, false);
}

#[test]
fn audition_note_outside_song_range_has_the_expected_frequency() {
    compare_audition(0, false, 12, true);
}

#[test]
fn already_used_filter_instrument_is_the_positive_control() {
    compare_audition(0, false, 48, true);
}

#[test]
fn widening_an_empty_voice_must_not_play_the_stopped_sequencer() {
    let mut song = fixture(false, true, 48);
    let mut transport = Transport::new(&mut song);
    assert!(transport.audition());
    assert!(transport.run_until(300, |t| t.jam_ready() && t.calls() > 0));
    assert_eq!(transport.shape().channels, 2);
    assert!(transport.jam(0, 48, 1));
    assert!(transport.run_until(100, |t| t.sc.player.ev[0].flags & GT2_EV_NOTE as u8 != 0));
    assert_eq!(
        transport.sc.shadow[11] & 1,
        0,
        "voice 2 has never been jammed"
    );

    let mut edit = gt2_edit::default();
    unsafe {
        assert_eq!(
            gt2_set_row(
                song.ptr_mut(),
                2,
                0,
                [0x60 + 48, 1, 0, 0].as_ptr(),
                &mut edit
            ),
            GT2_OK
        );
    }
    assert!(transport.edit(&edit));
    assert!(transport.run_until(1000, |t| t.shape().channels == 3 && t.sc.inflight_n == 0));
    assert_eq!(transport.sc.isr_playing, 0);
    assert_eq!(transport.sc.player.frozen, 1);
    assert_eq!(
        transport.sc.shadow[11] & 1,
        0,
        "the existing shape handover must not sound an unjammed voice while stopped"
    );
}

#[test]
fn stop_cancels_an_inflight_shape_replay_before_a_later_audition() {
    for sent in [false, true] {
        let mut song = fixture(false, true, 48);
        let mut transport = Transport::new(&mut song);
        assert!(transport.play(0, 0));
        assert!(transport.run_until(2000, |t| t.calls() >= 100));
        let mut edit = gt2_edit::default();
        unsafe {
            assert_eq!(
                gt2_set_row(song.ptr_mut(), 2, 0, [0x90, 1, 0, 0].as_ptr(), &mut edit),
                GT2_OK
            );
        }
        assert!(transport.edit(&edit));
        assert!(transport.run_until(1000, |t| t.sc.ff_on != 0));
        if sent {
            for _ in 0..100 {
                transport.pass();
                if transport.sc.ff_sent != 0 {
                    break;
                }
            }
            assert_ne!(transport.sc.ff_sent, 0);
        }
        assert!(transport.stop());
        assert!(transport.run_until(200, |t| t.sc.handback == 0 && t.sc.ff_on == 0));
        assert!(transport.audition());
        assert!(transport.run_until(300, |t| t.jam_ready()));
        assert!(transport.jam(0, 48, 1));
        assert!(transport.run_until(500, |t| t.sc.player.ev[0].flags & GT2_EV_NOTE as u8 != 0));
        for _ in 0..100 {
            transport.pass();
            transport.block_silent();
        }
        assert_eq!(transport.sc.handovers, 0, "sent={sent}");
        assert_eq!(transport.sc.shadow[18] & 1, 0, "sent={sent}");
    }
}

fn armed_shape_replay() -> (Song, Transport) {
    let mut song = fixture(false, true, 48);
    let mut transport = Transport::new(&mut song);
    assert!(transport.play(0, 0));
    assert!(transport.run_until(2000, |t| t.calls() >= 100));
    let mut edit = gt2_edit::default();
    unsafe {
        assert_eq!(
            gt2_set_row(song.ptr_mut(), 2, 0, [0x90, 1, 0, 0].as_ptr(), &mut edit),
            GT2_OK
        );
    }
    assert!(transport.edit(&edit));
    assert!(transport.run_until(1000, |t| t.sc.ff_on != 0));
    for _ in 0..100 {
        transport.pass();
        if transport.sc.ff_sent != 0 {
            break;
        }
    }
    assert_ne!(transport.sc.ff_sent, 0);
    unsafe { song_ctl_block(&mut *transport.sc, &mut *transport.chip) };
    assert_ne!(transport.sc.armed, 0);
    (song, transport)
}

fn stop_and_retake_before_ack(resume: bool) {
    let (_song, mut transport) = armed_shape_replay();
    assert!(transport.stop());
    if resume {
        assert!(transport.resume());
    } else {
        assert!(transport.audition());
    }
    assert_eq!(transport.sc.handback, 0);
    assert!(transport.run_until(1000, |t| t.sc.ff_on == 0 && t.sc.ff_sent == 0));
    assert_eq!(
        transport.sc.player.shape,
        transport.sc.shapes[transport.sc.active as usize]
    );
}

#[test]
fn review_stop_then_continue_before_ack_does_not_strand_a_sent_handover() {
    stop_and_retake_before_ack(true);
}

#[test]
fn review_stop_then_audition_before_ack_does_not_strand_a_sent_handover() {
    stop_and_retake_before_ack(false);
}

#[test]
fn review_completed_handover_ownership_is_reconciled_before_stop_cancellation() {
    let (_song, mut transport) = armed_shape_replay();
    for _ in 0..200 {
        transport.block_silent();
        if transport.sc.handover_seq == transport.sc.ff_seq && transport.sc.handover_ok != 0 {
            break;
        }
    }
    assert_eq!(transport.sc.handover_seq, transport.sc.ff_seq);
    assert_ne!(transport.sc.handover_ok, 0);
    assert_ne!(
        transport.sc.player.shape,
        transport.sc.shapes[transport.sc.active as usize]
    );
    let live_shape = transport.sc.player.shape;
    assert!(transport.stop());
    unsafe {
        song_ctl_commit(&mut *transport.sc);
        song_ctl_block(&mut *transport.sc, &mut *transport.chip);
        song_ctl_poll(&mut *transport.sc, 0);
    }
    assert_eq!(
        live_shape,
        transport.sc.shapes[transport.sc.active as usize]
    );
    assert_eq!(transport.sc.handovers, 1);
    assert_eq!(transport.sc.ff_on, 0);
    assert_eq!(transport.sc.ff_sent, 0);
}

#[test]
fn review_new_seek_waits_for_unpublished_stop_retirement_then_reaches_its_target() {
    let (_song, mut transport) = armed_shape_replay();
    for _ in 0..200 {
        transport.block_silent();
        if transport.sc.handover_seq == transport.sc.ff_seq && transport.sc.handover_ok != 0 {
            break;
        }
    }
    assert_eq!(transport.sc.handover_seq, transport.sc.ff_seq);
    assert_ne!(transport.sc.handover_ok, 0);
    assert!(transport.stop());
    unsafe { song_ctl_poll(&mut *transport.sc, 0) };
    assert_eq!(transport.sc.ff_on, 0);
    assert_ne!(transport.sc.ff_stop, 0);
    assert!(
        !transport.seek(222),
        "STOP retirement still owns this replay generation"
    );
    unsafe {
        song_ctl_commit(&mut *transport.sc);
        song_ctl_block(&mut *transport.sc, &mut *transport.chip);
        song_ctl_poll(&mut *transport.sc, 0);
    }
    assert_eq!(transport.sc.ff_stop, 0);
    assert!(transport.seek(222));
    assert_eq!(transport.sc.ff_target, 222);
    assert!(transport.resume());
    assert!(transport.run_until(2000, |t| t.sc.ff_on == 0
        && t.sc.ff_sent == 0
        && t.calls() >= 222));
}

#[test]
fn review_seek_further_refuses_a_replay_already_owned_by_stop_retirement() {
    let mut song = fixture(false, false, 48);
    let mut transport = Transport::new(&mut song);
    assert!(transport.play(0, 0));
    assert!(transport.run_until(2000, |t| t.calls() >= 100));
    assert!(transport.seek(200));
    assert!(transport.stop());
    assert_ne!(transport.sc.ff_stop, 0);
    assert_eq!(unsafe { song_ctl_seek_further(&mut *transport.sc, 300) }, 0);
    assert_eq!(
        transport.sc.ff_target, 200,
        "a refused extension leaves the target alone"
    );
    assert!(transport.run_until(1000, |t| t.sc.ff_stop == 0
        && t.sc.ff_on == 0
        && t.sc.deriving == 0));
    assert!(transport.seek(333));
    assert_eq!(unsafe { song_ctl_seek_further(&mut *transport.sc, 400) }, 1);
    assert!(transport.resume());
    assert!(transport.run_until(2000, |t| t.sc.ff_on == 0
        && t.sc.ff_sent == 0
        && t.calls() >= 400));
}

fn pulse_switch_song(old_simple: bool, next: bool) -> Song {
    let mut spec = SngSpec::new();
    spec.orders = vec![vec![(vec![0], 0), (vec![0], 0), (vec![0], 0)]];
    let mut plain = [0u8; 25];
    plain[..9].copy_from_slice(&[9, 0x80, 1, 0, 0, 0, 0, 2, 9]);
    let mut old = plain;
    old[GT2_INS_PULSE as usize] = 1;
    let mut new = plain;
    new[GT2_INS_PULSE as usize] = 4;
    spec.instr = vec![plain, old, new];
    spec.tables = vec![
        vec![(0x21, 0), (0xff, 1)],
        vec![
            (0x88, if old_simple { 0 } else { 0x91 }),
            (16, 0x10),
            (0xff, 2),
            (16, if old_simple { 1 } else { 0x10 }),
            (0xff, 4),
        ],
        vec![],
        vec![],
    ];
    spec.patterns = vec![(
        vec![[0x90, if next { 3 } else { 2 }, 0, 0]],
        [0xff, 0, 0, 0],
    )];
    Song::host_from(&spec.encode())
}

fn pulse_width(player: &gt2_player) -> u16 {
    u16::from(player.regs[2]) | (u16::from(player.regs[3] & 15) << 8)
}

fn buffered_pulse_encoding_switch(old_simple: bool) {
    let song = pulse_switch_song(old_simple, false);
    let next_song = pulse_switch_song(old_simple, true);
    let mut old = Box::<gt2_shape>::default();
    let mut next = Box::<gt2_shape>::default();
    let mut analysis = Box::<gt2_analysis>::default();
    let mut player = Box::<gt2_player>::default();
    let mut options = default_options();
    options.player_flags = GT2_PLAYER_BUFFERED as u16;
    unsafe {
        assert_eq!(
            gt2_play_init(&mut *player, &mut *old, song.ptr(), 0, &options),
            0
        );
        assert_eq!(
            gt2_shape_analyse(&mut *analysis, next_song.ptr(), 0, &options, &mut *next),
            0
        );
        assert_eq!(u32::from(old.flags) & GT2_PF_SIMPLEPULSE != 0, old_simple);
        assert_eq!(u32::from(next.flags) & GT2_PF_SIMPLEPULSE == 0, old_simple);
        player.frozen = 1;
        gt2_play_frame(&mut *player, Some(discard), std::ptr::null_mut());
        gt2_play_jam(&mut *player, 0, 48, 2, Some(discard), std::ptr::null_mut());
        player.chn[0].counter = 1;
        gt2_play_frame(&mut *player, Some(discard), std::ptr::null_mut());
        for _ in 0..10 {
            gt2_play_frame(&mut *player, Some(discard), std::ptr::null_mut());
            if player.chn[0].pulseptr == 2 {
                break;
            }
        }
    }
    assert_eq!(player.chn[0].pulseptr, 2);
    let before = pulse_width(&player);
    assert_eq!(before, if old_simple { 0x808 } else { 0x891 });
    unsafe {
        gt2_play_retain_shape(&mut *player, &*next);
        gt2_play_jam(&mut *player, 0, 48, 3, Some(discard), std::ptr::null_mut());
        player.chn[0].counter = 1;
        gt2_play_frame(&mut *player, Some(discard), std::ptr::null_mut());
    }
    assert_eq!(
        pulse_width(&player),
        before,
        "buffered note init must preserve the old width"
    );
    unsafe { gt2_play_frame(&mut *player, Some(discard), std::ptr::null_mut()) };
    assert_eq!(
        pulse_width(&player),
        if old_simple { 0x809 } else { 0x8a8 },
        "a modulation-first program must start from the converted held width"
    );
}

#[test]
fn review_simple_to_complex_pulse_preserves_buffered_init_and_modulation() {
    buffered_pulse_encoding_switch(true);
}

#[test]
fn review_complex_to_simple_pulse_preserves_buffered_init_and_modulation() {
    buffered_pulse_encoding_switch(false);
}

#[test]
fn release_before_shape_preparation_finishes_follows_the_retained_note() {
    let mut song = fixture(false, true, 48);
    let mut transport = Transport::new(&mut song);
    assert!(transport.audition());
    assert!(transport.run_until(300, |t| t.jam_ready() && t.calls() > 0));
    assert!(transport.jam(2, 12, 2));
    assert!(transport.jam(2, SONG_JAM_RELEASE, 2));
    assert!(
        !transport.jam(2, 24, 2),
        "an occupied deferred slot must refuse, not lose a note"
    );
    assert!(transport.run_until(500, |t| t.sc.player.ev[2].flags & GT2_EV_NOTE as u8 != 0));
    assert_eq!(transport.sc.shadow[18] & 1, 1);
    assert!(transport.run_until(100, |t| t.sc.shadow[18] & 1 == 0));
    assert_eq!(transport.sc.player.chn[2].note, 12);
    assert_eq!(transport.sc.jam_release, 0);
    assert_eq!(transport.sc.jam_wait[2][0], 0);
}

#[test]
fn ending_audition_cancels_a_retained_unsubmitted_note() {
    let mut song = fixture(false, true, 48);
    let mut transport = Transport::new(&mut song);
    assert!(transport.audition());
    assert!(transport.run_until(300, |t| t.jam_ready() && t.calls() > 0));
    assert!(transport.jam(2, 12, 2));
    assert!(transport.audition_end());
    for _ in 0..200 {
        transport.pass();
        transport.block_silent();
    }
    assert_eq!(transport.sc.audition, 0);
    assert_eq!(transport.sc.shadow[18] & 1, 0);
    assert_eq!(transport.sc.jam_wait[2][0], 0);
}

#[test]
fn ending_audition_cancels_a_submitted_note_before_its_call() {
    let mut song = fixture(false, true, 48);
    let mut transport = Transport::new(&mut song);
    assert!(transport.audition());
    assert!(transport.run_until(300, |t| t.jam_ready() && t.calls() > 0));
    assert!(transport.jam(2, 12, 2));
    for _ in 0..100 {
        transport.pass();
        unsafe { song_ctl_block(&mut *transport.sc, &mut *transport.chip) };
        if transport.sc.jam[2][0] != 0 {
            break;
        }
    }
    assert_ne!(transport.sc.jam[2][0], 0);
    assert!(transport.audition_end());
    transport.pass();
    transport.block_silent();
    assert_eq!(transport.sc.jam[2][0], 0);
    assert!(transport.audition());
    assert!(transport.run_until(100, |t| t.jam_ready()));
    assert!(transport.jam(0, 48, 1));
    assert!(transport.run_until(200, |t| t.sc.player.ev[0].flags & GT2_EV_NOTE as u8 != 0));
    for _ in 0..100 {
        transport.pass();
        transport.block_silent();
    }
    assert_eq!(
        transport.sc.shadow[18] & 1,
        0,
        "the cancelled note must not return"
    );
}

#[test]
fn background_narrowing_keeps_jammed_instruments_and_note_range() {
    let mut song = fixture(false, true, 48);
    let mut transport = Transport::new(&mut song);
    assert!(transport.audition());
    assert!(transport.run_until(300, |t| t.jam_ready() && t.calls() > 0));
    assert!(transport.jam(2, 12, 2));
    assert!(transport.run_until(500, |t| t.sc.player.ev[2].flags & GT2_EV_NOTE as u8 != 0));
    for _ in 0..500 {
        transport.pass();
        transport.block_silent();
    }
    assert_eq!(u32::from(transport.shape().flags) & GT2_PF_NOFILTER, 0);
    assert!(transport.shape().first_note <= 12);
    assert_ne!(transport.shape().ins_num[2], 0);
    assert_eq!(transport.sc.player.chn[2].instr, 2);
    assert_eq!(transport.sc.shadow[18] & 1, 1);
    assert_eq!(transport.sc.handovers, 0);
}

#[test]
fn an_unpacked_voice_jams_without_sequencing_while_other_voices_play() {
    let mut song = fixture(false, true, 48);
    let mut transport = Transport::new(&mut song);
    assert!(transport.play(0, 0));
    assert!(transport.run_until(300, |t| t.calls() > 2));
    let play_block = transport.sc.play_block;
    let before = transport.calls();
    assert!(transport.jam(2, 12, 2));
    assert!(transport.run_until(500, |t| t.sc.player.ev[2].flags & GT2_EV_NOTE as u8 != 0));
    let mut rows = 0;
    for _ in 0..500 {
        transport.pass();
        if transport.block_silent().is_some() {
            assert_eq!(transport.sc.player.ev[2].flags & GT2_EV_ROW as u8, 0);
            rows += usize::from(transport.sc.player.ev[0].flags & GT2_EV_ROW as u8 != 0);
        }
    }
    assert!(rows > 0);
    assert!(transport.calls() > before);
    assert_eq!(transport.sc.play_block, play_block);
    assert_eq!(transport.sc.isr_playing, 1);
    assert_eq!(transport.sc.handovers, 0);
    assert_eq!(transport.shape().channels, 2);
    assert_eq!(transport.sc.shadow[18] & 1, 1);
}

fn running_channel(c: &gt2_channel) -> [u8; 16] {
    [
        c.songptr,
        c.pattptr,
        c.newnote,
        c.counter,
        c.waveptr,
        c.wave,
        c.pulseptr,
        c.pulsetime,
        c.note,
        c.instr,
        c.gate,
        c.freqlo,
        c.freqhi,
        c.vibtime,
        c.wavetime,
        c.param,
    ]
}

#[test]
fn audition_after_stop_keeps_unrequested_song_voices_silent_and_their_state_held() {
    for flags in [
        0,
        GT2_PLAYER_BUFFERED,
        GT2_PLAYER_FULLBUFFERED,
        GT2_PLAYER_ZPGHOSTREGS,
    ] {
        let mut song = fixture(false, false, 48);
        let mut options = default_options();
        options.player_flags = flags as u16;
        assert_eq!(unsafe { song_ext_set_options(song.ptr_mut(), &options) }, 0);
        let mut transport = Transport::new(&mut song);
        assert!(transport.play(0, 0));
        assert!(transport.run_until(1000, |t| t.sc.player.chn[1].newnote != 0));
        assert!(transport.stop());
        assert!(transport.run_until(100, |t| t.sc.handback == 0));
        let held = running_channel(&transport.sc.player.chn[1]);
        assert!(transport.audition());
        assert!(transport.run_until(200, |t| t.jam_ready()));
        assert!(transport.jam(0, 12, 2));
        assert!(transport.run_until(500, |t| t.sc.player.ev[0].flags & GT2_EV_NOTE as u8 != 0));
        for _ in 0..200 {
            transport.pass();
            transport.block_silent();
            for voice in [1, 2] {
                assert_eq!(
                    transport.chip.env[voice].gate, 0,
                    "flags {flags}, voice {voice}"
                );
                assert_eq!(transport.sc.player.ev[voice].flags & GT2_EV_NOTE as u8, 0);
            }
        }
        assert!(transport.mute(2));
        transport.pass();
        transport.block_silent();
        assert!(transport.mute(0));
        transport.pass();
        transport.block_silent();
        assert!(transport.take(2));
        transport.pass();
        transport.block_silent();
        assert!(transport.take(0));
        transport.pass();
        transport.block_silent();
        assert_eq!(
            transport.chip.env[1].gate, 0,
            "restoration must not restart a song note"
        );
        assert!(transport.audition_end());
        transport.pass();
        transport.block_silent();
        assert_eq!(
            running_channel(&transport.sc.player.chn[1]),
            held,
            "flags {flags}"
        );
    }
}

fn encoding_song(include_second: bool, features: u8) -> Song {
    let mut spec = SngSpec::new();
    spec.orders = vec![vec![(vec![0], 0), (vec![0], 0), (vec![0], 0)]];
    let mut plain = [0u8; 25];
    plain[..9].copy_from_slice(&[9, 0x80, 1, 1, 0, 0, 0, 2, 9]);
    if features & 4 != 0 {
        plain[GT2_INS_FILTER as usize] = 1;
    }
    let mut second = plain;
    second[GT2_INS_FILTER as usize] = 1;
    if features & 1 != 0 {
        second[GT2_INS_PULSE as usize] = 4;
    }
    if features & 2 != 0 {
        second[GT2_INS_WAVE as usize] = 3;
    }
    spec.instr = vec![plain, second];
    spec.tables = vec![
        vec![(0x21, 0), (0xff, 1), (1, 0), (0xf7, 0x41), (0xff, 3)],
        vec![
            (0x88, 0),
            (16, 0x10),
            (0xff, 2),
            (0x88, 1),
            (16, 1),
            (0xff, 5),
        ],
        vec![(0x90, 0xf4), (0, 0x56), (16, 1), (0xff, 3)],
        vec![],
    ];
    spec.patterns = vec![(
        vec![
            [0x90, 1, 0, 0],
            [0x90, if include_second { 2 } else { 1 }, 0, 0],
        ],
        [0xff, 0, 0, 0],
    )];
    Song::host_from(&spec.encode())
}

unsafe extern "C" fn chip_write(ctx: *mut std::ffi::c_void, reg: u8, value: u8) {
    unsafe { sid_write(ctx.cast(), reg, value) };
}

fn call_and_render(player: &mut gt2_player, chip: &mut sid) -> [i16; 256] {
    let mut out = [0; 256];
    unsafe {
        gt2_play_frame(player, Some(chip_write), (chip as *mut sid).cast());
        sid_render(chip, out.as_mut_ptr(), out.len() as u32);
    }
    out
}

#[test]
fn live_shape_widening_preserves_running_voice_registers_and_pcm() {
    let mut options = [default_options(); 6];
    options[1].player_flags = GT2_PLAYER_BUFFERED as u16;
    options[2].player_flags = GT2_PLAYER_FULLBUFFERED as u16;
    options[3].hr_adsr = 0xf000;
    options[4].multispeed = 4;
    options[5].optimize_pulse = 0;
    for (option_index, option) in options.iter().enumerate() {
        for features in 0..8 {
            let song = encoding_song(false, features);
            let widened_song = encoding_song(true, features);
            let mut old = Box::<gt2_shape>::default();
            let mut next = Box::<gt2_shape>::default();
            let mut analysis = Box::<gt2_analysis>::default();
            let mut expected = Box::<gt2_player>::default();
            let mut actual = Box::<gt2_player>::default();
            let mut expected_chip = Box::<sid>::default();
            let mut actual_chip = Box::<sid>::default();
            unsafe {
                assert_eq!(
                    gt2_play_init(&mut *expected, &mut *old, song.ptr(), 0, option),
                    0
                );
                assert_eq!(
                    gt2_shape_analyse(&mut *analysis, widened_song.ptr(), 0, option, &mut *next),
                    0
                );
                gt2_play_start(&mut *actual, song.ptr(), &*old, std::ptr::null());
                sid_init(&mut *expected_chip, SID_MODEL_6581, SID_CLOCK_PAL, 44_118);
                sid_init(&mut *actual_chip, SID_MODEL_6581, SID_CLOCK_PAL, 44_118);
            }
            expected.frozen = 1;
            actual.frozen = 1;
            call_and_render(&mut expected, &mut expected_chip);
            call_and_render(&mut actual, &mut actual_chip);
            unsafe {
                gt2_play_jam(
                    &mut *expected,
                    0,
                    48,
                    1,
                    Some(chip_write),
                    (&mut *expected_chip as *mut sid).cast(),
                );
                gt2_play_jam(
                    &mut *actual,
                    0,
                    48,
                    1,
                    Some(chip_write),
                    (&mut *actual_chip as *mut sid).cast(),
                );
            }
            expected.chn[0].counter = 1;
            actual.chn[0].counter = 1;
            for _ in 0..7 {
                assert_eq!(
                    call_and_render(&mut expected, &mut expected_chip),
                    call_and_render(&mut actual, &mut actual_chip)
                );
            }
            unsafe { gt2_play_retain_shape(&mut *actual, &*next) };
            for frame in 0..128 {
                let expected_pcm = call_and_render(&mut expected, &mut expected_chip);
                let actual_pcm = call_and_render(&mut actual, &mut actual_chip);
                assert_eq!(
                    actual.regs, expected.regs,
                    "options {option_index}, features {features}, frame {frame}"
                );
                assert_eq!(
                    actual_pcm, expected_pcm,
                    "options {option_index}, features {features}, frame {frame}"
                );
            }
        }
    }
}
