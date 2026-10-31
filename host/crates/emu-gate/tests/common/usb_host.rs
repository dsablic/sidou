// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use fm1_core::Bus;
use fm1_soc::usb::Dma;
use fm1_soc::Soc;

const EP0_ADR: u32 = 0x11818;
const EP1_TADR: u32 = 0x1181C;
const EP4_TADR: u32 = 0x11838;
const USB_IO_CON0: u32 = 0x51000;
const DP_PULL_UP: u32 = 1 << 6;

struct DmaReadRecorder<'a> {
    soc: &'a mut Soc,
    reads: Vec<(u32, u8)>,
}

impl Dma for DmaReadRecorder<'_> {
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
    pub present: bool,
    unplug_next_frame: bool,
    pub ep0_in: Vec<Vec<u8>>,
    pub ep1_in: Vec<Vec<u8>>,
    pub ep4_in: Vec<Vec<u8>>,
    pub sie_in_audio: u64,
    sie_seen: u64,
}

impl UsbHost {
    pub fn plug(soc: &mut Soc) -> Self {
        let hz = soc.periph.hz;
        soc.usb.plug(hz);
        UsbHost::default()
    }

    pub fn start(&mut self) {
        self.present = true;
    }

    pub fn replug(&mut self) {
        self.unplug_next_frame = true;
    }

    pub fn configured(soc: &Soc) -> bool {
        soc.usb.product().is_some()
    }

    pub fn send(soc: &mut Soc, msg: &[u8]) {
        soc.usb.midi_send(msg);
    }

    pub fn pending(soc: &Soc) -> usize {
        soc.usb.midi_pending()
    }

    pub fn messages(packets: &[u8]) -> Vec<Vec<u8>> {
        let (mut sysex, mut out) = (Vec::new(), Vec::new());
        for p in packets.as_chunks::<4>().0 {
            fm1_soc::usb::midi_from_packet(p, &mut sysex, &mut out);
        }
        out
    }

    pub fn after_step(&mut self, soc: &mut Soc, in_audio: bool) {
        let n = soc.usb.sie_accesses;
        if in_audio {
            self.sie_in_audio += n - self.sie_seen;
        }
        self.sie_seen = n;
    }

    pub fn sie_accesses(soc: &Soc) -> u64 {
        soc.usb.sie_accesses
    }

    pub fn frame(&mut self, soc: &mut Soc) {
        if !self.present {
            return;
        }
        let pullup = soc.read32(USB_IO_CON0) & DP_PULL_UP != 0
            && !std::mem::take(&mut self.unplug_next_frame);
        let now = soc.periph.cycles;
        let mut usb = std::mem::take(&mut soc.usb);
        let bases = [
            usb.read(EP0_ADR, now),
            usb.read(EP1_TADR, now),
            usb.read(EP4_TADR, now),
        ];
        let mut recorder = DmaReadRecorder {
            soc,
            reads: Vec::new(),
        };
        usb.frame(&mut recorder, pullup);
        let reads = std::mem::take(&mut recorder.reads);
        soc.usb = usb;
        soc.usb.midi_take();
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
                0 => self.ep0_in.push(p),
                1 => self.ep1_in.push(p),
                _ => self.ep4_in.push(p),
            }
        }
    }
}
