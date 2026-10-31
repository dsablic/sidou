// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::panel::*;
use common::usb_host::UsbHost;
use common::{firmware, Firmware, Machine, USB_AUDIO_PER_SAMPLE};
use fm1_core::Bus;
use fm1_soc::usb::UsbEvent;

const CC_CUTOFF: u8 = 74;
const IDENTITY: &str = "FM-1_900";
const IN_MAX: usize = 64;
const CONFIG_LEN: usize = 178;

fn host(m: &mut Machine) -> &mut UsbHost {
    m.usb.as_mut().expect("a host")
}

fn up_and_config(m: &mut Machine, fw: &Firmware) -> (u8, u8) {
    let usb = fw.symbol("usb");
    (m.soc.read8(usb), m.soc.read8(usb + 1))
}

fn enumerate(m: &mut Machine, fw: &Firmware) {
    host(m).start();
    host(m).ep0_in.clear();
    for _ in 0..2_000 {
        if UsbHost::configured(&m.soc) {
            break;
        }
        m.usb_frames(1);
    }
    let events = m.soc.usb.events.clone();
    assert!(
        UsbHost::configured(&m.soc),
        "not enumerated in 2 s: {events:?}"
    );
    assert!(
        events.contains(&UsbEvent::Configured {
            product: "SIDOU FM-1".into(),
            vid: 0x1209,
            pid: 0x0001
        }),
        "{events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(e, UsbEvent::Failed(_))),
        "{events:?}"
    );
    let in0 = host(m).ep0_in.clone();
    let dev = in0
        .iter()
        .find(|p| p.len() == 18 && p[1] == 1)
        .expect("the device descriptor over EP0");
    assert_eq!(&dev[8..12], &[0x09, 0x12, 0x01, 0x00], "VID/PID 1209:0001");
    let at = in0
        .iter()
        .position(|p| p.len() == 64 && p[..2] == [9, 2])
        .expect("the whole config descriptor over EP0");
    let mut cfg = Vec::new();
    for p in &in0[at..] {
        cfg.extend_from_slice(p);
        if p.len() < 64 {
            break;
        }
    }
    assert_eq!(cfg.len(), CONFIG_LEN, "the config descriptor over EP0");
    assert_eq!(in0[at].len(), 64, "in 64-byte data packets");
    for (index, text) in [(1u8, "SIDOU contributors"), (2, "SIDOU FM-1")] {
        let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut want = vec![2 + utf16.len() as u8, 3];
        want.extend(&utf16);
        let d = if index == 2 {
            in0.iter()
                .find(|p| p.len() > 2 && p[1] == 3 && p[2..] == utf16[..])
                .cloned()
                .unwrap_or_default()
        } else {
            let at = fw.symbol("STR1");
            (0..want.len() as u32)
                .map(|i| m.soc.read8(at + i))
                .collect()
        };
        assert_eq!(d, want, "string {index}: {d:02x?}");
    }
    assert_eq!(up_and_config(m, fw), (1, 1), "not configured");
}

fn pack7(b: &[u8]) -> Vec<u8> {
    let (mut acc, mut nb, mut o) = (0u32, 0u32, Vec::new());
    for &x in b {
        acc |= u32::from(x) << nb;
        nb += 8;
        while nb >= 7 {
            o.push((acc & 0x7F) as u8);
            acc >>= 7;
            nb -= 7;
        }
    }
    if nb > 0 {
        o.push((acc & 0x7F) as u8);
    }
    o
}

fn unpack7(b: &[u8]) -> Vec<u8> {
    let (mut acc, mut nb, mut o) = (0u32, 0u32, Vec::new());
    for &x in b {
        acc |= u32::from(x & 0x7F) << nb;
        nb += 7;
        if nb >= 8 {
            o.push(acc as u8);
            acc >>= 8;
            nb -= 8;
        }
    }
    o
}

fn sysex_packets(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, c) in b.chunks(3).enumerate() {
        let last = (i + 1) * 3 >= b.len();
        out.push(if last { 4 + c.len() as u8 } else { 4 });
        out.extend((0..3).map(|j| c.get(j).copied().unwrap_or(0)));
    }
    out
}

fn message(cmd: u8, body: &[u8]) -> Vec<u8> {
    let mut msg = vec![0, 0x59, cmd, body.len() as u8, 0, 0];
    msg.extend_from_slice(body);
    let sum = body.iter().fold(0u8, |s, &b| s.wrapping_add(b));
    msg.push(!sum);
    let mut sx = vec![0xF0];
    sx.extend(pack7(&msg));
    sx.push(0xF7);
    sx
}

fn send(m: &mut Machine, bytes: &[u8]) {
    for msg in UsbHost::messages(bytes) {
        UsbHost::send(&mut m.soc, &msg);
    }
}

fn replies(in1: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut sx = Vec::new();
    for p in in1.concat().chunks(4) {
        let n = match p[0] & 15 {
            4 | 7 => 3,
            5 => 1,
            6 => 2,
            _ => 0,
        };
        for &b in &p[1..1 + n] {
            if b == 0xF0 {
                sx.clear();
            }
            sx.push(b);
            if b == 0xF7 && sx.len() > 2 {
                let d = unpack7(&sx[1..sx.len() - 1]);
                if d.len() >= 7 && d[..2] == [0, 0x59] {
                    out.push(d[2..].to_vec());
                }
            }
        }
    }
    out
}

fn events(in1: &[Vec<u8>]) -> Vec<[u8; 4]> {
    in1.concat()
        .chunks(4)
        .filter(|p| !(4..=7).contains(&(p[0] & 15)))
        .map(|p| [p[0], p[1], p[2], p[3]])
        .collect()
}

fn main_share(m: &mut Machine, ms: u64) -> f64 {
    let (main, cycles) = (m.main_steps, m.soc.periph.cycles);
    m.usb_frames(ms);
    (m.main_steps - main) as f64 / (m.soc.periph.cycles - cycles) as f64
}

#[test]
fn a_host_gets_the_identity_and_the_knobs_over_usb() {
    let Some(fw) = firmware("the USB test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.usb = Some(UsbHost::plug(&mut m.soc));
    run_past_boot(&mut m, &fw);
    m.usb_frames(20);
    assert_eq!(
        up_and_config(&mut m, &fw),
        (1, 0),
        "USB attached, no host yet"
    );
    let unconfigured = main_share(&mut m, 50);

    enumerate(&mut m, &fw);
    let configured = main_share(&mut m, 50);
    println!(
        "main loop's share of the cycles: {:.1} % with no host, {:.1} % configured",
        unconfigured * 100.0,
        configured * 100.0
    );
    assert!(
        unconfigured < 0.2,
        "the main loop did not sleep with no host"
    );
    assert!(
        configured > 0.5,
        "the main loop slept while a host was there"
    );

    host(&mut m).ep1_in.clear();
    send(&mut m, &sysex_packets(&message(0x11, &[])));
    m.usb_frames(50);
    let in1 = host(&mut m).ep1_in.clone();
    let sizes: Vec<usize> = in1.iter().map(Vec::len).collect();
    println!("identity reply in IN packets of {sizes:?} bytes");
    let r = replies(&in1);
    assert_eq!(r.len(), 1, "one reply to the handshake: {in1:02x?}");
    assert_eq!(r[0][0], 0x11);
    let id: String = r[0][4..31]
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as char)
        .collect();
    assert_eq!(id, IDENTITY);
    assert!(sizes.iter().all(|&n| n > 0 && n <= IN_MAX && n % 4 == 0));

    host(&mut m).ep1_in.clear();
    m.turn(K1, 3);
    m.usb_frames(60);
    let ev = events(&host(&mut m).ep1_in);
    println!("K1: {ev:02x?}");
    assert!(
        !ev.is_empty() && ev.iter().all(|p| p[..3] == [0x0B, 0xB0, CC_CUTOFF]),
        "K1 sent no CC 74 alone"
    );
    host(&mut m).ep1_in.clear();
    m.turn(PRESETS, 1);
    m.usb_frames(60);
    let ev = events(&host(&mut m).ep1_in);
    println!("PRESETS: {ev:02x?}");
    assert!(
        ev.iter().any(|p| p[..2] == [0x0C, 0xC0]),
        "PRESETS sent no program change"
    );
    assert!(host(&mut m).ep1_in.iter().all(|p| p.len() <= IN_MAX));

    let (sie, in_audio) = (
        UsbHost::sie_accesses(&m.soc),
        m.usb.as_ref().expect("a host").sie_in_audio,
    );
    println!("{sie} SIE accesses, {in_audio} inside an audio interrupt");
    assert!(sie > 0);
    assert_eq!(in_audio, 0, "USB polled nested in the audio interrupt");
}

#[test]
fn an_upgrade_request_gets_the_first_read_request_out() {
    let Some(fw) = firmware("the USB test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.usb = Some(UsbHost::plug(&mut m.soc));
    run_past_boot(&mut m, &fw);
    m.usb_frames(20);
    enumerate(&mut m, &fw);
    host(&mut m).ep1_in.clear();
    send(
        &mut m,
        &sysex_packets(&[0xF0, 0x22, 0x24, 0x35, 0x7F, 0xF7]),
    );
    m.usb_frames(100);
    let in1 = host(&mut m).ep1_in.clone();
    let r = replies(&in1);
    println!("after the upgrade request: {:02x?}", r);
    assert!(
        r.iter().any(|d| d[0] == 0x30),
        "no read request reached the host"
    );
    assert!(in1.iter().all(|p| p.len() <= IN_MAX));
    assert_eq!(m.usb.as_ref().expect("a host").sie_in_audio, 0);
}

fn sidou_frames(in1: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut frame: Vec<u8> = Vec::new();
    for packet in in1.concat().chunks(4) {
        let n = match packet[0] & 15 {
            4 | 7 => 3,
            5 => 1,
            6 => 2,
            _ => {
                assert!(frame.is_empty(), "a channel packet inside a SysEx frame");
                0
            }
        };
        for &b in &packet[1..=n] {
            if b >= 0xf8 {
                continue;
            }
            if b == 0xf0 {
                assert!(frame.is_empty(), "a SysEx frame cut by another");
            }
            frame.push(b);
            if b == 0xf7 {
                frames.push(std::mem::take(&mut frame));
            }
        }
    }
    assert!(
        frame.is_empty(),
        "an unterminated SysEx frame: {frame:02x?}"
    );
    frames
}

fn sd_frame(command: u8, sequence: u8, data: &[u8]) -> Vec<u8> {
    let mut out = vec![0xf0, 0x7d, 0x53, 0x44, command, sequence];
    for group in data.chunks(7) {
        out.push(
            group
                .iter()
                .enumerate()
                .fold(0, |mask, (i, b)| mask | ((b >> 7) << i)),
        );
        out.extend(group.iter().map(|b| b & 127));
    }
    let sum = out[4..].iter().fold(0u8, |sum, b| sum.wrapping_add(*b));
    out.extend([sum.wrapping_neg() & 127, 0xf7]);
    out
}

#[derive(Default)]
struct Link {
    taken: usize,
    frame: Vec<u8>,
    frames: std::collections::VecDeque<Vec<u8>>,
}

impl Link {
    fn send(&mut self, m: &mut Machine, command: u8, sequence: u8, data: &[u8]) {
        send(m, &sysex_packets(&sd_frame(command, sequence, data)));
    }

    fn recv(&mut self, m: &mut Machine, timeout: u64) -> (u8, u8, Vec<u8>) {
        let start = m.ms();
        loop {
            let in1 = &host(m).ep1_in;
            for packet in in1[self.taken..].concat().chunks(4) {
                let n = match packet[0] & 15 {
                    4 | 7 => 3,
                    5 => 1,
                    6 => 2,
                    _ => 0,
                };
                for &b in &packet[1..=n] {
                    if b >= 0xf8 {
                        continue;
                    }
                    if b == 0xf0 {
                        self.frame.clear();
                    }
                    self.frame.push(b);
                    if b == 0xf7 {
                        self.frames.push_back(std::mem::take(&mut self.frame));
                    }
                }
            }
            self.taken = in1.len();
            while let Some(f) = self.frames.pop_front() {
                if f.len() >= 8 && f[1..4] == [0x7d, 0x53, 0x44] {
                    let sum = f[4..f.len() - 1]
                        .iter()
                        .fold(0u8, |s, b| s.wrapping_add(*b));
                    assert_eq!(sum & 127, 0, "a reply's checksum: {f:02x?}");
                    let mut payload = Vec::new();
                    for group in f[6..f.len() - 2].chunks(8) {
                        let mask = group[0];
                        payload.extend(
                            group[1..]
                                .iter()
                                .enumerate()
                                .map(|(i, b)| b | ((mask >> i) & 1) << 7),
                        );
                    }
                    return (f[4], f[5], payload);
                }
            }
            assert!(m.ms() - start < timeout, "no reply in {timeout} ms");
            m.usb_frames(1);
        }
    }

    fn ack(&mut self, m: &mut Machine, sequence: u8) {
        let (command, s, payload) = self.recv(m, 2_000);
        assert_eq!(
            (command, s),
            (0x7e, sequence),
            "expected ACK {sequence}: {payload:02x?}"
        );
    }
}

fn usb_machine(fw: &Firmware) -> Machine {
    usb_machine_sie(fw, None)
}

fn usb_machine_sie(fw: &Firmware, sie_cycles: Option<u64>) -> Machine {
    let mut m = Machine::boot(&fw.image);
    m.usb = Some(UsbHost::plug(&mut m.soc));
    if let Some(n) = sie_cycles {
        m.soc.usb.sie_cycles = n;
    }
    run_past_boot(&mut m, fw);
    enumerate(&mut m, fw);
    let songs = fw.symbol("songs") + sidou_sys::ilp32::song_io::state as u32;
    for _ in 0..2_000 {
        if m.soc.read8(songs) == sidou_sys::SONG_IO_IDLE as u8 {
            break;
        }
        m.usb_frames(1);
    }
    assert_eq!(m.soc.read8(songs), sidou_sys::SONG_IO_IDLE as u8);
    host(&mut m).ep1_in.clear();
    m
}

fn name16(name: &str) -> Vec<u8> {
    (0..16)
        .map(|i| name.as_bytes().get(i).copied().unwrap_or(0))
        .collect()
}

const SYSEX_GAP_MS: f64 = 250.0;

const OTHER_TYPE: u8 = 0x7F;

fn put_file(m: &mut Machine, link: &mut Link, name: &str, data: &[u8]) -> (Vec<u8>, f64) {
    let mut meta = vec![0, 0];
    meta.extend(name16(name));
    meta.push(OTHER_TYPE);
    meta.extend((data.len() as u32).to_le_bytes());
    let crc = unsafe { sidou_sys::store_crc32(0, data.as_ptr().cast(), data.len() as u32) };
    meta.extend(crc.to_le_bytes());
    let mut seq = 10u8;
    link.send(m, 4, seq, &meta);
    link.ack(m, seq);
    let packets = data.len().div_ceil(256);
    let start = m.ms();
    for chunk in data.chunks(256) {
        seq += 1;
        link.send(m, 5, seq, chunk);
        link.ack(m, seq);
    }
    let up = (m.ms() - start) as f64 / packets as f64;
    seq += 1;
    link.send(m, 6, seq, &[]);
    link.ack(m, seq);
    (meta, up)
}

fn round_trip(m: &mut Machine, name: &str, data: &[u8]) -> (f64, f64) {
    let mut link = Link::default();
    let (meta, up) = put_file(m, &mut link, name, data);
    let packets = data.len().div_ceil(256);

    let mut get = vec![0, 0];
    get.extend(name16(name));
    link.send(m, 3, 60, &get);
    let (command, s, payload) = link.recv(m, 2_000);
    assert_eq!((command, s), (4, 60), "GET's metadata: {payload:02x?}");
    assert_eq!(payload[19..27], meta[19..27], "GET's length and CRC");
    link.send(m, 0x7e, s, &[]);
    let mut got = Vec::new();
    let start = m.ms();
    loop {
        let (command, s, payload) = link.recv(m, 2_000);
        link.send(m, 0x7e, s, &[]);
        match command {
            5 => got.extend(payload),
            6 => break,
            _ => panic!("GET: command {command:#x} {payload:02x?}"),
        }
    }
    let down = (m.ms() - start) as f64 / packets as f64;
    assert_eq!(got, data, "GET returned what PUT stored");
    (up, down)
}

fn transfer_file() -> Vec<u8> {
    (0..4096u32).map(|i| (i * 31 + i / 256) as u8).collect()
}

const STORE_POLL_WORST: u64 = 30_000;

fn timed_round_trip(fw: &Firmware, name: &str, chord: bool) -> (f64, f64) {
    let mut m = usb_machine(fw);
    assert_eq!(m.soc.read32(fw.symbol("store_fm1_window")), 0);
    if chord {
        for key in [30, 34, 37] {
            m.key(key, true);
        }
        m.usb_frames(50);
    }
    m.watch_calls(fw.symbol("store_fm1_service"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let counts = fw.symbol("store_fm1_counts");
    let (progs, holds) = (m.soc.read32(counts), m.soc.read32(counts + 16));
    let (up, down) = round_trip(&mut m, name, &transfer_file());
    let (progs, holds) = (
        m.soc.read32(counts) - progs,
        m.soc.read32(counts + 16) - holds,
    );
    let service = *m.watched_calls.iter().max().expect("store_fm1_service ran");
    let pass = *m.passes.iter().max().expect("passes ran");
    println!(
        "USB SysEx ({}): {up:.1} ms a DATA and its ACK up, {down:.1} ms down; {progs} programs, \
         all held: {holds}; store_fm1_service {service} at most, a pass {pass}",
        if chord { "a chord sounding" } else { "silent" }
    );
    assert_eq!(progs, holds, "every production program is held");
    assert!(service <= STORE_POLL_WORST, "store_fm1_service {service}");
    assert!(pass <= common::POLL_INSTRUCTIONS, "a pass {pass}");
    if chord {
        assert!(
            m.soc
                .periph
                .audio
                .iter()
                .rev()
                .take(2 * common::HALF_FRAMES)
                .any(|&s| s != 0),
            "the chord plays again after the transfer"
        );
    }
    (up, down)
}

#[test]
fn a_silent_usb_put_and_get_move_a_256_byte_packet_quickly() {
    let Some(fw) = firmware("the SysEx USB timing test") else {
        return;
    };
    let (up, down) = timed_round_trip(&fw, "USB TIMING", false);
    assert!(up <= 51.6, "PUT: {up:.1} ms a packet");
    assert!(down <= 12.0, "GET: {down:.1} ms a packet");
}

#[test]
fn a_sounding_usb_put_keeps_its_gaps_with_one_hold_a_packet() {
    let Some(fw) = firmware("the SysEx USB timing test") else {
        return;
    };
    let (up, down) = timed_round_trip(&fw, "USB CHORD", true);
    assert!(
        up >= SYSEX_GAP_MS,
        "PUT: {up:.1} ms a packet, under the gap"
    );
    assert!(up <= 290.0, "PUT: {up:.1} ms a packet");
    assert!(down <= 11.4, "GET: {down:.1} ms a packet");
}

fn get_part_way(m: &mut Machine, fw: &Firmware, link: &mut Link, name: &str, sequence: u8) {
    let mut get = vec![0, 0];
    get.extend(name16(name));
    link.send(m, 3, sequence, &get);
    let (command, s, _) = link.recv(m, 2_000);
    assert_eq!((command, s), (4, sequence), "GET's metadata");
    link.send(m, 0x7e, sequence, &[]);
    let state = fw.symbol("sidou_sysex");
    for _ in 0..200 {
        let active = m
            .soc
            .read8(state + sidou_sys::ilp32::sysex::tx_active as u32);
        let command = m
            .soc
            .read8(state + sidou_sys::ilp32::sysex::tx_command as u32);
        let at = m
            .soc
            .read16(state + sidou_sys::ilp32::sysex::tx_position as u32);
        if active != 0 && command == 5 && at >= 12 {
            return;
        }
        m.usb_frames(1);
    }
    panic!("the DATA reply never was part way out");
}

fn reply_file() -> Vec<u8> {
    (0..256u32).map(|i| (i * 7) as u8).collect()
}

#[test]
fn an_updater_identity_reply_cannot_overtake_sidous_queued_end_packet() {
    let Some(fw) = firmware("SysEx producer arbitration") else {
        return;
    };
    let mut m = usb_machine(&fw);
    put_file(&mut m, &mut Link::default(), "ARBITRATION", &reply_file());
    host(&mut m).ep1_in.clear();
    get_part_way(&mut m, &fw, &mut Link::default(), "ARBITRATION", 20);
    send(&mut m, &sysex_packets(&message(0x11, &[])));
    m.usb_frames(100);
    let packets = host(&mut m).ep1_in.clone();
    let mut frames = Vec::new();
    let mut frame = Vec::new();
    for packet in packets.concat().as_chunks::<4>().0 {
        let n = match packet[0] & 15 {
            4 | 7 => 3,
            5 => 1,
            6 => 2,
            _ => 0,
        };
        for &b in &packet[1..=n] {
            if b >= 0xf8 {
                continue;
            }
            if b == 0xf0 {
                assert!(
                    frame.is_empty(),
                    "a producer split an unfinished SysEx frame"
                );
            }
            if b == 0xf0 || !frame.is_empty() {
                frame.push(b);
            }
            if b == 0xf7 && !frame.is_empty() {
                frames.push(std::mem::take(&mut frame));
            }
        }
    }
    assert!(frame.is_empty(), "unterminated SysEx response");
    assert_eq!(
        frames.len(),
        3,
        "GET's metadata, its DATA, the updater's reply"
    );
    assert_eq!(&frames[0][..6], &[0xf0, 0x7d, 0x53, 0x44, 4, 20]);
    assert_eq!(&frames[1][..6], &[0xf0, 0x7d, 0x53, 0x44, 5, 21]);
    assert_eq!(
        replies(&packets).len(),
        1,
        "the updater identity still replies"
    );
}

fn ring_gap(m: &mut Machine, fw: &Firmware, sources: u8) {
    let ring = fw.symbol("in_ring");
    let epoch = ring + sidou_sys::ilp32::midi_ring::epoch as u32;
    let e = m.soc.read8(epoch);
    m.soc.write8(epoch, e.wrapping_add(1));
    m.soc
        .write8(ring + sidou_sys::ilp32::midi_ring::in_gap as u32, sources);
}

#[test]
fn a_ring_gap_aborts_only_its_ports_sysex_and_never_cuts_a_reply() {
    let Some(fw) = firmware("SysEx across a MIDI-in gap") else {
        return;
    };
    let mut m = usb_machine(&fw);
    let data = reply_file();
    put_file(&mut m, &mut Link::default(), "GAP", &data);
    let restarts = fw.symbol("midi_stats") + 8;
    let before = m.soc.read32(restarts);

    host(&mut m).ep1_in.clear();
    let mut link = Link::default();
    get_part_way(&mut m, &fw, &mut link, "GAP", 20);
    ring_gap(&mut m, &fw, sidou_sys::MIDI_SRC_TRS as u8);
    let (command, s, payload) = link.recv(&mut m, 2_000);
    assert_eq!((command, s), (5, 21));
    assert_eq!(payload, data, "the DATA whole");
    link.send(&mut m, 0x7e, 21, &[]);
    let (command, s, _) = link.recv(&mut m, 2_000);
    assert_eq!((command, s), (6, 22), "the transfer went on to its END");
    link.send(&mut m, 0x7e, 22, &[]);
    m.usb_frames(20);
    assert_eq!(
        m.soc.read32(restarts),
        before + 1,
        "the gap restarted TRS's parser"
    );
    assert_eq!(sidou_frames(&host(&mut m).ep1_in).len(), 3);

    host(&mut m).ep1_in.clear();
    get_part_way(&mut m, &fw, &mut Link::default(), "GAP", 40);
    ring_gap(&mut m, &fw, sidou_sys::MIDI_SRC_USB as u8);
    m.usb_frames(100);
    let frames = sidou_frames(&host(&mut m).ep1_in);
    assert_eq!(frames.len(), 3, "the metadata, the DATA, then a NAK");
    assert_eq!(&frames[1][4..6], &[5, 41], "the DATA, whole to its F7");
    assert_eq!(frames[1].len(), 8 + 256 + 37);
    assert_eq!(frames[2][4], 0x7f, "a NAK");
    assert_eq!(frames[2][7], sidou_sys::SYSEX_ABORTED as u8, "ABORTED");

    host(&mut m).ep1_in.clear();
    send(&mut m, &sysex_packets(&sd_frame(1, 2, &[])));
    m.usb_frames(100);
    let frames = sidou_frames(&host(&mut m).ep1_in);
    assert_eq!(frames.len(), 1, "the next INQ is answered: {frames:02x?}");
    assert_eq!(&frames[0][..6], &[0xf0, 0x7d, 0x53, 0x44, 1, 2]);
}

fn spp_inside(in1: &[Vec<u8>]) -> Option<bool> {
    let mut open = false;
    for packet in in1.concat().chunks(4) {
        match packet[0] & 15 {
            3 if packet[1] == 0xf2 => return Some(open),
            4 => open = true,
            5..=7 => open = false,
            _ => {}
        }
    }
    None
}

#[test]
fn clock_outs_song_position_waits_for_a_sysex_reply_to_end() {
    let Some(fw) = firmware("clock out against a SysEx reply") else {
        return;
    };
    let (play, edit) = (
        sidou_sys::PANEL_BTN_PLAY as u8,
        sidou_sys::PANEL_BTN_EDIT as u8,
    );
    let mut m = usb_machine(&fw);
    put_file(&mut m, &mut Link::default(), "SPP", &reply_file());
    let syncing = fw.symbol("syncing");
    m.soc
        .write8(syncing + sidou_sys::ilp32::midi_sync::clock_out as u32, 1);
    let tap = |m: &mut Machine, keys: &[u8]| {
        for &k in keys {
            m.key(k, true);
        }
        m.usb_frames(40);
        for &k in keys.iter().rev() {
            m.key(k, false);
        }
        m.usb_frames(40);
    };
    tap(&mut m, &[play]);
    m.usb_frames(1_000);
    tap(&mut m, &[play]);
    m.usb_frames(300);
    let mut held = 0;
    for delay in 0..16u64 {
        host(&mut m).ep1_in.clear();
        let mut link = Link::default();
        let mut get = vec![0, 0];
        get.extend(name16("SPP"));
        link.send(&mut m, 3, 30, &get);
        let (command, s, _) = link.recv(&mut m, 2_000);
        assert_eq!((command, s), (4, 30), "GET's metadata");
        let calls = fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::calls as u32;
        let before = m.soc.read32(calls);
        m.key(edit, true);
        m.usb_frames(10);
        m.key(play, true);
        m.usb_frames(delay);
        link.send(&mut m, 0x7e, 30, &[]);
        let out_state = syncing + sidou_sys::ilp32::midi_sync::out_state as u32;
        let waiting = syncing + sidou_sys::ilp32::midi_sync::out_common_wait as u32;
        let stopped_at = m.soc.read32(calls);
        for _ in 0..60 {
            m.usb_frames(1);
            let state = m.soc.read8(out_state);
            if m.soc.read8(waiting) != 0
                && m.soc.read32(calls) != stopped_at
                && state == sidou_sys::MIDI_SYNC_STOPPED as u8
            {
                held += 1;
                break;
            }
        }
        let (command, s, _) = link.recv(&mut m, 2_000);
        assert_eq!((command, s), (5, 31), "GET's DATA");
        m.key(play, false);
        m.key(edit, false);
        link.send(&mut m, 0x7e, 31, &[]);
        let (command, s, _) = link.recv(&mut m, 2_000);
        assert_eq!((command, s), (6, 32), "GET's END");
        link.send(&mut m, 0x7e, 32, &[]);
        m.usb_frames(20);
        assert!(
            m.soc.read32(calls) > before,
            "EDIT + PLAY went on from call {before}: a continue, not a play from the top"
        );
        let inside = spp_inside(&host(&mut m).ep1_in);
        println!(
            "continue {delay} ms before the DATA's ACK (stopped at call {stopped_at}): song \
             position inside {inside:?}"
        );
        assert_eq!(
            inside,
            Some(false),
            "the song position landed inside a SysEx reply (or none went)"
        );
        tap(&mut m, &[play]);
        m.usb_frames(100);
    }
    println!("{held} of the continues came while the DATA was part way out");
    assert!(held > 0, "no continue came while a reply was part way out");
}

fn transport_out(in1: &[Vec<u8>]) -> Vec<(u8, u32)> {
    events(in1)
        .iter()
        .filter_map(|p| match p[0] & 15 {
            15 if matches!(p[1], 0xfa..=0xfc) => Some((p[1], 0)),
            3 if p[1] == 0xf2 => Some((0xf2, u32::from(p[2]) | u32::from(p[3]) << 7)),
            _ => None,
        })
        .collect()
}

fn clock_out_stopped_part_way(fw: &Firmware) -> Machine {
    let mut m = usb_machine(fw);
    let syncing = fw.symbol("syncing");
    m.soc
        .write8(syncing + sidou_sys::ilp32::midi_sync::clock_out as u32, 1);
    let play = sidou_sys::PANEL_BTN_PLAY as u8;
    tap_keys(&mut m, &[play]);
    m.usb_frames(1_000);
    tap_keys(&mut m, &[play]);
    m.usb_frames(300);
    m
}

fn tap_keys(m: &mut Machine, keys: &[u8]) {
    for (i, &k) in keys.iter().enumerate() {
        if i > 0 {
            m.usb_frames(10);
        }
        m.key(k, true);
    }
    m.usb_frames(40);
    for &k in keys.iter().rev() {
        m.key(k, false);
    }
    m.usb_frames(40);
}

#[test]
fn clock_out_starts_a_play_from_the_top_and_continues_edit_play() {
    let Some(fw) = firmware("clock out's start against its continue") else {
        return;
    };
    let (play, edit) = (
        sidou_sys::PANEL_BTN_PLAY as u8,
        sidou_sys::PANEL_BTN_EDIT as u8,
    );
    let mut m = clock_out_stopped_part_way(&fw);
    let calls = fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::calls as u32;
    let stopped_at = m.soc.read32(calls);
    assert!(stopped_at > 20, "stopped part way: call {stopped_at}");
    host(&mut m).ep1_in.clear();
    tap_keys(&mut m, &[play]);
    m.usb_frames(100);
    let now = m.soc.read32(calls);
    assert!(now < stopped_at, "PLAY played from the top: call {now}");
    assert_eq!(
        transport_out(&host(&mut m).ep1_in),
        [(0xfa, 0)],
        "PLAY alone: a start"
    );
    m.usb_frames(1_000);
    tap_keys(&mut m, &[play]);
    m.usb_frames(300);
    let stopped_at = m.soc.read32(calls);
    host(&mut m).ep1_in.clear();
    tap_keys(&mut m, &[edit, play]);
    m.usb_frames(100);
    assert!(m.soc.read32(calls) > stopped_at, "EDIT + PLAY went on");
    let out = transport_out(&host(&mut m).ep1_in);
    assert_eq!(out.len(), 2, "a song position and a continue: {out:x?}");
    assert_eq!(out[0].0, 0xf2, "{out:x?}");
    assert!(out[0].1 > 0, "the song position where it stopped: {out:x?}");
    assert_eq!(out[1], (0xfb, 0), "{out:x?}");
}

#[test]
fn clock_outs_start_goes_out_during_a_sysex_reply() {
    let Some(fw) = firmware("clock out's start against a SysEx reply") else {
        return;
    };
    let play = sidou_sys::PANEL_BTN_PLAY as u8;
    let mut m = usb_machine(&fw);
    put_file(&mut m, &mut Link::default(), "SPP", &reply_file());
    let syncing = fw.symbol("syncing");
    m.soc
        .write8(syncing + sidou_sys::ilp32::midi_sync::clock_out as u32, 1);
    let calls = fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::calls as u32;
    let state = fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::state as u32;
    let tx_active = fw.symbol("sidou_sysex") + sidou_sys::ilp32::sysex::tx_active as u32;
    let (mo_w, mo_r) = (fw.symbol("mo_w"), fw.symbol("mo_r"));
    let mut during = 0;
    for delay in 0..16u64 {
        tap_keys(&mut m, &[play]);
        m.usb_frames(500);
        tap_keys(&mut m, &[play]);
        m.usb_frames(100);
        let stopped_at = m.soc.read32(calls);
        host(&mut m).ep1_in.clear();
        let mut link = Link::default();
        let mut get = vec![0, 0];
        get.extend(name16("SPP"));
        link.send(&mut m, 3, 30, &get);
        let (command, s, _) = link.recv(&mut m, 2_000);
        assert_eq!((command, s), (4, 30), "GET's metadata");
        m.key(play, true);
        m.usb_frames(delay);
        link.send(&mut m, 0x7e, 30, &[]);
        let mut overlap = false;
        for _ in 0..40 {
            m.usb_frames(1);
            let used = m.soc.read32(mo_w).wrapping_sub(m.soc.read32(mo_r));
            if m.soc.read8(state) == sidou_sys::SONG_PLAYING as u8
                && m.soc.read8(tx_active) != 0
                && used >= 60
            {
                overlap = true;
            }
        }
        let (command, s, _) = link.recv(&mut m, 2_000);
        assert_eq!((command, s), (5, 31), "GET's DATA");
        m.key(play, false);
        link.send(&mut m, 0x7e, 31, &[]);
        let (command, s, _) = link.recv(&mut m, 2_000);
        assert_eq!((command, s), (6, 32), "GET's END");
        link.send(&mut m, 0x7e, 32, &[]);
        m.usb_frames(40);
        let out = transport_out(&host(&mut m).ep1_in);
        println!(
            "PLAY {delay} ms before the DATA's ACK (stopped at call {stopped_at}): \
             overlap {overlap}, clock out {out:x?}"
        );
        during += usize::from(overlap);
        assert!(
            m.soc.read32(calls) < stopped_at,
            "PLAY played from the top (call {})",
            m.soc.read32(calls)
        );
        assert_eq!(out, [(0xfa, 0)], "PLAY's start, once, against a reply");
        tap_keys(&mut m, &[play]);
        m.usb_frames(100);
    }
    println!("{during} of the starts came while the reply filled the ring");
    assert!(during > 0, "no start came while a reply filled the ring");
}

fn fit_residual(m: &mut Machine, fw: &Firmware) -> (usize, f64) {
    use sidou_sys::ilp32::{midi_clock as mc, midi_sync as ms};
    let c = fw.symbol("syncing") + ms::clock as u32;
    let (head, count) = (
        u32::from(m.soc.read8(c + mc::head as u32)),
        u32::from(m.soc.read8(c + mc::count as u32)),
    );
    let w = sidou_sys::MIDI_CLOCK_WINDOW;
    let pts: Vec<(f64, f64)> = (0..count)
        .map(|i| {
            let k = (head + i) % w;
            let n = m.soc.read32(c + mc::number as u32 + 4 * k);
            let t = m.soc.read32(c + mc::stamp as u32 + 4 * k);
            (f64::from(n), f64::from(t))
        })
        .collect();
    let len = pts.len() as f64;
    let (mx, my) = (
        pts.iter().map(|p| p.0).sum::<f64>() / len,
        pts.iter().map(|p| p.1).sum::<f64>() / len,
    );
    let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let sxy: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let b = sxy / sxx;
    let worst = pts
        .iter()
        .map(|p| (p.1 - (my + b * (p.0 - mx))).abs())
        .fold(0.0, f64::max);
    (pts.len(), worst / (24e6 / 544.0) * 1e3)
}

#[test]
fn clocks_stamped_late_by_a_flash_hold_stay_out_of_the_clocks_fit() {
    let Some(fw) = firmware("the clock follower across flash holds") else {
        return;
    };
    let mut m = usb_machine(&fw);
    let per = 60.0 / 120.0 / 24.0 * m.soc.periph.hz as f64;
    let start = m.soc.periph.cycles as f64;
    m.trs_scheduled = (0..24 * 2 * 8)
        .map(|i| ((start + f64::from(i) * per) as u64, 0xf8))
        .collect();
    m.usb_frames(3_000);
    let (n, before) = fit_residual(&mut m, &fw);
    println!("before the PUT: {n} clocks in the fit, the worst {before:.2} ms off its line");
    assert!(n >= 90 && before < 0.6, "the fit before the holds");
    let hold = m.soc.periph.cycle_of_ms(30);
    m.masked_stalls = vec![
        (fw.symbol("flash_store_prog"), hold),
        (fw.symbol("flash_store_erase"), hold),
    ];
    let data: Vec<u8> = (0..1024u32).map(|i| (i * 13) as u8).collect();
    put_file(&mut m, &mut Link::default(), "HOLDS", &data);
    m.masked_stalls.clear();
    m.usb_frames(100);
    let (n, after) = fit_residual(&mut m, &fw);
    println!(
        "after {} holds of 30 ms: {n} clocks in the fit, the worst {after:.2} ms off its line",
        m.masked_stalls_taken
    );
    assert!(
        m.masked_stalls_taken >= 4,
        "{} holds",
        m.masked_stalls_taken
    );
    assert!(after < 0.6, "a clock {after:.2} ms off the fit's line");
}

#[test]
fn a_usb_sysex_flood_is_held_back_by_naks_not_lost() {
    let Some(fw) = firmware("USB back-pressure on a SysEx flood") else {
        return;
    };
    let mut m = usb_machine(&fw);
    for key in [30, 34, 37] {
        m.key(key, true);
    }
    m.usb_frames(50);
    let ring = fw.symbol("in_ring");
    let lost_at = ring + sidou_sys::ilp32::midi_ring::lost as u32;
    let lost = m.soc.read32(lost_at);
    let restarts = fw.symbol("midi_stats") + 8;
    let restarted = m.soc.read32(restarts);
    let dump: Vec<u8> = [0xf0, 0x43]
        .into_iter()
        .chain((0..8192u32).map(|i| (i % 127) as u8))
        .chain(std::iter::once(0xf7))
        .collect();
    let long_pass = m.soc.periph.cycle_of_ms(20);
    let drain = fw.symbol("midi_drain");
    let (start, mut naks, mut held) = (m.ms(), 0u64, 0u32);
    let mut next_pass = m.ms() + 5;
    UsbHost::send(&mut m.soc, &dump);
    while UsbHost::pending(&m.soc) > 0 {
        if m.ms() >= next_pass {
            m.long_pass = Some((drain, long_pass));
            held += 1;
            next_pass = m.ms() + 40;
        }
        let sent = m.soc.usb.out_packets;
        m.usb_frames(1);
        if m.soc.usb.out_packets == sent {
            naks += 1;
        }
    }
    m.usb_frames(200);
    let lost = m.soc.read32(lost_at) - lost;
    println!(
        "{} bytes of SysEx in {} ms with {held} passes of 20 ms, {naks} NAKs: {lost} events lost",
        dump.len(),
        m.ms() - start - 200
    );
    assert!(held >= 3);
    assert_eq!(lost, 0, "SIDOU's MIDI-in ring lost events to the flood");
    assert_eq!(
        m.soc.read32(restarts),
        restarted,
        "a gap restarted the parsers"
    );
    for key in [30, 34, 37] {
        m.key(key, false);
    }
}

#[test]
fn nothing_sleeps_in_idle_while_a_host_is_there_not_even_the_rings_wait() {
    use sidou_sys::ilp32::reg_ring as rr;
    let Some(fw) = firmware("no idle with a host") else {
        return;
    };
    let mut m = usb_machine(&fw);
    m.key(30, true);
    m.usb_frames(20);
    let tick = fw.symbol("live_tick");
    while m.cpu.pc != tick || !m.cpu.irq_levels.is_empty() {
        m.run_cycles(1);
    }
    let ring = fw.symbol("ring");
    let (tail, stage) = (
        m.soc.read32(ring + rr::tail as u32),
        m.soc.read32(ring + rr::stage as u32),
    );
    let size = sidou_sys::REG_RING_SIZE;
    let fill = size - (stage - tail);
    for k in 0..fill {
        let at = ring + rr::e as u32 + 2 * ((stage + k) % size);
        m.soc.write8(at, 0x1f);
        m.soc.write8(at + 1, 0);
    }
    m.soc.write32(ring + rr::stage as u32, stage + fill);
    m.soc.write32(ring + rr::head as u32, stage + fill);
    m.watch_calls(fw.symbol("wait_for_drain"));
    m.idles.clear();
    m.usb_frames(30);
    m.key(30, false);
    m.usb_frames(20);
    let waits = m.watched_calls.len();
    println!(
        "{waits} waits for the ring's room; main-loop idles {:x?}",
        m.idles
    );
    assert!(waits > 0, "the ring was never waited for");
    assert!(
        m.idles.is_empty(),
        "slept in idle with a host: {:x?}",
        m.idles
    );
}

const HOOK_KEY: u32 = 4;
const HOOK_LEN: u32 = 8;
const HOOK_DATA: u32 = 16;
const HOOK_PUT: u32 = 1;
const HOOK_COLLECT: u32 = 4;
const JOB_COLLECT: u8 = 2;
const ERASES: u32 = 4;
const CHORD: [u8; 3] = [30, 34, 37];

fn store_byte(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    m.soc.read8(fw.symbol("store_fm1_store") + field as u32)
}

fn writer_phase(m: &mut Machine, fw: &Firmware) -> u8 {
    store_byte(
        m,
        fw,
        sidou_sys::ilp32::store::w + sidou_sys::ilp32::store_writer::phase,
    )
}

fn hook_put(m: &mut Machine, fw: &Firmware, key: u32, fill: u8) {
    let h = fw.symbol("store_fm1_hook");
    m.soc.write32(h + HOOK_KEY, key);
    m.soc.write32(h + HOOK_LEN, 16);
    for i in 0..16 {
        m.soc.write8(h + HOOK_DATA + i, fill);
    }
    m.soc.write32(h, HOOK_PUT);
    for _ in 0..5_000 {
        if m.soc.read32(h) == 0 {
            return;
        }
        m.usb_frames(1);
    }
    panic!("the hook's put never ran");
}

#[derive(Clone, Copy, Debug)]
enum Pending {
    Record,
    Collection,
}

#[derive(Clone, Copy, Debug)]
enum Sound {
    Keys,
    Song,
    Notes,
}

fn sound(m: &mut Machine, fw: &Firmware, how: Sound) {
    match how {
        Sound::Keys => {
            for key in CHORD {
                m.key(key, true);
            }
        }
        Sound::Song => {
            m.key(sidou_sys::PANEL_BTN_PLAY as u8, true);
            m.usb_frames(30);
            m.key(sidou_sys::PANEL_BTN_PLAY as u8, false);
            let ctl = fw.symbol("song_transport");
            for _ in 0..5_000 {
                if m.soc
                    .read8(ctl + sidou_sys::ilp32::song_ctl::isr_playing as u32)
                    != 0
                {
                    break;
                }
                m.usb_frames(1);
            }
            assert_ne!(
                m.soc
                    .read8(ctl + sidou_sys::ilp32::song_ctl::isr_playing as u32),
                0,
                "the demo plays"
            );
        }
        Sound::Notes => {
            for note in [57, 61, 64] {
                send(m, &[0x09, 0x90, note, 100]);
            }
        }
    }
    m.usb_frames(50);
}

fn stall(m: &mut Machine, fw: &Firmware, pending: Pending, how: Sound) {
    if let Pending::Collection = pending {
        let window = fw.symbol("store_fm1_window");
        for key in CHORD {
            m.key(key, true);
        }
        m.soc.write32(window, 256);
        let head = store_byte(m, fw, sidou_sys::ilp32::store::head);
        let mut k = 0;
        while store_byte(m, fw, sidou_sys::ilp32::store::head) == head {
            assert!(k < 400, "the head never moved");
            hook_put(m, fw, 0x100 + k % 8, k as u8);
            k += 1;
        }
        while writer_phase(m, fw) != 0 {
            m.usb_frames(1);
        }
        for key in CHORD {
            m.key(key, false);
        }
        m.soc.write32(window, 0);
        let h = fw.symbol("store_fm1_hook");
        m.soc.write32(h, HOOK_COLLECT);
        for _ in 0..100 {
            if store_byte(m, fw, sidou_sys::ilp32::store::job) == JOB_COLLECT
                && writer_phase(m, fw) != 0
            {
                break;
            }
            m.usb_frames(1);
        }
        assert_eq!(
            store_byte(m, fw, sidou_sys::ilp32::store::job),
            JOB_COLLECT,
            "a collection of the old head"
        );
        sound(m, fw, how);
    } else {
        sound(m, fw, how);
        hook_put(m, fw, 0x200, 0x5a);
    }
    m.usb_frames(1_000);
    let job = store_byte(m, fw, sidou_sys::ilp32::store::job);
    assert!(
        writer_phase(m, fw) != 0 || job == JOB_COLLECT,
        "{pending:?}: the store's work waits for quiet while the device sounds"
    );
}

fn stop_song(m: &mut Machine, fw: &Firmware) {
    m.key(sidou_sys::PANEL_BTN_PLAY as u8, true);
    m.usb_frames(30);
    m.key(sidou_sys::PANEL_BTN_PLAY as u8, false);
    let ctl = fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::isr_playing as u32;
    for _ in 0..2_000 {
        if m.soc.read8(ctl) == 0 {
            return;
        }
        m.usb_frames(1);
    }
    panic!("the song never stopped");
}

fn toast(m: &mut Machine, fw: &Firmware) -> String {
    let at = fw.symbol("ui") + sidou_sys::ilp32::screen::toast as u32;
    (0..sidou_sys::SCREEN_TOAST_CHARS)
        .map(|i| m.soc.read8(at + i))
        .take_while(|&b| b != 0)
        .map(char::from)
        .collect()
}

fn waits_while_the_song_plays(
    m: &mut Machine,
    fw: &Firmware,
    link: &mut Link,
    sequence: u8,
    ms: u64,
) -> u64 {
    let counts = fw.symbol("store_fm1_counts");
    let (holds, want) = (m.soc.read32(counts + 16), fw.symbol("hold_want"));
    let start = m.ms();
    let mut waits = Vec::new();
    while m.ms() - start < ms {
        let (command, s, payload) = link.recv(m, 3_000);
        assert_eq!(
            (command, s, payload.len()),
            (0x7d, sequence, 0),
            "a WAIT for {sequence}: {payload:02x?}"
        );
        waits.push(m.ms() - start);
        assert!(toast(m, fw).starts_with("UPLOAD WAITS"), "{}", toast(m, fw));
        assert_eq!(
            m.soc.read8(want),
            0,
            "a hold asked for while the song plays"
        );
    }
    println!("WAITs for {sequence} at {waits:?} ms");
    assert!(waits[0] <= 50, "the first WAIT at once: {waits:?}");
    assert!(
        waits.len() as u64 > ms / u64::from(sidou_sys::SYSEX_WAIT_EVERY_MS),
        "{waits:?}"
    );
    for w in waits.windows(2) {
        let gap = w[1] - w[0];
        assert!(
            (u64::from(sidou_sys::SYSEX_WAIT_EVERY_MS)
                ..=u64::from(sidou_sys::SYSEX_WAIT_EVERY_MS) + 50)
                .contains(&gap),
            "WAITs {gap} ms apart"
        );
    }
    assert_eq!(
        m.soc.read32(counts + 16),
        holds,
        "a hold while the song plays"
    );
    let stopped = m.ms();
    stop_song(m, fw);
    stopped
}

fn put_while_the_store_waits(pending: Pending, how: Sound) {
    let Some(fw) = firmware("a PUT while the store waits for quiet") else {
        return;
    };
    let mut m = usb_machine(&fw);
    assert_eq!(m.soc.read32(fw.symbol("store_fm1_window")), 0);
    stall(&mut m, &fw, pending, how);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let counts = fw.symbol("store_fm1_counts");
    let (progs, holds) = (m.soc.read32(counts), m.soc.read32(counts + 16));
    let data = transfer_file();
    let mut link = Link::default();
    let mut meta = vec![0, 0];
    meta.extend(name16("USB WAITS"));
    meta.push(OTHER_TYPE);
    meta.extend((data.len() as u32).to_le_bytes());
    let crc = unsafe { sidou_sys::store_crc32(0, data.as_ptr().cast(), data.len() as u32) };
    meta.extend(crc.to_le_bytes());
    let mut asked = m.ms();
    link.send(&mut m, 4, 10, &meta);
    let song = matches!(how, Sound::Song);
    if song {
        asked = waits_while_the_song_plays(&mut m, &fw, &mut link, 10, 4_000);
    }
    let (command, s, payload) = link.recv(&mut m, 2_000);
    assert_eq!(
        (command, s),
        (0x7e, 10),
        "{pending:?}, {how:?}: the PUT's metadata is accepted, not refused: {payload:02x?}"
    );
    let claim = m.ms() - asked;
    let erases = m.soc.read32(counts + ERASES);
    let mut seq = 10u8;
    let start = m.ms();
    for chunk in data.chunks(256) {
        seq += 1;
        link.send(&mut m, 5, seq, chunk);
        link.ack(&mut m, seq);
        assert_eq!(
            m.soc.read32(counts + ERASES),
            erases,
            "an erase in the transfer"
        );
    }
    let up = (m.ms() - start) as f64 / data.len().div_ceil(256) as f64;
    seq += 1;
    link.send(&mut m, 6, seq, &[]);
    link.ack(&mut m, seq);
    assert_eq!(
        m.soc.read32(counts + ERASES),
        erases,
        "an erase in the transfer"
    );
    let (progs, holds) = (
        m.soc.read32(counts) - progs,
        m.soc.read32(counts + 16) - holds,
    );
    let pass = *m.passes.iter().max().expect("passes ran");
    println!(
        "PUT while the store waits ({pending:?}, {how:?}): claimed after {claim} ms, then \
         {up:.1} ms a DATA and its ACK; {progs} programs, all held: {holds}; a pass {pass}"
    );
    assert_eq!(progs, holds, "every production program is held");
    assert!(pass <= common::POLL_INSTRUCTIONS, "a pass {pass}");
    let (claim_max, up_max) = if song { (33, 60.0) } else { (25, 289.0) };
    assert!(claim <= claim_max, "the PUT's claim took {claim} ms");
    assert!(up <= up_max, "PUT: {up:.1} ms a packet");
    assert!(
        song || up >= SYSEX_GAP_MS,
        "PUT: {up:.1} ms a packet, under the gap"
    );
    m.usb_frames(10);
    assert!(
        song || m
            .soc
            .periph
            .audio
            .iter()
            .rev()
            .take(2 * common::HALF_FRAMES)
            .any(|&s| s != 0),
        "the sound plays again after the transfer"
    );
    let mut flash = m.soc.flash.mem.clone();
    let st = common::mount_dump(&mut flash);
    let mut file = sidou_sys::store_file::default();
    unsafe {
        assert_eq!(
            sidou_sys::store_file_open(&*st, c"USB WAITS".as_ptr(), &mut file),
            sidou_sys::STORE_OK
        );
        let mut got = vec![0u8; file.length as usize];
        sidou_sys::store_file_read(&*st, &file, 0, got.as_mut_ptr().cast(), file.length);
        assert_eq!(got, data, "the file as sent");
        if let Pending::Record = pending {
            let mut value = [0u8; 16];
            let n = sidou_sys::store_get(
                &*st,
                0x200,
                value.as_mut_ptr().cast(),
                16,
                std::ptr::null_mut(),
            );
            assert_eq!(
                (n, value),
                (16, [0x5a; 16]),
                "the record the PUT waited for"
            );
        }
    }

    if song {
        sound(&mut m, &fw, how);
    }
    hook_put(&mut m, &fw, 0x201, 0xa5);
    m.usb_frames(200);
    assert_ne!(writer_phase(&mut m, &fw), 0, "the record waits for quiet");
    let mut asked = m.ms();
    link.send(&mut m, 7, 100, &name16("USB WAITS"));
    if song {
        asked = waits_while_the_song_plays(&mut m, &fw, &mut link, 100, 2_000);
    }
    let (command, s, payload) = link.recv(&mut m, 2_000);
    assert_eq!(
        (command, s),
        (0x7e, 100),
        "{pending:?}, {how:?}: the DEL is accepted, not refused: {payload:02x?}"
    );
    let del = m.ms() - asked;
    println!("DEL while a record waits: ACK after {del} ms");
    assert!(del <= if song { 310 } else { 339 }, "the DEL took {del} ms");
    let mut flash = m.soc.flash.mem.clone();
    let st = common::mount_dump(&mut flash);
    let mut file = sidou_sys::store_file::default();
    let mut value = [0u8; 16];
    unsafe {
        assert_eq!(
            sidou_sys::store_file_open(&*st, c"USB WAITS".as_ptr(), &mut file),
            sidou_sys::STORE_NOT_FOUND,
            "deleted"
        );
        let n = sidou_sys::store_get(
            &*st,
            0x201,
            value.as_mut_ptr().cast(),
            16,
            std::ptr::null_mut(),
        );
        assert_eq!(
            (n, value),
            (16, [0xa5; 16]),
            "the record the DEL waited for"
        );
    }
}

#[test]
fn a_usb_put_with_keys_held_waits_out_a_collection_and_is_accepted() {
    put_while_the_store_waits(Pending::Collection, Sound::Keys);
}

#[test]
fn a_usb_put_with_keys_held_waits_out_a_record_and_is_accepted() {
    put_while_the_store_waits(Pending::Record, Sound::Keys);
}

#[test]
fn a_usb_put_with_a_song_playing_waits_out_a_collection_and_is_accepted() {
    put_while_the_store_waits(Pending::Collection, Sound::Song);
}

#[test]
fn a_usb_put_with_midi_notes_held_waits_out_a_collection_and_is_accepted() {
    put_while_the_store_waits(Pending::Collection, Sound::Notes);
}

fn logical_image(package: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..20 {
        out.extend_from_slice(&package[i * 0x30..i * 0x30 + 0x2F]);
    }
    out.extend_from_slice(&package[20 * 0x30..]);
    out
}

fn read_response(flags: u8, addr: u32, data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let mut body = vec![0, 0x59, 0x30, (n + 8) as u8, ((n + 8) >> 8) as u8, 0, flags];
    body.extend_from_slice(&addr.to_le_bytes());
    body.extend_from_slice(&[n as u8, (n >> 8) as u8, 0]);
    body.extend_from_slice(data);
    let sum = body[6..].iter().fold(0u8, |s, &b| s.wrapping_add(b));
    body.push(!sum);
    let mut sx = vec![0xF0];
    sx.extend(pack7(&body));
    sx.push(0xF7);
    sx
}

fn installer_step_one(m: &mut Machine, fw: &Firmware, image: &[u8]) -> (Vec<(u32, u32)>, bool) {
    const FINISH_CHECK: u32 = 0xE000_0000;
    const STEP_ONE_NAK_MS: u64 = 1_000;
    host(m).ep1_in.clear();
    UsbHost::send(&mut m.soc, &[0xF0, 0x22, 0x24, 0x35, 0x7F, 0xF7]);
    let mut served = Vec::new();
    let (mut taken, mut idle, mut naked) = (0usize, 0u64, 0u64);
    while idle < 8_000 {
        let sent = m.soc.usb.out_packets;
        m.usb_frames(1);
        idle += 1;
        if UsbHost::pending(&m.soc) == 0 || m.soc.usb.out_packets != sent {
            naked = 0;
        } else {
            naked += 1;
            assert!(
                naked <= STEP_ONE_NAK_MS,
                "the device NAKed the installer for {naked} ms after {} reads \
                 ({} packets unsent; pc in {})",
                served.len(),
                UsbHost::pending(&m.soc),
                fw.function_at(m.cpu.pc)
            );
        }
        assert!(
            m.resets.is_empty() && !m.reset_due(),
            "the device restarted ({:?}) after {} reads",
            m.resets,
            served.len()
        );
        let fresh = host(m).ep1_in.len();
        if fresh == taken {
            continue;
        }
        let r = replies(&host(m).ep1_in[taken..fresh]);
        taken = fresh;
        for d in &r {
            if d[0] != 0x30 || d.len() != 13 {
                continue;
            }
            let flags = d[4];
            let addr = u32::from_le_bytes([d[5], d[6], d[7], d[8]]);
            let n = u32::from_le_bytes([d[9], d[10], d[11], 0]);
            idle = 0;
            if addr == FINISH_CHECK {
                return (served, true);
            }
            assert!(
                n <= 512 && (addr + n) as usize <= image.len(),
                "a read outside the package: {addr:#x}+{n}"
            );
            m.usb_frames(10);
            let answer = read_response(flags, addr, &image[addr as usize..(addr + n) as usize]);
            UsbHost::send(&mut m.soc, &answer);
            served.push((addr, n));
        }
    }
    (served, false)
}

#[test]
fn the_running_firmware_serves_an_updates_whole_step_one() {
    const UPDATA_PARM: u32 = 0x01C7_FD88;
    const OTA_AREA: usize = 0xE0000;
    const OTA_RES: usize = 0xE4F00;
    let Some(fw) = firmware("the update's step 1") else {
        return;
    };
    let Some(build) = build_dir() else {
        return;
    };
    let package = std::fs::read(build.join("sidou.fwsc"))
        .expect("sidou.fwsc in the fm1 build directory: run sidou-pack (TESTING.md)");
    let image = logical_image(&package);
    let mut m = usb_machine(&fw);
    let (served, finished) = installer_step_one(&mut m, &fw, &image);
    println!("step 1: {} reads, {served:x?}", served.len());
    assert!(
        finished,
        "the device stopped asking after {} reads (pc in {})",
        served.len(),
        fw.function_at(m.cpu.pc)
    );
    let ota_off = served[2].0;
    assert_eq!(served[3].0, ota_off);
    let body = &served[4..];
    assert_eq!(body[0].0, ota_off + 0x20);
    for &(addr, n) in body {
        let at = OTA_AREA + 32 + (addr - ota_off - 0x20) as usize;
        assert_eq!(
            m.soc.flash.mem[at..at + n as usize],
            image[addr as usize..(addr + n) as usize],
            "the staged loader at {addr:#x}"
        );
    }
    assert_eq!(
        &m.soc.flash.mem[OTA_AREA + 16..OTA_AREA + 26],
        b"LOADER.BIN"
    );
    let answer = read_response(0, 0xE000_0000, b"success\0");
    m.stop_at_reset = true;
    send(&mut m, &sysex_packets(&answer));
    let identity = b"ota-FM-1_015";
    let mut ms = 0;
    while (0..12u32).any(|i| m.soc.read8(UPDATA_PARM + 8 + i) != identity[i as usize]) {
        assert!(!m.reset_due(), "a reset before the UPDATA_PARM record");
        assert!(ms < 2_000, "no UPDATA_PARM record 2 s after the success");
        m.usb_frames(1);
        ms += 1;
    }
    assert_eq!(&m.soc.flash.mem[OTA_RES + 8..OTA_RES + 20], identity);
    m.usb_frames(50);
    let pc = m.cpu.pc;
    let at = fw.function_at(pc);
    m.usb_frames(10);
    println!("after the record: the core waits at {pc:#x} ({at})");
    assert_eq!(m.cpu.pc, pc, "the core left the reset's wait");
    assert!(m.resets.is_empty() && !m.reset_due(), "{:?}", m.resets);
    let (user, system) = m.stack_use();
    println!("step 1's stacks: user {user}, system {system}");
}

fn foreign_sectors(flash: &mut [u8], secs: &[usize]) {
    let mut x = 0x1234_5678u32;
    for &s in secs {
        let s = &mut flash[s..s + 0x1000];
        s.fill(0xFF);
        s[..4].copy_from_slice(b"FELU");
        for b in &mut s[0x100..] {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *b = x as u8;
        }
    }
}

fn first_identity(m: &mut Machine, fw: &Firmware, limit_ms: u64) -> (u64, u64) {
    let mut attached = None;
    while !UsbHost::configured(&m.soc) {
        assert!(
            m.ms() < limit_ms,
            "not configured in {limit_ms} ms (attached at {attached:?}): {:?}",
            m.soc.usb.events
        );
        if attached.is_none() && up_and_config(m, fw).0 == 1 {
            attached = Some(m.ms());
        }
        m.usb_frames(1);
    }
    let attached = attached.expect("USB attached");
    loop {
        assert!(m.ms() < limit_ms, "no identity reply in {limit_ms} ms");
        host(m).ep1_in.clear();
        send(m, &sysex_packets(&message(0x11, &[])));
        for _ in 0..1_000 {
            m.usb_frames(1);
            if replies(&host(m).ep1_in).iter().any(|r| r[0] == 0x11) {
                return (attached, m.ms());
            }
        }
    }
}

const BOOT_WORK_MS: f64 = 100.0;
const STORE_LO: usize = 0x97000;

#[test]
fn a_first_boot_over_another_firmwares_sectors_answers_usb_at_once() {
    let Some(fw) = firmware("the first boot over another firmware's sectors") else {
        return;
    };
    let mut flash = common::settled_flash(&fw.image);
    let secs: Vec<usize> = (33..73).map(|k| STORE_LO + k * 0x1000).collect();
    foreign_sectors(&mut flash, &secs);
    let state = fw.symbol("store_fm1_store") + sidou_sys::ilp32::store::state as u32;
    for (erase_ms, prog_ms) in [(20u64, 3u64), (500, 10)] {
        let mut m = Machine::boot_blank(&fw.image);
        m.soc.flash.mem.copy_from_slice(&flash);
        let ms = |m: &Machine, n: u64| m.soc.periph.cycle_of_ms(n);
        m.masked_stalls = vec![
            (fw.symbol("fl_prog_ram"), ms(&m, prog_ms)),
            (fw.symbol("fl_erase4k_ram"), ms(&m, erase_ms)),
        ];
        m.watch_calls(fw.symbol("boot_feed"));
        m.plug_usb();
        let (attached, answered) = first_identity(&mut m, &fw, 30_000);
        let hz = m.soc.periph.hz as f64 / 1e3;
        let feeds = m.watched_starts.clone();
        let work = (feeds[feeds.len() - 1] - feeds[0]) as f64 / hz;
        let gap = feeds.windows(2).map(|w| w[1] - w[0]).max().unwrap_or(0) as f64 / hz;
        let left = secs
            .iter()
            .filter(|&&s| m.soc.flash.mem[s..s + 4] == *b"FELU")
            .count();
        println!(
            "erase {erase_ms} ms, program {prog_ms} ms: USB attached at {attached} ms, the \
             identity answered at {answered} ms; the boot's flash work {work:.0} ms ({} feeds, \
             the longest without one {gap:.0} ms); {left} of {} foreign sectors left to \
             reclaim",
            feeds.len(),
            secs.len()
        );
        assert!(
            answered <= 5_000,
            "the identity answered {answered} ms after power-on"
        );
        assert!(
            attached <= 1_000,
            "USB attached {attached} ms after power-on"
        );
        assert!(
            work <= BOOT_WORK_MS + erase_ms as f64 + 1.0,
            "the boot's flash work took {work:.0} ms in one stretch"
        );
        assert!(
            gap <= erase_ms as f64 + 1.0,
            "{gap:.0} ms between two watchdog feeds at boot"
        );
        assert!(
            m.resets.is_empty(),
            "the watchdog reset the chip: {:?}",
            m.resets
        );
        assert!(left > 0, "the boot reclaimed every sector itself");
        assert_eq!(m.soc.read8(state), sidou_sys::STORE_STATE_SIDOU as u8);
        if erase_ms == 20 {
            let end = m.ms() + 20_000;
            while secs.iter().any(|&s| m.soc.flash.mem[s..s + 4] == *b"FELU") {
                assert!(
                    m.ms() < end,
                    "the other firmware's sectors were never reclaimed"
                );
                m.usb_frames(10);
            }
            println!("reclaimed by {} ms", m.ms());
            hook_put(&mut m, &fw, 0x20, 0x5A);
            let mut f = m.soc.flash.mem.clone();
            for _ in 0..200 {
                if common::stored_value(&f, 0x20).is_some() {
                    break;
                }
                m.usb_frames(10);
                f = m.soc.flash.mem.clone();
            }
            assert_eq!(common::stored_value(&f, 0x20), Some(vec![0x5A; 16]));
            assert!(common::stored_value(&f, sidou_sys::SETTINGS_KEY).is_some());
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Uac {
    alt: u8,
    r48: u8,
    go: u8,
    flowing: u8,
    pkts: u32,
    underruns: u32,
    overruns: u32,
    missed: u32,
    stalls: u32,
    fill_lo: u32,
    fill_hi: u32,
    rate_sets: u32,
}

fn uac(m: &mut Machine, fw: &Firmware) -> Uac {
    let a = fw.symbol("uac");
    let w = |m: &mut Machine, i: u32| m.soc.read32(a + 12 + 4 * i);
    Uac {
        alt: m.soc.read8(a),
        r48: m.soc.read8(a + 1),
        go: m.soc.read8(a + 3),
        flowing: m.soc.read8(a + 4),
        pkts: w(m, 4),
        underruns: w(m, 6),
        overruns: w(m, 7),
        missed: w(m, 8),
        stalls: w(m, 9),
        fill_lo: w(m, 12),
        fill_hi: w(m, 13),
        rate_sets: w(m, 14),
    }
}

fn hand_over(m: &mut Machine, fw: &Firmware) -> (u8, u8, u32, u32) {
    let a = fw.symbol("usb_audio_in") + 1024;
    (
        m.soc.read8(a),
        m.soc.read8(a + 1),
        m.soc.read32(a + 4),
        m.soc.read32(a + 8),
    )
}

fn record(m: &mut Machine, hz: u32) {
    m.soc.usb.audio_record(hz);
    host(m).replug();
    let packets = m.soc.usb.audio_packets;
    for _ in 0..2_000 {
        m.usb_frames(1);
        if m.soc.usb.audio_packets > packets {
            break;
        }
    }
    assert_eq!(
        m.soc.usb.audio_rate, hz,
        "recording at {hz}: {:?}",
        m.soc.usb.events
    );
    host(m).ep4_in.clear();
}

fn ep4_empty(m: &Machine) -> u64 {
    m.soc.usb.audio_underruns
}

fn frames(packets: &[Vec<u8>]) -> Vec<(i16, i16)> {
    packets
        .concat()
        .chunks(4)
        .map(|f| {
            (
                i16::from_le_bytes([f[0], f[1]]),
                i16::from_le_bytes([f[2], f[3]]),
            )
        })
        .collect()
}

fn packet_sizes(packets: &[Vec<u8>]) -> (usize, usize, f64) {
    let n: Vec<usize> = packets.iter().map(|p| p.len() / 4).collect();
    let mean = n.iter().sum::<usize>() as f64 / n.len() as f64;
    (*n.iter().min().unwrap(), *n.iter().max().unwrap(), mean)
}

fn rms(x: impl Iterator<Item = i16>) -> f64 {
    let (mut s, mut n) = (0.0, 0usize);
    for v in x {
        s += f64::from(v) * f64::from(v);
        n += 1;
    }
    (s / n as f64).sqrt()
}

const A4_KEY: u8 = 30;

#[test]
fn a_host_records_the_dacs_output_over_usb_audio_at_44_1_and_48_khz() {
    let Some(fw) = firmware("the USB audio test") else {
        return;
    };
    assert_eq!(fw.symbol("ep4tx") % 8, 0, "ep4tx at 4 mod 8");
    let mut m = usb_machine(&fw);
    assert_eq!(hand_over(&mut m, &fw).0, 0, "no host records yet");
    m.key(A4_KEY, true);
    m.usb_frames(300);

    record(&mut m, 44_100);
    let empty = ep4_empty(&m);
    m.usb_frames(1000);
    let u = uac(&mut m, &fw);
    let packets = host(&mut m).ep4_in.clone();
    let (lo, hi, mean) = packet_sizes(&packets[100..]);
    println!(
        "44.1 kHz: {} packets, {lo}..{hi} frames (mean {mean:.4}); fill {}..{}; {:?}",
        packets.len(),
        u.fill_lo,
        u.fill_hi,
        hand_over(&mut m, &fw)
    );
    assert_eq!((u.alt, u.r48, u.go, u.flowing), (1, 0, 1, 1));
    assert!(
        packets.len() >= 995,
        "{} packets in 1000 frames",
        packets.len()
    );
    assert!(lo >= 43 && hi <= 46, "packet sizes {lo}..{hi}");
    assert!((mean - 44.1).abs() < 0.1, "mean packet {mean}");
    assert_eq!(
        (u.underruns, u.overruns, u.stalls, u.missed),
        (0, 0, 0, 0),
        "a glitch at 44.1 kHz"
    );
    assert_eq!(ep4_empty(&m), empty, "an IN token found no packet");
    assert_eq!(hand_over(&mut m, &fw).3, 0, "a block skipped");
    let usb = frames(&packets);
    let dac: Vec<(i16, i16)> = m.left().into_iter().zip(m.right()).collect();
    let first = usb
        .iter()
        .position(|&f| f != (0, 0))
        .expect("silence over USB");
    let probe = &usb[first..first + 256];
    let at = dac
        .windows(256)
        .position(|w| w == probe)
        .expect("the recording is not the DAC's output");
    let same = usb[first..]
        .iter()
        .zip(&dac[at..])
        .take_while(|(a, b)| a == b)
        .count();
    println!(
        "44.1 kHz: {} frames recorded, {same} of them the DAC's in order from frame {first}",
        usb.len()
    );
    assert!(
        same + first + 64 >= usb.len(),
        "only {same} frames match the DAC"
    );
    assert!(same > 40_000);
    assert!(
        rms(usb[first..].iter().map(|f| f.0)) > 500.0,
        "no sound recorded"
    );

    record(&mut m, 48_000);
    let u48_from = uac(&mut m, &fw);
    let empty = ep4_empty(&m);
    let dac_from = m.left().len();
    m.usb_frames(1000);
    let u48 = uac(&mut m, &fw);
    let packets = host(&mut m).ep4_in.clone();
    let (lo, hi, mean) = packet_sizes(&packets[100..]);
    let usb = frames(&packets[100..]);
    let dac = m.left();
    let (level, dac_level) = (
        rms(usb.iter().map(|f| f.0)),
        rms(dac[dac_from + 4410..].iter().copied()),
    );
    println!(
        "48 kHz: {} packets, {lo}..{hi} frames (mean {mean:.4}); fill {}..{}; level {level:.0} \
         against the DAC's {dac_level:.0}",
        packets.len(),
        u48.fill_lo,
        u48.fill_hi
    );
    assert_eq!((u48.r48, u48.rate_sets, u48.go), (1, u.rate_sets + 1, 1));
    assert!(lo >= 47 && hi <= 49, "packet sizes {lo}..{hi}");
    assert!((mean - 48.0).abs() < 0.1, "mean packet {mean}");
    assert!(
        (level / dac_level - 1.0).abs() < 0.03,
        "48 kHz level {level:.0}, the DAC's {dac_level:.0}"
    );
    assert_eq!(
        (
            u48.underruns - u48_from.underruns,
            u48.overruns - u48_from.overruns,
            u48.stalls - u48_from.stalls,
            u48.missed - u48_from.missed
        ),
        (0, 0, 0, 0),
        "a glitch at 48 kHz"
    );
    assert!(u48.pkts >= 995, "{} packets counted at 48 kHz", u48.pkts);
    assert_eq!(ep4_empty(&m), empty, "an IN token found no packet");
    assert_eq!(hand_over(&mut m, &fw).3, 0, "a block skipped");

    host(&mut m).replug();
    let unplugged = m.ms();
    while uac(&mut m, &fw).alt != 0 {
        m.usb_frames(1);
        assert!(m.ms() - unplugged < 1_000, "no bus reset");
    }
    host(&mut m).ep4_in.clear();
    m.usb_frames(15);
    assert!(host(&mut m).ep4_in.is_empty(), "packets after alt 0");
    let (want, left, ..) = hand_over(&mut m, &fw);
    assert_eq!((want, left), (0, 0), "the hand-over goes on after alt 0");
}

#[test]
fn recording_over_usb_audio_costs_the_audio_interrupt_little() {
    let Some(fw) = firmware("the USB audio cost test") else {
        return;
    };
    let chord = |hz: Option<u32>, sie: Option<u64>| {
        let mut m = usb_machine_sie(&fw, sie);
        if let Some(hz) = hz {
            record(&mut m, hz);
        }
        for k in [18, 25, 37] {
            m.key(k, true);
        }
        m.usb_frames(1500);
        m
    };
    let measure = |m: &mut Machine, ms: u64| {
        let (from, isr) = (m.alnk_blocks.len(), m.isr_steps);
        m.usb_frames(ms);
        let b = &m.alnk_blocks[from..];
        let samples = b.len() as f64 * 128.0;
        (
            b.iter().sum::<u64>() as f64 / samples,
            (m.isr_steps - isr) as f64 / samples,
        )
    };
    let mean =
        |v: &[(f64, f64)], f: fn(&(f64, f64)) -> f64| v.iter().map(f).sum::<f64>() / v.len() as f64;
    for (sie, what, bound) in [
        (Some(1), "the SIE at once", USB_AUDIO_PER_SAMPLE),
        (None, "fm1-emu's SIE (1 µs)", SIE_MODEL_PER_SAMPLE),
    ] {
        let mut quiet = chord(None, sie);
        let off: Vec<(f64, f64)> = (0..3).map(|_| measure(&mut quiet, 400)).collect();
        for hz in [44_100, 48_000] {
            let mut m = chord(Some(hz), sie);
            let on: Vec<(f64, f64)> = (0..3).map(|_| measure(&mut m, 400)).collect();
            let alnk = mean(&on, |x| x.0) - mean(&off, |x| x.0);
            let all = mean(&on, |x| x.1) - mean(&off, |x| x.1);
            println!(
                "{what}, {hz} Hz: the audio interrupt {:.2} recording, {:.2} not: +{alnk:.2} a \
                 sample; every interrupt +{all:.2} a sample ({on:?} / {off:?})",
                mean(&on, |x| x.0),
                mean(&off, |x| x.0)
            );
            let (user, system) = m.stack_use();
            println!("stacks: user {user} bytes, system {system}");
            assert!(
                alnk > 0.0,
                "{what}, {hz} Hz: recording costs nothing: is it running?"
            );
            assert!(
                alnk <= bound,
                "{what}, {hz} Hz: recording adds {alnk:.2} instructions a sample to the audio \
                 interrupt (at most {bound})"
            );
        }
    }
}

const SIE_MODEL_PER_SAMPLE: f64 = 3.0;
