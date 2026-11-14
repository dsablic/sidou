// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::{client, song, Fake};
use sidou_sysex::send::{file_type_of, sent_name, store_name, Sent};
use sidou_sysex::{FILE_SONG, INQ};

#[test]
fn a_song_goes_to_the_fm1_after_its_inq_and_lands_byte_for_byte() {
    let mut c = client(Fake::default());
    assert_eq!(c.inquire().unwrap().version, [1, 8, 0, 0]);
    assert_eq!(
        c.send_file("ACID", FILE_SONG, &song(), false, &mut |_| {}),
        Ok(Sent::Sent)
    );
    assert_eq!(c.port.stored.get("ACID"), Some(&(FILE_SONG, song())));
    assert_eq!(
        c.send_file("ACID", FILE_SONG, &song(), false, &mut |_| {}),
        Ok(Sent::Same)
    );
    assert_eq!(c.port.puts(), 1, "an identical file is left alone");
}

#[test]
fn a_port_that_does_not_answer_as_sidou_gets_nothing() {
    let mut c = client(Fake {
        silent: true,
        ..Fake::default()
    });
    let e = c.inquire().unwrap_err();
    assert!(e.contains("no answer on SIDOU FM-1"), "{e}");
    assert_eq!(
        c.port.sent.iter().map(|p| p.command).collect::<Vec<_>>(),
        [INQ]
    );
}

#[test]
fn a_different_file_is_replaced_only_when_asked() {
    let mut fake = Fake::default();
    fake.stored
        .insert("ACID".into(), (FILE_SONG, b"GTS5old".to_vec()));
    let mut c = client(fake);
    c.inquire().unwrap();
    let e = c
        .send_file("ACID", FILE_SONG, &song(), false, &mut |_| {})
        .unwrap_err();
    assert!(e.contains("--replace"), "{e}");
    assert_eq!(c.port.puts(), 0);
    assert_eq!(
        c.send_file("ACID", FILE_SONG, &song(), true, &mut |_| {}),
        Ok(Sent::Sent)
    );
    assert_eq!(c.port.stored["ACID"].1, song());
}

#[test]
fn busy_sends_the_whole_put_again_and_wait_holds_the_ack() {
    let mut c = client(Fake {
        busy: 2,
        waits: 3,
        ..Fake::default()
    });
    c.inquire().unwrap();
    let mut busy = 0;
    assert_eq!(
        c.send_file("ACID", FILE_SONG, &song(), false, &mut |_| busy += 1),
        Ok(Sent::Sent)
    );
    assert_eq!((busy, c.port.puts()), (2, 3));
    assert_eq!(c.port.stored["ACID"].1, song());
}

#[test]
fn a_full_store_is_said_in_words() {
    let mut c = client(Fake {
        full: true,
        ..Fake::default()
    });
    c.inquire().unwrap();
    let e = c
        .send_file("ACID", FILE_SONG, &song(), false, &mut |_| {})
        .unwrap_err();
    assert_eq!(
        e,
        "the FM-1 refused it: FULL (5): the store has no room for it"
    );
}

#[test]
fn only_songs_tunes_and_instruments_are_sent() {
    assert_eq!(file_type_of(b"GTS5...."), Some(FILE_SONG));
    assert_eq!(file_type_of(b"PSID...."), Some(2));
    assert_eq!(file_type_of(b"GTI5...."), Some(3));
    assert_eq!(file_type_of(b"FWSC...."), None, "never a firmware package");
    assert_eq!(
        file_type_of(&[0xf0, 0x7d, 0x53, 0x44]),
        None,
        "never raw SysEx, update frames included"
    );
    assert_eq!(store_name("out/Acid Line.sng"), "ACID LINE");
    let mut squelch = b"GTS5SQUELCH 2".to_vec();
    squelch.resize(36, 0);
    assert_eq!(
        sent_name("out/SQUELCH-2.sng", &squelch),
        "SQUELCH 2",
        "a song goes under its own name"
    );
    assert_eq!(
        sent_name("tune.sid", b"PSID...."),
        "TUNE",
        "anything else under its file's"
    );
    let mut unnamed = b"GTS5".to_vec();
    unnamed.resize(36, 0);
    assert_eq!(
        sent_name("x/My Song.sng", &unnamed),
        "MY SONG",
        "a song with no name under its file's"
    );
    assert_eq!(
        store_name("a-very-long-file-name-indeed.sng"),
        "A-VERY-LONG-FILE"
    );
}
