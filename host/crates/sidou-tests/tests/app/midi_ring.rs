// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The poll-interrupt to main-loop MIDI ring (`app/midi_ring.c`).

use sidou_sys::*;

const SIZE: u32 = MIDI_RING_SIZE;
const USB: u8 = MIDI_SRC_USB as u8;
const TRS: u8 = MIDI_SRC_TRS as u8;

fn ring() -> Box<midi_ring> {
    let mut r = Box::<midi_ring>::default();
    unsafe { midi_ring_init(&mut *r) };
    r
}

fn push(r: &mut midi_ring, source: u8, data: u32, stamp: u32) -> bool {
    unsafe { midi_ring_push(r, source, data, stamp) != 0 }
}

fn pop(r: &mut midi_ring) -> Option<(u8, u32, u32)> {
    let mut e = midi_ring_entry::default();
    (unsafe { midi_ring_pop(r, &mut e) } != 0).then_some((e.source, e.data, e.stamp))
}

/// The next entry's epoch.
fn pop_epoch(r: &mut midi_ring) -> Option<u8> {
    let mut e = midi_ring_entry::default();
    (unsafe { midi_ring_pop(r, &mut e) } != 0).then_some(e.epoch)
}

fn epoch(r: &midi_ring) -> u8 {
    unsafe { midi_ring_epoch(r) }
}

fn room(r: &midi_ring) -> u32 {
    unsafe { midi_ring_room(r) }
}

fn lost(r: &midi_ring) -> u32 {
    unsafe { midi_ring_lost(r) }
}

#[test]
fn entries_come_out_in_order_with_source_data_and_stamp() {
    let mut r = ring();
    assert_eq!(pop(&mut r), None);
    assert_eq!(room(&r), SIZE);
    assert!(push(&mut r, TRS, 0x90, 1000));
    assert!(push(&mut r, USB, 0x643C_9009, 1001));
    assert!(push(&mut r, TRS, 0x3C, 0xFFFF_FFFF));
    assert_eq!(room(&r), SIZE - 3);
    assert_eq!(pop(&mut r), Some((TRS, 0x90, 1000)));
    assert_eq!(pop(&mut r), Some((USB, 0x643C_9009, 1001)));
    assert_eq!(pop(&mut r), Some((TRS, 0x3C, 0xFFFF_FFFF)));
    assert_eq!(pop(&mut r), None);
    assert_eq!(room(&r), SIZE);
    assert_eq!(lost(&r), 0);
}

#[test]
fn a_full_ring_drops_the_new_entry_and_counts_it() {
    let mut r = ring();
    for i in 0..SIZE {
        assert!(push(&mut r, TRS, i & 0xFF, i), "entry {i}");
    }
    assert_eq!(room(&r), 0);
    assert!(!push(&mut r, TRS, 0x99, 9999), "pushed into a full ring");
    assert!(!push(&mut r, USB, 0x99, 9999));
    assert_eq!(lost(&r), 2);
    assert_eq!(pop(&mut r), Some((TRS, 0, 0)), "the oldest stays");
    assert!(push(&mut r, USB, 0x1234, 7));
    for i in 1..SIZE {
        assert_eq!(pop(&mut r), Some((TRS, i & 0xFF, i)));
    }
    assert_eq!(pop(&mut r), Some((USB, 0x1234, 7)));
    assert_eq!(pop(&mut r), None);
    assert_eq!(lost(&r), 2);
}

#[test]
fn input_lost_before_the_ring_is_counted() {
    let mut r = ring();
    unsafe { midi_ring_lose(&mut *r, TRS, 5) };
    assert_eq!(lost(&r), 5);
    assert_eq!(pop(&mut r), None, "a loss is not an entry");
}

#[test]
fn each_gap_of_losses_starts_one_new_epoch_and_entries_carry_theirs() {
    let mut r = ring();
    assert!(push(&mut r, TRS, 1, 0));
    for i in 1..SIZE {
        assert!(push(&mut r, TRS, i, i));
    }
    assert_eq!(epoch(&r), 0);
    // Two losses in a row: one gap.
    assert!(!push(&mut r, TRS, 0, 0));
    assert!(!push(&mut r, TRS, 0, 0));
    assert_eq!(epoch(&r), 1);
    assert_eq!(pop_epoch(&mut r), Some(0));
    assert!(push(&mut r, USB, 0, 0), "the gap ends");
    assert!(!push(&mut r, TRS, 0, 0), "a new gap");
    assert_eq!(epoch(&r), 2);
    // Losses before the ring join the gap they fall into, and start one after a push.
    unsafe { midi_ring_lose(&mut *r, TRS, 7) };
    assert_eq!(epoch(&r), 2);
    for _ in 0..SIZE - 1 {
        assert_eq!(pop_epoch(&mut r), Some(0));
    }
    assert_eq!(pop_epoch(&mut r), Some(1), "pushed after the first gap");
    assert_eq!(pop_epoch(&mut r), None);
    assert!(push(&mut r, TRS, 0, 0));
    unsafe { midi_ring_lose(&mut *r, TRS, 0) };
    assert_eq!(epoch(&r), 2, "losing nothing is no gap");
    unsafe { midi_ring_lose(&mut *r, TRS, 300) };
    assert_eq!(epoch(&r), 3, "one gap however many were lost");
    assert!(push(&mut r, TRS, 0, 0));
    assert_eq!(pop_epoch(&mut r), Some(2));
    assert_eq!(pop_epoch(&mut r), Some(3));
    assert_eq!(lost(&r), 3 + 7 + 300);
}

#[test]
fn the_indices_wrap_around_u32() {
    let mut r = ring();
    r.head = u32::MAX - 2;
    r.tail = u32::MAX - 2;
    assert_eq!(room(&r), SIZE);
    for i in 0..10 {
        assert!(push(&mut r, TRS, i, i));
    }
    assert_eq!(room(&r), SIZE - 10);
    for i in 0..10 {
        assert_eq!(pop(&mut r), Some((TRS, i, i)));
    }
    assert_eq!(pop(&mut r), None);
    assert_eq!(r.head, 7, "the head wrapped");
}

#[test]
fn a_long_interleaved_run_keeps_every_entry_in_order() {
    let mut r = ring();
    let (mut next_in, mut next_out) = (0u32, 0u32);
    // Bursts of 1..=40 pushes against takes of 1..=37: the fill wanders all over the ring.
    for round in 0..5000u32 {
        for _ in 0..1 + round * 7 % 40 {
            if room(&r) == 0 {
                break;
            }
            let src = if next_in % 3 == 0 { USB } else { TRS };
            assert!(push(&mut r, src, next_in, next_in.wrapping_mul(3)));
            next_in += 1;
        }
        for _ in 0..1 + round * 5 % 37 {
            match pop(&mut r) {
                Some((src, data, stamp)) => {
                    assert_eq!(data, next_out);
                    assert_eq!(stamp, next_out.wrapping_mul(3));
                    assert_eq!(src, if next_out % 3 == 0 { USB } else { TRS });
                    next_out += 1;
                }
                None => break,
            }
        }
    }
    assert!(next_in > 50 * SIZE, "only {next_in} entries went through");
    assert_eq!(lost(&r), 0);
}

/// A gap says whose input it lost (review 4.1): the ring holds the sources' bits until the
/// next push, which carries them in its entry and clears them; losses of both sources in one
/// gap are one gap with both bits.
#[test]
fn a_gap_carries_the_sources_it_lost() {
    let mut r = ring();
    let mut e = midi_ring_entry::default();
    assert!(push(&mut r, USB, 0, 0));
    unsafe { midi_ring_lose(&mut *r, TRS, 3) };
    assert_eq!(r.in_gap, TRS);
    assert!(push(&mut r, USB, 1, 0));
    assert_eq!(r.in_gap, 0, "the push ends the gap");
    for i in 2..SIZE {
        assert!(push(&mut r, USB, i, 0));
    }
    assert!(!push(&mut r, USB, 0, 0), "full: a USB loss");
    unsafe { midi_ring_lose(&mut *r, TRS, 1) };
    assert_eq!(r.in_gap, USB | TRS);
    assert_eq!(epoch(&r), 2, "one gap");
    let mut gaps = Vec::new();
    while unsafe { midi_ring_pop(&mut *r, &mut e) } != 0 {
        gaps.push((e.epoch, e.gap));
    }
    assert_eq!(gaps[0], (0, 0));
    assert_eq!(gaps[1], (1, TRS), "the entry after the first gap");
    assert!(gaps[2..].iter().all(|&g| g == (1, 0)));
    assert!(push(&mut r, TRS, 0, 0));
    assert!(unsafe { midi_ring_pop(&mut *r, &mut e) } != 0);
    assert_eq!((e.epoch, e.gap), (2, USB | TRS));
    // An unknown source still makes a gap, with every bit.
    unsafe { midi_ring_lose(&mut *r, 0, 1) };
    assert_eq!((epoch(&r), r.in_gap), (3, 0xFF));
}
