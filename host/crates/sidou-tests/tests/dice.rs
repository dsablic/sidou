// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! STEP's DICE settings (EDIT + SCL, K1 density, K3 scale): the DICE chunk in SIDOU's trailer,
//! kept with the song, and GoatTracker skipping it.

use sidou_sys::*;
use sidou_tests::song::*;

const TAG_DICE: u32 = tag(b"DICE");

fn get(song: &Song) -> (u8, u8, i32) {
    let (mut d, mut s) = (9u8, 9u8);
    let r = unsafe { song_ext_get_dice(song.ptr(), &mut d, &mut s) };
    (d, s, r)
}

fn plain_song() -> Song {
    let mut song = Song::host();
    song.read(&SngSpec::new().encode(), 0).unwrap();
    song
}

#[test]
fn no_chunk_is_the_default_the_roll_always_had() {
    let song = plain_song();
    assert_eq!(get(&song), (SONG_DICE_DENSITY_DEFAULT as u8, 0, 0));
    assert_eq!(song.ext_find(TAG_DICE), None, "reading adds nothing");
}

#[test]
fn the_chunk_round_trips_and_goattracker_keeps_only_the_gt2_part() {
    let mut song = plain_song();
    assert_eq!(unsafe { song_ext_set_dice(song.ptr_mut(), 4, 5) }, 0);
    assert_eq!(get(&song), (4, 5, 1));
    assert_eq!(song.ext_find(TAG_DICE), Some(vec![1, 4, 5]));
    let file = song.write(true);
    let plain = song.write(false);
    assert_eq!(&file[..plain.len()], &plain[..]);
    let mut back = Song::host();
    back.read(&file, 0).unwrap();
    assert_eq!(get(&back), (4, 5, 1), "the song keeps its DICE");
    assert_eq!(back.write(true), file, "SIDOU writes it back byte for byte");
    assert_eq!(
        gt2_ref::canonical(&file).expect("GoatTracker loads it"),
        plain
    );
}

#[test]
fn a_value_out_of_range_or_another_version_is_the_default() {
    let mut song = plain_song();
    for data in [[1u8, 5, 0], [1, 0, 6], [2, 4, 4]] {
        assert_eq!(song.ext_set(TAG_DICE, &data), 0);
        let (d, s, _) = get(&song);
        let bad_v = data[0] != 1;
        let want_d = if bad_v || data[1] > 4 { 2 } else { data[1] };
        let want_s = if bad_v || data[2] > 5 { 0 } else { data[2] };
        assert_eq!((d, s), (want_d, want_s), "{data:?}");
    }
    assert_eq!(song.ext_set(TAG_DICE, &[1, 3]), 0);
    assert_eq!(get(&song), (2, 0, 0), "a short chunk: the default");
}
