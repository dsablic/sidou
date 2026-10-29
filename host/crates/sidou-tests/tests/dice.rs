// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The song views' DICE settings (EDIT + SCL, K1 density, K2 range, K3 the lane's scale, K4
//! rhythm): the DICE chunk in SIDOU's trailer, kept with the song, and GoatTracker skipping it.

use sidou_sys::*;
use sidou_tests::song::*;

const TAG_DICE: u32 = tag(b"DICE");

fn read(song: &Song) -> (song_dice, i32) {
    let mut v = song_dice {
        density: 9,
        scale: [9; 3],
        rhythm: 9,
        range: 9,
    };
    let r = unsafe { song_ext_get_dice(song.ptr(), &mut v) };
    (v, r)
}

/// Density, lane 1's scale, rhythm, range and whether the chunk is there.
fn get(song: &Song) -> (u8, u8, u8, u8, i32) {
    let (v, r) = read(song);
    (v.density, v.scale[0], v.rhythm, v.range, r)
}

/// The three lanes' scales.
fn scales(song: &Song) -> [u8; 3] {
    read(song).0.scale
}

/// Every lane on one scale.
fn set(song: &mut Song, density: u8, scale: u8, rhythm: u8, range: u8) -> i32 {
    let v = song_dice {
        density,
        scale: [scale; 3],
        rhythm,
        range,
    };
    unsafe { song_ext_set_dice(song.ptr_mut(), &v) }
}

const DEFAULTS: (u8, u8, u8, u8) = (
    SONG_DICE_DENSITY_DEFAULT as u8,
    0,
    0,
    SONG_DICE_RANGE_DEFAULT as u8,
);

fn plain_song() -> Song {
    let mut song = Song::host();
    song.read(&SngSpec::new().encode(), 0).unwrap();
    song
}

#[test]
fn no_chunk_is_the_default_the_roll_always_had() {
    let song = plain_song();
    let (d, sc, rh, ra) = DEFAULTS;
    assert_eq!(get(&song), (d, sc, rh, ra, 0));
    assert_eq!(song.ext_find(TAG_DICE), None, "reading adds nothing");
}

#[test]
fn the_chunk_round_trips_and_goattracker_keeps_only_the_gt2_part() {
    let mut song = plain_song();
    assert_eq!(set(&mut song, 4, 5, 3, 2), 0);
    assert_eq!(get(&song), (4, 5, 3, 2, 1));
    assert_eq!(song.ext_find(TAG_DICE), Some(vec![1, 4, 5, 3, 2, 5, 5]));
    let file = song.write(true);
    let plain = song.write(false);
    assert_eq!(&file[..plain.len()], &plain[..]);
    let mut back = Song::host();
    back.read(&file, 0).unwrap();
    assert_eq!(get(&back), (4, 5, 3, 2, 1), "the song keeps its DICE");
    assert_eq!(back.write(true), file, "SIDOU writes it back byte for byte");
    assert_eq!(
        gt2_ref::canonical(&file).expect("GoatTracker loads it"),
        plain
    );
}

#[test]
fn a_value_out_of_range_or_another_version_is_the_default() {
    let mut song = plain_song();
    for data in [
        [1u8, 5, 0, 0, 1],
        [1, 0, 6, 0, 1],
        [1, 1, 1, 5, 0],
        [1, 1, 1, 4, 4],
        [2, 4, 4, 1, 2],
    ] {
        assert_eq!(song.ext_set(TAG_DICE, &data), 0);
        let (d, s, rh, ra, _) = get(&song);
        let bad_v = data[0] != 1;
        let want_d = if bad_v || data[1] > 4 { 2 } else { data[1] };
        let want_s = if bad_v || data[2] > 5 { 0 } else { data[2] };
        let want_rh = if bad_v || data[3] > 4 { 0 } else { data[3] };
        let want_ra = if bad_v || data[4] > 3 { 1 } else { data[4] };
        assert_eq!(
            (d, s, rh, ra),
            (want_d, want_s, want_rh, want_ra),
            "{data:?}"
        );
    }
    assert_eq!(song.ext_set(TAG_DICE, &[1, 3]), 0);
    let (d, sc, rh, ra, r) = get(&song);
    assert_eq!(r, 0, "a short chunk: the default");
    assert_eq!((d, sc, rh, ra), DEFAULTS, "every field the default");
}

/// 1.0 wrote three bytes (version, density, scale): its song reads with them and the rhythm and
/// range at their defaults, straight and two octaves; a 1.1 write keeps all five. (1.0 reads the
/// first three of a 1.1 chunk and, when its own EDIT + SCL turns one, writes three: the rhythm
/// and the range go back to the defaults.)
#[test]
fn a_one_point_zero_chunk_reads_with_the_new_fields_at_their_defaults() {
    let mut song = plain_song();
    assert_eq!(song.ext_set(TAG_DICE, &[1, 4, 3]), 0);
    assert_eq!(get(&song), (4, 3, 0, SONG_DICE_RANGE_DEFAULT as u8, 1));
    assert_eq!(song.ext_set(TAG_DICE, &[1, 4, 3, 2]), 0);
    assert_eq!(
        get(&song),
        (4, 3, 2, SONG_DICE_RANGE_DEFAULT as u8, 1),
        "four bytes: no range"
    );
    let file = song.write(true);
    let mut back = Song::host();
    back.read(&file, 0).unwrap();
    assert_eq!(back.write(true), file, "kept as written");
}

/// Each lane has its scale: byte 2 is lane 1's (what 1.0 reads), bytes 5 and 6 lanes 2 and 3's,
/// each lane 1's when the chunk is shorter (1.0's 3 bytes, the 5 of 1.1's first cut) or the byte
/// is out of range.
#[test]
fn each_lane_has_its_scale_and_lanes_two_and_three_default_to_lane_ones() {
    let mut song = plain_song();
    let v = song_dice {
        density: 1,
        scale: [3, 0, 5],
        rhythm: 2,
        range: 0,
    };
    assert_eq!(unsafe { song_ext_set_dice(song.ptr_mut(), &v) }, 0);
    assert_eq!(song.ext_find(TAG_DICE), Some(vec![1, 1, 3, 2, 0, 0, 5]));
    assert_eq!(scales(&song), [3, 0, 5]);
    for (data, want) in [
        (vec![1u8, 2, 4], [4, 4, 4]),
        (vec![1, 2, 4, 0, 1], [4, 4, 4]),
        (vec![1, 2, 4, 0, 1, 2], [4, 2, 4]),
        (vec![1, 2, 4, 0, 1, 9, 1], [4, 4, 1]),
        (vec![1, 2, 7, 0, 1, 2, 3], [0, 2, 3]),
    ] {
        assert_eq!(song.ext_set(TAG_DICE, &data), 0);
        assert_eq!(scales(&song), want, "{data:?}");
    }
}
