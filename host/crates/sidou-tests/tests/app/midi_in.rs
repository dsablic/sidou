// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use std::ffi::c_void;

use sidou_sys::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ev {
    Off(u8, u8, u8),
    On(u8, u8, u8),
    PolyPressure(u8, u8, u8),
    Cc(u8, u8, u8),
    Program(u8, u8),
    Pressure(u8, u8),
    Bend(u8, u16),
    RealTime(u8),
    SongPosition(u16),
    SongSelect(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sx {
    Start,
    Data(u8),
    End,
    Abort(u8),
}
use Sx::{Abort, Data, End, Start};

#[derive(Default)]
struct Out {
    events: Vec<Ev>,
    sysex: Vec<Sx>,
    raw: Vec<midi_msg>,
}

unsafe extern "C" fn on_msg(ctx: *mut c_void, m: *const midi_msg) {
    let out = unsafe { &mut *ctx.cast::<Out>() };
    let m = unsafe { *m };
    out.raw.push(m);
    let (c, a, b) = (m.channel, m.data1, m.data2);
    out.events.push(match m.kind {
        MIDI_NOTE_OFF => Ev::Off(c, a, b),
        MIDI_NOTE_ON => Ev::On(c, a, b),
        MIDI_POLY_PRESSURE => Ev::PolyPressure(c, a, b),
        MIDI_CC => Ev::Cc(c, a, b),
        MIDI_PROGRAM => Ev::Program(c, a),
        MIDI_CHANNEL_PRESSURE => Ev::Pressure(c, a),
        MIDI_PITCH_BEND => Ev::Bend(c, m.bend),
        MIDI_REALTIME => Ev::RealTime(a),
        MIDI_SONG_POSITION => Ev::SongPosition(m.bend),
        MIDI_SONG_SELECT => Ev::SongSelect(a),
        k => panic!("unknown kind {k}"),
    });
}

unsafe extern "C" fn on_sysex(ctx: *mut c_void, event: midi_sysex_event, byte: u8) {
    let ev = match event {
        MIDI_SYSEX_START => {
            assert_eq!(byte, 0xf0);
            Start
        }
        MIDI_SYSEX_DATA => {
            assert!(byte < 0x80);
            Data(byte)
        }
        MIDI_SYSEX_END => {
            assert_eq!(byte, 0xf7);
            End
        }
        MIDI_SYSEX_ABORT => {
            assert!((0x80..0xf7).contains(&byte));
            Abort(byte)
        }
        e => panic!("unknown SysEx event {e}"),
    };
    unsafe { &mut *ctx.cast::<Out>() }.sysex.push(ev);
}

struct Parser {
    m: Box<midi_in>,
    out: Box<Out>,
}

impl Parser {
    fn new() -> Self {
        let mut out = Box::<Out>::default();
        let mut m = Box::<midi_in>::default();
        let ctx: *mut Out = &mut *out;
        unsafe { midi_in_init(&mut *m, Some(on_msg), Some(on_sysex), ctx.cast()) };
        Parser { m, out }
    }

    fn feed(&mut self, bytes: &[u8]) -> &mut Self {
        unsafe { midi_in_feed(&mut *self.m, bytes.as_ptr(), bytes.len()) };
        self
    }

    fn take(&mut self) -> Vec<Ev> {
        std::mem::take(&mut self.out.events)
    }
}

fn parse(bytes: &[u8]) -> Vec<Ev> {
    Parser::new().feed(bytes).take()
}

#[test]
fn channel_messages() {
    assert_eq!(
        parse(&[
            0x93, 60, 100, 0x83, 60, 64, 0xa2, 61, 33, 0xb5, 74, 127, 0xc9, 7, 0xdf, 99, 0xe0,
            0x00, 0x40
        ]),
        [
            Ev::On(3, 60, 100),
            Ev::Off(3, 60, 64),
            Ev::PolyPressure(2, 61, 33),
            Ev::Cc(5, 74, 127),
            Ev::Program(9, 7),
            Ev::Pressure(15, 99),
            Ev::Bend(0, 8192),
        ]
    );
}

#[test]
fn pitch_bend_is_14_bit_lsb_first() {
    assert_eq!(
        parse(&[0xe1, 0x7f, 0x7f, 0x00, 0x00, 0x01, 0x02]),
        [Ev::Bend(1, 16383), Ev::Bend(1, 0), Ev::Bend(1, 0x101)]
    );
}

#[test]
fn velocity_zero_note_on_is_note_off() {
    assert_eq!(
        parse(&[0x90, 60, 0, 61, 1]),
        [Ev::Off(0, 60, 0), Ev::On(0, 61, 1)]
    );
}

#[test]
fn running_status() {
    assert_eq!(
        parse(&[0x90, 60, 100, 64, 100, 67, 0, 0xb0, 1, 2, 3, 4, 0xc0, 5, 6]),
        [
            Ev::On(0, 60, 100),
            Ev::On(0, 64, 100),
            Ev::Off(0, 67, 0),
            Ev::Cc(0, 1, 2),
            Ev::Cc(0, 3, 4),
            Ev::Program(0, 5),
            Ev::Program(0, 6),
        ]
    );
}

#[test]
fn running_status_survives_split_feeds() {
    let mut p = Parser::new();
    p.feed(&[0x91, 60]);
    assert!(p.take().is_empty());
    p.feed(&[100, 62]);
    assert_eq!(p.take(), [Ev::On(1, 60, 100)]);
    p.feed(&[90]);
    assert_eq!(p.take(), [Ev::On(1, 62, 90)]);
}

#[test]
fn real_time_bytes_inside_a_message_change_nothing() {
    assert_eq!(
        parse(&[0x90, 0xf8, 60, 0xfe, 100, 0xfa, 62, 0xff, 0xf9, 90]),
        [
            Ev::RealTime(0xf8),
            Ev::RealTime(0xfe),
            Ev::On(0, 60, 100),
            Ev::RealTime(0xfa),
            Ev::RealTime(0xff),
            Ev::RealTime(0xf9),
            Ev::On(0, 62, 90),
        ]
    );
}

#[test]
fn real_time_bytes_alone() {
    assert_eq!(
        parse(&[0xf8, 0xfc, 60, 100]),
        [Ev::RealTime(0xf8), Ev::RealTime(0xfc)],
        "no running status yet, so the data is dropped"
    );
}

#[test]
fn sysex_is_forwarded_and_otherwise_ignored() {
    let mut p = Parser::new();
    p.feed(&[0x90, 60, 100, 0xf0, 0x7e, 0x01, 0x90, 0x02]);
    assert_eq!(p.take(), [Ev::On(0, 60, 100)]);
    assert_eq!(p.out.sysex, [Start, Data(0x7e), Data(0x01), Abort(0x90)]);
    p.out.sysex.clear();

    // 0x90 above ended the SysEx: the note-on that follows is a message.
    p.feed(&[0x40, 0x64]);
    assert_eq!(p.take(), [Ev::On(0, 0x02, 0x40)]);
    assert!(p.out.sysex.is_empty());
}

#[test]
fn sysex_end_cancels_running_status() {
    let mut p = Parser::new();
    p.feed(&[0x90, 60, 100, 0xf0, 0x7d, 0x10, 0x20, 0xf7, 62, 100]);
    assert_eq!(p.take(), [Ev::On(0, 60, 100)]);
    assert_eq!(
        p.out.sysex,
        [Start, Data(0x7d), Data(0x10), Data(0x20), End]
    );
}

#[test]
fn real_time_inside_sysex_is_not_part_of_it() {
    let mut p = Parser::new();
    p.feed(&[0xf0, 0x01, 0xf8, 0x02, 0xfe, 0xf7]);
    assert_eq!(p.take(), [Ev::RealTime(0xf8), Ev::RealTime(0xfe)]);
    assert_eq!(p.out.sysex, [Start, Data(0x01), Data(0x02), End]);
}

#[test]
fn f0_inside_a_sysex_aborts_it_and_starts_another() {
    let mut p = Parser::new();
    p.feed(&[0xf0, 0x01, 0x02, 0xf0, 0x03, 0xf7]);
    assert!(p.take().is_empty());
    assert_eq!(
        p.out.sysex,
        [Start, Data(1), Data(2), Abort(0xf0), Start, Data(3), End]
    );
}

#[test]
fn any_other_status_byte_aborts_a_sysex() {
    for status in [
        0x80u8, 0x9f, 0xb0, 0xc5, 0xe0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6,
    ] {
        let mut p = Parser::new();
        p.feed(&[0xf0, 0x7d, status, 0x01, 0x02, 0x03, 0xf7]);
        assert_eq!(
            p.out.sysex,
            [Start, Data(0x7d), Abort(status)],
            "status {status:#x}"
        );
    }
}

#[test]
fn real_time_bytes_do_not_abort_a_sysex() {
    for rt in 0xf8..=0xffu8 {
        let mut p = Parser::new();
        p.feed(&[0xf0, 0x01, rt, 0x02, 0xf7]);
        assert_eq!(p.out.sysex, [Start, Data(1), Data(2), End], "{rt:#x}");
    }
}

#[test]
fn a_stray_sysex_end_is_not_forwarded() {
    let mut p = Parser::new();
    p.feed(&[0xf7, 0x01]);
    assert!(p.take().is_empty());
    assert!(p.out.sysex.is_empty());
}

#[test]
fn sysex_without_a_callback_is_skipped() {
    let mut out = Box::<Out>::default();
    let mut m = Box::<midi_in>::default();
    let ctx: *mut Out = &mut *out;
    let bytes = [0xf0, 1, 2, 0xf7, 0x90, 60, 1];
    unsafe {
        midi_in_init(&mut *m, Some(on_msg), None, ctx.cast());
        midi_in_feed(&mut *m, bytes.as_ptr(), bytes.len());
    }
    assert_eq!(out.events, [Ev::On(0, 60, 1)]);
}

#[test]
fn system_common_cancels_running_status() {
    // F1 (1 byte, skipped), F2 (2 bytes), F3 (1 byte), F6 (none), F4/F5 (undefined, none).
    for (sc, data, ev) in [
        (0xf1, &[0x10][..], None),
        (
            0xf2,
            &[0x10, 0x20],
            Some(Ev::SongPosition(0x10 | 0x20 << 7)),
        ),
        (0xf3, &[0x05], Some(Ev::SongSelect(5))),
        (0xf6, &[], None),
        (0xf4, &[], None),
        (0xf5, &[], None),
    ] {
        let mut bytes = vec![0x90, 60, 100, sc];
        bytes.extend_from_slice(data);
        bytes.extend_from_slice(&[62, 100, 64]);
        let mut want = vec![Ev::On(0, 60, 100)];
        want.extend(ev);
        assert_eq!(parse(&bytes), want, "status {sc:#x}");
    }
}

#[test]
fn song_position_is_14_bit_lsb_first_and_song_select_is_reported() {
    assert_eq!(
        parse(&[0xf2, 0x7f, 0x7f, 0xf2, 0x00, 0x00, 0xf2, 0x01, 0x02, 0xf3, 0x7f, 0xf3, 0]),
        [
            Ev::SongPosition(16383),
            Ev::SongPosition(0),
            Ev::SongPosition(0x101),
            Ev::SongSelect(127),
            Ev::SongSelect(0),
        ]
    );
    let m = Parser::new().feed(&[0xf2, 0x05, 0x01]).out.raw[0];
    assert_eq!((m.channel, m.data1, m.data2), (0, 0, 0), "{m:?}");
}

#[test]
fn real_time_bytes_inside_song_position_and_song_select() {
    assert_eq!(
        parse(&[0xf2, 0xf8, 0x10, 0xfa, 0x01, 0xf3, 0xfc, 0x09, 0xf8]),
        [
            Ev::RealTime(0xf8),
            Ev::RealTime(0xfa),
            Ev::SongPosition(0x90),
            Ev::RealTime(0xfc),
            Ev::SongSelect(9),
            Ev::RealTime(0xf8),
        ]
    );
}

#[test]
fn truncated_song_position_and_select_are_dropped() {
    assert_eq!(
        parse(&[0xf2, 0x10, 0x90, 60, 1, 0xf3, 0xf2, 0x01, 0xf6, 0xf3, 0xf0, 0xf7]),
        [Ev::On(0, 60, 1)]
    );
}

#[test]
fn truncated_messages_are_dropped() {
    assert_eq!(
        parse(&[0x90, 60, 0xb0, 7, 100, 0xe0, 0x10, 0x91, 61, 62, 0xc0]),
        [Ev::Cc(0, 7, 100), Ev::On(1, 61, 62)]
    );
}

#[test]
fn data_before_any_status_is_ignored() {
    assert_eq!(parse(&[1, 2, 3, 0x80, 4, 5]), [Ev::Off(0, 4, 5)]);
}

/// xorshift64*: deterministic, no dependencies.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
}

#[test]
fn random_streams_never_produce_out_of_range_data() {
    let mut rng = Rng(0x5eed_51d0_u64);
    let mut p = Parser::new();
    let mut total = 0usize;
    let mut sysex_open = false;
    for round in 0..2000 {
        let len = (rng.next() % 512) as usize;
        // Every few rounds bias towards status bytes, otherwise towards data.
        let status_odds = [2u64, 8, 32][round % 3];
        let bytes: Vec<u8> = (0..len)
            .map(|_| {
                let r = rng.next();
                if r.is_multiple_of(status_odds) {
                    0x80 | (r >> 8) as u8
                } else {
                    (r >> 8) as u8 & 0x7f
                }
            })
            .collect();
        p.feed(&bytes);
        for m in p.out.raw.drain(..) {
            total += 1;
            if m.kind == MIDI_REALTIME {
                assert!(m.data1 >= 0xf8 && m.channel == 0, "{m:?}");
                continue;
            }
            if m.kind == MIDI_SONG_POSITION || m.kind == MIDI_SONG_SELECT {
                assert!(m.channel == 0 && m.data2 == 0, "{m:?}");
                assert!(m.data1 <= 127 && m.bend <= 16383, "{m:?}");
                continue;
            }
            assert!(m.kind <= MIDI_PITCH_BEND, "{m:?}");
            assert!(m.channel <= 15, "{m:?}");
            assert!(m.data1 <= 127 && m.data2 <= 127, "{m:?}");
            assert!(m.bend <= 16383, "{m:?}");
            if m.kind == MIDI_NOTE_ON {
                assert!(m.data2 > 0, "{m:?}");
            }
        }
        // Every SysEx is a Start, data, and then exactly one End or Abort.
        let mut open = sysex_open;
        for &e in &p.out.sysex {
            match e {
                Start => {
                    assert!(!open, "a Start without an End or Abort first");
                    open = true;
                }
                Data(_) => assert!(open, "data outside a SysEx"),
                End | Abort(_) => {
                    assert!(open, "an End or Abort without a Start");
                    open = false;
                }
            }
        }
        sysex_open = open;
        p.out.sysex.clear();
        p.out.events.clear();
    }
    assert!(total > 10_000, "the streams produced {total} messages");
}

/// midi_in_channel (a USB packet's channel message whole, app/midi_rx.c) gives what feeding its
/// bytes does: the same messages, SysEx events and parser state, over a random stream of channel
/// messages, running status, SysEx and system messages between them.
#[test]
fn a_channel_message_whole_is_its_bytes_one_at_a_time() {
    let mut seed = 0x1234_5678u32;
    let mut next = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed >> 8
    };
    let (mut whole, mut bytes) = (Parser::new(), Parser::new());
    for _ in 0..20_000 {
        let r = next();
        if r % 4 == 0 {
            // anything else, byte by byte to both: SysEx, system common, running status, stray data
            let b = match next() % 6 {
                0 => 0xf0,
                1 => 0xf7,
                2 => 0xf1 + (next() % 7) as u8,
                3 => 0xf8 + (next() % 8) as u8,
                _ => (next() % 0x80) as u8,
            };
            whole.feed(&[b]);
            bytes.feed(&[b]);
        } else {
            let status = 0x80 + (next() % 0x70) as u8;
            let (d1, d2) = ((next() % 0x80) as u8, (next() % 0x80) as u8);
            unsafe { midi_in_channel(&mut *whole.m, status, d1, d2) };
            let n = if matches!(status & 0xf0, 0xc0 | 0xd0) {
                2
            } else {
                3
            };
            bytes.feed(&[status, d1, d2][..n]);
        }
        let (w, b) = (&*whole.m, &*bytes.m);
        assert_eq!(
            (w.status, w.count, w.data, w.in_sysex),
            (b.status, b.count, b.data, b.in_sysex)
        );
    }
    assert_eq!(whole.out.events, bytes.out.events);
    assert_eq!(whole.out.sysex, bytes.out.sysex);
    assert!(whole.out.events.len() > 10_000);
}
