// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! MIDI input onto the live instrument (`app/midi_rx.c`): ring entries by source through
//! their parsers, and the M1a default mapping.

use sidou_sys::*;

use crate::Rig;

const USB: u8 = MIDI_SRC_USB as u8;
const TRS: u8 = MIDI_SRC_TRS as u8;
const C4: u8 = 60;
const E4: u8 = 64;

/// A live instrument (writing into a recorder) fed by a `midi_rx`.
struct Midi {
    rig: Rig,
    rx: Box<midi_rx>,
    /// The ring epoch the next entries carry.
    epoch: u8,
}

impl Midi {
    fn new() -> Self {
        let mut rig = Rig::new(SID_MODEL_6581);
        let mut rx = Box::<midi_rx>::default();
        unsafe { midi_rx_init(&mut *rx, &mut *rig.live, 0) };
        Midi { rig, rx, epoch: 0 }
    }

    fn entry(&mut self, source: u8, data: u32) {
        let e = midi_ring_entry {
            stamp: 0,
            data,
            source,
            epoch: self.epoch,
        };
        unsafe { midi_rx_entry(&mut *self.rx, &e) };
    }

    fn trs(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.entry(TRS, u32::from(b));
        }
    }

    fn usb(&mut self, cin: u8, m: [u8; 3]) {
        self.entry(USB, u32::from_le_bytes([cin, m[0], m[1], m[2]]));
    }

    fn live(&self) -> &live {
        &self.rig.live
    }

    /// The notes held (gated), in voice order.
    fn held(&self) -> Vec<u8> {
        self.live()
            .voice
            .iter()
            .filter(|v| v.gate != 0)
            .map(|v| v.note)
            .collect()
    }
}

#[test]
fn trs_note_on_and_off_play_and_release_the_note() {
    let mut m = Midi::new();
    m.trs(&[0x90, C4, 100]);
    assert_eq!(m.held(), [C4]);
    assert!(m.rig.gated(0));
    assert_eq!(
        m.rig.voice_freq(0),
        unsafe { live_freq_table }[usize::from(C4)]
    );
    m.trs(&[0x80, C4, 0]);
    assert_eq!(m.held(), [] as [u8; 0]);
    assert!(!m.rig.gated(0));
}

#[test]
fn notes_play_on_every_channel() {
    for ch in 0..16u8 {
        let mut m = Midi::new();
        m.trs(&[0x90 | ch, E4, 1]);
        assert_eq!(m.held(), [E4], "channel {}", ch + 1);
        m.usb(0x8, [0x80 | ch, E4, 64]);
        assert_eq!(m.held(), [] as [u8; 0], "channel {}", ch + 1);
    }
}

#[test]
fn running_status_and_velocity_zero_work_over_trs() {
    let mut m = Midi::new();
    m.trs(&[0x90, C4, 100, E4, 100]);
    assert_eq!(m.held(), [C4, E4]);
    m.trs(&[C4, 0]);
    assert_eq!(m.held(), [E4]);
}

#[test]
fn usb_packets_play_notes() {
    let mut m = Midi::new();
    m.usb(0x9, [0x90, C4, 100]);
    m.usb(0x9, [0x95, E4, 100]);
    assert_eq!(m.held(), [C4, E4]);
    m.usb(0x9, [0x90, C4, 0]);
    assert_eq!(m.held(), [E4]);
}

#[test]
fn a_usb_message_between_the_bytes_of_a_trs_message_leaves_both_whole() {
    let mut m = Midi::new();
    m.trs(&[0x90, C4]);
    m.usb(0x9, [0x91, E4, 100]);
    m.usb(0xF, [0xF8, 0, 0]);
    m.trs(&[100]);
    let mut held = m.held();
    held.sort_unstable();
    assert_eq!(held, [C4, E4]);
}

#[test]
fn ccs_follow_the_default_mapping() {
    let mut m = Midi::new();
    m.trs(&[0xB0, 74, 0]);
    assert_eq!(m.live().cutoff, 0);
    m.usb(0xB, [0xB3, 74, 127]);
    assert_eq!(m.live().cutoff, 2047);
    m.trs(&[0xB0, 74, 64]);
    assert_eq!(u32::from(m.live().cutoff), (64 * 2047 + 63) / 127);

    m.trs(&[0xB0, 71, 127]);
    assert_eq!(m.live().resonance, 15);
    m.trs(&[0xB0, 71, 0]);
    assert_eq!(m.live().resonance, 0);

    m.trs(&[0xB0, 1, 127]);
    assert_eq!(m.live().pulse_width, 4095);
    m.trs(&[0xB0, 1, 0]);
    assert_eq!(m.live().pulse_width, 0);

    m.trs(&[0xB0, 7, 0]);
    assert_eq!(m.live().volume, 0);
    m.trs(&[0xB0, 7, 127]);
    assert_eq!(m.live().volume, 15);

    m.trs(&[0x90, C4, 100, E4, 100]);
    m.trs(&[0xB0, 123, 0]);
    assert_eq!(m.held(), [] as [u8; 0], "all notes off");
}

#[test]
fn other_ccs_change_nothing() {
    let mut m = Midi::new();
    m.rig.clear();
    // The effect slots' CCs (102..116) are tested in fx_live.rs.
    for cc in
        (0..128u8).filter(|c| ![1, 7, 71, 74, 120, 123].contains(c) && !(102..=116).contains(c))
    {
        m.trs(&[0xB0, cc, 99]);
    }
    assert!(m.rig.log().is_empty(), "writes: {:?}", m.rig.log());
}

#[test]
fn program_change_selects_the_preset() {
    let mut m = Midi::new();
    m.trs(&[0x90, C4, 100]);
    m.trs(&[0xC0, 1]);
    assert_eq!(m.live().preset, 1);
    assert_eq!(
        m.held(),
        [] as [u8; 0],
        "a preset change releases the notes"
    );
    assert_eq!(m.live().cutoff, crate::preset(1).cutoff);
    m.usb(0xC, [0xCA, 3, 0]);
    assert_eq!(m.live().preset, 3);
    m.trs(&[0xC0, LIVE_PRESET_COUNT as u8]);
    assert_eq!(
        m.live().preset,
        3,
        "a program past the last preset is ignored"
    );
    m.trs(&[0xC0, 127]);
    assert_eq!(m.live().preset, 3);
    m.trs(&[0xC0, 0]);
    assert_eq!(m.live().preset, 0);
}

#[test]
fn sysex_real_time_bend_and_pressure_are_ignored() {
    let mut m = Midi::new();
    m.trs(&[0x90, C4, 100]);
    m.rig.clear();
    m.trs(&[0xF0, 0x7D, 0x01, 0xF8, 0x02, 0xF7]);
    m.trs(&[
        0xE0, 0x00, 0x7F, 0xD0, 0x40, 0xA0, C4, 0x40, 0xF2, 1, 2, 0xFE,
    ]);
    m.usb(0x4, [0xF0, 0x7E, 0x7F]);
    m.usb(0x7, [0x06, 0x01, 0xF7]);
    m.usb(0xE, [0xE0, 0, 0x40]);
    m.usb(0x0, [0x90, E4, 100]);
    assert!(m.rig.log().is_empty(), "writes: {:?}", m.rig.log());
    assert_eq!(m.held(), [C4]);
    m.trs(&[0x80, C4, 0]);
    assert_eq!(m.held(), [] as [u8; 0], "the parser still works after them");
}

#[test]
fn entries_of_an_unknown_source_are_ignored() {
    let mut m = Midi::new();
    m.rig.clear();
    for src in [0u8, 3, 255] {
        m.entry(src, 0x90);
        m.entry(src, u32::from(C4));
        m.entry(src, 100);
        m.entry(src, u32::from_le_bytes([0x09, 0x90, C4, 100]));
    }
    assert!(m.rig.log().is_empty());
}

#[test]
fn a_gap_restarts_the_parsers_at_the_first_entry_after_it() {
    let mut m = Midi::new();
    m.trs(&[0x90, E4, 100]);
    m.usb(0x9, [0x90, 67, 100]);
    // The tail before the gap is taken whole: this note-on starts before it, ends after it.
    m.trs(&[0x90, C4]);
    assert_eq!(m.held(), [E4, 67]);
    assert_eq!(m.rx.restarts, 0);
    m.epoch = 1; // input lost here
    m.trs(&[100]);
    assert_eq!(m.rx.restarts, 1);
    assert_eq!(
        m.held(),
        [] as [u8; 0],
        "notes released, and the cut note-on is not completed across the gap"
    );
    m.trs(&[C4, 100]);
    assert_eq!(m.held(), [] as [u8; 0], "no running status across the gap");
    m.trs(&[0x90, C4, 100]);
    m.usb(0x9, [0x90, E4, 100]);
    let mut held = m.held();
    held.sort_unstable();
    assert_eq!(held, [C4, E4]);
    assert_eq!(m.rx.restarts, 1, "one restart per gap");
}

#[test]
fn a_gap_inside_a_sysex_leaves_nothing_behind() {
    let mut m = Midi::new();
    m.trs(&[0x90, C4, 100]);
    // A SysEx (which also cancels running status) cut by a gap.
    m.trs(&[0xF0, 0x7D, 0x01]);
    m.usb(0x4, [0xF0, 0x7E, 0x00]);
    m.rig.clear();
    m.epoch = 1;
    // Its tail after the gap: data bytes, then F7, on both sources. None of it may be read
    // as channel data (with no running status left, nothing can play), and the gap releases
    // the note.
    m.trs(&[E4, 100, 0x7F, 0xF7]);
    m.usb(0x7, [E4, 100, 0xF7]);
    m.trs(&[67, 100]);
    assert_eq!(m.rx.restarts, 1);
    assert_eq!(m.held(), [] as [u8; 0]);
    assert!(
        m.rig
            .log()
            .iter()
            .all(|&(reg, value)| reg % 7 == 4 && value & 1 == 0),
        "writes other than gate releases: {:?}",
        m.rig.log()
    );
    // The parsers work afterwards.
    m.trs(&[0x90, C4, 100]);
    m.usb(0x9, [0x90, E4, 100]);
    let mut held = m.held();
    held.sort_unstable();
    assert_eq!(held, [C4, E4]);
}

#[test]
fn a_gap_after_the_last_entry_releases_the_notes_at_once() {
    let mut m = Midi::new();
    m.trs(&[0x90, C4, 100]);
    assert_eq!(unsafe { midi_rx_check_lost(&mut *m.rx, 0) }, 0, "no gap");
    assert_eq!(m.held(), [C4]);
    // The note-off may be among what was lost: nothing comes after it yet.
    assert_eq!(unsafe { midi_rx_check_lost(&mut *m.rx, 1) }, 1);
    assert_eq!(m.held(), [] as [u8; 0]);
    // The entries after the gap carry its epoch: no second restart.
    m.epoch = 1;
    m.trs(&[0x90, C4, 100]);
    assert_eq!(m.held(), [C4]);
    assert_eq!(m.rx.restarts, 1);
}

#[test]
fn an_epoch_that_is_not_newer_never_restarts() {
    let mut m = Midi::new();
    // The drain read epoch 0, then took an entry from epoch 1 that came in meanwhile.
    m.epoch = 1;
    m.trs(&[0x90, C4, 100]);
    assert_eq!(m.rx.restarts, 1);
    assert_eq!(
        unsafe { midi_rx_check_lost(&mut *m.rx, 0) },
        0,
        "restarted backwards"
    );
    assert_eq!(unsafe { midi_rx_check_lost(&mut *m.rx, 1) }, 0);
    assert_eq!(m.held(), [C4]);
}

#[test]
fn epochs_wrap() {
    let mut rig = Rig::new(SID_MODEL_6581);
    let mut rx = Box::<midi_rx>::default();
    unsafe { midi_rx_init(&mut *rx, &mut *rig.live, 250) };
    assert_eq!(unsafe { midi_rx_check_lost(&mut *rx, 250) }, 0);
    assert_eq!(unsafe { midi_rx_check_lost(&mut *rx, 255) }, 1);
    assert_eq!(
        unsafe { midi_rx_check_lost(&mut *rx, 3) },
        1,
        "255 -> 3 is newer"
    );
    assert_eq!(
        unsafe { midi_rx_check_lost(&mut *rx, 255) },
        0,
        "3 -> 255 is older"
    );
    assert_eq!(rx.restarts, 2);
}

#[test]
fn losses_from_before_init_are_not_held_against_it() {
    let mut rig = Rig::new(SID_MODEL_6581);
    let mut rx = Box::<midi_rx>::default();
    unsafe { midi_rx_init(&mut *rx, &mut *rig.live, 41) };
    assert_eq!(unsafe { midi_rx_check_lost(&mut *rx, 41) }, 0);
    assert_eq!(unsafe { midi_rx_check_lost(&mut *rx, 42) }, 1);
}
