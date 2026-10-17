// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! USB-MIDI end to end in fm1-emu, with a host and the device controller modelled in the test
//! crate (common::usb_host; fm1-emu itself has no USB controller). The host enumerates the
//! device, asks for its identity as Felucca's installer does (the update protocol's handshake,
//! cmd 0x11), and reads EP1 IN while K1 and K4 turn. On v0.2.0-m2-preview the device received
//! all of this and a host saw almost none of what it sent (docs/hardware-test.md, section 8).
//! What this checks is the firmware's side, as the USB fix made it:
//! - every IN packet carries at most 16 bytes (usb/usb.c EP1_IN_MAX), so the identity reply
//!   goes as several;
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

/// Felucca's ids: K1 (cutoff, CC 74) and K4 (the preset, a program change).
const K1: u8 = 2;
const K4: u8 = 5;
const CC_CUTOFF: u8 = 74;
/// The identity SIDOU's development builds answer (CMakeLists.txt SIDOU_PRODUCT).
const IDENTITY: &str = "FM-1_900";
/// usb/usb.c EP1_IN_MAX.
const IN_MAX: usize = 16;
/// The config descriptor's length (usb/usb.c CFG_LEN_PLAIN).
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

/// usb/usb.c's `usb`: up, config, ...
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
    assert!(sizes.iter().all(|&n| n > 0 && n <= IN_MAX && n % 4 == 0));
    assert!(
        sizes.len() > 1,
        "the reply fits one packet: the cap is untested"
    );

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
    // K4: a program change.
    host(&mut m).in1.clear();
    m.turn(K4, 1);
    m.usb_frames(60);
    let ev = events(&host(&mut m).in1);
    println!("K4: {ev:02x?}");
    assert!(
        ev.iter().any(|p| p[..2] == [0x0C, 0xC0]),
        "K4 sent no program change"
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
    // The upgrade request (usb/usb.c sysex_byte), then step 1's first read request (cmd 0x30).
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
