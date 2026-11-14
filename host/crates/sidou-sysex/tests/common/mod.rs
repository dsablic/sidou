// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(dead_code, reason = "each test binary uses its own subset")]

use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use sidou_sysex::send::{Client, Port, Timing};
use sidou_sysex::{crc32, Packet, ACK, DATA, DIR, END, GET, INQ, MAX_DATA_LEN, NAK, PUT, WAIT};

pub const USER_SOUNDS: &str = "USER SOUNDS";

pub fn name16(name: &str) -> Vec<u8> {
    (0..16)
        .map(|i| name.as_bytes().get(i).copied().unwrap_or(0))
        .collect()
}

fn text_of(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as char)
        .collect()
}

#[derive(Default)]
pub struct Fake {
    pub stored: BTreeMap<String, (u8, Vec<u8>)>,
    pub replies: VecDeque<Vec<u8>>,
    pub upload: Option<(String, u8, Vec<u8>)>,
    pub sent: Vec<Packet>,
    pub busy: u32,
    pub waits: u32,
    pub full: bool,
    pub silent: bool,
    pub corrupt: bool,
    pub stream: VecDeque<(u8, u8, Vec<u8>)>,
    pub nak_get: Option<u8>,
    pub nak_after_meta: Option<u8>,
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
                self.stream = self
                    .stored
                    .iter()
                    .filter(|(name, _)| name.as_str() != USER_SOUNDS)
                    .map(|(name, (t, b))| {
                        let mut d = name16(name);
                        d.push(*t);
                        d.extend((b.len() as u32).to_le_bytes());
                        d.extend(crc32(b).to_le_bytes());
                        let e = (DIR, seq, d);
                        seq = (seq + 1) & 0x7f;
                        e
                    })
                    .collect();
                self.stream.push_back((END, seq, Vec::new()));
                self.next_stream();
            }
            GET => self.serve(p),
            ACK => self.next_stream(),
            PUT => {
                if self.busy > 0 {
                    self.busy -= 1;
                    return self.reply(NAK, p.sequence, vec![4, 0]);
                }
                if self.full {
                    return self.reply(NAK, p.sequence, vec![5, 0]);
                }
                let name = text_of(&p.data[2..18]);
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

    fn next_stream(&mut self) {
        if let Some((command, seq, d)) = self.stream.pop_front() {
            self.reply(command, seq, d);
        }
    }

    fn serve(&mut self, p: &Packet) {
        if let Some(reason) = self.nak_get {
            return self.reply(NAK, p.sequence, vec![reason, 0]);
        }
        let name = text_of(&p.data[2..18]);
        let Some((file_type, bytes)) = self.stored.get(&name).cloned() else {
            return self.reply(NAK, p.sequence, vec![6, 0]);
        };
        let mut meta = vec![0, 0];
        meta.extend(name16(&name));
        meta.push(file_type);
        meta.extend((bytes.len() as u32).to_le_bytes());
        meta.extend(crc32(&bytes).to_le_bytes());
        let mut seq = p.sequence;
        let mut served = bytes.clone();
        if self.corrupt {
            served[0] ^= 1;
        }
        self.stream.push_back((PUT, seq, meta));
        if let Some(reason) = self.nak_after_meta {
            self.stream
                .push_back((NAK, (seq + 1) & 0x7f, vec![reason, 0]));
            return self.next_stream();
        }
        for chunk in served.chunks(MAX_DATA_LEN) {
            seq = (seq + 1) & 0x7f;
            self.stream.push_back((DATA, seq, chunk.to_vec()));
        }
        self.stream.push_back((END, (seq + 1) & 0x7f, Vec::new()));
        self.next_stream();
    }

    pub fn puts(&self) -> usize {
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

pub fn client(fake: Fake) -> Client<Fake> {
    let timing = Timing {
        answer: Duration::ZERO,
        ack: Duration::ZERO,
        busy: vec![Duration::from_millis(1); 3],
        sleep: |_| {},
    };
    Client::new(fake, timing)
}

pub fn song() -> Vec<u8> {
    let mut b = b"GTS5".to_vec();
    b.extend((0..700u32).map(|i| (i * 7) as u8));
    b
}
