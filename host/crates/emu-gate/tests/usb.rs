// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! USB-MIDI end to end in fm1-emu, with a host and the device controller modelled in the test
//! crate (common::usb_host; fm1-emu itself has no USB controller). The host enumerates the
//! device, asks for its identity as Felucca's installer does (the update protocol's handshake,
//! cmd 0x11), and reads EP1 IN while K1 and PRESETS turn. On v0.2.0-m2-preview the device received
//! all of this and a host saw almost none of what it sent (docs/hardware-test.md, section 8).
//! What this checks is the firmware's side, as the USB fix made it:
//! - it presents as "SIDOU FM-1" by "SIDOU contributors" (usb.c's string hook);
//! - every IN packet carries at most 64 bytes, whole event packets (Felucca's src/usb.c ep1_tx,
//!   SysEx first);
//! - no SIE access happens while an audio interrupt is open: USB's poll waits for the render's
//!   end (main.c fm1_timer5_irq), as Felucca's does;
//! - while the host has the device configured the main loop never sleeps in `idle`: it spins
//!   to the next tick (main.c wait_for_interrupt), so it runs nearly every cycle the
//!   interrupts leave it; with no host it sleeps as before.
//!
//! The chip's own behaviour (the DMA against a sleeping core, the host's IN tokens) is not
//! modelled; only the device can confirm the fix (docs/hardware-test.md).

mod common;

use common::usb_host::UsbHost;
use common::{build_dir, run_past_boot, skip_message, Firmware, Machine};
use fm1_core::Bus;

/// Felucca's ids: K1 (cutoff, CC 74) and PRESETS (the preset, a program change).
const K1: u8 = 2;
const PRESETS: u8 = 6;
const CC_CUTOFF: u8 = 74;
/// The identity SIDOU's development builds answer (CMakeLists.txt SIDOU_PRODUCT).
const IDENTITY: &str = "FM-1_900";
/// src/usb.c ep1_tx: one full-speed bulk packet.
const IN_MAX: usize = 64;
/// The config descriptor's length (src/usb.c CFG_LEN_PLAIN).
const CONFIG_LEN: usize = 101;

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

fn host(m: &mut Machine) -> &mut UsbHost {
    m.usb.as_mut().expect("a host")
}

/// src/usb.c's `usb`: up, config, ...
fn up_and_config(m: &mut Machine, fw: &Firmware) -> (u8, u8) {
    let usb = fw.symbol("usb");
    (m.soc.read8(usb), m.soc.read8(usb + 1))
}

fn setup(m: &mut Machine, pkt: [u8; 8]) {
    let usb = m.usb.as_mut().expect("a host");
    usb.setup(&mut m.soc, pkt);
    m.usb_frames(3);
}

/// Bus reset, address 7, the device and config descriptors, configuration 1.
fn enumerate(m: &mut Machine, fw: &Firmware) {
    host(m).bus_reset();
    m.usb_frames(3);
    setup(m, [0x00, 0x05, 0x07, 0, 0, 0, 0, 0]);
    host(m).in0.clear();
    setup(m, [0x80, 0x06, 0x00, 0x01, 0, 0, 0x12, 0]);
    let dev = host(m).in0.concat();
    assert_eq!(dev.len(), 18, "the device descriptor: {dev:02x?}");
    assert_eq!(&dev[8..12], &[0x09, 0x12, 0x01, 0x00], "VID/PID 1209:0001");
    host(m).in0.clear();
    setup(m, [0x80, 0x06, 0x00, 0x02, 0, 0, 0xFF, 0]);
    m.usb_frames(3);
    let cfg = host(m).in0.concat();
    assert_eq!(cfg.len(), CONFIG_LEN, "the config descriptor over EP0");
    assert_eq!(host(m).in0[0].len(), 64, "in 64-byte data packets");
    // The strings: SIDOU's, through usb.c's string hook (platform/fm1/VENDORED.md).
    for (index, text) in [(1u8, "SIDOU contributors"), (2, "SIDOU FM-1")] {
        host(m).in0.clear();
        setup(m, [0x80, 0x06, index, 0x03, 0x09, 0x04, 0xFF, 0]);
        m.usb_frames(3);
        let d = host(m).in0.concat();
        let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(
            d[..2],
            [2 + utf16.len() as u8, 3],
            "string {index}: {d:02x?}"
        );
        assert_eq!(d[2..], utf16[..], "string {index}");
    }
    setup(m, [0x00, 0x09, 0x01, 0, 0, 0, 0, 0]);
    assert_eq!(host(m).address(), 7);
    assert_eq!(host(m).stalls, 0);
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

/// A SysEx message as USB-MIDI event packets (CIN 4 continues, 5..7 end).
fn sysex_packets(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, c) in b.chunks(3).enumerate() {
        let last = (i + 1) * 3 >= b.len();
        out.push(if last { 4 + c.len() as u8 } else { 4 });
        out.extend((0..3).map(|j| c.get(j).copied().unwrap_or(0)));
    }
    out
}

/// The update protocol's message (src/ota.c ota_send_msg's layout) as a SysEx.
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
    for chunk in bytes.chunks(64) {
        loop {
            let usb = m.usb.as_mut().expect("a host");
            if usb.out1(&mut m.soc, chunk) {
                break;
            }
            m.usb_frames(1);
        }
    }
}

/// The update protocol's messages in what the host has read, decoded, each from cmd on.
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

/// The non-SysEx event packets the host has read.
fn events(in1: &[Vec<u8>]) -> Vec<[u8; 4]> {
    in1.concat()
        .chunks(4)
        .filter(|p| !(4..=7).contains(&(p[0] & 15)))
        .map(|p| [p[0], p[1], p[2], p[3]])
        .collect()
}

/// The main loop's share of the cycles over `ms` (common::Machine counts its instructions,
/// one a cycle; an `idle` skips cycles without any).
fn main_share(m: &mut Machine, ms: u64) -> f64 {
    let (main, cycles) = (m.main_steps, m.soc.periph.cycles);
    m.usb_frames(ms);
    (m.main_steps - main) as f64 / (m.soc.periph.cycles - cycles) as f64
}

#[test]
fn a_host_gets_the_identity_and_the_knobs_over_usb() {
    let Some(fw) = fixture("the USB test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.usb = Some(UsbHost::default());
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

    // The handshake, as fm1_install.py --info sends it.
    host(&mut m).in1.clear();
    send(&mut m, &sysex_packets(&message(0x11, &[])));
    m.usb_frames(50);
    let in1 = host(&mut m).in1.clone();
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
    // Whole event packets, at most one bulk packet each: the 56-byte reply goes in one, as
    // Felucca sends it (the device answers it so: docs/hardware-results/usb-out-diagnosis.md).
    assert!(sizes.iter().all(|&n| n > 0 && n <= IN_MAX && n % 4 == 0));

    // K1: CC 74 on channel 1.
    host(&mut m).in1.clear();
    m.turn(K1, 3);
    m.usb_frames(60);
    let ev = events(&host(&mut m).in1);
    println!("K1: {ev:02x?}");
    assert!(
        !ev.is_empty() && ev.iter().all(|p| p[..3] == [0x0B, 0xB0, CC_CUTOFF]),
        "K1 sent no CC 74 alone"
    );
    // PRESETS: a program change.
    host(&mut m).in1.clear();
    m.turn(PRESETS, 1);
    m.usb_frames(60);
    let ev = events(&host(&mut m).in1);
    println!("PRESETS: {ev:02x?}");
    assert!(
        ev.iter().any(|p| p[..2] == [0x0C, 0xC0]),
        "PRESETS sent no program change"
    );
    assert!(host(&mut m).in1.iter().all(|p| p.len() <= IN_MAX));

    let usb = m.usb.as_ref().expect("a host");
    println!(
        "{} SIE accesses, {} inside an audio interrupt",
        usb.sie_accesses, usb.sie_in_audio
    );
    assert!(usb.sie_accesses > 0);
    assert_eq!(
        usb.sie_in_audio, 0,
        "USB polled nested in the audio interrupt"
    );
}

#[test]
fn an_upgrade_request_gets_the_first_read_request_out() {
    let Some(fw) = fixture("the USB test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.usb = Some(UsbHost::default());
    run_past_boot(&mut m, &fw);
    m.usb_frames(20);
    enumerate(&mut m, &fw);
    host(&mut m).in1.clear();
    // The upgrade request (src/usb.c sysex_byte), then step 1's first read request (cmd 0x30).
    send(
        &mut m,
        &sysex_packets(&[0xF0, 0x22, 0x24, 0x35, 0x7F, 0xF7]),
    );
    m.usb_frames(100);
    let in1 = host(&mut m).in1.clone();
    let r = replies(&in1);
    println!("after the upgrade request: {:02x?}", r);
    assert!(
        r.iter().any(|d| d[0] == 0x30),
        "no read request reached the host"
    );
    assert!(in1.iter().all(|p| p.len() <= IN_MAX));
    assert_eq!(m.usb.as_ref().expect("a host").sie_in_audio, 0);
}

/// SIDOU's frames in what the host read (real-time bytes aside), each from F0 to F7; panics on
/// a frame cut by another or left open.
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

// ---- SIDOU's SysEx file transfers over USB, timed (docs/reference/sysex.md) ----

/// SIDOU's frame: `F0 7D 53 44 cmd seq packed checksum F7` (app/sysex.c's packing: a mask of
/// the high bits, then up to seven low-seven-bit bytes).
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

/// A host on the far end of the cable, stop-and-wait as tools/sysex-roundtrip: SIDOU's reply
/// frames as they come out of EP1 IN, decoded.
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

    /// The next SIDOU frame: (command, sequence, decoded payload); one USB frame (1 ms) at a
    /// time, at most `timeout` ms.
    fn recv(&mut self, m: &mut Machine, timeout: u64) -> (u8, u8, Vec<u8>) {
        let start = m.ms();
        loop {
            let in1 = &host(m).in1;
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

/// A USB host with the device enumerated and the song I/O settled after the boot.
fn usb_machine(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    m.usb = Some(UsbHost::default());
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
    host(&mut m).in1.clear();
    m
}

/// PUT then GET of `data` as a named file (stop-and-wait, 256-byte DATA packets, an ACK
/// each), as tools/sysex-roundtrip runs it on the device. Returns the emulated ms a DATA packet
/// and its ACK take, on average, up (PUT) and down (GET).
fn name16(name: &str) -> Vec<u8> {
    (0..16)
        .map(|i| name.as_bytes().get(i).copied().unwrap_or(0))
        .collect()
}

/// PUT `data` as a named file (type 2), stop-and-wait from sequence 10; returns the metadata
/// and the emulated ms a DATA packet and its ACK took, on average.
fn put_file(m: &mut Machine, link: &mut Link, name: &str, data: &[u8]) -> (Vec<u8>, f64) {
    let mut meta = vec![0, 0];
    meta.extend(name16(name));
    meta.push(2);
    meta.extend((data.len() as u32).to_le_bytes());
    // SAFETY: data is valid for its length; the CRC routine does not retain it.
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

/// PUT then GET of `data` as a named file (stop-and-wait, 256-byte DATA packets, an ACK
/// each), as tools/sysex-roundtrip runs it on the device. Returns the emulated ms a DATA packet
/// and its ACK take, on average, up (PUT) and down (GET).
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

/// SysEx runs in the store's share of a pass (store_fm1_service, emu-gate store.rs's
/// STORE_POLL_WORST), a whole reply frame a poll as far as USB's ring takes it.
const STORE_POLL_WORST: u64 = 30_000;

/// A 4 KiB PUT and GET over USB with the production store (no write windows: every program a
/// held one); returns the emulated ms a DATA packet and its ACK take, up and down, and checks
/// the store's share and the passes stay within their bounds meanwhile.
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

/// SysEx over USB moves a 256-byte packet in a few ms of USB time (101 event packets, 16 to a
/// 64-byte IN packet). On flash day (docs/hardware-results/2026-10-22-flash-day.md) the device
/// took about 280 ms per DATA and its ACK up and 200 ms down, as fm1-emu did (293.5 and 205.5
/// ms here): the reply went out an event packet a SysEx poll (about 500 a second), and every
/// program was a hold of its own, STORE_FORCE_GAP_MS (100 ms) after the last. Now a reply goes
/// out whole, as fast as USB's ring takes it, and a packet's programs share one hold, which
/// stays on between packets while the device is silent (the first 500 ms of a PUT find that
/// out). Bounds: the measurement (42.4 up, 10.3 down) plus at most 10 %.
#[test]
fn a_silent_usb_put_and_get_move_a_256_byte_packet_quickly() {
    let Some(fw) = fixture("the SysEx USB timing test") else {
        return;
    };
    let (up, down) = timed_round_trip(&fw, "USB TIMING", false);
    assert!(up <= 46.0, "PUT: {up:.1} ms a packet");
    assert!(down <= 11.0, "GET: {down:.1} ms a packet");
}

/// The same while a chord sounds: a packet's programs share one hold, ramped down and back up,
/// and the holds stay STORE_FORCE_GAP_MS apart, so the chord plays between packets. Before:
/// 292.6 ms up, 205.6 down. Bounds: the measurement (105.7 up, 10.4 down) plus at most 10 %.
#[test]
fn a_sounding_usb_put_keeps_its_gaps_with_one_hold_a_packet() {
    let Some(fw) = fixture("the SysEx USB timing test") else {
        return;
    };
    let (up, down) = timed_round_trip(&fw, "USB CHORD", true);
    assert!(up <= 116.0, "PUT: {up:.1} ms a packet");
    assert!(down <= 11.0, "GET: {down:.1} ms a packet");
}

/// A GET of `name` (256 bytes or more), and the main loop until its first DATA reply (101
/// event packets, more than USB's ring of 64 holds) is part way out on the wire.
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

/// A file for the arbitration tests: one 256-byte DATA reply.
fn reply_file() -> Vec<u8> {
    (0..256u32).map(|i| (i * 7) as u8).collect()
}

/// The update protocol's reply cannot split a SIDOU reply that is part way out: a GET's DATA,
/// 101 event packets, more than USB's ring holds (a reply goes out whole as the ring takes it,
/// so a short one such as INQ's is never part way).
#[test]
fn an_updater_identity_reply_cannot_overtake_sidous_queued_end_packet() {
    let Some(fw) = fixture("SysEx producer arbitration") else {
        return;
    };
    let mut m = usb_machine(&fw);
    put_file(&mut m, &mut Link::default(), "ARBITRATION", &reply_file());
    host(&mut m).in1.clear();
    get_part_way(&mut m, &fw, &mut Link::default(), "ARBITRATION", 20);
    send(&mut m, &sysex_packets(&message(0x11, &[])));
    m.usb_frames(100);
    let packets = host(&mut m).in1.clone();
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

/// A gap in SIDOU's MIDI-in ring that lost `sources`' input (MIDI_SRC_* bits), as the
/// producer leaves it (app/midi_ring.c lose).
fn ring_gap(m: &mut Machine, fw: &Firmware, sources: u8) {
    let ring = fw.symbol("in_ring");
    let epoch = ring + sidou_sys::ilp32::midi_ring::epoch as u32;
    let e = m.soc.read8(epoch);
    m.soc.write8(epoch, e.wrapping_add(1));
    m.soc
        .write8(ring + sidou_sys::ilp32::midi_ring::in_gap as u32, sources);
}

/// Lost input aborts only the SysEx transfer of the port that lost it, and never leaves a reply
/// frame open on the wire (review 4.1): a TRS gap while a USB GET's DATA goes out leaves it
/// whole and the transfer going on; a USB gap lets it finish to its F7, then NAKs ABORTED (9);
/// the next request is answered. Before, either gap reset SysEx mid-frame: the reply stopped
/// after its first packets with no F7.
#[test]
fn a_ring_gap_aborts_only_its_ports_sysex_and_never_cuts_a_reply() {
    let Some(fw) = fixture("SysEx across a MIDI-in gap") else {
        return;
    };
    let mut m = usb_machine(&fw);
    let data = reply_file();
    put_file(&mut m, &mut Link::default(), "GAP", &data);
    let restarts = fw.symbol("midi_stats") + 8;
    let before = m.soc.read32(restarts);

    host(&mut m).in1.clear();
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
    assert_eq!(sidou_frames(&host(&mut m).in1).len(), 3);

    host(&mut m).in1.clear();
    get_part_way(&mut m, &fw, &mut Link::default(), "GAP", 40);
    ring_gap(&mut m, &fw, sidou_sys::MIDI_SRC_USB as u8);
    m.usb_frames(100);
    let frames = sidou_frames(&host(&mut m).in1);
    assert_eq!(frames.len(), 3, "the metadata, the DATA, then a NAK");
    assert_eq!(&frames[1][4..6], &[5, 41], "the DATA, whole to its F7");
    assert_eq!(frames[1].len(), 8 + 256 + 37);
    assert_eq!(frames[2][4], 0x7f, "a NAK");
    assert_eq!(frames[2][7], sidou_sys::SYSEX_ABORTED as u8, "ABORTED");

    host(&mut m).in1.clear();
    send(&mut m, &sysex_packets(&sd_frame(1, 2, &[])));
    m.usb_frames(100);
    let frames = sidou_frames(&host(&mut m).in1);
    assert_eq!(frames.len(), 1, "the next INQ is answered: {frames:02x?}");
    assert_eq!(&frames[0][..6], &[0xf0, 0x7d, 0x53, 0x44, 1, 2]);
}

/// Whether clock out's song position (CIN 3, F2) went inside a SysEx frame among the host's
/// IN packets (after an F0, before its F7), real-time bytes aside (they may go anywhere); None
/// if none went.
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

/// CLOCK OUT's song position never lands inside a SysEx reply (review 4.2): the song stopped
/// mid-way, a GET's DATA part way out on USB, then EDIT + PLAY (go on from where it stopped),
/// which sends a song position and a continue. SPP is system common: inside the reply it ends
/// the host's frame. It now waits for the frame's F7. The continue is tried at a range of
/// times against the reply, so at least one of them falls while the reply is part way out.
#[test]
fn clock_outs_song_position_waits_for_a_sysex_reply_to_end() {
    let Some(fw) = fixture("clock out against a SysEx reply") else {
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
    m.usb_frames(1_500);
    tap(&mut m, &[play]);
    m.usb_frames(300);
    let mut held = 0;
    for delay in 0..16u64 {
        host(&mut m).in1.clear();
        let mut link = Link::default();
        let mut get = vec![0, 0];
        get.extend(name16("SPP"));
        link.send(&mut m, 3, 30, &get);
        let (command, s, _) = link.recv(&mut m, 2_000);
        assert_eq!((command, s), (4, 30), "GET's metadata");
        m.key(edit, true);
        m.key(play, true);
        m.usb_frames(delay);
        link.send(&mut m, 0x7e, 30, &[]);
        // a ms at a time: does the song play on while the song position waits for the reply?
        let out_state = syncing + sidou_sys::ilp32::midi_sync::out_state as u32;
        let waiting = syncing + sidou_sys::ilp32::midi_sync::out_common_wait as u32;
        let calls = fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::calls as u32;
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
        let inside = spp_inside(&host(&mut m).in1);
        println!("continue {delay} ms before the DATA's ACK: song position inside {inside:?}");
        assert_eq!(
            inside,
            Some(false),
            "the song position landed inside a SysEx reply (or none went)"
        );
        tap(&mut m, &[play]); // stop again
        m.usb_frames(100);
    }
    println!("{held} of the continues came while the DATA was part way out");
    assert!(held > 0, "no continue came while a reply was part way out");
}

/// The clock follower's fit (app/midi_clock.h): its clocks' (number, stamp) in samples, oldest
/// first, and the largest distance of one from the least-squares line through them all, in ms.
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

/// A flash operation with interrupts masked stamps the TRS clocks that arrive meanwhile late,
/// together, at the first poll after it; the clock follower leaves them out of its fit (review
/// 4.4: midi_clock_hold had no caller, so they went in, and the phase drifted). A 120 BPM clock
/// on TRS, followed, while a USB PUT stores a file; every flash program and erase is made to
/// last 30 ms with interrupts masked, as a sector erase does on the chip (fm1-emu's flash is
/// instant). Afterwards no clock in the fit is more than the TRS poll's jitter off its line.
#[test]
fn clocks_stamped_late_by_a_flash_hold_stay_out_of_the_clocks_fit() {
    let Some(fw) = fixture("the clock follower across flash holds") else {
        return;
    };
    let mut m = usb_machine(&fw);
    let per = 60.0 / 120.0 / 24.0 * m.soc.periph.hz as f64;
    let start = m.soc.periph.cycles as f64;
    m.trs_at = (0..24 * 2 * 8)
        .map(|i| ((start + f64::from(i) * per) as u64, 0xf8))
        .collect();
    m.usb_frames(3_000);
    let (n, before) = fit_residual(&mut m, &fw);
    println!("before the PUT: {n} clocks in the fit, the worst {before:.2} ms off its line");
    assert!(n >= 90 && before < 0.6, "the fit before the holds");
    let hold = m.soc.periph.cycle_of_ms(30);
    m.stalls = vec![
        (fw.symbol("flash_store_prog"), hold),
        (fw.symbol("flash_store_erase"), hold),
    ];
    let data: Vec<u8> = (0..1024u32).map(|i| (i * 13) as u8).collect();
    put_file(&mut m, &mut Link::default(), "HOLDS", &data);
    m.stalls.clear();
    m.usb_frames(100);
    let (n, after) = fit_residual(&mut m, &fw);
    println!(
        "after {} holds of 30 ms: {n} clocks in the fit, the worst {after:.2} ms off its line",
        m.stalled
    );
    assert!(m.stalled >= 4, "{} holds", m.stalled);
    assert!(after < 0.6, "a clock {after:.2} ms off the fit's line");
}

/// A host that floods EP1 OUT with a foreign SysEx dump as fast as the device takes it (a
/// packet again as soon as the last one was read: two a millisecond, at USB's 2 kHz poll, 32
/// events), while a chord plays and, every 40 ms, the main loop is held 20 ms before it drains
/// MIDI (a long pass): SIDOU's MIDI-in ring never loses an event, the device NAKs the host
/// instead (review 4.5: EP1 NAKed only on usb.c's own ring, which never holds SysEx, so a dump
/// overflowed SIDOU's 256 entries in a long pass, and the gap released every note and reset
/// SysEx).
#[test]
fn a_usb_sysex_flood_is_held_back_by_naks_not_lost() {
    let Some(fw) = fixture("USB back-pressure on a SysEx flood") else {
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
    // F0 43 (Yamaha's id), 8 KiB of data bytes, F7: 16 events a packet
    let dump: Vec<u8> = [0xf0, 0x43]
        .into_iter()
        .chain((0..8192u32).map(|i| (i % 127) as u8))
        .chain(std::iter::once(0xf7))
        .collect();
    let packets = sysex_packets(&dump);
    let step = m.soc.periph.cycle_of_ms(1) / 8;
    let long_pass = m.soc.periph.cycle_of_ms(20);
    let drain = fw.symbol("midi_drain");
    let (start, mut naks, mut held) = (m.ms(), 0u64, 0u32);
    let mut next_pass = m.ms() + 5;
    for chunk in packets.chunks(64) {
        loop {
            if m.ms() >= next_pass {
                m.busy = Some((drain, long_pass));
                held += 1;
                next_pass = m.ms() + 40;
            }
            let usb = m.usb.as_mut().expect("a host");
            if usb.out1(&mut m.soc, chunk) {
                break;
            }
            naks += 1;
            let before = m.ms();
            m.run_cycles(step);
            if m.ms() != before {
                let usb = m.usb.as_mut().expect("a host");
                usb.frame(&mut m.soc);
            }
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
