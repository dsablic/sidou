// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use sidou_sysex::send::{file_type_of, sent_name, store_name, Client, Port, Sent, Timing};
use sidou_sysex::{crc32, Packet, ACK, DATA, DIR, END, FILE_SONG, INQ, NAK, PUT, WAIT};

#[derive(Default)]
struct Fake {
    stored: BTreeMap<String, (u8, Vec<u8>)>,
    replies: VecDeque<Vec<u8>>,
    upload: Option<(String, u8, Vec<u8>)>,
    sent: Vec<Packet>,
    busy: u32,
    waits: u32,
    full: bool,
    silent: bool,
    listing: VecDeque<(u8, Vec<u8>)>,
}

impl Fake {
    fn reply(&mut self, command: u8, sequence: u8, data: Vec<u8>) {
        self.replies.push_back(
            Packet {
                command,
                sequence,
                data,
            }
            .encode()
            .unwrap(),
        );
    }

    fn take(&mut self, p: &Packet) {
        match p.command {
            INQ => {
                let mut d = vec![1, 1, 8, 0, 0, 1, 0];
                d.extend(40_000u32.to_le_bytes());
                d.extend([6, 1]);
                self.reply(INQ, p.sequence, d);
            }
            DIR => {
                let mut seq = p.sequence;
                self.listing = self
                    .stored
                    .iter()
                    .map(|(name, (t, b))| {
                        let mut d: Vec<u8> = (0..16)
                            .map(|i| name.as_bytes().get(i).copied().unwrap_or(0))
                            .collect();
                        d.push(*t);
                        d.extend((b.len() as u32).to_le_bytes());
                        d.extend(crc32(b).to_le_bytes());
                        let e = (seq, d);
                        seq = (seq + 1) & 0x7f;
                        e
                    })
                    .collect();
                self.listing.push_back((seq, Vec::new()));
                self.next_listing();
            }
            ACK => self.next_listing(),
            PUT => {
                if self.busy > 0 {
                    self.busy -= 1;
                    return self.reply(NAK, p.sequence, vec![4, 0]);
                }
                if self.full {
                    return self.reply(NAK, p.sequence, vec![5, 0]);
                }
                let name: String = p.data[2..18]
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as char)
                    .collect();
                self.upload = Some((name, p.data[18], Vec::new()));
                self.reply(ACK, p.sequence, Vec::new());
            }
            DATA => {
                self.upload.as_mut().unwrap().2.extend(&p.data);
                for _ in 0..self.waits {
                    self.reply(WAIT, p.sequence, Vec::new());
                }
                self.waits = 0;
                self.reply(ACK, p.sequence, Vec::new());
            }
            END => {
                let (name, t, bytes) = self.upload.take().unwrap();
                self.stored.insert(name, (t, bytes));
                self.reply(ACK, p.sequence, Vec::new());
            }
            _ => {}
        }
    }

    fn next_listing(&mut self) {
        if let Some((seq, d)) = self.listing.pop_front() {
            let command = if d.is_empty() { END } else { DIR };
            self.reply(command, seq, d);
        }
    }

    fn puts(&self) -> usize {
        self.sent.iter().filter(|p| p.command == PUT).count()
    }
}

impl Port for Fake {
    fn name(&self) -> &str {
        "SIDOU FM-1"
    }

    fn send(&mut self, frame: &[u8]) -> Result<(), String> {
        let p = Packet::decode(frame).map_err(|e| e.to_string())?;
        self.sent.push(p.clone());
        if !self.silent {
            self.take(&p);
        }
        Ok(())
    }

    fn recv(&mut self, _timeout: Duration) -> Option<Vec<u8>> {
        self.replies.pop_front()
    }
}

fn client(fake: Fake) -> Client<Fake> {
    let timing = Timing {
        answer: Duration::ZERO,
        ack: Duration::ZERO,
        busy: vec![Duration::from_millis(1); 3],
        sleep: |_| {},
    };
    Client::new(fake, timing)
}

fn song() -> Vec<u8> {
    let mut b = b"GTS5".to_vec();
    b.extend((0..700u32).map(|i| (i * 7) as u8));
    b
}

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
