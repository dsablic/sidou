// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! PSID tunes built at test time (M3 Task 6a), for the PSID environment's tests and the
//! oracle's corpus (Task 7): a few lines of 6502 each, assembled by gt2-ref's assembler in
//! strict mode (illegal opcodes through `.BYTE`), wrapped in a PSID v2 header. None is music:
//! each makes the environment show one behaviour, and its writes say what it saw.
//!
//! The writes the fixtures make, so a test can read them back from the write stream:
//! - init's first instructions write A, X and Y to registers `$02`, `$03` and `$04`
//!   ([`INIT_RECORD`]), so a test sees the call's registers;
//! - a play call's first instructions write `$01` to register `$01` ([`BANK_RECORD`]), after
//!   mapping the I/O page in themselves if the call came with it out (an interrupt handler's,
//!   [`IRQ_BANK_RECORD`], leave `$01` alone);
//! - play routines count their calls in `$FB` and write the count to register `$00`.

use gt2_ref::assemble;

/// The header's clock bits (flags bits 2-3).
pub const CLOCK_PAL: u16 = 1 << 2;
pub const CLOCK_NTSC: u16 = 2 << 2;
/// The PlaySID flag (flags bit 1): the speed bits repeat for songs above 32.
pub const FLAG_PLAYSID: u16 = 1 << 1;

/// A PSID file's fields. `data` is the memory image from `load` on.
#[derive(Clone, Debug)]
pub struct Psid {
    pub version: u16,
    pub load: u16,
    pub init: u16,
    pub play: u16,
    pub songs: u16,
    pub start: u16,
    pub speed: u32,
    pub flags: u16,
    /// The second and third SID's address bytes (`$Dxx0` as `xx`; 0 for none).
    pub sid2: u8,
    pub sid3: u8,
    pub data: Vec<u8>,
}

impl Psid {
    /// A one-song VBI tune of the segments `(address, source)`: init is the first segment's
    /// address, play the second's (0, the tune's own interrupt handler, with one segment).
    pub fn new(segments: &[(u16, &str)]) -> Psid {
        let (load, data) = image(segments);
        Psid {
            version: 2,
            load,
            init: segments[0].0,
            play: segments.get(1).map_or(0, |s| s.0),
            songs: 1,
            start: 1,
            speed: 0,
            flags: CLOCK_PAL,
            sid2: 0,
            sid3: 0,
            data,
        }
    }

    pub fn play(mut self, play: u16) -> Psid {
        self.play = play;
        self
    }

    pub fn speed(mut self, speed: u32) -> Psid {
        self.speed = speed;
        self
    }

    pub fn flags(mut self, flags: u16) -> Psid {
        self.flags = flags;
        self
    }

    /// A second SID at `$Dxx0` (`xx` $42-$7F or $E0-$FE, even), and with `sid3` a third; a
    /// PSID v3 file, v4 with a third.
    pub fn sid2(mut self, sid2: u8) -> Psid {
        self.sid2 = sid2;
        self.version = self.version.max(3);
        self
    }

    pub fn sid3(mut self, sid3: u8) -> Psid {
        self.sid3 = sid3;
        self.version = 4;
        self
    }

    pub fn songs(mut self, songs: u16) -> Psid {
        self.songs = songs;
        self
    }

    /// The file's bytes: a v2 header (`$7C` bytes; v1's `$76`), the data at `load`.
    pub fn bytes(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"PSID");
        let off: u16 = if self.version == 1 { 0x76 } else { 0x7C };
        for w in [
            self.version,
            off,
            self.load,
            self.init,
            self.play,
            self.songs,
            self.start,
        ] {
            b.extend_from_slice(&w.to_be_bytes());
        }
        b.extend_from_slice(&self.speed.to_be_bytes());
        let mut meta = [0u8; 96];
        meta[..13].copy_from_slice(b"SIDOU FIXTURE");
        meta[32..37].copy_from_slice(b"SIDOU");
        meta[64..68].copy_from_slice(b"2026");
        b.extend_from_slice(&meta);
        if self.version > 1 {
            b.extend_from_slice(&self.flags.to_be_bytes());
            b.extend_from_slice(&[0, 0, self.sid2, self.sid3]);
        }
        b.extend_from_slice(&self.data);
        b
    }
}

/// Assembles each segment (strict) at its address and lays them out from the lowest, the
/// gaps zero: the load address and the image.
pub fn image(segments: &[(u16, &str)]) -> (u16, Vec<u8>) {
    let parts: Vec<(u16, Vec<u8>)> = segments
        .iter()
        .map(|&(at, src)| {
            let code = assemble(&format!(".ORG (${at:04X})\n{src}"), true)
                .unwrap_or_else(|e| panic!("segment at ${at:04X}: {e}"));
            (at, code)
        })
        .collect();
    let load = parts.iter().map(|p| p.0).min().unwrap();
    let end = parts
        .iter()
        .map(|p| p.0 as usize + p.1.len())
        .max()
        .unwrap();
    assert!(end <= 0x10000, "the image runs past $FFFF");
    let mut data = vec![0u8; end - load as usize];
    for (at, code) in parts {
        let o = (at - load) as usize;
        assert!(
            data[o..o + code.len()].iter().all(|&b| b == 0),
            "segments overlap"
        );
        data[o..o + code.len()].copy_from_slice(&code);
    }
    (load, data)
}

/// Init's record of its registers: A, X, Y to SID registers 2, 3, 4 (12 cycles; the writes
/// on the call's cycles 3, 7 and 11).
pub const INIT_RECORD: &str = "sta $d402\nstx $d403\nsty $d404\n";

/// A call's record of `$01` as it started: to SID register 1, mapping the I/O page in first
/// (`$35`) so the write reaches the SID whatever the bank (12 cycles; the write on cycle 11).
pub const BANK_RECORD: &str = "lda <$01\nldx #$35\nstx <$01\nsta $d401\n";

/// An interrupt handler's record of `$01`: to SID register 1, the bank left as it is (the
/// next call's vector depends on it) (7 cycles; the write on cycle 6).
pub const IRQ_BANK_RECORD: &str = "lda <$01\nsta $d401\n";

/// A play call's count: `$FB` incremented and written to SID register 0 (12 cycles; the write
/// on cycle 11).
pub const COUNT: &str = "inc <$fb\nlda <$fb\nsta $d400\n";

/// Where the fixtures put init and play.
pub const INIT: u16 = 0x1000;
pub const PLAY: u16 = 0x1100;

/// A play routine: the bank's record, the count, RTS (27 cycles, the last write on 23).
pub fn play_src() -> String {
    format!("{BANK_RECORD}{COUNT}rts\n")
}

/// A VBI tune: init records its registers and sets the volume; play records and counts.
pub fn vbi() -> Psid {
    let init = format!("{INIT_RECORD}lda #$0f\nsta $d418\nrts\n");
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

/// A CIA tune whose init sets CIA 1 timer A's latch (`$DC04`, `$DC05`) to `latch`.
pub fn cia(latch: u16) -> Psid {
    let init = format!(
        "{INIT_RECORD}lda #${:02x}\nsta $dc04\nlda #${:02x}\nsta $dc05\nrts\n",
        latch & 0xFF,
        latch >> 8
    );
    Psid::new(&[(INIT, &init), (PLAY, &play_src())]).speed(1)
}

/// A CIA tune that leaves the latch at the KERNAL's value.
pub fn cia_default() -> Psid {
    let init = format!("{INIT_RECORD}rts\n");
    Psid::new(&[(INIT, &init), (PLAY, &play_src())]).speed(1)
}

/// A CIA tune whose init sets the latch to `$1000` and whose play call n sets it to
/// `n * $100 + $FF` (high byte then low byte), so each period is `(n + 1) * $100`.
pub fn cia_changing() -> Psid {
    let init = format!("{INIT_RECORD}lda #$00\nsta $dc04\nlda #$10\nsta $dc05\nrts\n");
    let play = format!("{BANK_RECORD}{COUNT}sta $dc05\nlda #$ff\nsta $dc04\nrts\n");
    Psid::new(&[(INIT, &init), (PLAY, &play)]).speed(1)
}

/// 40 songs, speed bits 0 and 31 set; init sets the latch to 9,999 (a period of 10,000).
pub fn forty_songs() -> Psid {
    let init = format!("{INIT_RECORD}lda #$0f\nsta $dc04\nlda #$27\nsta $dc05\nrts\n");
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
        .songs(40)
        .speed(0x8000_0001)
}

/// Play address 0 through the KERNAL's vector `$0314`: init installs the handler, which
/// records S to register 6, then the bank, counts, and leaves through `JMP $EA31`, or (with
/// `rti`) restores Y, X and A from the stack as the KERNAL's exit does and returns with RTI.
pub fn irq_kernal(rti: bool) -> Psid {
    let init = format!(
        "{INIT_RECORD}lda #handler % 256\nsta $0314\nlda #handler / 256\nsta $0315\nrts\n\
         handler = ${PLAY:04x}\n"
    );
    let exit = if rti {
        "pla\ntay\npla\ntax\npla\nrti\n"
    } else {
        "jmp $ea31\n"
    };
    let handler = format!("tsx\nstx $d406\n{IRQ_BANK_RECORD}{COUNT}{exit}");
    Psid::new(&[(INIT, &init), (PLAY, &handler)]).play(0)
}

/// Play address 0 through the hardware vector `$FFFE`: init maps the KERNAL out (`$01 = $35`)
/// and sets the vector; the handler records S and the bank, counts, and returns with RTI.
pub fn irq_hardware() -> Psid {
    let init = format!(
        "{INIT_RECORD}lda #$35\nsta <$01\nlda #handler % 256\nsta $fffe\nlda #handler / 256\nsta $ffff\nrts\n\
         handler = ${PLAY:04x}\n"
    );
    let handler = format!("tsx\nstx $d406\n{IRQ_BANK_RECORD}{COUNT}rti\n");
    Psid::new(&[(INIT, &init), (PLAY, &handler)]).play(0)
}

/// Init and play at the given addresses, each recording the bank it was called with.
pub fn banks(init: u16, play: u16) -> Psid {
    let init_src = format!("{BANK_RECORD}rts\n");
    Psid::new(&[(init, &init_src), (play, &play_src())])
}

/// `n` times round a loop of 1,286 cycles (`n * 1286 + 1` cycles in all, `n` 1-256), with X
/// and Y.
pub fn delay(n: u32) -> String {
    assert!((1..=256).contains(&n));
    format!(
        "ldy #${:02x}\nd_outer: ldx #$00\nd_inner: dex\nbne d_inner\ndey\nbne d_outer\n",
        n & 0xFF
    )
}

/// An init of about two seconds (six rounds of 256 delays: 1,975,350 cycles and the RTS).
pub fn slow_init() -> Psid {
    let init = format!(
        "lda #$06\nsta <$fc\ns_loop: {}dec <$fc\nbne s_loop\nlda #$0f\nsta $d418\nrts\n",
        delay(256)
    );
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

/// An init that never returns.
pub fn init_forever() -> Psid {
    Psid::new(&[(INIT, "forever: jmp forever\n"), (PLAY, &play_src())])
}

/// A play routine whose first call runs `delays` delays (23: 29,579 cycles, about 1.5 PAL
/// frames), the rest briefly. Init returns at once (6 cycles).
pub fn play_overrun(delays: u32) -> Psid {
    let play = format!(
        "{BANK_RECORD}lda <$fb\nbne quick\n{}quick: {COUNT}rts\n",
        delay(delays)
    );
    Psid::new(&[(INIT, "rts\n"), (PLAY, &play)])
}

/// A play routine whose third call never returns.
pub fn play_forever() -> Psid {
    let play = format!("{BANK_RECORD}{COUNT}cmp #$03\nbne done\nforever: jmp forever\ndone: rts\n");
    Psid::new(&[(INIT, "rts\n"), (PLAY, &play)])
}

/// A play routine that JAMs (opcode `$02`) on its second call, at `PLAY + 20`.
pub fn jam() -> Psid {
    let play = format!("{BANK_RECORD}{COUNT}cmp #$02\nbne done\n.BYTE ($02)\ndone: rts\n");
    Psid::new(&[(INIT, "rts\n"), (PLAY, &play)])
}

/// An init that writes the SID 256 times, 90 cycles apart (about 23,000 cycles), then
/// returns; play counts. The slicing tests' tune.
pub fn busy_init() -> Psid {
    let init = format!(
        "{INIT_RECORD}ldx #$00\nb_loop: stx $d400\nldy #$10\nb_wait: dey\nbne b_wait\ninx\nbne b_loop\nrts\n"
    );
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

/// Code that takes exactly `n` cycles (0, or 2 and up: no instruction takes 1): rounds of
/// 1,286 cycles (plus 1), a countdown loop of `5k + 1`, and padding of NOPs and one `BIT <$FC`.
/// Uses X and Y, not A.
pub fn exact(n: u32) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static LABELS: AtomicU32 = AtomicU32::new(0);
    assert!(n != 1, "one cycle is not an instruction");
    let mut out = String::new();
    let mut rem = n;
    // Rounds of up to 256 delays (329,217 cycles), as many as it takes, the last leaving at
    // least 2 cycles and not 1.
    while rem >= 1294 {
        let id = LABELS.fetch_add(1, Ordering::Relaxed);
        let mut m = ((rem - 8) / 1286).min(256);
        if rem - (1286 * m + 1) == 1 {
            m -= 1;
        }
        out += &format!(
            "ldy #${:02x}\nr{id}o: ldx #$00\nr{id}i: dex\nbne r{id}i\ndey\nbne r{id}o\n",
            m & 0xFF
        );
        rem -= 1286 * m + 1;
    }
    if rem >= 6 {
        let id = LABELS.fetch_add(1, Ordering::Relaxed);
        let mut k = ((rem - 1) / 5).min(256);
        if rem - (5 * k + 1) == 1 {
            k -= 1;
        }
        if k > 0 {
            out += &format!("ldx #${:02x}\nr{id}l: dex\nbne r{id}l\n", k & 0xFF);
            rem -= 5 * k + 1;
        }
    }
    assert!(rem != 1);
    if rem % 2 == 1 {
        out += "bit <$fc\n";
        rem -= 3;
    }
    out + &"nop\n".repeat((rem / 2) as usize)
}

/// Init sets voice 3 to noise at full pitch, then eight times reads OSC3 into voice 1's
/// frequency: each `STA $D400` writes the value 4 cycles after the `LDA`'s read.
pub fn osc3_noise() -> Psid {
    let init = "lda #$ff\nsta $d40e\nsta $d40f\nlda #$81\nsta $d412\nldx #$08\n\
                o_loop: lda $d41b\nsta $d400\nldy #$20\no_wait: dey\nbne o_wait\ndex\nbne o_loop\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())])
}

/// Init gates voice 3 with the fastest attack and decay and a sustain of 0, waits for ENV3 to
/// rise, then to come back to 0 (a loop that never ends if ENV3 reads 0 or never falls), and
/// writes X (0) to register 0: that write is 6 cycles after the last read.
pub fn env3_wait() -> Psid {
    let init = "lda #$00\nsta $d413\nsta $d414\nldx #$00\nlda #$11\nsta $d412\n\
                w1: lda $d41c\nbeq w1\nw2: lda $d41c\nbne w2\nstx $d400\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())])
}

/// Init waits for the raster to reach `line` (low eight bits, `$D012`) and writes X (0) to
/// register 0: 8 cycles after the read that saw it (`LDA`, `CMP`, `BNE`, `STA`).
pub fn raster_wait(line: u8) -> Psid {
    let init =
        format!("ldx #$00\nr_loop: lda $d012\ncmp #${line:02x}\nbne r_loop\nstx $d400\nrts\n");
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

/// Init waits for bit 7 of `$D011` (the raster's ninth bit) and writes X (0) to register 0:
/// 6 cycles after the `BIT`'s read.
pub fn raster_high_wait() -> Psid {
    let init = "ldx #$00\nh_loop: bit $d011\nbpl h_loop\nstx $d400\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())])
}

/// A CIA tune (latch `latch`) whose play call writes `$D012`'s value to register 0, the read
/// 4 cycles before the write, the first instruction.
pub fn raster_in_play(latch: u16) -> Psid {
    let play = "lda $d012\nsta $d400\nrts\n";
    let init = format!(
        "lda #${:02x}\nsta $dc04\nlda #${:02x}\nsta $dc05\nrts\n",
        latch & 0xFF,
        latch >> 8
    );
    Psid::new(&[(INIT, &init), (PLAY, play)]).speed(1)
}

/// A CIA tune: init sets the latch to `$0900`, then reads timer A's counter, low then high
/// byte, into registers 0 and 1 (each write 4 cycles after its read); play reads high then low
/// byte into registers 2 and 3 the same way.
pub fn cia_counter() -> Psid {
    let init = "lda #$00\nsta $dc04\nlda #$09\nsta $dc05\nlda $dc04\nsta $d400\nlda $dc05\nsta $d401\nrts\n";
    let play = "lda $dc05\nsta $d402\nlda $dc04\nsta $d403\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, play)]).speed(1)
}

/// Init reads `$D019`, `$DC0D` and `$DD0D` into registers 0-2; play writes `$D019` as it
/// reads it into register 3, reads it again into 4, then `$DC0D` twice into 5 and 6 and
/// `$DD0D` into 7. A CIA tune with `cia`.
pub fn irq_flags(cia: bool) -> Psid {
    let init = "lda $d019\nsta $d400\nlda $dc0d\nsta $d401\nlda $dd0d\nsta $d402\nrts\n";
    let play = "lda $d019\nsta $d403\nsta $d019\nlda $d019\nsta $d404\n\
                lda $dc0d\nsta $d405\nlda $dc0d\nsta $d406\nlda $dd0d\nsta $d407\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, play)]).speed(u32::from(cia))
}

/// Init writes `$A5` to register 5 and, `d` cycles later, reads register 5 back into
/// register 0 (the read is the `LDA`'s last cycle; `d` is 4 or more, not 5).
pub fn bus_read(d: u32) -> Psid {
    let init = format!(
        "lda #$a5\nsta $d405\n{}lda $d405\nsta $d400\nrts\n",
        exact(d - 4)
    );
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

/// Init stores `A` to each of `addrs` in turn, the value being 1, 2, 3 ...
pub fn stores(addrs: &[u16]) -> Psid {
    let mut init = String::new();
    for (i, a) in addrs.iter().enumerate() {
        init += &format!("lda #${:02x}\nsta ${a:04x}\n", i + 1);
    }
    init += "rts\n";
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

/// Init writes `$11` to chip A's register 5 and `$22` to the second SID's (at `$D420`), reads
/// register 13 (write-only: the bus) of each, A's into register 0 and B's into register 1.
pub fn two_sid_buses() -> Psid {
    let init = "lda #$11\nsta $d405\nlda #$22\nsta $d425\nlda $d40d\nldx $d42d\n\
                sta $d400\nstx $d401\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())]).sid2(0x42)
}

/// Init writes the VIC, CIA and colour RAM and reads them back into registers 0-7: `$D021`
/// through its mirror at `$D061`, colour RAM `$D800` and `$DBFF` (0), `$DC02` and `$DD02`
/// (separately), `$DC02` through its mirror `$DC12`, `$DD0D` after a write (0), `$D011` after
/// a write of `$9B` (`$1B`: the raster's ninth bit is the line's, 0 so early).
pub fn io_readback() -> Psid {
    let init = "lda #$11\nsta $d021\nlda #$22\nsta $d061\nlda $d021\nsta $d400\n\
                lda #$77\nsta $d800\nsta $dbff\nlda $d800\nsta $d401\nlda $dbff\nsta $d402\n\
                lda #$33\nsta $dc02\nlda #$44\nsta $dd02\nlda $dc02\nsta $d403\nlda $dd02\nsta $d404\n\
                lda #$55\nsta $dc12\nlda $dc02\nsta $d405\n\
                lda #$81\nsta $dd0d\nlda $dd0d\nsta $d406\n\
                lda #$9b\nsta $d011\nlda $d011\nsta $d407\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())])
}

/// Init maps RAM in (`$01 = $34`) and writes `$77` to `$D021`, `$D400`, `$DC02` and `$D800`,
/// reads them back into zero page `$F0`-`$F3` and `$D012` into `$F4`, and returns.
pub fn io_ram() -> Psid {
    let init = "lda #$34\nsta <$01\nlda #$77\nsta $d021\nsta $d400\nsta $dc02\nsta $d800\n\
                lda $d021\nsta <$f0\nlda $d400\nsta <$f1\nlda $dc02\nsta <$f2\n\
                lda $d800\nsta <$f3\nlda $d012\nsta <$f4\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())])
}

/// Init does a read-modify-write of a SID register: `ASL $D405` of `$05`.
pub fn rmw() -> Psid {
    Psid::new(&[
        (INIT, "lda #$05\nsta $d405\nasl $d405\nrts\n"),
        (PLAY, &play_src()),
    ])
}
