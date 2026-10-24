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
//! - a host that records (USB audio out: platform/fm1/usb_audio.h) gets the DAC's samples, bit
//!   for bit at 44.1 kHz and resampled at 48, a packet each frame, and recording costs the audio
//!   interrupt little.
//!
//! The chip's own behaviour (the DMA against a sleeping core, the host's IN tokens) is not
//! modelled; only the device can confirm the fix (docs/hardware-test.md).

mod common;

use common::usb_host::UsbHost;
use common::{build_dir, run_past_boot, skip_message, Firmware, Machine, USB_AUDIO_PER_SAMPLE};
use fm1_core::Bus;

/// Felucca's ids: K1 (cutoff, CC 74) and PRESETS (the preset, a program change).
const K1: u8 = 2;
const PRESETS: u8 = 6;
const CC_CUTOFF: u8 = 74;
/// The identity SIDOU's development builds answer (CMakeLists.txt SIDOU_PRODUCT).
const IDENTITY: &str = "FM-1_900";
/// src/usb.c ep1_tx: one full-speed bulk packet.
const IN_MAX: usize = 64;
/// The config descriptor's length (src/usb.c CFG_LEN_PLAIN): USB-MIDI and the audio input
/// (FELUCCA_UAC, 44.1 and 48 kHz), 9 + 92 + 63 + 14.
const CONFIG_LEN: usize = 178;

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

/// While a host has the device configured nothing sleeps in `idle` (hardware-test 8: IN
/// packets were lost under it), not even the register ring's wait for the audio interrupt
/// (review 2.4: platform/fm1/audio.c's wait_for_drain idled where main.c's wait_for_interrupt
/// spins). The ring is filled to the brim (with writes to the SID's unused register $1F, which
/// do nothing) at the start of a live_tick while a note sounds, so the tick's own writes wait
/// for the interrupt to make room.
#[test]
fn nothing_sleeps_in_idle_while_a_host_is_there_not_even_the_rings_wait() {
    use sidou_sys::ilp32::reg_ring as rr;
    let Some(fw) = fixture("no idle with a host") else {
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

// ---- A PUT while the store's own write waits for quiet (the device, 2026-10-23) ----

/// store_fm1_test's layout (store_fm1.h): the operation, its key, its length, its result, then
/// its data; and its PUT.
const HOOK_KEY: u32 = 4;
const HOOK_LEN: u32 = 8;
const HOOK_DATA: u32 = 16;
const HOOK_PUT: u32 = 1;
const HOOK_COLLECT: u32 = 4;
/// store.c's maintenance jobs.
const JOB_COLLECT: u8 = 2;
/// store_fm1_stats's erases.
const ERASES: u32 = 4;
/// A chord: A4, C#5, E5 (Felucca's key ids; app/panel.h).
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

/// The test hook's put (as a settings or profile save): `key` gets 16 bytes of `fill`; returns
/// once the store has taken it (it programs on its own polls).
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

/// What the store is busy with when the PUT comes.
#[derive(Clone, Copy, Debug)]
enum Pending {
    /// A record (a settings save) taken while the device sounds: its programs wait for quiet.
    Record,
    /// A collection that maintenance started (the device's likely case: its INQ said the pool
    /// was full, where maintenance moves the coldest sector's records while quiet), then the
    /// device sounded: its held programs (a copy's, its victim's kill) wait for quiet.
    Collection,
}

/// What sounds meanwhile.
#[derive(Clone, Copy, Debug)]
enum Sound {
    /// A chord held on the panel's keys on LIVE.
    Keys,
    /// The demo song playing.
    Song,
    /// A chord held by USB-MIDI notes.
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

/// The store as the device had it: busy with `pending`, its programs waiting for a quiet that
/// `how` keeps away. A collection needs a victim: a sector of records, mostly superseded, that
/// is no longer the head; it is filled in write windows with a chord down (so no maintenance
/// starts meanwhile), then the collection is asked for (as quiet maintenance on the device
/// started one) with held programs, and the sound comes before quiet does.
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

/// On the device (2026-10-23) every PUT with a chord held on LIVE's keys was refused BUSY, and
/// went through as soon as the keys were up: the store had a write under way (quiet maintenance
/// had started a collection), whose held programs wait for quiet, and store_file_begin refused
/// while the writer (or a collection) was busy. The emulator's chord test found the store idle.
/// Now the PUT waits for that write, which runs in the PUT's own paced holds, then claims;
/// the transfer goes on with one hold a packet, and nothing is erased meanwhile.
fn put_while_the_store_waits(pending: Pending, how: Sound) {
    let Some(fw) = fixture("a PUT while the store waits for quiet") else {
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
    meta.push(2);
    meta.extend((data.len() as u32).to_le_bytes());
    // SAFETY: data is valid for its length; the CRC routine does not retain it.
    let crc = unsafe { sidou_sys::store_crc32(0, data.as_ptr().cast(), data.len() as u32) };
    meta.extend(crc.to_le_bytes());
    let asked = m.ms();
    link.send(&mut m, 4, 10, &meta);
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
    // Bounds: the measurement (claimed after 22 ms at most, the record's case: its programs in
    // one hold; 112.0 ms a packet, its first DATA a forced gap after that hold) plus 10 %.
    assert!(claim <= 25, "the PUT's claim took {claim} ms");
    assert!(up <= 123.0, "PUT: {up:.1} ms a packet");
    m.usb_frames(10);
    assert!(
        m.soc
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
    // SAFETY: st, file and the output buffer outlive each store call.
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

    // A DEL meets the same: a record taken meanwhile waits for quiet, and the DEL waits for it.
    hook_put(&mut m, &fw, 0x201, 0xa5);
    m.usb_frames(200);
    assert_ne!(writer_phase(&mut m, &fw), 0, "the record waits for quiet");
    let asked = m.ms();
    link.send(&mut m, 7, 100, &name16("USB WAITS"));
    let (command, s, payload) = link.recv(&mut m, 2_000);
    assert_eq!(
        (command, s),
        (0x7e, 100),
        "{pending:?}, {how:?}: the DEL is accepted, not refused: {payload:02x?}"
    );
    let del = m.ms() - asked;
    println!("DEL while a record waits: ACK after {del} ms");
    // Bound: the measurement (128 ms at most: a forced gap, then the record and the tombstone
    // in one hold) plus 10 %.
    assert!(del <= 141, "the DEL took {del} ms");
    let mut flash = m.soc.flash.mem.clone();
    let st = common::mount_dump(&mut flash);
    let mut file = sidou_sys::store_file::default();
    let mut value = [0u8; 16];
    // SAFETY: st, file and value outlive each store call.
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

/// fm1_install.py's logical image: the package without the marker byte after each of its
/// first 20 blocks of 0x30.
fn logical_image(package: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..20 {
        out.extend_from_slice(&package[i * 0x30..i * 0x30 + 0x2F]);
    }
    out.extend_from_slice(&package[20 * 0x30..]);
    out
}

/// fm1_install.py's answer to a read request (`response`): the request's flags, address and
/// length, then the data.
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

/// Step 1 of an update as fm1_install.py runs it against the running firmware: the upgrade
/// request, then every read request answered from the package's logical image (10 ms after
/// it, the installer's DELAY["reply"]), until the device asks for the host's "success"
/// (FINISH_CHECK). The host's OUT packets queue as the host's MIDI driver queues them: one a
/// frame, a NAKed one again the next. Returns the reads served and whether the device asked
/// for the "success"; panics when the device NAKs the host for longer than
/// `STEP_ONE_NAK_MS`, which step 1 never needs (the session takes each frame at once).
fn installer_step_one(m: &mut Machine, fw: &Firmware, image: &[u8]) -> (Vec<(u32, u32)>, bool) {
    const FINISH_CHECK: u32 = 0xE000_0000;
    const STEP_ONE_NAK_MS: u64 = 1_000;
    host(m).in1.clear();
    let mut out: std::collections::VecDeque<Vec<u8>> =
        sysex_packets(&[0xF0, 0x22, 0x24, 0x35, 0x7F, 0xF7])
            .chunks(64)
            .map(<[u8]>::to_vec)
            .collect();
    let mut served = Vec::new();
    let (mut taken, mut idle, mut naked) = (0usize, 0u64, 0u64);
    // fm1_install.py's idle_check: 8 s without a request ends step 1.
    while idle < 8_000 {
        if let Some(chunk) = out.front() {
            let usb = m.usb.as_mut().expect("a host");
            if usb.out1(&mut m.soc, chunk) {
                out.pop_front();
                naked = 0;
            } else {
                naked += 1;
                assert!(
                    naked <= STEP_ONE_NAK_MS,
                    "the device NAKed the installer for {naked} ms after {} reads \
                     ({} packets unsent; pc in {})",
                    served.len(),
                    out.len(),
                    fw.function_at(m.cpu.pc)
                );
            }
        }
        m.usb_frames(1);
        idle += 1;
        let at = fw.function_at(m.cpu.pc);
        assert!(
            !at.starts_with("fm1_reboot") && !at.starts_with("fm1_cstart"),
            "the device restarted ({at}) after {} reads",
            served.len()
        );
        // Only the packets read since the last look: a request is whole in them, as the
        // device sends one whole (usb.c ep1_tx, SysEx first).
        let fresh = host(m).in1.len();
        if fresh == taken {
            continue;
        }
        let r = replies(&host(m).in1[taken..fresh]);
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
            out.extend(sysex_packets(&answer).chunks(64).map(<[u8]>::to_vec));
            served.push((addr, n));
        }
    }
    (served, false)
}

/// The update's step 1 through the running firmware, every read of it, then step 2's hand-off,
/// as on the device (fm1_install.py): the package's head and loader read, the loader staged in
/// flash, the host's "success", the UPDATA_PARM record in flash and RAM, the restart into the
/// loader. On 0.3.0's candidate the device stopped after four reads on the device (the first
/// read and three tries of the second; here, the host NAKed for good after two answers): the
/// session never emptied SIDOU's MIDI-in ring, which the answers filled until it held EP1 back.
/// The machine's stack check (common::Machine's Drop) covers the session's depth.
#[test]
fn the_running_firmware_serves_an_updates_whole_step_one() {
    /// fm1_sys.h FM1_UPDATA_PARM, and ota.c OTA_AREA and OTA_RES.
    const UPDATA_PARM: u32 = 0x01C7_FD88;
    const OTA_AREA: usize = 0xE0000;
    const OTA_RES: usize = 0xE4F00;
    let Some(fw) = fixture("the update's step 1") else {
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
    // The head (two reads), the loader's head twice (ota.c steps 2 and 4), then its body: every
    // byte of it staged as the package holds it.
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
    // The staged loader's JLFS head (ota.c ota_stage_head), written last.
    assert_eq!(
        &m.soc.flash.mem[OTA_AREA + 16..OTA_AREA + 26],
        b"LOADER.BIN"
    );
    // Step 2's hand-off: the host's "success" (fm1_install.py serve), then the record, flash
    // copy first, and the core reset into the loader.
    let answer = read_response(0, 0xE000_0000, b"success\0");
    send(&mut m, &sysex_packets(&answer));
    let identity = b"ota-FM-1_015";
    let mut ms = 0;
    while (0..12u32).any(|i| m.soc.read8(UPDATA_PARM + 8 + i) != identity[i as usize]) {
        assert!(ms < 2_000, "no UPDATA_PARM record 2 s after the success");
        m.usb_frames(1);
        ms += 1;
    }
    assert_eq!(&m.soc.flash.mem[OTA_RES + 8..OTA_RES + 20], identity);
    let (user, system) = m.stack_use();
    println!("step 1's stacks: user {user}, system {system}");
}

// ---- the first boot over another firmware's sectors ----

/// Another firmware's data in `secs` sectors of the store's region (as Felucca's projects and
/// samples, platform/fm1/hal/fm1_flash.h FL_DATA_*: its own commit-record magic at offset 0, a
/// payload from 0x100 to the sector's end, the guard band included).
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

/// Until the device answers the descriptor request: a host polling the attached device as a
/// real one does (a bus reset, SET_ADDRESS and GET_DESCRIPTOR every 100 ms), then the whole
/// enumeration, then the updater's identity handshake once a second, as fm1_install.py's
/// wait_for. Returns (attached, answered): emulated ms from power-on.
fn first_identity(m: &mut Machine, fw: &Firmware, limit_ms: u64) -> (u64, u64) {
    let mut attached = None;
    while attached.is_none() {
        assert!(m.ms() < limit_ms, "USB never attached in {limit_ms} ms");
        if up_and_config(m, fw).0 == 1 {
            attached = Some(m.ms());
        }
        m.usb_frames(1);
    }
    loop {
        assert!(m.ms() < limit_ms, "no device descriptor in {limit_ms} ms");
        host(m).bus_reset();
        m.usb_frames(3);
        setup(m, [0x00, 0x05, 0x07, 0, 0, 0, 0, 0]);
        host(m).in0.clear();
        setup(m, [0x80, 0x06, 0x00, 0x01, 0, 0, 0x12, 0]);
        if host(m).in0.concat().len() == 18 {
            break;
        }
        m.usb_frames(100);
    }
    enumerate(m, fw);
    loop {
        assert!(m.ms() < limit_ms, "no identity reply in {limit_ms} ms");
        host(m).in1.clear();
        send(m, &sysex_packets(&message(0x11, &[])));
        for _ in 0..1_000 {
            m.usb_frames(1);
            if replies(&host(m).in1).iter().any(|r| r[0] == 0x11) {
                return (attached.unwrap(), m.ms());
            }
        }
    }
}

/// store_fm1.c's BOOT_WORK_MS: the most flash work the boot does after the mount.
const BOOT_WORK_MS: f64 = 100.0;
const STORE_LO: usize = 0x97000;

/// The first boot after an install over another firmware (2026-10-23, a real FM-1: Felucca
/// 1.6.1 had written its data over 40-odd sectors of SIDOU's earlier store, and SIDOU's boot
/// erased them all before it started USB, at the flash's own pace with the interrupts masked):
/// the boot mounts, does at most BOOT_WORK_MS of flash work and starts; the rest (each other
/// firmware's sector erased before the session's first write, store.c recovery) goes on in the
/// main loop's passes, one operation a pass, so USB answers within a second whatever the store
/// still has to do. With the datasheet's worst times (20 ms an erase, 3 ms a program) and with
/// the RAM driver's own limits (500 ms, 10 ms: fl_erase4k_ram, fl_prog_ram). Before the fix the
/// second took 20.4 s to attach USB, the boot's flash work one stretch of 20.1 s.
#[test]
fn a_first_boot_over_another_firmwares_sectors_answers_usb_at_once() {
    let Some(fw) = fixture("the first boot over another firmware's sectors") else {
        return;
    };
    let mut flash = common::settled_flash(&fw.image);
    // 40 sectors, SIDOU's first sectors (its settings among them) left as they were.
    let secs: Vec<usize> = (33..73).map(|k| STORE_LO + k * 0x1000).collect();
    foreign_sectors(&mut flash, &secs);
    let state = fw.symbol("store_fm1_store") + sidou_sys::ilp32::store::state as u32;
    for (erase_ms, prog_ms) in [(20u64, 3u64), (500, 10)] {
        let mut m = Machine::boot_blank(&fw.image);
        m.soc.flash.mem.copy_from_slice(&flash);
        let ms = |m: &Machine, n: u64| m.soc.periph.cycle_of_ms(n);
        m.stalls = vec![
            (fw.symbol("fl_prog_ram"), ms(&m, prog_ms)),
            (fw.symbol("fl_erase4k_ram"), ms(&m, erase_ms)),
        ];
        m.watch_calls(fw.symbol("boot_feed"));
        m.usb = Some(UsbHost::default());
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
        assert!(left > 0, "the boot reclaimed every sector itself");
        assert_eq!(m.soc.read8(state), sidou_sys::STORE_STATE_SIDOU as u8);
        if erase_ms == 20 {
            // The quiet device reclaims the rest in the main loop, and the store takes writes
            // again: the settings are still SIDOU's, and a put commits.
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

// ---- USB audio out (platform/fm1/usb_audio.h) ----

/// src/usb.c's `uac` (FELUCCA_UAC), by its fields' offsets on pi32v2: nine bytes of flags, then
/// fill_min and the counters, each a word.
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

/// usb_audio.c's block (`usb_audio_in`): want, left (after the 256 sample words), blocks,
/// skipped.
fn hand_over(m: &mut Machine, fw: &Firmware) -> (u8, u8, u32, u32) {
    let a = fw.symbol("usb_audio_in") + 1024;
    (
        m.soc.read8(a),
        m.soc.read8(a + 1),
        m.soc.read32(a + 4),
        m.soc.read32(a + 8),
    )
}

/// SET_INTERFACE on the audio streaming interface (2): alt 1 starts the stream, and the host
/// reads it from the frame after; alt 0 stops it, the host's reading first.
fn record(m: &mut Machine, on: bool) {
    if !on {
        host(m).ep4_reading = false;
    }
    setup(m, [0x01, 0x0B, u8::from(on), 0, 2, 0, 0, 0]);
    host(m).ep4_reading = on;
}

/// SET_CUR sampling frequency on EP 0x84: the SETUP, then its 3-byte data stage.
fn set_rate(m: &mut Machine, hz: u32) {
    setup(m, [0x22, 0x01, 0x00, 0x01, 0x84, 0x00, 0x03, 0x00]);
    let usb = m.usb.as_mut().expect("a host");
    usb.out0(&mut m.soc, &hz.to_le_bytes()[..3]);
    m.usb_frames(3);
    assert_eq!(host(m).stalls, 0, "SET_CUR {hz} stalled");
}

/// The EP4 packets' frames as (L, R).
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

/// A4's key id (14 + note - 53), held so the chip sounds.
const A4_KEY: u8 = 30;

/// A host records what the DAC plays: at 44.1 kHz the same samples, bit for bit, a packet of 44
/// or 45 frames each frame (one more or fewer while the ring's fill is held in its band); at 48
/// kHz (SET_CUR) packets of 47..49 at the same level. No frame is repeated or dropped, and no IN
/// token finds an empty endpoint. When the host stops, the hand-over stops too.
#[test]
fn a_host_records_the_dacs_output_over_usb_audio_at_44_1_and_48_khz() {
    let Some(fw) = fixture("the USB audio test") else {
        return;
    };
    // usb.c's EP4 IN buffer on an 8-byte boundary, as EP1's must be (app.ld: it is in the
    // compiler's merged globals, where no rule places it).
    assert_eq!(fw.symbol("ep4tx") % 8, 0, "ep4tx at 4 mod 8");
    let mut m = usb_machine(&fw);
    assert_eq!(hand_over(&mut m, &fw).0, 0, "no host records yet");
    m.key(A4_KEY, true);
    m.usb_frames(300);

    record(&mut m, true);
    m.usb_frames(1000);
    let u = uac(&mut m, &fw);
    let packets = host(&mut m).in4.clone();
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
    assert_eq!(host(&mut m).ep4_empty, 0, "an IN token found no packet");
    assert_eq!(hand_over(&mut m, &fw).3, 0, "a block skipped");
    // The DAC's samples, bit for bit: the recording from its first sound on is a run of what
    // the DAC played.
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

    set_rate(&mut m, 48_000);
    host(&mut m).in4.clear();
    let dac_from = m.left().len();
    m.usb_frames(1000);
    let u48 = uac(&mut m, &fw);
    let packets = host(&mut m).in4.clone();
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
        (u48.underruns, u48.overruns, u48.stalls, u48.missed),
        (0, 0, 0, 0),
        "a glitch at 48 kHz"
    );
    assert!(u48.pkts >= 995, "{} packets counted at 48 kHz", u48.pkts);
    assert_eq!(host(&mut m).ep4_empty, 0, "an IN token found no packet");
    assert_eq!(hand_over(&mut m, &fw).3, 0, "a block skipped");

    record(&mut m, false);
    host(&mut m).in4.clear();
    m.usb_frames(50);
    assert!(host(&mut m).in4.is_empty(), "packets after alt 0");
    let (want, left, ..) = hand_over(&mut m, &fw);
    assert_eq!((want, left), (0, 0), "the hand-over goes on after alt 0");
}

/// What recording costs the interrupts, a chord held, recording and not by turns: the audio
/// interrupt (the TIMER5 work that nests in a render: packets, the producer's quarters), and
/// every interrupt together (TIMER5's copy of the block, the producer, 48 kHz's resampler, and
/// the packets).
#[test]
fn recording_over_usb_audio_costs_the_audio_interrupt_little() {
    let Some(fw) = fixture("the USB audio cost test") else {
        return;
    };
    let mut m = usb_machine(&fw);
    for k in [18, 25, 37] {
        m.key(k, true);
    }
    m.usb_frames(1500); // to the sustain level: the chip's cost steady
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
    let mut rows = Vec::new();
    for hz in [44_100, 48_000] {
        let (mut on, mut off) = (Vec::new(), Vec::new());
        for _ in 0..3 {
            record(&mut m, true);
            set_rate(&mut m, hz);
            m.usb_frames(50);
            on.push(measure(&mut m, 400));
            record(&mut m, false);
            m.usb_frames(50);
            off.push(measure(&mut m, 400));
        }
        let mean = |v: &[(f64, f64)], f: fn(&(f64, f64)) -> f64| {
            v.iter().map(f).sum::<f64>() / v.len() as f64
        };
        let alnk = mean(&on, |x| x.0) - mean(&off, |x| x.0);
        let all = mean(&on, |x| x.1) - mean(&off, |x| x.1);
        println!(
            "{hz} Hz: the audio interrupt {:.2} recording, {:.2} not: +{alnk:.2} a sample; every \
             interrupt +{all:.2} a sample ({on:?} / {off:?})",
            mean(&on, |x| x.0),
            mean(&off, |x| x.0)
        );
        rows.push((hz, alnk, all));
    }
    let (user, system) = m.stack_use();
    println!("stacks: user {user} bytes, system {system}");
    for (hz, alnk, _) in rows {
        assert!(
            alnk > 0.0,
            "{hz} Hz: recording costs nothing: is it running?"
        );
        assert!(
            alnk <= USB_AUDIO_PER_SAMPLE,
            "{hz} Hz: recording adds {alnk:.2} instructions a sample to the audio interrupt"
        );
    }
}
