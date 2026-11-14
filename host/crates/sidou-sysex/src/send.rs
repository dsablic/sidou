// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use crate::{
    crc32, put_frames_at, Packet, ABORT, ACK, DATA, DIR, END, FILE_SONG, GET, INQ, NAK, PUT, WAIT,
};

pub const DEFAULT_PORT: &str = "SIDOU FM-1";
pub const FILE_INSTRUMENT: u8 = 3;
pub const FILE_USER_SOUNDS: u8 = 4;
pub const NAK_MALFORMED: u8 = 1;
pub const NAK_IO: u8 = 7;
const NAK_NOT_FOUND: u8 = 6;
const META_BYTES: usize = 27;
const NAME_BYTES: usize = 16;
const INQ_BYTES: usize = 13;
const DIR_BYTES: usize = 25;
const NAK_BUSY: u8 = 4;
const PROTOCOL: u8 = 1;

pub trait Port {
    fn name(&self) -> &str;
    fn send(&mut self, frame: &[u8]) -> Result<(), String>;
    fn recv(&mut self, timeout: Duration) -> Option<Vec<u8>>;
}

pub struct Timing {
    pub answer: Duration,
    pub ack: Duration,
    pub busy: Vec<Duration>,
    pub sleep: fn(Duration),
}

impl Default for Timing {
    fn default() -> Self {
        let busy = [1, 2, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5]
            .iter()
            .map(|&s| Duration::from_secs(s))
            .collect();
        Timing {
            answer: Duration::from_secs(3),
            ack: Duration::from_secs(10),
            busy,
            sleep: std::thread::sleep,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Device {
    pub version: [u8; 4],
    pub free: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub name: String,
    pub file_type: u8,
    pub length: u32,
    pub crc: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sent {
    Sent,
    Same,
}

pub type GetRefusal = (Option<u8>, String);

enum Refusal {
    Nak(u8, String),
    Other(String),
}

pub struct Client<P: Port> {
    pub port: P,
    seq: u8,
    timing: Timing,
}

pub fn nak_words(reason: u8) -> &'static str {
    match reason {
        1 => "malformed (1)",
        2 => "a checksum error (2)",
        3 => "out of sequence (3)",
        4 => "BUSY (4): a song saving or loading, or another transfer under way",
        5 => "FULL (5): the store has no room for it",
        6 => "not found (6)",
        7 => "an I/O error or invalid content (7)",
        8 => "a timeout (8)",
        9 => "aborted (9)",
        12 => "RSID (12): SIDOU plays PSID tunes only",
        13 => "MUS (13): not supported",
        14 => "a broken .sid header (14)",
        15 => "data past $FFFF (15)",
        _ => "refused",
    }
}

pub fn file_type_of(bytes: &[u8]) -> Option<u8> {
    match bytes.get(..4)? {
        b"GTS2" | b"GTS3" | b"GTS4" | b"GTS5" => Some(FILE_SONG),
        b"PSID" | b"RSID" => Some(crate::FILE_SID),
        b"GTI2" | b"GTI3" | b"GTI4" | b"GTI5" => Some(FILE_INSTRUMENT),
        _ => None,
    }
}

pub fn store_name(file_name: &str) -> String {
    let stem = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name)
        .rsplit_once('.')
        .map_or(file_name, |(s, _)| s);
    let upper: String = stem
        .chars()
        .map(|c| c.to_ascii_uppercase())
        .map(|c| if (' '..='_').contains(&c) { c } else { '?' })
        .collect();
    let trimmed: String = upper.trim_start().chars().take(16).collect();
    let name = trimmed.trim_end().to_string();
    if name.is_empty() {
        "FILE".to_string()
    } else {
        name
    }
}

const SONG_NAME: std::ops::Range<usize> = 4..36;

pub fn sent_name(file_name: &str, bytes: &[u8]) -> String {
    if file_type_of(bytes) == Some(FILE_SONG) {
        if let Some(head) = bytes.get(SONG_NAME) {
            let own: String = head
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as char)
                .collect();
            if !own.trim().is_empty() {
                return store_name(&format!("{own}.sng"));
            }
        }
    }
    store_name(file_name)
}

fn le32(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

impl<P: Port> Client<P> {
    pub fn new(port: P, timing: Timing) -> Self {
        Client {
            port,
            seq: 0,
            timing,
        }
    }

    fn next(&mut self) -> u8 {
        let s = self.seq;
        self.seq = (self.seq + 1) & 0x7f;
        s
    }

    fn send(&mut self, command: u8, sequence: u8, data: Vec<u8>) -> Result<(), String> {
        let frame = Packet {
            command,
            sequence,
            data,
        }
        .encode()
        .map_err(|e| e.to_string())?;
        self.port.send(&frame)
    }

    fn recv(&mut self, timeout: Duration) -> Result<Packet, String> {
        loop {
            let frame = self.port.recv(timeout).ok_or_else(|| {
                format!(
                    "no answer on {} (is it an FM-1 running SIDOU?)",
                    self.port.name()
                )
            })?;
            if let Ok(p) = Packet::decode(&frame) {
                return Ok(p);
            }
        }
    }

    fn drain(&mut self) {
        while self.port.recv(Duration::ZERO).is_some() {}
    }

    pub fn inquire(&mut self) -> Result<Device, String> {
        self.drain();
        self.send(INQ, 0, Vec::new())?;
        let p = self.recv(self.timing.answer)?;
        if p.command != INQ || p.data.len() != INQ_BYTES || p.data[0] != PROTOCOL {
            return Err(format!(
                "{} did not answer as SIDOU (its INQ): nothing is sent",
                self.port.name()
            ));
        }
        self.seq = 1;
        Ok(Device {
            version: [p.data[1], p.data[2], p.data[3], p.data[4]],
            free: le32(&p.data, 7),
        })
    }

    pub fn dir(&mut self) -> Result<Vec<Entry>, String> {
        self.drain();
        let seq = self.next();
        self.send(DIR, seq, Vec::new())?;
        let mut out = Vec::new();
        loop {
            let p = self.recv(self.timing.answer)?;
            if p.command == NAK {
                return Err(format!(
                    "the FM-1 refused the listing: {}",
                    nak_words(p.data[0])
                ));
            }
            self.send(ACK, p.sequence, Vec::new())?;
            if p.command == END {
                self.seq = (p.sequence + 1) & 0x7f;
                return Ok(out);
            }
            if p.command != DIR || p.data.len() != DIR_BYTES {
                return Err("a damaged listing from the FM-1".to_string());
            }
            let name: String = p.data[..16]
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as char)
                .collect();
            out.push(Entry {
                name,
                file_type: p.data[16],
                length: le32(&p.data, 17),
                crc: le32(&p.data, 21),
            });
        }
    }

    pub fn get(&mut self, name: &str) -> Result<Option<(u8, Vec<u8>)>, String> {
        self.get_refusing(name).map_err(|(_, words)| words)
    }

    pub fn get_refusing(&mut self, name: &str) -> Result<Option<(u8, Vec<u8>)>, GetRefusal> {
        self.drain();
        let seq = self.next();
        let mut target = vec![0, 0];
        target.extend((0..NAME_BYTES).map(|i| name.as_bytes().get(i).copied().unwrap_or(0)));
        self.send(GET, seq, target).map_err(|e| (None, e))?;
        let meta = self.recv(self.timing.answer).map_err(|e| (None, e))?;
        if meta.command == NAK {
            return match meta.data.first().copied().unwrap_or(0) {
                NAK_NOT_FOUND => Ok(None),
                reason => Err((
                    Some(reason),
                    format!("the FM-1 refused it: {}", nak_words(reason)),
                )),
            };
        }
        if meta.command != PUT || meta.data.len() != META_BYTES {
            return Err((None, "a damaged answer from the FM-1".to_string()));
        }
        let (file_type, length, crc) = (meta.data[18], le32(&meta.data, 19), le32(&meta.data, 23));
        self.send(ACK, meta.sequence, Vec::new())
            .map_err(|e| (None, e))?;
        let mut bytes = Vec::new();
        loop {
            let p = self.recv(self.timing.ack).map_err(|e| (None, e))?;
            if p.command == NAK {
                let reason = p.data.first().copied().unwrap_or(0);
                return Err((
                    Some(reason),
                    format!("the FM-1 stopped sending: {}", nak_words(reason)),
                ));
            }
            if p.command != DATA && p.command != END {
                return Err((None, "a damaged answer from the FM-1".to_string()));
            }
            self.send(ACK, p.sequence, Vec::new())
                .map_err(|e| (None, e))?;
            self.seq = (p.sequence + 1) & 0x7f;
            if p.command == END {
                break;
            }
            bytes.extend(p.data);
        }
        if bytes.len() != length as usize || crc32(&bytes) != crc {
            return Err((
                None,
                format!("{name} came over damaged (its length or CRC is wrong): read it again"),
            ));
        }
        Ok(Some((file_type, bytes)))
    }

    fn expect_ack(&mut self, sequence: u8, waited: &mut bool) -> Result<(), Refusal> {
        loop {
            let p = self.recv(self.timing.ack).map_err(Refusal::Other)?;
            if p.command == WAIT && p.sequence == sequence {
                *waited = true;
                continue;
            }
            if p.command == NAK {
                let reason = p.data.first().copied().unwrap_or(0);
                let words = if reason == NAK_BUSY && *waited {
                    "the FM-1 waited a minute for quiet and gave up: stop the song (or the recording of its USB audio) and send again".to_string()
                } else {
                    format!("the FM-1 refused it: {}", nak_words(reason))
                };
                return Err(Refusal::Nak(reason, words));
            }
            if p.command != ACK || p.sequence != sequence {
                return Err(Refusal::Other(format!(
                    "an unexpected answer {:#04x} (sequence {}) from the FM-1",
                    p.command, p.sequence
                )));
            }
            return Ok(());
        }
    }

    fn put_once(
        &mut self,
        name: &str,
        file_type: u8,
        data: &[u8],
        waited: &mut bool,
    ) -> Result<(), Refusal> {
        self.drain();
        let frames = put_frames_at(name, file_type, data, self.seq)
            .map_err(|e| Refusal::Other(e.to_string()))?;
        for frame in &frames {
            let p = Packet::decode(frame).map_err(|e| Refusal::Other(e.to_string()))?;
            self.seq = (p.sequence + 1) & 0x7f;
            self.port.send(frame).map_err(Refusal::Other)?;
            if let Err(e) = self.expect_ack(p.sequence, waited) {
                if matches!(e, Refusal::Other(_)) {
                    let seq = self.next();
                    let _ = self.send(ABORT, seq, Vec::new());
                }
                return Err(e);
            }
        }
        Ok(())
    }

    pub fn put(
        &mut self,
        name: &str,
        file_type: u8,
        data: &[u8],
        on_busy: &mut dyn FnMut(Duration),
    ) -> Result<(), String> {
        let waits = self.timing.busy.clone();
        let mut attempt = 0;
        loop {
            let mut waited = false;
            match self.put_once(name, file_type, data, &mut waited) {
                Ok(()) => return Ok(()),
                Err(Refusal::Nak(NAK_BUSY, _)) if !waited && attempt < waits.len() => {
                    on_busy(waits[attempt]);
                    (self.timing.sleep)(waits[attempt]);
                    attempt += 1;
                }
                Err(Refusal::Nak(_, words) | Refusal::Other(words)) => return Err(words),
            }
        }
    }

    pub fn send_file(
        &mut self,
        name: &str,
        file_type: u8,
        data: &[u8],
        replace: bool,
        on_busy: &mut dyn FnMut(Duration),
    ) -> Result<Sent, String> {
        if let Some(there) = self.dir()?.into_iter().find(|e| e.name == name) {
            if there.length == data.len() as u32 && there.crc == crc32(data) {
                return Ok(Sent::Same);
            }
            if !replace {
                return Err(format!(
                    "{name} is already on the FM-1 with different content: --replace replaces it"
                ));
            }
        }
        self.put(name, file_type, data, on_busy)?;
        Ok(Sent::Sent)
    }
}

pub struct NoPort;

impl Port for NoPort {
    fn name(&self) -> &str {
        ""
    }
    fn send(&mut self, _frame: &[u8]) -> Result<(), String> {
        Err(String::new())
    }
    fn recv(&mut self, _timeout: Duration) -> Option<Vec<u8>> {
        None
    }
}
