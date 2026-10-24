// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A USB host and the FM-1's USB0 device controller, modelled in the test crate over fm1-emu's
//! plain register storage (fm1-emu has no USB controller: its USB0 registers keep what was
//! written). `Machine::run_cycles` calls `after_step` after every instruction once
//! `Machine::usb` is set, and the model answers what the instruction asked of the SIE, so the
//! firmware's next instruction finds it done, as on the chip.
//!
//! What it models, from platform/fm1/hal/fm1_usb.h and src/usb.c (Felucca's driver):
//! - the SIE behind CON1 (MUSB's byte map): a write `(r << 8) | v` or a read
//!   `(r << 8) | 0x4000` while CON0 bit 2 (USB_NRST) is set, done with bit 15 and, for a read,
//!   the byte in bits 0..7; the indexed endpoint registers 16..23 of EP0, EP1 and EP4;
//! - the interrupt flags INTRTX1, INTRRX1 and INTRUSB, cleared by their read;
//! - SOF (CON0 bit 13, cleared by writing bit 12) and the frame number, once a frame;
//! - the endpoint DMA: an IN packet (EP0's data stage, EP1 IN) is read from RAM at the address
//!   and count registers when the host takes it, at the next frame after TxPktRdy, as the
//!   controller reads it when the host's IN token comes; a SETUP, an EP0 OUT data stage or an
//!   EP1 OUT packet is written to RAM at EP0's address or EP1's RX address;
//! - EP4 IN, the USB audio stream (isochronous): while the host records (`ep4_reading`), the
//!   packet the device readied is read from RAM at EP4's own address and count registers once a
//!   frame, and a frame that finds none sets TXCSR1's UnderRun, as the chip does.
//!
//! What it does not: timing below a frame, NAKs other than EP1 OUT's while its last packet is
//! unread, data toggles, stalls beyond counting them, EP2 and EP3, the PHY. The chip's own
//! behaviour is what docs/hardware-test.md checks; this is the firmware's side of it.

use fm1_core::Bus;
use fm1_soc::Soc;

const CON0: u32 = 0x11800;
const CON1: u32 = 0x11804;
const EP0_ADR: u32 = 0x11818;
const EP4_CNT: u32 = 0x11834;
const EP4_TADR: u32 = 0x11838;

fn cnt(ep: u32) -> u32 {
    0x11808 + 4 * ep
}

fn tadr(ep: u32) -> u32 {
    0x1181C + 8 * (ep - 1)
}

fn radr(ep: u32) -> u32 {
    0x11820 + 8 * (ep - 1)
}

#[derive(Default)]
pub struct UsbHost {
    /// FADDR..INDEX (0..15), as written.
    regs: [u8; 16],
    /// EP0's, EP1's and EP4's indexed registers 16..23 (CSR0 / TXCSR1 at 17, RXCSR1 at 20,
    /// the RX count at 22 and 23); EP2 and EP3's slots unused.
    ep: [[u8; 8]; 5],
    intr_tx: u8,
    intr_rx: u8,
    intr_usb: u8,
    frame: u16,
    ep0_in: bool,
    ep1_in: bool,
    /// EP0 data stages the host has taken.
    pub in0: Vec<Vec<u8>>,
    /// EP1 IN packets the host has taken.
    pub in1: Vec<Vec<u8>>,
    /// The host reads EP4 IN once a frame (an audio recorder with the stream selected).
    pub ep4_reading: bool,
    ep4_in: bool,
    /// EP4 IN packets the host has taken.
    pub in4: Vec<Vec<u8>>,
    /// Frames whose EP4 IN token found no packet (UnderRun).
    pub ep4_empty: u32,
    /// STALL handshakes the device asked for on EP0.
    pub stalls: u32,
    /// SIE accesses made while an ALNK0 interrupt was open (nested interrupts included).
    pub sie_in_audio: u64,
    pub sie_accesses: u64,
}

impl UsbHost {
    fn index(&self) -> usize {
        usize::from(self.regs[14])
    }

    fn sie_read(&mut self, r: u32) -> u8 {
        match r {
            2 => std::mem::take(&mut self.intr_tx),
            4 => std::mem::take(&mut self.intr_rx),
            6 => std::mem::take(&mut self.intr_usb),
            3 | 5 => 0,
            12 => self.frame as u8,
            13 => (self.frame >> 8) as u8,
            0..=15 => self.regs[r as usize],
            16..=23 if matches!(self.index(), 0 | 1 | 4) => {
                self.ep[self.index()][(r - 16) as usize]
            }
            _ => 0,
        }
    }

    fn sie_write(&mut self, r: u32, v: u8) {
        let i = self.index();
        match r {
            0..=15 => self.regs[r as usize] = v,
            17 if i == 0 => {
                let csr = &mut self.ep[0][1];
                if v & 0x40 != 0 {
                    *csr &= !0x01; // ServicedRxPktRdy
                }
                if v & 0x80 != 0 {
                    *csr &= !0x10; // ServicedSetupEnd
                }
                if v & 0x20 != 0 {
                    self.stalls += 1;
                }
                if v == 0 {
                    *csr &= !0x04; // SentStall
                }
                if v & 0x02 != 0 {
                    *csr |= 0x02; // TxPktRdy until the host takes it
                    self.ep0_in = true;
                }
            }
            17 if i == 1 => {
                if v & 0x08 != 0 {
                    self.ep[1][1] = v & !0x09; // FlushFIFO: nothing pending
                    self.ep1_in = false;
                } else {
                    self.ep[1][1] = v;
                    self.ep1_in = v & 0x01 != 0;
                }
            }
            17 if i == 4 => {
                if v & 0x08 != 0 {
                    self.ep[4][1] = v & !0x09; // FlushFIFO: nothing pending
                    self.ep4_in = false;
                } else {
                    self.ep[4][1] = v; // TxPktRdy, and UnderRun cleared by the write
                    self.ep4_in = v & 0x01 != 0;
                }
            }
            20 if i == 1 => self.ep[1][4] = v & !0x01, // the packet is released
            16..=23 if matches!(i, 0 | 1 | 4) => self.ep[i][(r - 16) as usize] = v,
            _ => {}
        }
    }

    /// After every instruction: SOF's clear, and an SIE access the instruction started.
    pub fn after_step(&mut self, soc: &mut Soc, in_audio: bool) {
        let con0 = soc.read32(CON0);
        if con0 & (1 << 12) != 0 {
            soc.write32(CON0, con0 & !(3 << 12));
        }
        if con0 & 4 == 0 {
            return;
        }
        let con1 = soc.read32(CON1);
        if con1 & 0x8000 != 0 {
            return;
        }
        self.sie_accesses += 1;
        if in_audio {
            self.sie_in_audio += 1;
        }
        let r = (con1 >> 8) & 0x3F;
        if con1 & 0x4000 != 0 {
            let d = self.sie_read(r);
            soc.write32(CON1, (con1 & 0x7F00) | 0x8000 | u32::from(d));
        } else {
            self.sie_write(r, con1 as u8);
            soc.write32(CON1, con1 | 0x8000);
        }
    }

    /// A bus reset: the device sees it at its next poll.
    pub fn bus_reset(&mut self) {
        self.intr_usb |= 0x04;
        self.regs[0] = 0;
    }

    /// One SETUP packet into EP0's buffer.
    pub fn setup(&mut self, soc: &mut Soc, pkt: [u8; 8]) {
        let a = soc.read32(EP0_ADR);
        for (i, b) in pkt.iter().enumerate() {
            soc.write8(a + i as u32, *b);
        }
        self.ep[0][1] |= 0x01;
        self.intr_tx |= 0x01;
    }

    /// EP0's OUT data stage after a SETUP that announced one (SET_CUR's 3 bytes).
    pub fn out0(&mut self, soc: &mut Soc, data: &[u8]) {
        let a = soc.read32(EP0_ADR);
        for (i, b) in data.iter().enumerate() {
            soc.write8(a + i as u32, *b);
        }
        self.ep[0][1] |= 0x01;
        self.ep[0][6] = data.len() as u8;
        self.intr_tx |= 0x01;
    }

    /// One EP1 OUT packet; false (NAK) while the last one is still unread.
    pub fn out1(&mut self, soc: &mut Soc, data: &[u8]) -> bool {
        if self.ep[1][4] & 0x01 != 0 {
            return false;
        }
        let a = soc.read32(radr(1));
        for (i, b) in data.iter().enumerate() {
            soc.write8(a + i as u32, *b);
        }
        self.ep[1][4] |= 0x01;
        self.ep[1][6] = data.len() as u8;
        self.ep[1][7] = 0;
        self.intr_rx |= 0x02;
        true
    }

    /// One 1 ms frame: SOF and the frame number, and the IN packets the device has readied,
    /// read from RAM now.
    pub fn frame(&mut self, soc: &mut Soc) {
        self.frame = (self.frame + 1) & 0x7FF;
        let con0 = soc.read32(CON0);
        if con0 & 4 != 0 {
            soc.write32(CON0, con0 | (1 << 13));
        }
        if self.ep0_in {
            let (a, n) = (soc.read32(EP0_ADR), soc.read32(cnt(0)).min(64));
            self.in0.push((0..n).map(|i| soc.read8(a + i)).collect());
            self.ep[0][1] &= !0x02;
            self.intr_tx |= 0x01;
            self.ep0_in = false;
        }
        if self.ep1_in {
            let (a, n) = (soc.read32(tadr(1)), soc.read32(cnt(1)).min(64));
            self.in1.push((0..n).map(|i| soc.read8(a + i)).collect());
            self.ep[1][1] &= !0x01;
            self.intr_tx |= 0x02;
            self.ep1_in = false;
        }
        if self.ep4_reading {
            if self.ep4_in {
                let (a, n) = (soc.read32(EP4_TADR), soc.read32(EP4_CNT).min(1023));
                self.in4.push((0..n).map(|i| soc.read8(a + i)).collect());
                self.ep[4][1] &= !0x01;
                self.ep4_in = false;
            } else {
                self.ep[4][1] |= 0x04; // UnderRun: the IN token found nothing
                self.ep4_empty += 1;
            }
        }
    }

    /// The device's address (FADDR).
    pub fn address(&self) -> u8 {
        self.regs[0]
    }
}
