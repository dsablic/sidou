// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use gt2_ref::assemble;

pub const CLOCK_PAL: u16 = 1 << 2;
pub const CLOCK_NTSC: u16 = 2 << 2;
pub const FLAG_PLAYSID: u16 = 1 << 1;

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
    pub sid2: u8,
    pub sid3: u8,
    pub data: Vec<u8>,
}

impl Psid {
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

pub const LDA_IMM_STA_ABS_CYCLES: u32 = 6;

pub const INIT_RECORD: &str = "sta $d402\nstx $d403\nsty $d404\n";

pub const BANK_RECORD: &str = "lda <$01\nldx #$35\nstx <$01\nsta $d401\n";

pub const IRQ_BANK_RECORD: &str = "lda <$01\nsta $d401\n";

pub const COUNT: &str = "inc <$fb\nlda <$fb\nsta $d400\n";

pub const INIT: u16 = 0x1000;
pub const PLAY: u16 = 0x1100;

pub fn play_src() -> String {
    format!("{BANK_RECORD}{COUNT}rts\n")
}

pub fn vbi() -> Psid {
    let init = format!("{INIT_RECORD}lda #$0f\nsta $d418\nrts\n");
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

pub fn cia(latch: u16) -> Psid {
    let init = format!(
        "{INIT_RECORD}lda #${:02x}\nsta $dc04\nlda #${:02x}\nsta $dc05\nrts\n",
        latch & 0xFF,
        latch >> 8
    );
    Psid::new(&[(INIT, &init), (PLAY, &play_src())]).speed(1)
}

pub fn cia_default() -> Psid {
    let init = format!("{INIT_RECORD}rts\n");
    Psid::new(&[(INIT, &init), (PLAY, &play_src())]).speed(1)
}

pub fn cia_changing() -> Psid {
    let init = format!("{INIT_RECORD}lda #$00\nsta $dc04\nlda #$10\nsta $dc05\nrts\n");
    let play = format!("{BANK_RECORD}{COUNT}sta $dc05\nlda #$ff\nsta $dc04\nrts\n");
    Psid::new(&[(INIT, &init), (PLAY, &play)]).speed(1)
}

pub fn forty_songs() -> Psid {
    let init = format!("{INIT_RECORD}lda #$0f\nsta $dc04\nlda #$27\nsta $dc05\nrts\n");
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
        .songs(40)
        .speed(0x8000_0001)
}

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

pub fn irq_hardware() -> Psid {
    let init = format!(
        "{INIT_RECORD}lda #$35\nsta <$01\nlda #handler % 256\nsta $fffe\nlda #handler / 256\nsta $ffff\nrts\n\
         handler = ${PLAY:04x}\n"
    );
    let handler = format!("tsx\nstx $d406\n{IRQ_BANK_RECORD}{COUNT}rti\n");
    Psid::new(&[(INIT, &init), (PLAY, &handler)]).play(0)
}

pub fn banks(init: u16, play: u16) -> Psid {
    let init_src = format!("{BANK_RECORD}rts\n");
    Psid::new(&[(init, &init_src), (play, &play_src())])
}

pub fn delay(n: u32) -> String {
    assert!((1..=256).contains(&n));
    format!(
        "ldy #${:02x}\nd_outer: ldx #$00\nd_inner: dex\nbne d_inner\ndey\nbne d_outer\n",
        n & 0xFF
    )
}

pub fn slow_init() -> Psid {
    let init = format!(
        "lda #$06\nsta <$fc\ns_loop: {}dec <$fc\nbne s_loop\nlda #$0f\nsta $d418\nrts\n",
        delay(256)
    );
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

pub fn init_forever() -> Psid {
    Psid::new(&[(INIT, "forever: jmp forever\n"), (PLAY, &play_src())])
}

pub fn play_overrun(delays: u32) -> Psid {
    let play = format!(
        "{BANK_RECORD}lda <$fb\nbne quick\n{}quick: {COUNT}rts\n",
        delay(delays)
    );
    Psid::new(&[(INIT, "rts\n"), (PLAY, &play)])
}

pub fn play_forever() -> Psid {
    let play = format!("{BANK_RECORD}{COUNT}cmp #$03\nbne done\nforever: jmp forever\ndone: rts\n");
    Psid::new(&[(INIT, "rts\n"), (PLAY, &play)])
}

pub fn jam() -> Psid {
    let play = format!("{BANK_RECORD}{COUNT}cmp #$02\nbne done\n.BYTE ($02)\ndone: rts\n");
    Psid::new(&[(INIT, "rts\n"), (PLAY, &play)])
}

pub fn busy_init() -> Psid {
    let init = format!(
        "{INIT_RECORD}ldx #$00\nb_loop: stx $d400\nldy #$10\nb_wait: dey\nbne b_wait\ninx\nbne b_loop\nrts\n"
    );
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

pub fn exact(n: u32) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static LABELS: AtomicU32 = AtomicU32::new(0);
    assert!(n != 1, "one cycle is not an instruction");
    let mut out = String::new();
    let mut rem = n;
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

pub fn osc3_noise() -> Psid {
    let init = "lda #$ff\nsta $d40e\nsta $d40f\nlda #$81\nsta $d412\nldx #$08\n\
                o_loop: lda $d41b\nsta $d400\nldy #$20\no_wait: dey\nbne o_wait\ndex\nbne o_loop\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())])
}

pub fn env3_wait() -> Psid {
    let init = "lda #$00\nsta $d413\nsta $d414\nldx #$00\nlda #$11\nsta $d412\n\
                w1: lda $d41c\nbeq w1\nw2: lda $d41c\nbne w2\nstx $d400\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())])
}

pub fn raster_wait(line: u8) -> Psid {
    let init =
        format!("ldx #$00\nr_loop: lda $d012\ncmp #${line:02x}\nbne r_loop\nstx $d400\nrts\n");
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

pub fn raster_high_wait() -> Psid {
    let init = "ldx #$00\nh_loop: bit $d011\nbpl h_loop\nstx $d400\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())])
}

pub fn raster_in_play(latch: u16) -> Psid {
    let play = "lda $d012\nsta $d400\nrts\n";
    let init = format!(
        "lda #${:02x}\nsta $dc04\nlda #${:02x}\nsta $dc05\nrts\n",
        latch & 0xFF,
        latch >> 8
    );
    Psid::new(&[(INIT, &init), (PLAY, play)]).speed(1)
}

pub fn cia_counter() -> Psid {
    let init = "lda #$00\nsta $dc04\nlda #$09\nsta $dc05\nlda $dc04\nsta $d400\nlda $dc05\nsta $d401\nrts\n";
    let play = "lda $dc05\nsta $d402\nlda $dc04\nsta $d403\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, play)]).speed(1)
}

pub fn irq_flags(cia: bool) -> Psid {
    let init = "lda $d019\nsta $d400\nlda $dc0d\nsta $d401\nlda $dd0d\nsta $d402\nrts\n";
    let play = "lda $d019\nsta $d403\nsta $d019\nlda $d019\nsta $d404\n\
                lda $dc0d\nsta $d405\nlda $dc0d\nsta $d406\nlda $dd0d\nsta $d407\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, play)]).speed(u32::from(cia))
}

pub fn bus_read(d: u32) -> Psid {
    let init = format!(
        "lda #$a5\nsta $d405\n{}lda $d405\nsta $d400\nrts\n",
        exact(d - 4)
    );
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

pub fn stores(addrs: &[u16]) -> Psid {
    let mut init = String::new();
    for (i, a) in addrs.iter().enumerate() {
        init += &format!("lda #${:02x}\nsta ${a:04x}\n", i + 1);
    }
    init += "rts\n";
    Psid::new(&[(INIT, &init), (PLAY, &play_src())])
}

pub fn two_sid_buses() -> Psid {
    let init = "lda #$11\nsta $d405\nlda #$22\nsta $d425\nlda $d40d\nldx $d42d\n\
                sta $d400\nstx $d401\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())]).sid2(0x42)
}

pub fn io_readback() -> Psid {
    let init = "lda #$11\nsta $d021\nlda #$22\nsta $d061\nlda $d021\nsta $d400\n\
                lda #$77\nsta $d800\nsta $dbff\nlda $d800\nsta $d401\nlda $dbff\nsta $d402\n\
                lda #$33\nsta $dc02\nlda #$44\nsta $dd02\nlda $dc02\nsta $d403\nlda $dd02\nsta $d404\n\
                lda #$55\nsta $dc12\nlda $dc02\nsta $d405\n\
                lda #$81\nsta $dd0d\nlda $dd0d\nsta $d406\n\
                lda #$9b\nsta $d011\nlda $d011\nsta $d407\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())])
}

pub fn io_ram() -> Psid {
    let init = "lda #$34\nsta <$01\nlda #$77\nsta $d021\nsta $d400\nsta $dc02\nsta $d800\n\
                lda $d021\nsta <$f0\nlda $d400\nsta <$f1\nlda $dc02\nsta <$f2\n\
                lda $d800\nsta <$f3\nlda $d012\nsta <$f4\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, &play_src())])
}

pub fn rmw() -> Psid {
    Psid::new(&[
        (INIT, "lda #$05\nsta $d405\nasl $d405\nrts\n"),
        (PLAY, &play_src()),
    ])
}
