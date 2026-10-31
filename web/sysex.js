// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
export const INQ = 0x01, DIR = 0x02, GET = 0x03, PUT = 0x04, DATA = 0x05, END = 0x06, DEL = 0x07,
  ABORT = 0x08, WAIT = 0x7d, ACK = 0x7e, NAK = 0x7f;
export const MAX_DATA = 256;
const HEADER = [0x7d, 0x53, 0x44];

export const NAK_TEXT = {
  1: "the FM-1 found the packet malformed or out of range",
  2: "the FM-1 saw a checksum error (a bad USB link?)",
  3: "the FM-1 saw packets out of sequence",
  4: "the FM-1 is busy",
  5: "the FM-1's store is full",
  6: "the FM-1 has no such file",
  7: "the FM-1 had an I/O error or did not accept the file's content",
  8: "the FM-1 timed out waiting for the next packet",
  9: "the transfer was aborted",
};

export class SysexError extends Error {
  constructor(message, { nak = null, pool = null } = {}) {
    super(message);
    this.name = "SysexError";
    this.nak = nak;
    this.pool = pool;
  }
}

export function nakMessage(reason, pool) {
  const base = NAK_TEXT[reason] ?? `the FM-1 refused with reason ${reason}`;
  if (reason === 4) {
    return `${base} (${pool} clean flash sectors free): stop any playing song, wait a few seconds and try again`;
  }
  if (reason === 5) {
    return `${base}: delete a song on the FM-1 (menu, SONG) and try again`;
  }
  if (reason === 7) {
    return `${base}: if a song was just imported, SAVE AS first; otherwise try again`;
  }
  return base;
}

export function busyWaits(total = 60000, first = 1000, cap = 5000) {
  const waits = [];
  for (let w = first, sum = 0; sum < total; w = Math.min(w * 2, cap)) {
    waits.push(Math.min(w, total - sum));
    sum += waits[waits.length - 1];
  }
  return waits;
}

const TABLE = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
export function crc32(bytes) {
  let c = 0xffffffff;
  for (const b of bytes) c = TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

export function pack(bytes) {
  const out = [];
  for (let i = 0; i < bytes.length; i += 7) {
    const group = bytes.slice(i, i + 7);
    let mask = 0;
    group.forEach((b, j) => { mask |= ((b >> 7) & 1) << j; });
    out.push(mask, ...group.map((b) => b & 0x7f));
  }
  return Uint8Array.from(out);
}

export function unpack(packed) {
  const out = [];
  for (let i = 0; i < packed.length;) {
    const mask = packed[i];
    const group = packed.slice(i + 1, i + 8);
    if (group.length === 0 || mask >= 0x80 || mask >> group.length) {
      throw new SysexError("the FM-1 sent a malformed packed payload");
    }
    group.forEach((b, j) => out.push(b | (((mask >> j) & 1) << 7)));
    i += 1 + group.length;
  }
  return Uint8Array.from(out);
}

export function encodeFrame(command, sequence, payload = new Uint8Array()) {
  if (payload.length > MAX_DATA) throw new SysexError("payload over 256 bytes");
  const packed = pack(payload);
  let sum = command + sequence;
  for (const b of packed) sum += b;
  return Uint8Array.from([0xf0, ...HEADER, command, sequence & 0x7f, ...packed, -sum & 0x7f, 0xf7]);
}

export function decodeFrame(frame) {
  const b = Array.from(frame).filter((x) => x < 0xf8);
  if (b[0] !== 0xf0 || b[b.length - 1] !== 0xf7 || HEADER.some((x, i) => b[i + 1] !== x)) return null;
  const body = b.slice(4, -1);
  if (body.length < 3 || body.some((x) => x >= 0x80)) throw new SysexError("a damaged frame from the FM-1");
  if (body.reduce((s, x) => s + x, 0) & 0x7f) throw new SysexError("a frame from the FM-1 with a bad checksum");
  return { command: body[0], sequence: body[1], data: unpack(body.slice(2, -1)) };
}

export function validName(name) {
  return /^[\x21-\x5f][\x20-\x5f]{0,15}$/.test(name) && name !== "MIDI INSTRUMENT" && name !== "MIDI SONG";
}

function name16(name) {
  if (!validName(name)) throw new SysexError(`"${name}" is not a valid SIDOU file name`);
  const out = new Uint8Array(16);
  for (let i = 0; i < name.length; i++) out[i] = name.charCodeAt(i);
  return out;
}

export const songsIn = (files) => files.filter((f) => f.type === 1);

export function getNakMessage(e) {
  switch (e.nak) {
    case 4: return "the FM-1 is busy (saving, loading, or in the middle of an import): wait a moment and try again";
    case 5: return "the FM-1 could not read that song out (too big for the transfer)";
    case 6: return "the FM-1 has no such song any more: press Check this pair to list its songs again";
    case 7: return "the FM-1 had an I/O error reading that song: try again";
    default: return e.message;
  }
}

function u32(n) {
  return [n & 255, (n >>> 8) & 255, (n >>> 16) & 255, (n >>> 24) & 255];
}

export function putPayload(name, type, length, crc, target = 0) {
  return Uint8Array.from([target, 0, ...(target === 0 ? name16(name) : new Uint8Array(16)), type, ...u32(length), ...u32(crc)]);
}

const text = (b) => String.fromCharCode(...b).replace(/\0+$/, "");
const le32 = (b, i) => (b[i] | (b[i + 1] << 8) | (b[i + 2] << 16) | (b[i + 3] << 24)) >>> 0;

export class Client {
  constructor(send, { timeout = 3000, ackTimeout = 10000, busy = busyWaits(), sleep = (ms) => new Promise((r) => setTimeout(r, ms)) } = {}) {
    this.busy = busy;
    this.sleep = sleep;
    this.send = send;
    this.timeout = timeout;
    this.ackTimeout = ackTimeout;
    this.queue = [];
    this.waiter = null;
    this.seq = 0;
  }

  push(bytes) {
    let frame;
    try { frame = decodeFrame(bytes); } catch (e) { frame = { error: e }; }
    if (!frame) return;
    this.queue.push(frame);
    this.waiter?.();
  }

  drain() {
    this.queue.length = 0;
  }

  async recv(ms) {
    const end = Date.now() + ms;
    while (!this.queue.length) {
      const left = end - Date.now();
      if (left <= 0) throw new SysexError("no answer from the FM-1 (is it a SIDOU, and is this the right port pair?)");
      await new Promise((resolve) => {
        const timer = setTimeout(resolve, left);
        this.waiter = () => { clearTimeout(timer); resolve(); };
      });
      this.waiter = null;
    }
    const frame = this.queue.shift();
    if (frame.error) throw frame.error;
    return frame;
  }

  next() {
    const s = this.seq;
    this.seq = (this.seq + 1) & 0x7f;
    return s;
  }

  async expectAck(sequence, ms = this.ackTimeout, onWait = () => {}) {
    for (;;) {
      const f = await this.recv(ms);
      if (f.command === WAIT && f.sequence === sequence) {
        this.waited = true;
        onWait();
        continue;
      }
      if (f.command === NAK) {
        const message = f.data[0] === 4 && this.waited
          ? "the FM-1 waited a minute for quiet and gave up: stop the song (or the recording of its USB audio) and send again"
          : nakMessage(f.data[0], f.data[1]);
        throw new SysexError(message, { nak: f.data[0], pool: f.data[1] });
      }
      if (f.command !== ACK || f.sequence !== sequence) {
        throw new SysexError(`unexpected answer ${f.command.toString(16)} (sequence ${f.sequence}) from the FM-1`);
      }
      return;
    }
  }

  async inquire() {
    this.drain();
    this.send(encodeFrame(INQ, 0));
    const f = await this.recv(this.timeout);
    if (f.command === NAK) throw new SysexError(nakMessage(f.data[0], f.data[1]), { nak: f.data[0] });
    if (f.command !== INQ || f.data.length !== 13) throw new SysexError("that port did not answer like a SIDOU FM-1");
    const d = f.data;
    this.seq = 1;
    return {
      protocol: d[0], version: Array.from(d.slice(1, 5)), store: d[5] | (d[6] << 8),
      free: le32(d, 7), pool: d[11], windowed: (d[12] & 1) === 1,
    };
  }

  async dir() {
    this.drain();
    let seq = this.next();
    this.send(encodeFrame(DIR, seq));
    const files = [];
    for (;;) {
      const f = await this.recv(this.timeout);
      if (f.command === NAK) throw new SysexError(nakMessage(f.data[0], f.data[1]), { nak: f.data[0] });
      this.send(encodeFrame(ACK, f.sequence));
      if (f.command === END) {
        this.seq = (f.sequence + 1) & 0x7f;
        return files;
      }
      if (f.command !== DIR || f.data.length !== 25) throw new SysexError("a damaged DIR reply from the FM-1");
      files.push({ name: text(f.data.slice(0, 16)), type: f.data[16], length: le32(f.data, 17), crc: le32(f.data, 21) });
    }
  }

  async get({ target = 0, name = "" } = {}, { onProgress = () => {} } = {}) {
    this.drain();
    const request = new Uint8Array(18);
    request[0] = target;
    if (target === 0) request.set(name16(name), 2);
    else if (target !== 2) throw new SysexError("GET reads a stored song (target 0) or the current song (target 2)");
    const wait = (ms) => this.recv(ms).then((f) => {
      if (f.command === NAK) throw new SysexError(getNakMessage({ nak: f.data[0], message: nakMessage(f.data[0], f.data[1]) }), { nak: f.data[0], pool: f.data[1] });
      return f;
    });
    try {
      const seq = this.next();
      this.send(encodeFrame(GET, seq, request));
      const meta = await wait(this.ackTimeout);
      if (meta.command !== PUT || meta.data.length !== 27 || meta.data[0] !== target) {
        throw new SysexError("a damaged reply to the FM-1's GET");
      }
      const total = le32(meta.data, 19), crc = le32(meta.data, 23);
      if (total > 65536) throw new SysexError("the FM-1 announced a file over 64 KiB");
      this.send(encodeFrame(ACK, meta.sequence));
      const out = new Uint8Array(total);
      let got = 0;
      onProgress(0, total);
      for (;;) {
        const f = await wait(this.ackTimeout);
        if (f.command === END) {
          this.send(encodeFrame(ACK, f.sequence));
          this.seq = (f.sequence + 1) & 0x7f;
          break;
        }
        if (f.command !== DATA || f.data.length < 1 || got + f.data.length > total) {
          throw new SysexError("a damaged DATA packet from the FM-1");
        }
        out.set(f.data, got);
        got += f.data.length;
        this.send(encodeFrame(ACK, f.sequence));
        onProgress(got, total);
      }
      if (got !== total) throw new SysexError(`the FM-1 sent ${got} bytes of the ${total} it announced`);
      if (crc32(out) !== crc) throw new SysexError("the song arrived damaged (its checksum does not match): try again");
      return out;
    } catch (e) {
      if (e.nak == null) this.send(encodeFrame(ABORT, this.next()));
      throw e;
    }
  }

  async put(name, data, { type = 1, onProgress = () => {}, onBusy = () => {}, onWait = () => {} } = {}) {
    for (let attempt = 0; ; attempt++) {
      try {
        this.waited = false;
        return await this.putOnce(name, data, type, onProgress, onWait);
      } catch (e) {
        if (e.nak !== 4 || this.waited || attempt >= this.busy.length) throw e;
        onBusy(this.busy[attempt], attempt + 1);
        await this.sleep(this.busy[attempt]);
      }
    }
  }

  async loadNow(data, { onProgress = () => {}, onWait = () => {} } = {}) {
    this.waited = false;
    return this.putOnce("", data, 1, onProgress, onWait, 2);
  }

  async del(name, { onBusy = () => {}, onWait = () => {} } = {}) {
    const payload = name16(name);
    for (let attempt = 0; ; attempt++) {
      this.drain();
      this.waited = false;
      try {
        const seq = this.next();
        this.send(encodeFrame(DEL, seq, payload));
        return await this.expectAck(seq, this.ackTimeout, onWait);
      } catch (e) {
        if (e.nak === 6) throw new SysexError(`the FM-1 has no file named ${name}`, { nak: 6 });
        if (e.nak !== 4 || this.waited || attempt >= this.busy.length) throw e;
        onBusy(this.busy[attempt], attempt + 1);
        await this.sleep(this.busy[attempt]);
      }
    }
  }

  async putOnce(name, data, type, onProgress, onWait = () => {}, target = 0) {
    this.drain();
    const total = data.length;
    onProgress(0, total);
    try {
      let seq = this.next();
      this.send(encodeFrame(PUT, seq, putPayload(name, type, total, crc32(data), target)));
      await this.expectAck(seq, this.ackTimeout, onWait);
      for (let off = 0; off < total; off += MAX_DATA) {
        seq = this.next();
        this.send(encodeFrame(DATA, seq, data.slice(off, off + MAX_DATA)));
        await this.expectAck(seq, this.ackTimeout, onWait);
        onProgress(Math.min(off + MAX_DATA, total), total);
      }
      seq = this.next();
      this.send(encodeFrame(END, seq));
      await this.expectAck(seq, this.ackTimeout, onWait);
    } catch (e) {
      if (e.nak == null) this.send(encodeFrame(ABORT, this.next()));
      throw e;
    }
  }
}
