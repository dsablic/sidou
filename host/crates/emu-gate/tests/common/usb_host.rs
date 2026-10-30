// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A USB host on the cable: fm1-emu's own (fm1_soc::usb: the FM-1's USB0 controller, a Mentor
//! MUSB core behind JieLi's register window, and a full-speed host at the other end), which
//! is a model of the controller independent of SIDOU's Felucca driver.
//!
//! The cable is fm1-emu's (`Usb::plug`: the SIE answers, at its own pace), but the test, not
//! the SoC's clock, runs the host's 1 ms frames (`frame`, from `Machine::usb_frames`), so it
//! decides when a host is there and sees what crosses the bus. In each frame fm1-emu's host
//! does what a computer does: after the D+ pull-up a bus reset and the enumeration (the device
//! descriptor, SET_ADDRESS, the configuration, the language and the product string,
//! SET_CONFIGURATION), then once a frame up to 64 bytes of MIDI to the device (`send`) and one
//! IN poll, and, recording (`record`), SET_INTERFACE, SET_CUR of the rate and an isochronous IN
//! transaction a frame.
//!
//! What this adds on top: the IN packets as they leave the device's RAM, read through the
//! controller's DMA (`in0`, `in1`, `in4`: fm1-emu hands the host's IN data on only as complete
//! MIDI messages, which lose the packets' bounds), and which SIE accesses happen inside the
//! audio interrupt (`after_step`).
//!
//! What fm1-emu's host cannot do (docs/upstream-reports.md, row 11): send a control request of
//! the test's own (a string other than the product's, SET_INTERFACE to alternate setting 0,
//! SET_CUR on a running stream: a new rate takes a new enumeration, `replug`), tell the
//! device's address, or send more than one bulk OUT transaction in a frame.

use fm1_core::Bus;
use fm1_soc::usb::Dma;
use fm1_soc::Soc;

const EP0_ADR: u32 = 0x11818;
const EP1_TADR: u32 = 0x1181C;
const EP4_TADR: u32 = 0x11838;
/// USB_IO_CON0: bit 6 is the D+ pull-up (the device attached).
const USB_IO_CON0: u32 = 0x51000;

/// The SoC under fm1-emu's host, the bytes its DMA reads for the host noted.
struct Tap<'a> {
    soc: &'a mut Soc,
    reads: Vec<(u32, u8)>,
}

impl Dma for Tap<'_> {
    fn dma_read8(&mut self, addr: u32) -> u8 {
        let b = self.soc.read8(addr);
        self.reads.push((addr, b));
        b
    }
    fn dma_write8(&mut self, addr: u32, v: u8) {
        self.soc.write8(addr, v);
    }
}

#[derive(Default)]
pub struct UsbHost {
    /// A host on the bus: its frames run (none before `start`, as with no computer there).
    pub present: bool,
    /// The next frame drops the pull-up as the host sees it (the cable pulled and put back).
    unplug: bool,
    /// EP0's IN data packets the host has taken.
    pub in0: Vec<Vec<u8>>,
    /// EP1 IN packets the host has taken.
    pub in1: Vec<Vec<u8>>,
    /// EP4 IN packets (the recording) the host has taken.
    pub in4: Vec<Vec<u8>>,
    /// SIE accesses made while an ALNK0 interrupt was open (nested interrupts included).
    pub sie_in_audio: u64,
    sie_seen: u64,
}

impl UsbHost {
    /// The cable into the FM-1, from now on: the SIE answers. No host yet (`start`).
    pub fn plug(soc: &mut Soc) -> Self {
        let hz = soc.periph.hz;
        soc.usb.plug(hz);
        UsbHost::default()
    }

    /// A host from the next frame on: it enumerates the device once it is attached.
    pub fn start(&mut self) {
        self.present = true;
    }

    /// The cable out for a frame and back: the host enumerates the device again.
    pub fn replug(&mut self) {
        self.unplug = true;
    }

    /// Configured by the host (its enumeration done).
    pub fn configured(soc: &Soc) -> bool {
        soc.usb.product().is_some()
    }

    /// One complete MIDI message (SysEx included) queued for the device: the host sends it
    /// at the next frames, up to 64 bytes a frame, again the next frame while NAKed.
    pub fn send(soc: &mut Soc, msg: &[u8]) {
        soc.usb.midi_send(msg);
    }

    /// Event packets still waiting to go to the device.
    pub fn pending(soc: &Soc) -> usize {
        soc.usb.midi_pending()
    }

    /// USB-MIDI event packets (4 bytes each) as messages, the way a host's driver joins them.
    pub fn messages(packets: &[u8]) -> Vec<Vec<u8>> {
        let (mut sysex, mut out) = (Vec::new(), Vec::new());
        for p in packets.as_chunks::<4>().0 {
            fm1_soc::usb::midi_from_packet(p, &mut sysex, &mut out);
        }
        out
    }

    /// After every instruction: an SIE access the instruction made inside the audio interrupt.
    pub fn after_step(&mut self, soc: &mut Soc, in_audio: bool) {
        let n = soc.usb.sie_accesses;
        if in_audio {
            self.sie_in_audio += n - self.sie_seen;
        }
        self.sie_seen = n;
    }

    /// SIE accesses so far.
    pub fn sie_accesses(soc: &Soc) -> u64 {
        soc.usb.sie_accesses
    }

    /// One 1 ms frame of fm1-emu's host, and the IN packets it took, from the endpoints'
    /// buffers as the DMA read them.
    pub fn frame(&mut self, soc: &mut Soc) {
        if !self.present {
            return;
        }
        let pullup = soc.read32(USB_IO_CON0) & 0x40 != 0 && !std::mem::take(&mut self.unplug);
        let now = soc.periph.cycles;
        let mut usb = std::mem::take(&mut soc.usb);
        let bases = [
            usb.read(EP0_ADR, now),
            usb.read(EP1_TADR, now),
            usb.read(EP4_TADR, now),
        ];
        let mut tap = Tap {
            soc,
            reads: Vec::new(),
        };
        usb.frame(&mut tap, pullup);
        let reads = std::mem::take(&mut tap.reads);
        soc.usb = usb;
        // (the same IN data as messages: `in1` has it as packets)
        soc.usb.midi_take();
        // Each packet: a run of consecutive bytes from an endpoint's buffer address.
        let mut packets: Vec<(usize, Vec<u8>)> = Vec::new();
        let mut last = None;
        for (addr, b) in reads {
            let start = bases.iter().position(|&a| a == addr);
            match (start, last) {
                (_, Some(l)) if addr == l + 1 => packets.last_mut().expect("a packet").1.push(b),
                (Some(ep), _) => packets.push((ep, vec![b])),
                _ => panic!("a DMA read at {addr:#x}, in no endpoint's buffer"),
            }
            last = Some(addr);
        }
        for (ep, p) in packets {
            match ep {
                0 => self.in0.push(p),
                1 => self.in1.push(p),
                _ => self.in4.push(p),
            }
        }
    }
}
