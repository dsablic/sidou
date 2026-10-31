// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;
use sidou_tests::song::Rng;

const FLAGS_NTSC_8580: u16 = 0x28;

#[derive(Clone)]
struct Psid {
    magic: [u8; 4],
    version: u16,
    off: Option<u16>,
    load: u16,
    init: u16,
    play: u16,
    songs: u16,
    start: u16,
    speed: u32,
    meta: [u8; 96],
    flags: u16,
    sid2: u8,
    sid3: u8,
    data: Vec<u8>,
    tail: Vec<u8>,
}

impl Psid {
    fn new(version: u16) -> Psid {
        Psid {
            magic: *b"PSID",
            version,
            off: None,
            load: 0x1000,
            init: 0x1000,
            play: 0x1003,
            songs: 1,
            start: 1,
            speed: 0,
            meta: [0; 96],
            flags: 0,
            sid2: 0,
            sid3: 0,
            data: vec![0x60; 16],
            tail: Vec::new(),
        }
    }

    fn header_len(&self) -> usize {
        if self.version == 1 {
            0x76
        } else {
            0x7C
        }
    }

    fn string(&mut self, field: usize, text: &[u8]) {
        self.meta[field * 32..field * 32 + text.len()].copy_from_slice(text);
    }

    fn bytes(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&self.magic);
        for w in [
            self.version,
            self.off.unwrap_or(self.header_len() as u16),
            self.load,
            self.init,
            self.play,
            self.songs,
            self.start,
        ] {
            b.extend_from_slice(&w.to_be_bytes());
        }
        b.extend_from_slice(&self.speed.to_be_bytes());
        b.extend_from_slice(&self.meta);
        if self.version > 1 {
            b.extend_from_slice(&self.flags.to_be_bytes());
            b.extend_from_slice(&[0, 0, self.sid2, self.sid3]);
        }
        b.extend_from_slice(&self.data);
        b.extend_from_slice(&self.tail);
        b
    }

    fn parse(&self) -> Result<psid_header, i32> {
        parse(&self.bytes())
    }

    fn ok(&self) -> psid_header {
        self.parse().unwrap_or_else(|e| panic!("refused with {e}"))
    }
}

fn parse(b: &[u8]) -> Result<psid_header, i32> {
    let exact: Box<[u8]> = b.into();
    let mut h = psid_header::default();
    let r = unsafe { psid_parse(exact.as_ptr(), exact.len() as u32, &mut h) };
    if r == PSID_OK as i32 {
        Ok(h)
    } else {
        Err(r)
    }
}

fn text(s: &[std::os::raw::c_char]) -> Vec<u8> {
    s.iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect()
}

fn refused(p: &Psid, code: i32) {
    assert_eq!(p.parse().err(), Some(code), "{:02x?}", &p.bytes()[..8]);
}

#[test]
fn a_plain_v2_file_is_read_field_by_field() {
    let mut p = Psid::new(2);
    p.load = 0x0801;
    p.init = 0x0810;
    p.play = 0x0820;
    p.songs = 5;
    p.start = 3;
    p.speed = 0x8000_0001;
    p.flags = FLAGS_NTSC_8580;
    p.data = vec![0xEA; 100];
    let h = p.ok();
    assert_eq!(
        (h.version, h.load, h.init, h.play, h.songs, h.start),
        (2, 0x0801, 0x0810, 0x0820, 5, 3)
    );
    assert_eq!((h.speed, h.flags), (0x8000_0001, 0x28));
    assert_eq!((h.data_off, h.data_len), (0x7C, 100));
    assert_eq!(
        (h.clock, h.model1, h.model2, h.sids, h.warn),
        (2, 2, 2, 1, 0)
    );
    assert_eq!((h.sid2, h.sid3), (0, 0));
}

#[test]
fn not_a_sid_file() {
    refused(
        &Psid {
            magic: *b"MIDI",
            ..Psid::new(2)
        },
        PSID_E_NOT_SID as i32,
    );
    refused(
        &Psid {
            magic: *b"psid",
            ..Psid::new(2)
        },
        PSID_E_NOT_SID as i32,
    );
    assert_eq!(parse(&[]).err(), Some(PSID_E_NOT_SID as i32));
    let full = Psid::new(1).bytes();
    for n in [0, 1, 4, 0x20, 0x75] {
        assert_eq!(
            parse(&full[..n]).err(),
            Some(PSID_E_NOT_SID as i32),
            "n {n}"
        );
    }
    parse(&full[..0x76 + 1]).unwrap();
    let mut rsid = Psid::new(2);
    rsid.magic = *b"RSID";
    assert_eq!(
        parse(&rsid.bytes()[..0x75]).err(),
        Some(PSID_E_NOT_SID as i32)
    );
}

#[test]
fn rsid_is_refused_with_its_own_code() {
    for v in 2..=5 {
        let mut p = Psid::new(v);
        p.magic = *b"RSID";
        p.load = 0;
        p.play = 0;
        refused(&p, PSID_E_RSID as i32);
    }
    let mut p = Psid::new(9);
    p.magic = *b"RSID";
    refused(&p, PSID_E_RSID as i32);
}

#[test]
fn versions_one_to_five_only() {
    for v in [0u16, 6, 7, 0x0100, 0xFFFF] {
        refused(
            &Psid {
                off: Some(0x7C),
                ..Psid::new(v)
            },
            PSID_E_HEADER as i32,
        );
    }
    for v in 1..=5 {
        assert_eq!(Psid::new(v).ok().version, v);
    }
}

#[test]
fn the_data_offset_is_the_versions_and_inside_the_file() {
    for (v, wrong) in [
        (1u16, 0x7Cu16),
        (2, 0x76),
        (3, 0x76),
        (4, 0x80),
        (5, 0),
        (2, 0xFFFF),
        (1, 0),
    ] {
        refused(
            &Psid {
                off: Some(wrong),
                ..Psid::new(v)
            },
            PSID_E_HEADER as i32,
        );
    }
    let mut p = Psid::new(2);
    p.data.clear();
    refused(&p, PSID_E_TOO_LONG as i32);
}

#[test]
fn a_v2_header_needs_its_flag_bytes_in_the_file() {
    let full = Psid::new(2).bytes();
    for n in 0x76..0x7C {
        assert_eq!(parse(&full[..n]).err(), Some(PSID_E_HEADER as i32), "n {n}");
    }
    let mut p = Psid::new(2);
    p.off = Some(0x76);
    refused(&p, PSID_E_HEADER as i32);
}

#[test]
fn the_mus_flag_is_refused_from_v2_and_ignored_in_v1() {
    for v in 2..=5 {
        refused(
            &Psid {
                flags: 1,
                ..Psid::new(v)
            },
            PSID_E_MUS as i32,
        );
    }
    let mut p = Psid::new(1);
    p.data = vec![0x00, 0x01, 0, 0, 0x60];
    assert_eq!(p.ok().flags, 0);
}

#[test]
fn load_address_zero_reads_two_more_bytes_of_the_file() {
    let mut p = Psid::new(2);
    p.load = 0;
    p.init = 0;
    p.play = 0;
    p.data = vec![0x00, 0xC0, 0x60, 0x60, 0x60];
    let h = p.ok();
    assert_eq!((h.load, h.init, h.play), (0xC000, 0xC000, 0));
    assert_eq!((h.data_off, h.data_len), (0x7E, 3));
    for (len, want) in [
        (0, Err(PSID_E_HEADER)),
        (1, Err(PSID_E_HEADER)),
        (2, Err(PSID_E_TOO_LONG)),
        (3, Ok(1)),
    ] {
        p.data = vec![0x00, 0xC0, 0x60][..len].to_vec();
        match want {
            Ok(n) => assert_eq!(p.ok().data_len, n),
            Err(e) => refused(&p, e as i32),
        }
    }
}

#[test]
fn the_data_must_have_a_byte_and_fit_the_address_space() {
    let mut p = Psid::new(2);
    p.data.clear();
    refused(&p, PSID_E_TOO_LONG as i32);
    for (load, len, ok) in [
        (0xFFFFu16, 1usize, true),
        (0xFFFF, 2, false),
        (0xFFFE, 2, true),
        (0xFFFE, 3, false),
        (0x0002, 0xFFFE, true),
        (0x0002, 0xFFFF, false),
        (0x1000, 0xF000, true),
        (0x1000, 0xF001, false),
        (0x1000, 0x2_0000, false),
    ] {
        p.load = load;
        p.init = load;
        p.data = vec![0x60; len];
        if ok {
            let h = p.ok();
            assert_eq!(h.data_len as usize, len);
            assert!(h.load as u32 + h.data_len <= 0x10000);
        } else {
            refused(&p, PSID_E_TOO_LONG as i32);
        }
    }
}

#[test]
fn the_data_may_not_cover_the_6510_port() {
    let mut p = Psid::new(2);
    p.load = 1;
    p.data = vec![0x60; 8];
    refused(&p, PSID_E_LOAD as i32);
    p.load = 0;
    p.data = vec![0x00, 0x00, 0x60];
    refused(&p, PSID_E_LOAD as i32);
    p.data = vec![0x01, 0x00, 0x60];
    refused(&p, PSID_E_LOAD as i32);
    p.data = vec![0x02, 0x00, 0x60];
    assert_eq!(p.ok().load, 2);
    p.load = 2;
    p.data = vec![0x60];
    assert_eq!(p.ok().load, 2);
}

#[test]
fn songs_one_to_256_start_song_corrected_init_defaults() {
    for songs in [0u16, 257, 0x8000, 0xFFFF] {
        refused(
            &Psid {
                songs,
                ..Psid::new(2)
            },
            PSID_E_HEADER as i32,
        );
    }
    for songs in [1u16, 2, 255, 256] {
        assert_eq!(
            Psid {
                songs,
                ..Psid::new(2)
            }
            .ok()
            .songs,
            songs
        );
    }
    for (songs, start, want, warn) in [
        (3u16, 0u16, 1u16, true),
        (3, 4, 1, true),
        (3, 0xFFFF, 1, true),
        (3, 3, 3, false),
        (256, 256, 256, false),
        (1, 1, 1, false),
    ] {
        let h = Psid {
            songs,
            start,
            ..Psid::new(2)
        }
        .ok();
        assert_eq!(h.start, want, "{songs} {start}");
        assert_eq!(h.warn & PSID_W_START as u8 != 0, warn, "{songs} {start}");
    }
    let h = Psid {
        init: 0,
        load: 0x2000,
        ..Psid::new(2)
    }
    .ok();
    assert_eq!(h.init, 0x2000);
    let h = Psid {
        init: 0x1234,
        ..Psid::new(2)
    }
    .ok();
    assert_eq!(h.init, 0x1234);
}

#[test]
fn clock_models_and_the_playsid_flag() {
    for (flags, clock, m1) in [
        (0u16, 0u8, 0u8),
        (4, 1, 0),
        (8, 2, 0),
        (12, 3, 0),
        (0x10, 0, 1),
        (0x20, 0, 2),
        (0x30, 0, 3),
    ] {
        let h = Psid {
            flags,
            ..Psid::new(2)
        }
        .ok();
        assert_eq!((h.clock, h.model1), (clock, m1), "{flags:#x}");
        assert_eq!(h.warn, 0);
    }
    let h = Psid {
        flags: 2,
        ..Psid::new(3)
    }
    .ok();
    assert_eq!(h.warn, PSID_W_PLAYSID as u8);
    assert_eq!(h.speed_repeats, 1);
    let h = Psid::new(1).ok();
    assert_eq!(
        (h.flags, h.clock, h.model1, h.warn, h.speed_repeats),
        (0, 0, 0, 0, 1)
    );
}

fn speed(p: &Psid, song: u32) -> i32 {
    psid_speed_bit_of(&p.ok(), song)
}

fn psid_speed_bit_of(h: &psid_header, song: u32) -> i32 {
    unsafe { psid_speed_bit(h, song) }
}

#[test]
fn the_speed_bit_follows_the_flags() {
    let mut p = Psid::new(2);
    p.songs = 64;
    p.speed = 0x8000_0001;
    assert_eq!(
        (speed(&p, 1), speed(&p, 2), speed(&p, 31), speed(&p, 32)),
        (1, 0, 0, 1)
    );
    for song in [33, 40, 64] {
        assert_eq!(speed(&p, song), 1, "song {song}");
    }
    p.speed = 0x0000_0001;
    for song in [33, 40, 64] {
        assert_eq!(speed(&p, song), 0, "song {song}");
    }
    for mut q in [
        Psid {
            flags: 2,
            ..p.clone()
        },
        Psid {
            version: 1,
            ..p.clone()
        },
    ] {
        q.speed = 0x8000_0001;
        assert_eq!((speed(&q, 33), speed(&q, 34), speed(&q, 64)), (1, 0, 1));
        assert_eq!(speed(&q, 32), 1);
    }
}

#[test]
fn a_second_sid_in_v3_and_v4() {
    for a in [0x42u8, 0x44, 0x7E, 0xE0, 0xFE, 0x60] {
        for v in [3, 4] {
            let h = Psid {
                sid2: a,
                ..Psid::new(v)
            }
            .ok();
            assert_eq!(h.sid2, 0xD000 + a as u16 * 16, "{a:#x}");
            assert_eq!((h.sids, h.warn & PSID_W_SID2 as u8), (2, 0));
        }
    }
    for a in [
        0x01u8, 0x40, 0x41, 0x43, 0x7F, 0x80, 0xB0, 0xDE, 0xDF, 0xE1, 0xFF,
    ] {
        let h = Psid {
            sid2: a,
            ..Psid::new(3)
        }
        .ok();
        assert_eq!((h.sid2, h.sids), (0, 1), "{a:#x}");
        assert_eq!(h.warn, PSID_W_SID2 as u8, "{a:#x}");
    }
    let h = Psid::new(3).ok();
    assert_eq!((h.sid2, h.sids, h.warn), (0, 1, 0));
    let h = Psid {
        sid2: 0x42,
        sid3: 0x44,
        ..Psid::new(2)
    }
    .ok();
    assert_eq!((h.sid2, h.sid3, h.sids, h.warn), (0, 0, 1, 0));
}

#[test]
fn a_third_sid_only_in_v4_and_never_the_second_s_address() {
    let h = Psid {
        sid2: 0x42,
        sid3: 0x44,
        ..Psid::new(4)
    }
    .ok();
    assert_eq!((h.sid2, h.sid3, h.sids), (0xD420, 0xD440, 3));
    let h = Psid {
        sid2: 0x42,
        sid3: 0x44,
        ..Psid::new(3)
    }
    .ok();
    assert_eq!((h.sid2, h.sid3, h.sids), (0xD420, 0, 2));
    let h = Psid {
        sid2: 0x42,
        sid3: 0x42,
        ..Psid::new(4)
    }
    .ok();
    assert_eq!((h.sid3, h.sids, h.warn), (0, 2, PSID_W_SID3 as u8));
    let h = Psid {
        sid2: 0x42,
        sid3: 0x43,
        ..Psid::new(4)
    }
    .ok();
    assert_eq!((h.sid3, h.warn), (0, PSID_W_SID3 as u8));
}

#[test]
fn the_second_and_third_model_in_v3_and_v4() {
    let f = |m1: u16, m2: u16, m3: u16| (0x10 * m1) | (0x40 * m2) | (0x100 * m3);
    let h = Psid {
        flags: f(2, 0, 0),
        sid2: 0x42,
        sid3: 0x44,
        ..Psid::new(4)
    }
    .ok();
    assert_eq!((h.model1, h.model2, h.model3), (2, 2, 2));
    let h = Psid {
        flags: f(2, 1, 3),
        sid2: 0x42,
        sid3: 0x44,
        ..Psid::new(4)
    }
    .ok();
    assert_eq!((h.model1, h.model2, h.model3), (2, 1, 3));
    let h = Psid {
        flags: f(0, 2, 1),
        ..Psid::new(3)
    }
    .ok();
    assert_eq!((h.model2, h.model3), (2, 0));
    let h = Psid {
        flags: f(1, 2, 2),
        ..Psid::new(2)
    }
    .ok();
    assert_eq!((h.model1, h.model2, h.model3), (1, 1, 1));
}

#[test]
fn strings_with_no_nul_bytes_after_one_and_high_bytes() {
    let mut p = Psid::new(2);
    p.string(0, &[b'A'; 32]);
    p.string(1, b"Author\0ignored after the NUL");
    p.string(2, &[0xE9, b'x', 0x01, 0x7F, 0xFF, b'~', b' ', 0x80]);
    let h = p.ok();
    assert_eq!(text(&h.name), vec![b'A'; 32]);
    assert_eq!(text(&h.author), b"Author");
    assert_eq!(text(&h.released), b"?x???~ ?");
    p.meta = [b'Z'; 96];
    let h = p.ok();
    for s in [&h.name, &h.author, &h.released] {
        assert_eq!(s[s.len() - 1], 0);
    }
    assert_eq!(text(&h.name), vec![b'Z'; 32]);
}

#[test]
fn v5_packs_the_strings_and_falls_back_to_three_fields() {
    let mut p = Psid::new(5);
    let long: Vec<u8> = (0..40).map(|i| b'a' + (i % 26) as u8).collect();
    p.meta = [0; 96];
    p.meta[..40].copy_from_slice(&long);
    p.meta[41..47].copy_from_slice(b"Author");
    p.meta[50..58].copy_from_slice(b"Released");
    let h = p.ok();
    assert_eq!(text(&h.name), long);
    assert_eq!(text(&h.author), b"Author");
    assert_eq!(text(&h.released), b"Released");
    p.meta = [b'r'; 96];
    p.meta[0] = b'n';
    p.meta[1] = 0;
    p.meta[2] = b'a';
    p.meta[3] = 0;
    let h = p.ok();
    assert_eq!(
        (text(&h.name), text(&h.author)),
        (b"n".to_vec(), b"a".to_vec())
    );
    assert_eq!(text(&h.released).len(), 63);
    let mut q = Psid::new(5);
    q.string(0, b"TitleThatIsExactly32OfCharacters");
    q.string(1, b"Author");
    q.string(2, b"Released");
    let h = q.ok();
    assert_eq!(text(&h.name), b"TitleThatIsExactly32OfCharacters");
    assert_eq!(
        (text(&h.author), text(&h.released)),
        (b"Author".to_vec(), b"Released".to_vec())
    );
    let mut r = Psid::new(4);
    r.meta[..5].copy_from_slice(b"n\0a\0r");
    let h = r.ok();
    assert_eq!(
        (text(&h.name), text(&h.author), text(&h.released)),
        (b"n".to_vec(), vec![], vec![])
    );
}

fn v5(count: u8, scheme: u8, start: u8) -> Psid {
    let mut p = Psid::new(5);
    p.sid2 = scheme << 4 | count;
    p.sid3 = start;
    p
}

#[test]
fn v5_multi_sid_addresses() {
    let one = Psid::new(5).ok();
    assert_eq!((one.sids, one.sid2, one.sid3), (1, 0, 0));
    for (scheme, want) in [
        (0u8, vec![0xD420u16, 0xD440, 0xD460, 0xD480]),
        (1, vec![0xD440, 0xD480, 0xD4C0, 0xD500]),
        (2, vec![0xD480, 0xD500, 0xD580, 0xD600]),
        (3, vec![0xD500, 0xD600, 0xD700, 0xDE00, 0xDF00]),
        (4, vec![0xD420, 0xD480, 0xD4A0, 0xD500]),
        (5, vec![0xD420, 0xD500, 0xD520, 0xD600]),
        (6, vec![0xD500, 0xDE00, 0xDF00]),
    ] {
        let h = v5(want.len() as u8 + 1, scheme, 0).ok();
        assert_eq!(h.sids as usize, want.len() + 1, "scheme {scheme}");
        assert_eq!(
            (h.sid2, h.sid3),
            (want[0], *want.get(1).unwrap_or(&0)),
            "scheme {scheme}"
        );
        assert_eq!(
            h.warn & PSID_W_MORE_SIDS as u8 != 0,
            want.len() > 2,
            "scheme {scheme}"
        );
    }
    for (scheme, most) in [
        (0u8, 15u8),
        (1, 15),
        (2, 12),
        (3, 6),
        (4, 15),
        (5, 12),
        (6, 4),
    ] {
        v5(most, scheme, 0).ok();
        if most < 15 {
            refused(&v5(most + 1, scheme, 0), PSID_E_HEADER as i32);
        }
    }
    let h = v5(3, 3, 0xE0).ok();
    assert_eq!((h.sid2, h.sid3), (0xDE00, 0xDF00));
    refused(&v5(4, 3, 0xE0), PSID_E_HEADER as i32);
    let h = v5(4, 0, 0x60).ok();
    assert_eq!((h.sid2, h.sid3), (0xD600, 0xD620));
    let h = v5(3, 4, 0x60).ok();
    assert_eq!((h.sid2, h.sid3), (0xD600, 0xD620));
    let h = v5(3, 0, 0x43).ok();
    assert_eq!((h.sid2, h.sid3), (0xD420, 0xD440));
    for scheme in 7..16 {
        refused(&v5(2, scheme, 0), PSID_E_HEADER as i32);
    }
    assert_eq!(v5(1, 0, 0).ok().sids, 1);
    assert_eq!(v5(0, 3, 0x42).ok().sids, 1);
    let h = v5(15, 0, 0).ok();
    assert_eq!((h.sids, h.warn), (15, PSID_W_MORE_SIDS as u8));
    let h = Psid {
        flags: 0x10 | 0x80 | 0x200,
        ..v5(3, 0, 0)
    }
    .ok();
    assert_eq!((h.model1, h.model2, h.model3), (1, 1, 1));
}

#[test]
fn v5_fm_opl_and_the_embedded_song_lengths() {
    let mut p = v5(6, 3, 0);
    p.flags = 1 << 11;
    refused(&p, PSID_E_HEADER as i32);
    let mut q = v5(3, 3, 0);
    q.flags = 1 << 11;
    assert_eq!(q.ok().warn, PSID_W_FM_OPL as u8);
    Psid {
        sid2: 0xF0,
        flags: 1 << 11,
        ..Psid::new(4)
    }
    .ok();

    let mut p = Psid::new(5);
    p.songs = 3;
    p.flags = 1 << 10;
    p.data = vec![0x60; 20];
    p.tail = vec![0; 12];
    let h = p.ok();
    assert_eq!(
        (h.data_off, h.data_len, h.lengths_off),
        (0x7C, 20, 0x7C + 20)
    );
    p.load = 0;
    p.data = vec![0x00, 0x10, 0x60, 0x60];
    let h = p.ok();
    assert_eq!((h.data_off, h.data_len), (0x7E, 2));
    p.data = vec![];
    p.tail = vec![0; 11];
    refused(&p, PSID_E_HEADER as i32);
    p.tail = vec![0; 12];
    refused(&p, PSID_E_HEADER as i32);
    p.load = 0x1000;
    refused(&p, PSID_E_TOO_LONG as i32);
    let mut q = Psid::new(4);
    q.flags = 1 << 10;
    q.data = vec![0x60; 20];
    let h = q.ok();
    assert_eq!((h.data_len, h.lengths_off), (20, 0));
}

#[test]
fn fm1_sids_bounds_bug_files_are_refused() {
    let mut b = vec![0u8; 124];
    b[..4].copy_from_slice(b"PSID");
    b[5] = 2;
    b[7] = 0x7C;
    b[15] = 1;
    b[17] = 1;
    assert_eq!(
        parse(&b).err(),
        Some(PSID_E_HEADER as i32),
        "the minimal crafted file: 124 bytes, the data offset at its end"
    );
    b.push(0);
    assert_eq!(
        parse(&b).err(),
        Some(PSID_E_HEADER as i32),
        "one byte of data still leaves the load address short"
    );
    let mut c = vec![0u8; 124];
    c[..4].copy_from_slice(b"PSID");
    c[5] = 2;
    c[6] = 0x20;
    c[8] = 0;
    c[11] = 0x10;
    c[13] = 0x10;
    c[15] = 1;
    c[17] = 1;
    assert_eq!(
        parse(&c).err(),
        Some(PSID_E_HEADER as i32),
        "the upstream report's file: the data offset $2000, past the end"
    );
    c[8] = 0x10;
    assert_eq!(
        parse(&c).err(),
        Some(PSID_E_HEADER as i32),
        "a nonzero load address: the old loader's length wrapped"
    );
}

#[test]
fn a_refusal_leaves_the_header_empty() {
    let mut h = psid_header {
        songs: 77,
        ..Default::default()
    };
    h.name[0] = b'x' as _;
    let b = Psid {
        songs: 0,
        ..Psid::new(2)
    }
    .bytes();
    assert_eq!(
        unsafe { psid_parse(b.as_ptr(), b.len() as u32, &mut h) },
        PSID_E_HEADER as i32
    );
    assert_eq!((h.songs, h.name[0]), (0, 0));
}

fn check(b: &[u8]) {
    let Ok(h) = parse(b) else { return };
    let n = b.len() as u64;
    assert!((1..=5).contains(&h.version));
    assert!(h.data_len >= 1);
    assert!(h.data_off as u64 + h.data_len as u64 <= n, "{h:?}");
    assert!(h.load as u64 + h.data_len as u64 <= 0x10000, "{h:?}");
    assert!(h.load >= 2, "{h:?}");
    assert!((1..=256).contains(&h.songs) && (1..=h.songs).contains(&h.start));
    assert_eq!(h.flags & 1, 0);
    assert!(h.sids >= 1);
    for a in [h.sid2, h.sid3] {
        assert!(
            a == 0
                || (0xD420..=0xDFE0).contains(&a)
                    && a % 0x20 == 0
                    && !(0xD800..0xDE00).contains(&a)
        );
    }
    if h.lengths_off != 0 {
        assert!(h.data_off as u64 + h.data_len as u64 <= h.lengths_off as u64);
    }
    for s in [&h.name, &h.author, &h.released] {
        assert_eq!(s[s.len() - 1], 0);
        assert!(text(s).iter().all(|&c| (0x20..0x7F).contains(&c)));
    }
    for song in [1, 2, h.songs as u32, 33, 256] {
        assert!(matches!(unsafe { psid_speed_bit(&h, song) }, 0 | 1));
    }
}

fn random_header(rng: &mut Rng) -> Vec<u8> {
    let version = [1, 2, 3, 4, 5, 2, 3, 4, 5, 0, 6][rng.below(11) as usize];
    let mut p = Psid::new(version);
    p.magic = if rng.chance(3) { *b"RSID" } else { p.magic };
    if rng.chance(70) {
        p.off = Some(p.header_len() as u16);
    } else {
        p.off = Some(rng.below(0x10000) as u16);
    }
    p.load = [0, 1, 2, 0x0801, 0x1000, 0xFFFF, rng.word() as u16][rng.below(7) as usize];
    p.init = [0, p.load, rng.word() as u16][rng.below(3) as usize];
    p.play = rng.word() as u16;
    p.songs = [1, 2, 32, 33, 256, 257, 0, rng.word() as u16][rng.below(8) as usize];
    p.start = [0, 1, p.songs, p.songs.wrapping_add(1), rng.word() as u16][rng.below(5) as usize];
    p.speed = rng.word() << 16 ^ rng.word();
    for b in p.meta.iter_mut() {
        *b = if rng.chance(30) { 0 } else { rng.byte() };
    }
    p.flags = if rng.chance(50) {
        (rng.word() as u16) & 0xFFFE
    } else {
        rng.word() as u16
    };
    p.sid2 = rng.byte();
    p.sid3 = rng.byte();
    let len = [0u32, 1, 2, 3, 16, 300, 70_000][rng.below(7) as usize];
    p.data = (0..len).map(|_| rng.byte()).collect();
    p.tail = (0..rng.below(2) * rng.below(1100)).map(|_| 0).collect();
    p.bytes()
}

fn fuzz(seeds: u64) -> (u64, u64) {
    let mut accepted = 0;
    let mut mutated_accepted = 0;
    for seed in 0..seeds {
        let mut rng = Rng(0x5eed_9500 + seed);
        let mut b = random_header(&mut rng);
        check(&b);
        accepted += parse(&b).is_ok() as u64;
        for _ in 0..8 {
            let mut m = b.clone();
            for _ in 0..rng.below(4) {
                if !m.is_empty() {
                    let at = rng.below(m.len().min(0x90) as u32) as usize;
                    m[at] = rng.byte();
                }
            }
            match rng.below(3) {
                0 => m.truncate(rng.below(m.len() as u32 + 1) as usize),
                1 => m.extend((0..rng.below(8)).map(|_| rng.byte())),
                _ => {}
            }
            check(&m);
            mutated_accepted += parse(&m).is_ok() as u64;
        }
        if seed % 16 == 0 {
            for n in 0..b.len().min(0x90) {
                check(&b[..n]);
            }
        }
        b.clear();
    }
    eprintln!(
        "psid fuzz: {seeds} seeds, {accepted} generated and {mutated_accepted} mutated files accepted"
    );
    (accepted, mutated_accepted)
}

#[test]
fn fuzz_headers() {
    let (a, m) = fuzz(200);
    assert!(a > 10 && m > 100, "accepted {a} and {m}");
}

#[test]
#[ignore = "long: cargo test --release -p sidou-tests --test psid_file -- --ignored"]
fn fuzz_headers_long() {
    let (a, m) = fuzz(4000);
    assert!(a > 200 && m > 2000, "accepted {a} and {m}");
}
