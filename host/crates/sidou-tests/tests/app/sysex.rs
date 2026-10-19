// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Wire-level tests against the C engine, simulated NOR, and USB event packets.
use super::store::{fresh, Rig, Rng};
use sidou_sys::*;
use std::ffi::c_void;

#[derive(Default)]
struct Hooks {
    packets: Vec<[u8; 4]>,
    toasts: Vec<u8>,
    blocked: bool,
    source: Vec<u8>,
    reads: Vec<(u32, u32)>,
    opens: Vec<(u8, u8, bool, u32)>,
    closes: usize,
    completed: usize,
    refuse_open: i32,
    refuse_complete: i32,
    writing: Vec<bool>,
    pending: usize,
    committed: Vec<(u32, u32, u32)>,
    unavailable: i32,
}

unsafe fn context<'a>(ctx: *mut c_void) -> &'a mut Hooks {
    unsafe { &mut *ctx.cast::<Hooks>() }
}

unsafe extern "C" fn send(ctx: *mut c_void, packet: *const u8) -> i32 {
    let h = unsafe { context(ctx) };
    if h.blocked {
        return 0;
    }
    h.packets.push(
        unsafe { std::slice::from_raw_parts(packet, 4) }
            .try_into()
            .unwrap(),
    );
    1
}

unsafe extern "C" fn toast(ctx: *mut c_void, reason: u8) {
    unsafe { context(ctx) }.toasts.push(reason);
}

unsafe extern "C" fn open(
    ctx: *mut c_void,
    target: u8,
    index: u8,
    writing: i32,
    length: u32,
) -> i32 {
    let h = unsafe { context(ctx) };
    h.opens.push((target, index, writing != 0, length));
    h.refuse_open
}

unsafe extern "C" fn size(ctx: *mut c_void) -> u32 {
    unsafe { context(ctx) }.source.len() as u32
}

unsafe extern "C" fn read(ctx: *mut c_void, offset: u32, buf: *mut c_void, n: u32) -> i32 {
    let h = unsafe { context(ctx) };
    h.reads.push((offset, n));
    let Some(src) = h.source.get(offset as usize..offset as usize + n as usize) else {
        return -1;
    };
    unsafe { std::slice::from_raw_parts_mut(buf.cast::<u8>(), n as usize) }.copy_from_slice(src);
    n as i32
}

unsafe extern "C" fn complete(ctx: *mut c_void, file: *const store_file) -> i32 {
    let h = unsafe { context(ctx) };
    h.completed += 1;
    let file = unsafe { &*file };
    h.committed.push((file.key, file.fid, file.length));
    if h.pending != 0 {
        h.pending -= 1;
        return SYSEX_PENDING;
    }
    h.refuse_complete
}

unsafe extern "C" fn close(ctx: *mut c_void) {
    unsafe { context(ctx) }.closes += 1;
}

unsafe extern "C" fn writing(ctx: *mut c_void, on: i32) {
    let h = unsafe { context(ctx) };
    assert!(h.writing.last().is_none_or(|last| *last != (on != 0)));
    h.writing.push(on != 0);
}

unsafe extern "C" fn available(ctx: *mut c_void) -> i32 {
    unsafe { context(ctx) }.unavailable
}

fn frame(command: u8, sequence: u8, payload: &[u8]) -> Vec<u8> {
    let mut wire = vec![0xf0, 0x7d, 0x53, 0x44, command, sequence];
    for group in payload.chunks(7) {
        wire.push(
            group
                .iter()
                .enumerate()
                .fold(0, |m, (i, b)| m | ((b >> 7) << i)),
        );
        wire.extend(group.iter().map(|b| b & 127));
    }
    let sum = wire[4..].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    wire.extend([sum.wrapping_neg() & 127, 0xf7]);
    let packet = sidou_sysex::Packet {
        command,
        sequence,
        data: payload.to_vec(),
    };
    if packet.validate().is_ok() {
        assert_eq!(packet.encode().unwrap(), wire);
    }
    wire
}

fn decode(wire: &[u8]) -> (u8, u8, Vec<u8>) {
    assert_eq!(&wire[..4], &[0xf0, 0x7d, 0x53, 0x44]);
    assert_eq!(wire.last(), Some(&0xf7));
    assert!(wire[1..wire.len() - 1].iter().all(|b| *b < 128));
    assert_eq!(
        wire[4..wire.len() - 1]
            .iter()
            .fold(0u8, |a, b| a.wrapping_add(*b))
            & 127,
        0
    );
    let mut payload = Vec::new();
    for group in wire[6..wire.len() - 2].chunks(8) {
        assert!(group.len() > 1);
        assert_eq!(group[0] >> (group.len() - 1), 0, "canonical mask");
        payload.extend(
            group[1..]
                .iter()
                .enumerate()
                .map(|(i, b)| b | (((group[0] >> i) & 1) << 7)),
        );
    }
    let packet = sidou_sysex::Packet::decode(wire).unwrap();
    assert_eq!(
        (packet.command, packet.sequence, &packet.data),
        (wire[4], wire[5], &payload)
    );
    (wire[4], wire[5], payload)
}

fn named(name: &str) -> Vec<u8> {
    let mut p = vec![0; 18];
    p[2..2 + name.len()].copy_from_slice(name.as_bytes());
    p
}

fn upload_metadata(mut p: Vec<u8>, data: &[u8]) -> Vec<u8> {
    p.push(1);
    p.extend((data.len() as u32).to_le_bytes());
    p.extend(unsafe { store_crc32(0, data.as_ptr().cast(), data.len() as u32) }.to_le_bytes());
    p
}

struct Engine {
    engine: Box<sysex>,
    rig: Rig,
    hooks: Box<Hooks>,
    now: u32,
}

impl Engine {
    fn new(windows: bool) -> Self {
        let mut rig = fresh(0x5359_5345, 16);
        let mut hooks = Box::<Hooks>::default();
        let callbacks = sysex_hooks {
            ctx: (&mut *hooks as *mut Hooks).cast(),
            open: Some(open),
            size: Some(size),
            read: Some(read),
            complete: Some(complete),
            close: Some(close),
            send: Some(send),
            toast: Some(toast),
            writing: Some(writing),
            available: Some(available),
        };
        let mut engine = Box::<sysex>::default();
        unsafe {
            sysex_init(
                &mut *engine,
                &mut *rig.st,
                &callbacks,
                [1, 2, 3, 4].as_ptr(),
                windows.into(),
            )
        };
        Self {
            engine,
            rig,
            hooks,
            now: 0,
        }
    }

    fn bytes(&mut self, port: u8, wire: &[u8]) {
        for &b in wire {
            unsafe { sysex_byte(&mut *self.engine, port, b, self.now) };
        }
    }

    fn request(&mut self, command: u8, sequence: u8, payload: &[u8]) {
        self.bytes(0, &frame(command, sequence, payload));
    }

    fn poll(&mut self) {
        unsafe { sysex_poll(&mut *self.engine, self.now) };
        // The platform supplies its current window afresh on every store pass.
        self.rig.st.window = self.rig.window;
        self.rig.poll();
    }

    fn wire(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for packet in self.hooks.packets.drain(..) {
            let n = match packet[0] {
                4 => 3,
                5 => 1,
                6 => 2,
                7 => 3,
                _ => panic!("bad USB CIN: {packet:?}"),
            };
            if packet[0] != 4 {
                assert_eq!(packet[n], 0xf7);
                assert!(packet[n + 1..].iter().all(|b| *b == 0));
            }
            bytes.extend_from_slice(&packet[1..=n]);
        }
        bytes
    }

    fn response(&mut self) -> (u8, u8, Vec<u8>) {
        let mut wire = Vec::new();
        for _ in 0..20_000 {
            self.poll();
            wire.extend(self.wire());
            if wire.last() == Some(&0xf7) {
                return decode(&wire);
            }
        }
        panic!("no complete response, state {:?}", self.engine);
    }

    fn ack(&mut self, sequence: u8) {
        assert_eq!(self.response(), (0x7e, sequence, vec![]));
    }

    fn nak(&mut self, sequence: u8, reason: u32) {
        let (cmd, seq, data) = self.response();
        assert_eq!((cmd, seq), (0x7f, sequence));
        assert_eq!(data.len(), 2);
        assert_eq!(data[0], reason as u8);
        assert_eq!(self.hooks.toasts.last(), Some(&(reason as u8)));
    }

    fn active(&self) -> bool {
        unsafe { sysex_active(&*self.engine) != 0 }
    }

    fn upload(&mut self, target: Vec<u8>, data: &[u8], chunks: usize) {
        self.request(4, 126, &upload_metadata(target, data));
        assert_eq!(self.hooks.writing.last(), Some(&true));
        assert_eq!(self.hooks.toasts.last(), Some(&(SYSEX_RECEIVING as u8)));
        assert!(self.hooks.packets.is_empty());
        self.ack(126);
        let mut seq = 127;
        for bytes in data.chunks(chunks) {
            self.request(5, seq, bytes);
            self.ack(seq);
            seq = (seq + 1) & 127;
        }
        self.request(6, seq, &[]);
        self.ack(seq);
        assert!(!self.active());
        assert_eq!(self.hooks.writing.last(), Some(&false));
        assert_eq!(self.hooks.toasts.last(), Some(&(SYSEX_SAVED as u8)));
    }
}

#[test]
fn a_refused_trs_upload_drops_its_tail_without_overwriting_the_refusal() {
    let mut e = Engine::new(false);
    let data = vec![0x25; 32];
    e.hooks.unavailable = SYSEX_BUSY as i32;
    e.bytes(1, &frame(4, 0, &upload_metadata(named("REFUSAL"), &data)));
    assert_eq!(e.hooks.toasts.last(), Some(&(SYSEX_BUSY as u8)));
    e.bytes(1, &frame(5, 1, &data));
    e.bytes(1, &frame(6, 2, &[]));
    assert_eq!(e.hooks.toasts.last(), Some(&(SYSEX_BUSY as u8)));
    assert!(e.hooks.writing.is_empty());
    assert!(!e.active());
    e.hooks.unavailable = 0;
    e.bytes(1, &frame(4, 0, &upload_metadata(named("REFUSAL"), &data)));
    e.bytes(1, &frame(5, 1, &data));
    for _ in 0..256 {
        e.poll();
    }
    e.bytes(1, &frame(6, 2, &[]));
    for _ in 0..256 {
        e.poll();
    }
    assert_eq!(e.rig.file("REFUSAL"), Some(data));
}

#[test]
fn unsupported_trs_requests_do_not_cancel_a_packet_waiting_for_flash() {
    let mut e = Engine::new(false);
    let data = vec![0xa7; 32];
    e.bytes(
        1,
        &frame(4, 0, &upload_metadata(named("TRS IGNORE"), &data)),
    );
    e.bytes(1, &frame(5, 1, &data[..16]));
    for command in [1, 2, 3, 7, 0x7e, 0x7f] {
        e.bytes(1, &frame(command, 97, &[]));
        assert_eq!(e.engine.claimed, 1, "unsupported TRS command {command}");
    }
    for _ in 0..256 {
        e.poll();
    }
    assert_eq!(e.engine.sequence, 2);
    e.bytes(1, &frame(5, 2, &data[16..]));
    for _ in 0..256 {
        e.poll();
    }
    e.bytes(1, &frame(6, 3, &[]));
    for _ in 0..256 {
        e.poll();
    }
    assert_eq!(e.rig.file("TRS IGNORE"), Some(data));
    assert!(!e.active());
    assert!(e.wire().is_empty());
}

#[test]
fn inquiry_is_packetised_and_reports_version() {
    let mut e = Engine::new(false);
    e.bytes(0, &[0xf0, 0x7e, 0x7f, 6, 1, 0xf7]);
    for _ in 0..20 {
        e.poll();
    }
    assert_eq!(
        e.wire(),
        [0xf0, 0x7e, 0x7f, 6, 2, 0x7d, 0x53, 0x44, 1, 0, 1, 2, 3, 4, 0xf7]
    );
    e.request(1, 47, &[]);
    let (cmd, seq, p) = e.response();
    assert_eq!((cmd, seq), (1, 47));
    assert_eq!(&p[..7], &[1, 1, 2, 3, 4, STORE_FORMAT as u8, 0]);
    assert_eq!(
        u32::from_le_bytes(p[7..11].try_into().unwrap()),
        e.rig.stats().free
    );
    assert_eq!(p[11], e.rig.stats().pool as u8);
    assert_eq!(p[12], 0);
    assert!(!e.active());
    for port in 0..2 {
        e.bytes(port, &[0xf0, 0x7e, 1, 6, 1, 0xf7]);
        e.bytes(port, &[0xf0, 0x01, 0x53, 0x44, 1, 0, 0, 0xf7]);
    }
    e.bytes(1, &[0xf0, 0x7e, 0, 6, 1, 0xf7]);
    for _ in 0..20 {
        e.poll();
    }
    assert!(e.wire().is_empty());
}

#[test]
fn store_upload_download_directory_and_delete_roundtrip() {
    let mut e = Engine::new(true);
    let data: Vec<u8> = (0..4097).map(|n| (n * 137 + 19) as u8).collect();
    e.upload(named("ROUNDTRIP"), &data, 256);
    assert_eq!(e.rig.file("ROUNDTRIP"), Some(data.clone()));
    e.request(3, 127, &named("ROUNDTRIP"));
    assert_eq!(
        e.response(),
        (4, 127, upload_metadata(named("ROUNDTRIP"), &data))
    );
    e.request(0x7e, 127, &[]);
    let mut decoded = Vec::new();
    let mut seq = 0;
    loop {
        let (cmd, got_seq, payload) = e.response();
        assert_eq!(got_seq, seq);
        e.request(0x7e, seq, &[]);
        seq = (seq + 1) & 127;
        if cmd == 6 {
            assert!(payload.is_empty());
            break;
        }
        assert_eq!(cmd, 5);
        assert!(!payload.is_empty() && payload.len() <= 256);
        decoded.extend(payload);
    }
    assert_eq!(decoded, data);
    assert!(!e.active());
    e.request(2, 12, &[]);
    let (cmd, seq, p) = e.response();
    assert_eq!((cmd, seq), (2, 12));
    assert_eq!(&p[..9], b"ROUNDTRIP");
    assert_eq!(p[16], 1);
    assert_eq!(
        u32::from_le_bytes(p[17..21].try_into().unwrap()),
        data.len() as u32
    );
    e.request(0x7e, 12, &[]);
    assert_eq!(e.response(), (6, 13, vec![]));
    e.request(0x7e, 13, &[]);
    e.request(7, 14, &named("ROUNDTRIP")[2..]);
    e.ack(14);
    assert_eq!(e.rig.file("ROUNDTRIP"), None);
    e.request(3, 15, &named("ROUNDTRIP"));
    e.nak(15, SYSEX_NOT_FOUND);
    e.request(2, 16, &[]);
    assert_eq!(e.response(), (6, 16, vec![]));
    e.request(0x7e, 16, &[]);
    e.rig.assert_clean();
}

#[test]
fn every_packing_length_and_short_packet_programming() {
    for n in 0..=256 {
        let mut e = Engine::new(true);
        let data: Vec<u8> = (0..n).map(|i| (i * 193 + n) as u8).collect();
        e.upload(named("LENGTHS"), &data, 256);
        assert_eq!(e.rig.file("LENGTHS"), Some(data));
        e.rig.assert_clean();
    }
}

#[test]
fn small_aligned_packets_preserve_partial_page_program_bound() {
    let mut worst = 0;
    for window in [16, 17, 31, 32, 63, 64, 127, 128, 255, 256] {
        for packet in (16..=256).step_by(16) {
            let mut e = Engine::new(true);
            e.rig.window = window;
            e.rig.st.window = window;
            let data: Vec<u8> = (0..65536).map(|i| (i * 139) as u8).collect();
            e.upload(named("SMALL PACKETS"), &data, packet);
            assert_eq!(e.rig.file("SMALL PACKETS"), Some(data));
            assert!(e.rig.sim.page_progs_max <= STORE_PAGE_PROGS as u16);
            worst = worst.max(e.rig.sim.page_progs_max);
            e.rig.assert_clean();
        }
    }
    eprintln!("small DATA packets: at most {worst}/{STORE_PAGE_PROGS} programs per page");
}

#[test]
fn nonfinal_unaligned_data_is_refused_before_programming() {
    let mut e = Engine::new(true);
    assert_eq!(e.rig.file_now("KEEP", &[9, 8, 7]), STORE_OK);
    e.rig.maintain();
    e.request(4, 0, &upload_metadata(named("KEEP"), &[3; 32]));
    e.ack(0);
    let programs = e.rig.sim.progs;
    e.request(5, 1, &[3]);
    e.nak(1, SYSEX_MALFORMED);
    assert_eq!(e.rig.sim.progs, programs);
    assert_eq!(e.rig.file("KEEP"), Some(vec![9, 8, 7]));
}

#[test]
fn empty_and_maximum_files_never_buffer_a_dump() {
    for n in [0, 65536] {
        let mut e = Engine::new(true);
        let data = vec![0xa5; n];
        e.upload(named("BOUNDARY"), &data, 256);
        assert_eq!(e.rig.file("BOUNDARY"), Some(data));
        e.rig.assert_clean();
    }
    const { assert!(ilp32::sysex::SIZE <= 1024) };
}

#[test]
fn disabled_windows_accept_explicit_writes_and_preserve_another_owner() {
    let mut e = Engine::new(false);
    e.upload(named("EXPLICIT"), &[1], 256);
    assert_eq!(e.rig.file("EXPLICIT"), Some(vec![1]));
    e.engine.hooks.available = None;
    e.request(7, 3, &named("EXPLICIT")[2..]);
    e.ack(3);
    assert_eq!(e.rig.file("EXPLICIT"), None);
    assert!(e.hooks.opens.is_empty());
    assert_eq!(e.rig.file_begin("OTHER OWNER", 1), STORE_OK);
    unsafe { sysex_reset(&mut *e.engine) };
    assert_eq!(e.rig.file_result(), STORE_BUSY);
    e.request(4, 5, &upload_metadata(named("REFUSED"), &[8]));
    e.nak(5, SYSEX_BUSY);
    assert_eq!(e.rig.file_result(), STORE_BUSY);
    unsafe { store_file_abort(&mut *e.rig.st) };

    let mut e = Engine::new(true);
    e.request(4, 0, &upload_metadata(named("OUR CLAIM"), &[1]));
    e.ack(0);
    // An independent store backstop may end our claim before the engine runs.
    unsafe { store_file_abort(&mut *e.rig.st) };
    assert_eq!(e.rig.file_begin("NEW OWNER", 1), STORE_OK);
    unsafe { sysex_reset(&mut *e.engine) };
    assert_eq!(e.rig.file_result(), STORE_BUSY);
    unsafe { store_file_abort(&mut *e.rig.st) };
}

#[test]
fn shared_save_freeze_refuses_all_write_targets_before_claim_or_ownership() {
    let mut e = Engine::new(false);
    assert_eq!(e.rig.file_now("EXISTS", &[9]), STORE_OK);
    e.rig.maintain();
    e.hooks.unavailable = SYSEX_BUSY as i32;
    let stats = e.rig.stats();
    let programs = e.rig.sim.progs;
    for target in 0..=2 {
        let mut p = named("FROZEN");
        if target != 0 {
            p.fill(0);
            p[0] = target;
            p[1] = u8::from(target == 1);
        }
        e.request(4, target, &upload_metadata(p, &[1; 16]));
        e.nak(target, SYSEX_BUSY);
    }
    e.request(7, 3, &named("EXISTS")[2..]);
    e.nak(3, SYSEX_BUSY);
    assert_eq!(e.rig.sim.progs, programs);
    assert_eq!(e.rig.stats().pool, stats.pool);
    assert_eq!(e.rig.stats().live, stats.live);
    assert!(e.hooks.writing.is_empty());
    assert!(e.hooks.opens.is_empty());
    assert_eq!(e.rig.file("EXISTS"), Some(vec![9]));
}

#[test]
fn delayed_ack_excludes_storage_backpressure_and_end_waits_for_commit() {
    let mut e = Engine::new(false);
    let data = vec![0xc7; 257];
    e.request(4, 10, &upload_metadata(named("DELAYED"), &data));
    e.ack(10);
    e.request(5, 11, &data[..256]);
    // A real flash hook may defer programming for longer than the sender deadline.
    e.rig.sim.later = 100;
    for _ in 0..5 {
        e.now += 1000;
        e.poll();
        unsafe { store_file_idle(&mut *e.rig.st, 1000) };
    }
    assert!(e.wire().is_empty());
    assert!(e.active());
    assert_eq!(e.hooks.writing, [true]);
    e.rig.sim.later = 0;
    e.ack(11);
    e.request(5, 12, &data[256..]);
    e.ack(12);
    e.request(6, 13, &[]);
    assert_eq!(e.rig.file("DELAYED"), None);
    unsafe { sysex_poll(&mut *e.engine, e.now) };
    assert!(e.wire().is_empty());
    e.ack(13);
    assert_eq!(e.rig.file("DELAYED"), Some(data));
    assert_eq!(e.hooks.writing, [true, false]);
}

#[test]
fn blocked_ack_starts_sender_deadline_only_after_the_final_usb_packet_is_accepted() {
    let mut e = Engine::new(false);
    e.hooks.blocked = true;
    e.request(4, 0, &upload_metadata(named("ACK DEADLINE"), &[7; 16]));
    e.poll();
    for _ in 0..6 {
        e.now += 1000;
        e.poll();
        unsafe { store_file_idle(&mut *e.rig.st, 1000) };
    }
    assert!(e.wire().is_empty());
    assert_eq!(e.hooks.toasts, [SYSEX_RECEIVING as u8]);
    assert_eq!(e.hooks.writing, [true]);
    e.hooks.blocked = false;
    e.ack(0);
    e.now += SYSEX_TIMEOUT_MS - 1;
    e.request(5, 1, &[7; 16]);
    e.ack(1);
    e.now += SYSEX_TIMEOUT_MS;
    e.nak(2, SYSEX_TIMEOUT);
    assert_eq!(e.hooks.writing, [true, false]);
}

#[test]
fn write_owner_releases_on_every_abort_path_and_survives_irrevocable_commit() {
    for abort in 0..5 {
        let mut e = Engine::new(false);
        e.request(4, 0, &upload_metadata(named("ABORT OWNER"), &[]));
        assert_eq!(e.hooks.writing, [true]);
        e.ack(0);
        match abort {
            0 => {
                e.request(8, 9, &[]);
                e.nak(9, SYSEX_ABORTED);
            }
            1 => {
                unsafe { sysex_lost(&mut *e.engine, 0) };
                e.nak(1, SYSEX_ABORTED);
            }
            2 => {
                e.now = 2000;
                e.nak(1, SYSEX_TIMEOUT);
            }
            3 => unsafe { sysex_reset(&mut *e.engine) },
            _ => {
                e.request(5, 1, &[1]);
                e.nak(1, SYSEX_MALFORMED);
            }
        }
        assert_eq!(e.hooks.writing, [true, false]);
        assert!(e.rig.file("ABORT OWNER").is_none());
    }

    for zero_length in [true, false] {
        let mut e = Engine::new(false);
        let data = if zero_length { vec![] } else { vec![7; 16] };
        e.request(4, 0, &upload_metadata(named("COMMIT OWNER"), &data));
        e.ack(0);
        let end = if zero_length {
            1
        } else {
            e.request(5, 1, &data);
            e.ack(1);
            2
        };
        e.request(6, end, &[]);
        for _ in 0..1000 {
            e.poll();
            if e.rig.st.w.key == e.rig.st.f.key && e.rig.st.w.phase != 0 {
                break;
            }
        }
        assert_ne!(e.rig.st.w.phase, 0, "reach the irrevocable FILE record");
        unsafe { sysex_reset(&mut *e.engine) };
        assert_eq!(
            e.hooks.writing,
            [true],
            "commit programs still need ownership"
        );
        for _ in 0..1000 {
            e.poll();
            if !e.active() {
                break;
            }
        }
        assert!(!e.active());
        assert_eq!(e.hooks.writing, [true, false]);
        assert_eq!(e.rig.file("COMMIT OWNER"), Some(data));
        assert!(!e.hooks.toasts.contains(&(SYSEX_SAVED as u8)));
    }
}

#[test]
fn post_program_metadata_failure_ends_the_transfer_without_sender_timeout() {
    let mut e = Engine::new(true);
    e.request(4, 0, &upload_metadata(named("FAILED HEADER"), &[7]));
    e.ack(0);
    e.request(5, 1, &[7]);
    for _ in 0..100 {
        e.poll();
        if e.rig.st.f.fill == 0 && e.rig.st.f.cdone == 1 {
            break;
        }
    }
    assert_eq!(
        e.engine.tx_active, 0,
        "DATA ACK must wait for chunk metadata"
    );
    assert_eq!(e.rig.st.f.fill, 0);
    e.rig.sim.fail = 100;
    e.rig.poll();
    unsafe { sysex_poll(&mut *e.engine, e.now) };
    assert_eq!(e.hooks.toasts.last(), Some(&(SYSEX_IO as u8)));
    e.nak(1, SYSEX_IO);
    assert!(e.rig.file("FAILED HEADER").is_none());
}

#[test]
fn backpressure_keeps_pending_reply_intact_and_cannot_overwrite_packet_buffer() {
    let mut e = Engine::new(true);
    e.hooks.blocked = true;
    e.request(1, 4, &[]);
    e.poll();
    e.request(4, 5, &upload_metadata(named("OVERWRITE"), &[3; 256]));
    for _ in 0..10 {
        e.poll();
    }
    assert!(e.wire().is_empty());
    e.hooks.blocked = false;
    let (cmd, seq, p) = e.response();
    assert_eq!((cmd, seq), (1, 4));
    assert_eq!(&p[..5], &[1, 1, 2, 3, 4]);
    e.nak(5, SYSEX_BUSY);
    assert!(e.rig.file("OVERWRITE").is_none());
}

#[test]
fn per_port_ownership_realtime_abort_lost_and_timeout() {
    let mut e = Engine::new(true);
    e.request(4, 90, &upload_metadata(named("OWNER"), &[0xff; 10]));
    e.ack(90);
    e.bytes(1, &frame(8, 99, &[]));
    e.bytes(1, &frame(5, 91, &[1; 10]));
    unsafe { sysex_lost(&mut *e.engine, 1) };
    assert!(e.active());
    let wire: Vec<u8> = frame(5, 91, &[0xff; 10])
        .into_iter()
        .flat_map(|b| [0xf8, b, 0xfe])
        .collect();
    e.bytes(0, &wire);
    e.ack(91);
    e.now = 1999;
    e.poll();
    assert!(e.active());
    e.now = 2000;
    e.nak(92, SYSEX_TIMEOUT);
    assert!(e.rig.file("OWNER").is_none());
    e.request(4, 1, &upload_metadata(named("ABORT"), &[]));
    e.ack(1);
    e.request(8, 117, &[]);
    e.nak(117, SYSEX_ABORTED);
    e.request(4, 2, &upload_metadata(named("LOST"), &[1]));
    e.ack(2);
    unsafe { sysex_lost(&mut *e.engine, 0) };
    e.nak(3, SYSEX_ABORTED);
    e.bytes(0, &[0xf0, 0x7d, 0x53, 0x44, 1, 7]);
    e.now += 1999;
    e.bytes(0, &[0xf8]);
    e.poll();
    assert!(e.active());
    e.now += 1;
    e.nak(7, SYSEX_TIMEOUT);
}

#[test]
fn idle_deadline_is_enforced_before_a_late_byte_can_revive_state() {
    let mut e = Engine::new(true);
    e.request(4, 0, &upload_metadata(named("LATE"), &[9]));
    e.ack(0);
    e.now = 2000;
    e.request(5, 1, &[9]);
    e.nak(1, SYSEX_TIMEOUT);
    assert!(e.rig.file("LATE").is_none());

    let mut e = Engine::new(true);
    e.bytes(0, &[0xf0, 0x7d, 0x53, 0x44, 1, 7]);
    e.now = 2000;
    e.bytes(0, &[0x78, 0xf7]);
    e.nak(7, SYSEX_TIMEOUT);
}

#[test]
fn sequence_checksum_canonical_and_length_errors_preserve_existing_file() {
    for error in 0..7 {
        let mut e = Engine::new(true);
        assert_eq!(e.rig.file_now("KEEP", &[9, 8, 7]), STORE_OK);
        e.rig.maintain();
        e.request(4, 0, &upload_metadata(named("KEEP"), &[4, 5]));
        e.ack(0);
        let mut wire = frame(5, 1, &[4, 5]);
        let why = match error {
            0 => {
                wire = frame(5, 2, &[4, 5]);
                SYSEX_SEQUENCE
            }
            1 => {
                let at = wire.len() - 2;
                wire[at] ^= 1;
                SYSEX_CHECKSUM
            }
            2 => {
                wire = frame(5, 1, &[4, 5, 6]);
                SYSEX_MALFORMED
            }
            3 => {
                wire = frame(5, 1, &[]);
                SYSEX_MALFORMED
            }
            4 => {
                wire[6] |= 4;
                let at = wire.len() - 2;
                wire[at] = wire[at].wrapping_sub(4) & 127;
                SYSEX_MALFORMED
            }
            5 => {
                wire = frame(5, 1, &[0; 257]);
                SYSEX_MALFORMED
            }
            _ => {
                wire = frame(6, 1, &[]);
                SYSEX_IO
            }
        };
        e.bytes(0, &wire);
        e.nak(if error == 0 { 2 } else { 1 }, why);
        assert_eq!(e.rig.file("KEEP"), Some(vec![9, 8, 7]));
        assert!(!e.active());
    }
    let mut e = Engine::new(true);
    e.request(4, 0, &upload_metadata(named("CRC"), &[1, 2]));
    e.ack(0);
    e.request(5, 1, &[1, 3]);
    e.ack(1);
    e.request(6, 2, &[]);
    e.nak(2, SYSEX_IO);
    assert!(e.rig.file("CRC").is_none());
}

#[test]
fn current_source_crc_prepass_restarts_and_always_releases_hooks() {
    for target in [1, 2] {
        let mut e = Engine::new(true);
        e.hooks.source = (0..777).map(|i| (i * 137) as u8).collect();
        let mut request = vec![0; 18];
        request[0] = target;
        request[1] = if target == 1 { 63 } else { 0 };
        e.request(3, 10, &request);
        assert_eq!(
            e.response(),
            (4, 10, upload_metadata(request.clone(), &e.hooks.source))
        );
        assert!(e.hooks.reads.iter().all(|(_, n)| *n <= 256));
        assert_eq!(e.hooks.reads[0], (0, 256));
        e.request(0x7e, 10, &[]);
        assert_eq!(e.response(), (5, 11, e.hooks.source[..256].to_vec()));
        assert_eq!(e.hooks.reads.last(), Some(&(0, 256)));
        e.request(8, 88, &[]);
        e.nak(88, SYSEX_ABORTED);
        assert_eq!(e.hooks.closes, 1);
        e.hooks.reads.clear();
        e.upload(request, &[0xa7; 700], 256);
        assert_eq!(e.hooks.completed, 1);
        assert_eq!(e.hooks.closes, 2);
        let reserved = if target == 1 {
            "MIDI INSTRUMENT"
        } else {
            "MIDI SONG"
        };
        assert_eq!(e.rig.file(reserved), Some(vec![0xa7; 700]));
        assert_eq!(
            e.rig.file_open(reserved).unwrap().type_,
            if target == 1 { 2 } else { 1 }
        );
        e.request(3, 1, &named(reserved));
        e.nak(1, SYSEX_MALFORMED);
        e.request(2, 2, &[]);
        assert_eq!(e.response(), (6, 2, vec![]));
        e.request(0x7e, 2, &[]);
    }
}

#[test]
fn source_refusals_failed_claim_and_import_refusal_close_exactly_once() {
    let mut e = Engine::new(true);
    let mut target = vec![0; 18];
    target[0] = 2;
    e.hooks.refuse_open = SYSEX_BUSY as i32;
    e.request(3, 0, &target);
    e.nak(0, SYSEX_BUSY);
    assert_eq!(e.hooks.closes, 0);
    e.hooks.refuse_open = 0;
    assert_eq!(e.rig.file_begin("OTHER", 1), STORE_OK);
    e.request(4, 1, &upload_metadata(target.clone(), &[1]));
    e.nak(1, SYSEX_BUSY);
    assert_eq!(e.hooks.closes, 1);
    assert_eq!(e.rig.file_result(), STORE_BUSY);
    assert!(
        e.hooks.writing.is_empty(),
        "failed claim must not take ownership"
    );
    unsafe { store_file_abort(&mut *e.rig.st) };
    e.hooks.refuse_complete = SYSEX_IO as i32;
    e.request(4, 2, &upload_metadata(target, &[1]));
    e.ack(2);
    e.request(5, 3, &[1]);
    e.ack(3);
    e.request(6, 4, &[]);
    e.nak(4, SYSEX_IO);
    assert_eq!(e.hooks.completed, 1);
    assert_eq!(e.hooks.closes, 2);
    assert_eq!(e.hooks.writing, [true, false]);
    assert!(!e.hooks.toasts.contains(&(SYSEX_SAVED as u8)));
}

#[test]
fn pending_import_pins_committed_file_and_ownership_without_ack_or_sender_timeout() {
    for refusal in [0, SYSEX_IO as i32] {
        let mut e = Engine::new(false);
        let mut target = vec![0; 18];
        target[0] = 2;
        e.hooks.pending = 6;
        e.hooks.refuse_complete = refusal;
        e.request(4, 10, &upload_metadata(target, &[8; 16]));
        e.ack(10);
        e.request(5, 11, &[8; 16]);
        e.ack(11);
        e.request(6, 12, &[]);
        for _ in 0..1000 {
            e.poll();
            if e.hooks.completed != 0 {
                break;
            }
        }
        assert_eq!(e.hooks.completed, 1);
        assert_eq!(e.rig.file("MIDI SONG"), Some(vec![8; 16]));
        for calls in 2..=6 {
            e.now += 3000;
            e.poll();
            assert_eq!(e.hooks.completed, calls, "once per poll, not a busy loop");
            assert!(e.wire().is_empty());
            assert!(e.active());
            assert_eq!(e.hooks.writing, [true]);
            assert_eq!(e.hooks.closes, 0);
            assert_eq!(e.hooks.toasts, [SYSEX_RECEIVING as u8]);
        }
        if refusal == 0 {
            e.ack(12);
            assert_eq!(e.hooks.toasts.last(), Some(&(SYSEX_SAVED as u8)));
        } else {
            e.nak(12, SYSEX_IO);
            assert!(!e.hooks.toasts.contains(&(SYSEX_SAVED as u8)));
        }
        assert_eq!(e.hooks.completed, 7);
        assert!(e.hooks.committed.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(e.hooks.writing, [true, false]);
        assert_eq!(e.hooks.closes, 1);
        assert!(!e.active());
    }
}

#[test]
fn aborted_pending_import_drains_before_close_and_cannot_be_replaced_by_a_new_put() {
    for abort in 0..3 {
        let mut e = Engine::new(false);
        let mut target = vec![0; 18];
        target[0] = 2;
        e.hooks.pending = 1000;
        e.request(4, 0, &upload_metadata(target, &[8; 16]));
        e.ack(0);
        e.request(5, 1, &[8; 16]);
        e.ack(1);
        e.request(6, 2, &[]);
        for _ in 0..1000 {
            e.poll();
            if e.hooks.completed != 0 {
                break;
            }
        }
        assert_eq!(e.hooks.completed, 1);
        let pinned = (e.engine.file.key, e.engine.file.fid, e.engine.file.length);
        match abort {
            0 => {
                e.request(8, 55, &[]);
                e.hooks.blocked = true;
                let completed = e.hooks.completed;
                e.poll();
                assert_eq!(e.hooks.completed, completed + 1);
                e.hooks.blocked = false;
                e.nak(55, SYSEX_ABORTED);
            }
            1 => {
                unsafe { sysex_lost(&mut *e.engine, 0) };
                e.nak(2, SYSEX_ABORTED);
            }
            _ => unsafe { sysex_reset(&mut *e.engine) },
        }
        assert!(e.active());
        assert_eq!(e.hooks.closes, 0);
        assert_eq!(e.hooks.writing, [true]);
        e.request(4, 80, &upload_metadata(named("REPLACE PIN"), &[1]));
        e.nak(80, SYSEX_BUSY);
        assert_eq!(
            (e.engine.file.key, e.engine.file.fid, e.engine.file.length),
            pinned
        );
        unsafe { sysex_reset(&mut *e.engine) };
        assert!(
            e.active(),
            "disconnect/reset must keep unfinished import pinned"
        );
        assert_eq!(e.hooks.closes, 0);
        e.hooks.pending = 0;
        let completed = e.hooks.completed;
        e.now += 10_000;
        e.poll();
        assert_eq!(e.hooks.completed, completed + 1);
        assert_eq!(e.hooks.closes, 1);
        assert_eq!(e.hooks.writing, [true, false]);
        assert!(!e.active());
        assert!(e.wire().is_empty(), "aborted import must not ACK its END");
        assert!(!e.hooks.toasts.contains(&(SYSEX_SAVED as u8)));
        assert!(e.hooks.committed.windows(2).all(|pair| pair[0] == pair[1]));
        e.poll();
        assert_eq!(e.hooks.closes, 1);
    }
}

#[test]
fn drained_pool_and_flash_failure_release_only_owned_claims() {
    let mut e = Engine::new(true);
    e.upload(named("LARGE"), &vec![0x5a; 65536], 256);
    let pool = e.rig.stats().pool;
    e.request(4, 0, &upload_metadata(named("NEEDS POOL"), &vec![0; 65536]));
    e.nak(0, SYSEX_BUSY);
    assert_eq!(e.rig.stats().pool, pool);
    assert!(e.rig.file("NEEDS POOL").is_none());
    e.rig.maintain();
    e.upload(named("SECOND LARGE"), &vec![0x6b; 65536], 256);
    e.rig.maintain();
    let live = e.rig.stats().live;
    e.request(
        4,
        0,
        &upload_metadata(named("NO CAPACITY"), &vec![0; 65536]),
    );
    e.nak(0, SYSEX_FULL);
    assert_eq!(e.rig.stats().live, live);

    let mut e = Engine::new(true);
    let mut target = vec![0; 18];
    target[0] = 2;
    e.request(4, 0, &upload_metadata(target, &[8; 256]));
    e.ack(0);
    e.rig.sim.fail = 100;
    e.request(5, 1, &[8; 256]);
    e.nak(1, SYSEX_IO);
    assert_eq!(e.hooks.closes, 1);
    assert_eq!(e.hooks.completed, 0);
    assert!(e.rig.file("MIDI SONG").is_none());
    assert_eq!(e.rig.st.window, e.rig.window);
}

#[test]
fn download_ack_sequence_and_timeout_release_frozen_source() {
    for wrong_ack in [true, false] {
        let mut e = Engine::new(true);
        e.hooks.source = vec![0x83; 600];
        let mut target = vec![0; 18];
        target[0] = 2;
        e.request(3, 0, &target);
        assert_eq!(e.response().0, 4);
        for _ in 0..10 {
            e.poll();
        }
        assert!(e.wire().is_empty(), "download must wait for metadata ACK");
        if wrong_ack {
            e.request(0x7e, 1, &[]);
            e.nak(1, SYSEX_SEQUENCE);
        } else {
            e.now = 2000;
            e.nak(0, SYSEX_TIMEOUT);
        }
        assert_eq!(e.hooks.closes, 1);
        assert!(!e.active());
    }
}

#[test]
fn trs_upload_is_silent_and_early_packet_aborts() {
    let mut e = Engine::new(true);
    let data = vec![0x81; 256];
    e.bytes(1, &frame(4, 0, &upload_metadata(named("TRS"), &data)));
    e.bytes(1, &frame(5, 1, &data));
    for _ in 0..2000 {
        e.poll();
    }
    e.bytes(1, &frame(6, 2, &[]));
    for _ in 0..2000 {
        e.poll();
    }
    assert_eq!(e.rig.file("TRS"), Some(data));
    assert!(e.wire().is_empty());
    e.bytes(
        1,
        &frame(4, 10, &upload_metadata(named("TOO FAST"), &[0; 512])),
    );
    e.bytes(1, &frame(5, 11, &[0; 256]));
    e.bytes(1, &frame(5, 12, &[0; 256]));
    assert_eq!(e.hooks.toasts.last(), Some(&(SYSEX_BUSY as u8)));
    assert!(!e.active());
    assert!(e.wire().is_empty());
    assert!(e.rig.file("TOO FAST").is_none());
}

#[test]
fn packet_start_before_storage_ready_is_rejected_even_if_its_tail_is_late() {
    let mut e = Engine::new(true);
    e.bytes(
        1,
        &frame(4, 0, &upload_metadata(named("EARLY START"), &[8; 512])),
    );
    e.bytes(1, &frame(5, 1, &[8; 256]));
    let early = frame(5, 2, &[8; 256]);
    e.bytes(1, &early[..1]);
    for _ in 0..1000 {
        e.poll();
    }
    e.bytes(1, &early[1..]);
    assert_eq!(e.hooks.toasts.last(), Some(&(SYSEX_BUSY as u8)));
    assert!(!e.active());
    assert!(e.rig.file("EARLY START").is_none());

    let mut e = Engine::new(true);
    e.request(4, 0, &upload_metadata(named("ABORT PROGRAM"), &[8; 256]));
    e.ack(0);
    e.request(5, 1, &[8; 256]);
    e.request(8, 99, &[]);
    e.nak(99, SYSEX_ABORTED);
    assert!(e.rig.file("ABORT PROGRAM").is_none());
}

#[test]
fn malformed_headers_status_truncation_and_names() {
    let mut e = Engine::new(true);
    for name in ["", " LOWER", "lower", "MIDI SONG", "MIDI INSTRUMENT"] {
        e.request(3, 3, &named(name));
        e.nak(3, SYSEX_MALFORMED);
    }
    let mut bad = named("A");
    bad[4] = b'B';
    e.request(3, 4, &bad);
    e.nak(4, SYSEX_MALFORMED);
    for bytes in [
        vec![0xf0, 0x7d, 0x53, 0x44, 1, 2, 0xf7],
        vec![0xf0, 0x7d, 0x53, 0x44, 1, 2, 0x90],
    ] {
        e.bytes(0, &bytes);
        e.nak(2, SYSEX_MALFORMED);
    }
    e.request(0x55, 9, &[]);
    e.nak(9, SYSEX_MALFORMED);
    let mut too_big = upload_metadata(named("FULL"), &[]);
    too_big[19..23].copy_from_slice(&65537u32.to_le_bytes());
    e.request(4, 5, &too_big);
    e.nak(5, SYSEX_FULL);
}

#[test]
fn deterministic_random_and_mutated_streams_recover() {
    let mut e = Engine::new(true);
    let mut rng = Rng::new(0x5359_5345_585f_465a);
    for i in 0..500 {
        let mut wire = if i % 3 == 0 {
            rng.bytes(300)
        } else {
            let payload = upload_metadata(named("MUTATE"), &[0; 512]);
            frame(
                if i % 2 == 0 { 4 } else { 5 },
                i as u8 & 127,
                if i % 2 == 0 { &payload } else { &[0; 256] },
            )
        };
        for _ in 0..4 {
            let at = rng.below(wire.len() as u64) as usize;
            wire[at] ^= rng.next() as u8;
        }
        e.bytes((i % 2) as u8, &wire);
        for _ in 0..8 {
            e.poll();
        }
        unsafe { sysex_reset(&mut *e.engine) };
        e.hooks.packets.clear();
        assert!(!e.active());
        assert_eq!(e.rig.st.window, e.rig.window);
    }
    e.request(1, 127, &[]);
    let (cmd, seq, _) = e.response();
    assert_eq!((cmd, seq), (1, 127));
    e.rig.assert_clean();
}
