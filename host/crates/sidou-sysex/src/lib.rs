// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Host-only SIDOU SysEx packets. This crate does not open MIDI or USB devices.
//!
//! [`Packet`] accepts the request and response forms documented by the protocol.
//! It validates individual packets, not transfer sequencing, file contents or CRCs.
//! [`Packet::decode`] expects exactly one frame, ignoring MIDI real-time bytes.
//! Upload sessions require 16-byte-aligned DATA offsets and nonfinal DATA lengths
//! divisible by 16; only the final DATA may be short. Individual DATA validation
//! cannot know the final file offset, so it accepts every nonempty packet length.

use std::fmt;

pub const INQ: u8 = 0x01;
pub const DIR: u8 = 0x02;
pub const GET: u8 = 0x03;
pub const PUT: u8 = 0x04;
pub const DATA: u8 = 0x05;
pub const END: u8 = 0x06;
pub const DEL: u8 = 0x07;
pub const ABORT: u8 = 0x08;
pub const ACK: u8 = 0x7e;
pub const NAK: u8 = 0x7f;

pub const MAX_DATA_LEN: usize = 256;
const HEADER: [u8; 4] = [0xf0, 0x7d, 0x53, 0x44];
const MAX_PACKED_LEN: usize = MAX_DATA_LEN + MAX_DATA_LEN.div_ceil(7);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Framing,
    ForeignHeader,
    StatusByte,
    TooLong,
    NoncanonicalPacking,
    Checksum,
    UnknownCommand,
    Sequence,
    Payload,
    Name,
    Target,
    FileType,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Framing => "expected one complete SIDOU SysEx frame",
            Self::ForeignHeader => "not a SIDOU manufacturer and family header",
            Self::StatusByte => "non-data MIDI status inside a frame or packed payload",
            Self::TooLong => "payload exceeds 256 decoded bytes",
            Self::NoncanonicalPacking => "missing packed data or nonzero unused mask bits",
            Self::Checksum => "incorrect SysEx checksum",
            Self::UnknownCommand => "unknown SIDOU command",
            Self::Sequence => "sequence is not seven-bit",
            Self::Payload => "invalid command payload length or fields",
            Self::Name => "invalid or reserved store filename",
            Self::Target => "invalid target, instrument or unused target fields",
            Self::FileType => "invalid file type for target",
        })
    }
}

impl std::error::Error for Error {}

/// A command and sequence with unpacked payload bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Packet {
    pub command: u8,
    pub sequence: u8,
    pub data: Vec<u8>,
}

impl Packet {
    /// Validate and encode one complete frame, including checksum and delimiters.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let packed = pack(&self.data)?;
        let mut frame = Vec::with_capacity(packed.len() + 8);
        frame.extend(HEADER);
        frame.extend([self.command, self.sequence]);
        frame.extend(packed);
        frame.push(0_u8.wrapping_sub(sum7(&frame[4..])) & 0x7f);
        frame.push(0xf7);
        Ok(frame)
    }

    /// Decode one frame, discarding bytes `F8`–`FF` even outside its delimiters.
    /// Foreign manufacturers/families return [`Error::ForeignHeader`].
    pub fn decode(frame: &[u8]) -> Result<Self, Error> {
        let mut bytes = Vec::with_capacity(MAX_PACKED_LEN + 8);
        for byte in frame.iter().copied().filter(|byte| *byte < 0xf8) {
            if bytes.len() == MAX_PACKED_LEN + 8 {
                return Err(Error::TooLong);
            }
            bytes.push(byte);
        }
        if bytes.first() != Some(&0xf0) || bytes.last() != Some(&0xf7) {
            return Err(Error::Framing);
        }
        if bytes.len() < HEADER.len() + 1 {
            return Err(Error::Framing);
        }
        if bytes[..HEADER.len()] != HEADER {
            return Err(Error::ForeignHeader);
        }
        if bytes.len() < 8 {
            return Err(Error::Framing);
        }
        let body = &bytes[4..bytes.len() - 1];
        if body.iter().any(|byte| *byte >= 0x80) {
            return Err(Error::StatusByte);
        }
        if sum7(body) != 0 {
            return Err(Error::Checksum);
        }
        let packet = Self {
            command: body[0],
            sequence: body[1],
            data: unpack(&body[2..body.len() - 1])?,
        };
        packet.validate()?;
        Ok(packet)
    }

    /// Validate the documented request or response payload for this command.
    pub fn validate(&self) -> Result<(), Error> {
        if self.sequence >= 0x80 {
            return Err(Error::Sequence);
        }
        if self.data.len() > MAX_DATA_LEN {
            return Err(Error::TooLong);
        }
        let data = self.data.as_slice();
        match self.command {
            INQ if data.is_empty() => Ok(()),
            INQ if data.len() == 13 => {
                if data[1..5].iter().any(|byte| *byte >= 0x80) || data[12] & !1 != 0 {
                    return Err(Error::Payload);
                }
                Ok(())
            }
            DIR if data.is_empty() => Ok(()),
            DIR if data.len() == 25 => {
                validate_name(&data[..16])?;
                validate_type(data[16], 0)
            }
            GET if data.len() == 18 => validate_target(data),
            PUT if data.len() == 27 => {
                validate_target(data)?;
                validate_type(data[18], data[0])
            }
            DATA if !data.is_empty() => Ok(()),
            END | ABORT | ACK if data.is_empty() => Ok(()),
            DEL if data.len() == 16 => validate_name(data),
            NAK if data.len() == 2 && (1..=9).contains(&data[0]) => Ok(()),
            INQ | DIR | GET | PUT | DATA | END | DEL | ABORT | ACK | NAK => Err(Error::Payload),
            _ => Err(Error::UnknownCommand),
        }
    }
}

/// Pack zero to 256 raw bytes into canonical seven-bit groups, mask first.
pub fn pack(data: &[u8]) -> Result<Vec<u8>, Error> {
    if data.len() > MAX_DATA_LEN {
        return Err(Error::TooLong);
    }
    let mut packed = Vec::with_capacity(data.len() + data.len().div_ceil(7));
    for group in data.chunks(7) {
        let mask = group
            .iter()
            .enumerate()
            .fold(0, |mask, (index, byte)| mask | ((byte >> 7) << index));
        packed.push(mask);
        packed.extend(group.iter().map(|byte| byte & 0x7f));
    }
    Ok(packed)
}

/// Unpack canonical groups, rejecting lone masks, status bytes and unused bits.
pub fn unpack(packed: &[u8]) -> Result<Vec<u8>, Error> {
    if packed.len() > MAX_PACKED_LEN {
        return Err(Error::TooLong);
    }
    if packed.iter().any(|byte| *byte >= 0x80) {
        return Err(Error::StatusByte);
    }
    let mut data = Vec::with_capacity(packed.len() - packed.len().div_ceil(8));
    for group in packed.chunks(8) {
        let count = group.len() - 1;
        if count == 0 || group[0] >> count != 0 {
            return Err(Error::NoncanonicalPacking);
        }
        for (index, low) in group[1..].iter().enumerate() {
            data.push(low | (((group[0] >> index) & 1) << 7));
        }
    }
    if data.len() > MAX_DATA_LEN {
        return Err(Error::TooLong);
    }
    Ok(data)
}

fn sum7(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)) & 0x7f
}

fn validate_name(name: &[u8]) -> Result<(), Error> {
    let end = name
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name.len());
    if end == 0
        || name[0] == b' '
        || name[..end].iter().any(|byte| !(0x20..=0x5f).contains(byte))
        || name[end..].iter().any(|byte| *byte != 0)
        || &name[..end] == b"MIDI INSTRUMENT"
        || &name[..end] == b"MIDI SONG"
    {
        return Err(Error::Name);
    }
    Ok(())
}

fn validate_target(data: &[u8]) -> Result<(), Error> {
    match data[0] {
        0 if data[1] == 0 => validate_name(&data[2..18]),
        1 if (1..=63).contains(&data[1]) && data[2..18].iter().all(|byte| *byte == 0) => Ok(()),
        2 if data[1..18].iter().all(|byte| *byte == 0) => Ok(()),
        _ => Err(Error::Target),
    }
}

fn validate_type(file_type: u8, target: u8) -> Result<(), Error> {
    if !(1..=127).contains(&file_type) || (target != 0 && file_type != 1) {
        return Err(Error::FileType);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(command: u8, data: Vec<u8>) -> Packet {
        Packet {
            command,
            sequence: 0,
            data,
        }
    }

    fn name() -> Vec<u8> {
        let mut name = vec![0; 16];
        name[..4].copy_from_slice(b"TEST");
        name
    }

    fn get() -> Vec<u8> {
        let mut data = vec![0, 0];
        data.extend(name());
        data
    }

    fn put() -> Vec<u8> {
        let mut data = get();
        data.push(1);
        data.extend(65536_u32.to_le_bytes());
        data.extend(0xdeadbeef_u32.to_le_bytes());
        data
    }

    fn wire(command: u8, sequence: u8, packed: &[u8]) -> Vec<u8> {
        let mut frame = vec![0xf0, 0x7d, 0x53, 0x44, command, sequence];
        frame.extend_from_slice(packed);
        let sum = frame[4..]
            .iter()
            .fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
        frame.extend([0_u8.wrapping_sub(sum) & 0x7f, 0xf7]);
        frame
    }

    #[test]
    fn packing_all_lengths_matches_independent_bit_positions() {
        for len in 0_usize..=256 {
            let data: Vec<u8> = (0..len).map(|i| (i * 73 + 129) as u8).collect();
            let packed = pack(&data).unwrap();
            assert_eq!(packed.len(), len + len.div_ceil(7));
            for (group, bytes) in data.chunks(7).zip(packed.chunks(8)) {
                let mask = group
                    .iter()
                    .enumerate()
                    .fold(0, |mask, (i, byte)| mask | ((byte >> 7) << i));
                assert_eq!(bytes[0], mask);
                for (raw, low) in group.iter().zip(&bytes[1..]) {
                    assert_eq!(*low, raw & 0x7f);
                }
            }
            assert_eq!(unpack(&packed).unwrap(), data);
        }
        assert_eq!(pack(&[0x80, 0x01, 0xff]).unwrap(), [5, 0, 1, 0x7f]);
    }

    #[test]
    fn packing_rejects_noncanonical_status_and_oversize_inputs() {
        assert!(pack(&[0; 257]).is_err());
        assert!(unpack(&[0; 294]).is_err());
        for packed in [
            vec![0],
            vec![0; 9],
            vec![2, 0],
            vec![0x40, 0, 0],
            vec![0, 0x80],
            vec![0x80, 0],
        ] {
            assert!(unpack(&packed).is_err(), "{packed:?}");
        }
        for count in 1..7 {
            for bit in count..7 {
                let mut packed = vec![0; count + 1];
                packed[0] = 1 << bit;
                assert_eq!(unpack(&packed), Err(Error::NoncanonicalPacking));
            }
        }
    }

    #[test]
    fn inquiry_matches_documented_wire_example() {
        assert_eq!(
            packet(INQ, vec![]).encode().unwrap(),
            [0xf0, 0x7d, 0x53, 0x44, 1, 0, 0x7f, 0xf7]
        );
    }

    #[test]
    fn every_request_and_response_encodes_and_decodes() {
        let mut entry = name();
        entry.extend_from_slice(&put()[18..]);
        let inquiry = vec![1, 0, 2, 0, 1, 1, 0, 0xff, 0xff, 0, 0, 20, 1];
        for (command, data) in [
            (INQ, vec![]),
            (INQ, inquiry),
            (DIR, vec![]),
            (DIR, entry),
            (GET, get()),
            (PUT, put()),
            (DATA, vec![0xff; 256]),
            (END, vec![]),
            (DEL, name()),
            (ABORT, vec![]),
            (ACK, vec![]),
            (NAK, vec![9, 20]),
        ] {
            let mut packet = packet(command, data);
            packet.sequence = 127;
            let frame = packet.encode().unwrap();
            assert_eq!(Packet::decode(&frame).unwrap(), packet);
            let sum: u32 = frame[4..frame.len() - 1]
                .iter()
                .map(|byte| u32::from(*byte))
                .sum();
            assert_eq!(sum % 128, 0);
        }
    }

    #[test]
    fn realtime_bytes_are_discarded_at_every_frame_position() {
        let packet = packet(PUT, put());
        let mut frame = vec![];
        for byte in packet.encode().unwrap() {
            frame.extend(0xf8..=0xff);
            frame.push(byte);
        }
        frame.extend(0xf8..=0xff);
        assert_eq!(Packet::decode(&frame).unwrap(), packet);
    }

    #[test]
    fn refuses_corruption_truncation_trailing_bytes_and_other_statuses() {
        let frame = packet(PUT, put()).encode().unwrap();
        for len in 0..frame.len() {
            assert!(Packet::decode(&frame[..len]).is_err());
        }
        for index in 4..frame.len() - 1 {
            let mut corrupt = frame.clone();
            corrupt[index] ^= 1;
            assert!(Packet::decode(&corrupt).is_err());
        }
        let mut extra = frame.clone();
        extra.push(0);
        assert!(Packet::decode(&extra).is_err());
        for status in 0x80..=0xf7 {
            let mut corrupt = frame.clone();
            corrupt.insert(6, status);
            assert!(Packet::decode(&corrupt).is_err());
        }
        assert!(Packet::decode(&wire(DATA, 0, &[2, 0])).is_err());
        assert!(Packet::decode(&wire(DATA, 0, &[0; 294])).is_err());
    }

    #[test]
    fn foreign_and_updater_headers_are_not_sidou_packets() {
        for prefix in [[0x22, 0x24, 0x35], [0x7e, 0x7f, 0x06], [0x7d, 0x53, 0x45]] {
            let frame = [0xf0, prefix[0], prefix[1], prefix[2], 0x11, 0, 0, 0xf7];
            assert_eq!(Packet::decode(&frame), Err(Error::ForeignHeader));
        }
    }

    #[test]
    fn command_ranges_and_payload_shapes_are_validated_in_both_directions() {
        for command in 0..=255 {
            if ![INQ, DIR, GET, PUT, DATA, END, DEL, ABORT, ACK, NAK].contains(&command) {
                assert!(packet(command, vec![]).encode().is_err());
                if command < 128 {
                    assert!(Packet::decode(&wire(command, 0, &[])).is_err());
                }
            }
        }
        for (command, invalid) in [
            (INQ, vec![0]),
            (DIR, vec![0]),
            (GET, vec![0; 17]),
            (PUT, vec![0; 26]),
            (DATA, vec![]),
            (DATA, vec![0; 257]),
            (END, vec![0]),
            (DEL, vec![0; 15]),
            (ABORT, vec![0]),
            (ACK, vec![0]),
            (NAK, vec![0, 0]),
            (NAK, vec![10, 0]),
        ] {
            assert!(packet(command, invalid.clone()).encode().is_err());
            if let Ok(packed) = pack(&invalid) {
                assert!(Packet::decode(&wire(command, 0, &packed)).is_err());
            }
        }
        let mut invalid = packet(INQ, vec![]);
        invalid.sequence = 128;
        assert!(invalid.encode().is_err());
    }

    #[test]
    fn target_instrument_type_and_name_rules_are_enforced() {
        for target in 1..=2 {
            let mut data = vec![0; 18];
            data[0] = target;
            data[1] = if target == 1 { 63 } else { 0 };
            assert!(packet(GET, data.clone()).encode().is_ok());
            data.extend([1, 0, 0, 0, 0, 0, 0, 0, 0]);
            assert!(packet(PUT, data.clone()).encode().is_ok());
            data[18] = 2;
            assert!(packet(PUT, data).encode().is_err());
            let mut nonzero_name = vec![0; 18];
            nonzero_name[0] = target;
            nonzero_name[1] = if target == 1 { 1 } else { 0 };
            nonzero_name[2] = b'A';
            assert!(packet(GET, nonzero_name).encode().is_err());
        }
        for (index, value) in [(0, 3), (1, 1), (2, b' '), (2, b'a'), (7, b'X')] {
            let mut data = get();
            data[index] = value;
            assert!(packet(GET, data).encode().is_err());
        }
        for instrument in [0, 64, 255] {
            let mut data = vec![0; 18];
            data[0] = 1;
            data[1] = instrument;
            assert!(packet(GET, data).encode().is_err());
        }
        for kind in [0, 128, 255] {
            let mut data = put();
            data[18] = kind;
            assert!(packet(PUT, data).encode().is_err());
            let mut entry = name();
            entry.extend([kind, 0, 0, 0, 0, 0, 0, 0, 0]);
            assert!(packet(DIR, entry).encode().is_err());
        }
        assert!(packet(DEL, vec![b'_'; 16]).encode().is_ok());
        assert!(packet(DEL, vec![0; 16]).encode().is_err());
        for reserved in [b"MIDI INSTRUMENT".as_slice(), b"MIDI SONG".as_slice()] {
            let mut data = vec![0; 16];
            data[..reserved.len()].copy_from_slice(reserved);
            assert!(packet(DEL, data).encode().is_err());
        }
    }

    #[test]
    fn inquiry_reserved_flags_and_non_seven_bit_versions_are_refused() {
        for (index, value) in [(1, 128), (2, 255), (3, 128), (4, 255), (12, 2), (12, 255)] {
            let mut data = vec![0; 13];
            data[0] = 1;
            data[index] = value;
            assert!(packet(INQ, data.clone()).encode().is_err());
            assert!(Packet::decode(&wire(INQ, 0, &pack(&data).unwrap())).is_err());
        }
    }
}
