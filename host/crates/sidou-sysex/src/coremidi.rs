// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::{c_char, c_void, CString};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::send::Port;

type Ref = u32;
type CFStringRef = *const c_void;
const UTF8: u32 = 0x0800_0100;
const NAME_MAX: usize = 256;
const LIST_BYTES: usize = 1024;

#[link(name = "CoreMIDI", kind = "framework")]
unsafe extern "C" {
    static kMIDIPropertyName: CFStringRef;
    fn MIDIClientCreate(
        name: CFStringRef,
        notify: *const c_void,
        ctx: *mut c_void,
        out: *mut Ref,
    ) -> i32;
    fn MIDIOutputPortCreate(client: Ref, name: CFStringRef, out: *mut Ref) -> i32;
    fn MIDIInputPortCreate(
        client: Ref,
        name: CFStringRef,
        read: extern "C" fn(*const u8, *mut c_void, *mut c_void),
        ctx: *mut c_void,
        out: *mut Ref,
    ) -> i32;
    fn MIDIGetNumberOfDestinations() -> usize;
    fn MIDIGetDestination(i: usize) -> Ref;
    fn MIDIGetNumberOfSources() -> usize;
    fn MIDIGetSource(i: usize) -> Ref;
    fn MIDIObjectGetStringProperty(obj: Ref, prop: CFStringRef, out: *mut CFStringRef) -> i32;
    fn MIDIPortConnectSource(port: Ref, source: Ref, ctx: *mut c_void) -> i32;
    fn MIDIPacketListInit(list: *mut u8) -> *mut u8;
    fn MIDIPacketListAdd(
        list: *mut u8,
        size: usize,
        cur: *mut u8,
        time: u64,
        n: usize,
        data: *const u8,
    ) -> *mut u8;
    fn MIDISend(port: Ref, dest: Ref, list: *const u8) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(alloc: *const c_void, s: *const c_char, enc: u32) -> CFStringRef;
    fn CFStringGetCString(s: CFStringRef, buf: *mut c_char, size: isize, enc: u32) -> bool;
    fn CFRelease(cf: CFStringRef);
}

#[derive(Default)]
struct Inbox {
    partial: Vec<u8>,
    frames: Vec<Vec<u8>>,
}

type Shared = (Mutex<Inbox>, Condvar);

pub struct MidiPort {
    name: String,
    out: Ref,
    dest: Ref,
    inbox: Arc<Shared>,
}

fn cf(s: &str) -> CFStringRef {
    let c = CString::new(s).unwrap_or_default();
    unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8) }
}

fn name_of(obj: Ref) -> String {
    let mut s: CFStringRef = std::ptr::null();
    if unsafe { MIDIObjectGetStringProperty(obj, kMIDIPropertyName, &mut s) } != 0 || s.is_null() {
        return String::new();
    }
    let mut buf = [0 as c_char; NAME_MAX];
    let ok = unsafe { CFStringGetCString(s, buf.as_mut_ptr(), NAME_MAX as isize, UTF8) };
    unsafe { CFRelease(s) };
    if !ok {
        return String::new();
    }
    buf.iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8 as char)
        .collect()
}

fn find(count: usize, get: unsafe extern "C" fn(usize) -> Ref, name: &str) -> Option<Ref> {
    (0..count)
        .map(|i| unsafe { get(i) })
        .find(|&e| name_of(e) == name)
}

pub fn port_names() -> Vec<String> {
    let n = unsafe { MIDIGetNumberOfDestinations() };
    (0..n)
        .map(|i| name_of(unsafe { MIDIGetDestination(i) }))
        .collect()
}

extern "C" fn read(list: *const u8, port_ctx: *mut c_void, _src: *mut c_void) {
    let shared = unsafe { &*(port_ctx as *const Shared) };
    let mut inbox = shared.0.lock().unwrap_or_else(|e| e.into_inner());
    let count = unsafe { std::ptr::read_unaligned(list as *const u32) };
    let mut packet = unsafe { list.add(4) };
    for _ in 0..count {
        let len = usize::from(unsafe { std::ptr::read_unaligned(packet.add(8) as *const u16) });
        let data = unsafe { std::slice::from_raw_parts(packet.add(10), len) };
        for &b in data {
            if b == 0xf0 {
                inbox.partial.clear();
            }
            inbox.partial.push(b);
            if b == 0xf7 {
                let frame = std::mem::take(&mut inbox.partial);
                inbox.frames.push(frame);
            }
        }
        let next = packet as usize + 10 + len;
        packet = if cfg!(target_arch = "aarch64") {
            (next + 3) & !3
        } else {
            next
        } as *const u8;
    }
    shared.1.notify_all();
}

impl MidiPort {
    pub fn open(name: &str) -> Result<MidiPort, String> {
        let missing = || {
            format!(
                "no MIDI port named \"{name}\" (the ports: {}). Is the FM-1 plugged in and running SIDOU?",
                port_names().join(", ")
            )
        };
        let dest = find(
            unsafe { MIDIGetNumberOfDestinations() },
            MIDIGetDestination,
            name,
        )
        .ok_or_else(missing)?;
        let source =
            find(unsafe { MIDIGetNumberOfSources() }, MIDIGetSource, name).ok_or_else(missing)?;
        let inbox: Arc<Shared> = Arc::default();
        let (mut client, mut out, mut input) = (0, 0, 0);
        let ctx = Arc::into_raw(inbox.clone()) as *mut c_void;
        let failed = unsafe {
            MIDIClientCreate(
                cf("sidou-sysex"),
                std::ptr::null(),
                std::ptr::null_mut(),
                &mut client,
            ) != 0
                || MIDIOutputPortCreate(client, cf("out"), &mut out) != 0
                || MIDIInputPortCreate(client, cf("in"), read, ctx, &mut input) != 0
                || MIDIPortConnectSource(input, source, std::ptr::null_mut()) != 0
        };
        if failed {
            return Err(format!("CoreMIDI could not open \"{name}\""));
        }
        Ok(MidiPort {
            name: name.to_string(),
            out,
            dest,
            inbox,
        })
    }
}

impl Port for MidiPort {
    fn name(&self) -> &str {
        &self.name
    }

    fn send(&mut self, frame: &[u8]) -> Result<(), String> {
        let mut list = vec![0u8; LIST_BYTES + frame.len()];
        let ok = unsafe {
            let cur = MIDIPacketListInit(list.as_mut_ptr());
            !MIDIPacketListAdd(
                list.as_mut_ptr(),
                list.len(),
                cur,
                0,
                frame.len(),
                frame.as_ptr(),
            )
            .is_null()
                && MIDISend(self.out, self.dest, list.as_ptr()) == 0
        };
        if ok {
            Ok(())
        } else {
            Err(format!("CoreMIDI could not send to \"{}\"", self.name))
        }
    }

    fn recv(&mut self, timeout: Duration) -> Option<Vec<u8>> {
        let end = Instant::now() + timeout;
        let (lock, ready) = &*self.inbox;
        let mut inbox = lock.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if !inbox.frames.is_empty() {
                return Some(inbox.frames.remove(0));
            }
            let left = end.checked_duration_since(Instant::now())?;
            if left.is_zero() {
                return None;
            }
            inbox = ready
                .wait_timeout(inbox, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}
