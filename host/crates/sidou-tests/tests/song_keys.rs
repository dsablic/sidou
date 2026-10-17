// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;
use sidou_tests::song::{Song, TAG_KEYS};

#[test]
fn keys_keep_the_preset_identity_and_legacy_chunks_leave_it_unspecified() {
    let mut song = Song::device();
    let mut keys = song_keys {
        count: 3,
        preset: 2,
        ..Default::default()
    };
    keys.param[..3].copy_from_slice(&[1234, 7, 2048]);
    assert_eq!(unsafe { song_ext_set_keys(song.ptr_mut(), &keys) }, 0);
    let mut loaded = Song::device();
    loaded.read(&song.write(true), 4).unwrap();
    let mut got = song_keys::default();
    assert_eq!(unsafe { song_ext_get_keys(loaded.ptr(), &mut got) }, 1);
    assert_eq!((got.preset, &got.param[..3]), (2, &keys.param[..3]));
    let legacy = [1u8, 1, 0x34, 0x12];
    assert_eq!(
        unsafe { song_ext_set(loaded.ptr_mut(), TAG_KEYS, legacy.as_ptr(), 4) },
        0
    );
    assert_eq!(unsafe { song_ext_get_keys(loaded.ptr(), &mut got) }, 1);
    assert_eq!(got.preset, SONG_KEYS_PRESET_NONE as u8);
    assert_eq!(got.param[0], 0x1234);
}
