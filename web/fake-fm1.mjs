// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { ACK, ABORT, Client, DATA, DEL, DIR, END, GET, INQ, NAK, PUT, WAIT, crc32, decodeFrame, encodeFrame, validName } from "./sysex.js";

const hex = (b) => Buffer.from(b).toString("hex");
export class Device {
  constructor({ files = [], free = 40000, nak = null, mute = null, waitForQuiet = null, current = null, nakEveryGet = null, chunk = 256, corruptDataByte = null, saveAsFirst = false } = {}) {
    this.saveAsFirst = saveAsFirst;
    this.corruptDataByte = corruptDataByte;
    this.current = current;
    this.nakEveryGet = nakEveryGet;
    this.chunk = chunk;
    this.out = null;
    this.files = files.map(({ bytes, ...f }) => (bytes ? { ...f, length: bytes.length, crc: crc32(bytes) } : { ...f }));
    this.waitForQuiet = waitForQuiet;
    this.free = free;
    this.nak = nak;
    this.mute = mute;
    this.sent = [];
    this.upload = null;
    this.expect = null;
    this.stored = new Map(files.filter((f) => f.bytes).map((f) => [f.name, Uint8Array.from(f.bytes)]));
    this.nData = 0;
  }
  attach(client) { this.client = client; return (bytes) => this.receive(bytes); }
  reply(command, sequence, data = new Uint8Array()) {
    queueMicrotask(() => this.client.push(encodeFrame(command, sequence, data)));
  }
  receive(bytes) {
    this.sent.push(hex(bytes));
    const f = decodeFrame(bytes);
    assert.ok(f, "only SIDOU frames go out");
    if (f.command === INQ) {
      if (this.mute === "inq") return;
      return this.reply(INQ, f.sequence, Uint8Array.from([1, 0, 2, 0, 1, 0x34, 0x12, ...u32(this.free), 6, 1]));
    }
    if (f.command === DIR) {
      this.listing = [...this.files];
      this.dirSeq = f.sequence;
      return this.dirNext();
    }
    if (f.command === GET) return this.get(f);
    if (f.command === ACK) return this.out ? this.outNext() : this.dirNext();
    if (f.command === ABORT) { this.upload = null; return; }
    if (this.nak?.command === f.command && this.nak.at === (f.command === DATA ? this.nData++ : 0) && this.nak.times !== 0) {
      if (this.nak.times) this.nak.times--;
      this.upload = null;
      return this.reply(NAK, f.sequence, Uint8Array.from([this.nak.reason, this.nak.pool ?? 0]));
    }
    if (f.command === DEL) {
      assert.equal(f.data.length, 16, "DEL is a name");
      const name = Buffer.from(f.data).toString("latin1").replace(/\0+$/, "");
      assert.ok(validName(name));
      assert.ok(f.data.slice(name.length).every((b) => b === 0), "NUL padding");
      const there = this.files.find((x) => x.name === name);
      if (!there) return this.reply(NAK, f.sequence, Uint8Array.from([6, 0]));
      this.files = this.files.filter((x) => x !== there);
      this.stored.delete(name);
      this.free += there.length;
      this.deleted = [...(this.deleted ?? []), name];
      return this.reply(ACK, f.sequence);
    }
    if (f.command === PUT) {
      const d = f.data;
      assert.equal(d.length, 2 + 16 + 1 + 4 + 4);
      assert.ok([0, 2].includes(d[0]) && d[1] === 0, "target 0 or 2, instrument 0");
      if (d[0] === 2) {
        assert.ok(d.slice(2, 18).every((b) => b === 0), "target 2 has no name");
        assert.equal(d[18], 1, "target 2 is a song");
        if (this.saveAsFirst) return this.reply(NAK, f.sequence, Uint8Array.from([4, 0]));
        this.upload = { target: 2, type: 1, length: le(d, 19), crc: le(d, 23), data: [], expect: (f.sequence + 1) & 0x7f };
        return this.reply(ACK, f.sequence);
      }
      const name = Buffer.from(d.slice(2, 18)).toString("latin1").replace(/\0+$/, "");
      assert.ok(validName(name));
      assert.ok(d.slice(2 + name.length, 18).every((b) => b === 0), "NUL padding");
      this.upload = { name, type: d[18], length: le(d, 19), crc: le(d, 23), data: [], expect: (f.sequence + 1) & 0x7f };
      const old = this.files.find((x) => x.name === name)?.length ?? 0;
      if (this.upload.length > this.free + old) {
        this.upload = null;
        return this.reply(NAK, f.sequence, Uint8Array.from([5, 0]));
      }
      return this.reply(ACK, f.sequence);
    }
    const u = this.upload;
    assert.ok(u, "DATA/END only inside a PUT");
    assert.equal(f.sequence, u.expect, "sequence increments");
    u.expect = (u.expect + 1) & 0x7f;
    if (f.command === DATA) {
      const have = u.data.length;
      assert.ok(f.data.length >= 1 && f.data.length <= 256);
      assert.equal(have % 16, 0, "DATA starts 16-byte aligned");
      u.data.push(...f.data);
      const q = this.waitForQuiet;
      u.packets = (u.packets ?? 0) + 1;
      if (q && q.at === u.packets - 1) {
        this.waitForQuiet = null;
        for (let i = 0; i < q.waits; i++) {
          setTimeout(() => this.client.push(encodeFrame(WAIT, f.sequence)), i * q.every);
        }
        setTimeout(() => {
          if (q.give_up) {
            this.upload = null;
            this.client.push(encodeFrame(NAK, f.sequence, Uint8Array.from([4, 3])));
          } else this.client.push(encodeFrame(ACK, f.sequence));
        }, q.waits * q.every);
        return;
      }
      return this.reply(ACK, f.sequence);
    }
    assert.equal(f.command, END);
    assert.equal(u.data.length, u.length, "exact length");
    assert.equal(crc32(Uint8Array.from(u.data)), u.crc, "CRC");
    if (u.target === 2) {
      this.current = Uint8Array.from(u.data);
      this.loads = [...(this.loads ?? []), this.current];
      this.upload = null;
      return this.reply(ACK, f.sequence);
    }
    this.free += this.files.find((x) => x.name === u.name)?.length ?? 0;
    this.free -= u.length;
    this.files = this.files.filter((x) => x.name !== u.name).concat({ name: u.name, type: u.type, length: u.length, crc: u.crc });
    this.stored.set(u.name, Uint8Array.from(u.data));
    this.upload = null;
    this.reply(ACK, f.sequence);
  }
  get(f) {
    const d = f.data;
    assert.equal(d.length, 18, "GET is target, instrument and a name");
    assert.equal(d[1], 0, "no instrument");
    if (this.nakEveryGet) return this.reply(NAK, f.sequence, Uint8Array.from([this.nakEveryGet, 3]));
    let bytes, type = 1, name = new Uint8Array(16);
    if (d[0] === 2) {
      assert.ok(d.slice(2).every((b) => b === 0), "target 2 has no name");
      bytes = this.current;
    } else {
      assert.equal(d[0], 0);
      const n = Buffer.from(d.slice(2, 18)).toString("latin1").replace(/\0+$/, "");
      const file = this.files.find((x) => x.name === n);
      if (!file || !this.stored.has(n)) return this.reply(NAK, f.sequence, Uint8Array.from([6, 0]));
      bytes = this.stored.get(n);
      type = file.type;
      name = d.slice(2, 18);
    }
    let seq = f.sequence;
    this.out = [[PUT, Uint8Array.from([d[0], 0, ...name, type, ...u32(bytes.length), ...u32(crc32(bytes))])]];
    for (let o = 0; o < bytes.length; o += this.chunk) this.out.push([DATA, bytes.slice(o, o + this.chunk)]);
    if (this.corruptDataByte !== null) this.out[1][1] = this.out[1][1].map((b, i) => (i === this.corruptDataByte ? b ^ 1 : b));
    this.out.push([END, new Uint8Array()]);
    this.outSeq = seq;
    this.outNext();
  }
  outNext() {
    const next = this.out.shift();
    if (!next) { this.out = null; return; }
    this.reply(next[0], this.outSeq, next[1]);
    this.outSeq = (this.outSeq + 1) & 0x7f;
  }
  dirNext() {
    if (!this.listing) return;
    const e = this.listing.shift();
    const seq = this.dirSeq;
    this.dirSeq = (this.dirSeq + 1) & 0x7f;
    if (!e) { this.listing = null; return this.reply(END, seq); }
    const name = new Uint8Array(16);
    name.set(Buffer.from(e.name));
    this.reply(DIR, seq, Uint8Array.from([...name, e.type, ...u32(e.length), ...u32(e.crc)]));
  }
}
function u32(v) { return [v & 255, (v >>> 8) & 255, (v >>> 16) & 255, (v >>> 24) & 255]; }
function le(b, i) { return (b[i] | (b[i + 1] << 8) | (b[i + 2] << 16) | (b[i + 3] << 24)) >>> 0; }
export const connect = (opts, copts = { timeout: 300, ackTimeout: 300, busy: [] }) => {
  const device = new Device(opts);
  const client = new Client(null, copts);
  client.send = device.attach(client);
  return { device, client };
};

import { FONT as LL_FONT, HEARTBEAT as LL_HEARTBEAT, KEYFRAME as LL_KEYFRAME, PALETTE as LL_PALETTE,
  PANEL as LL_PANEL, START as LL_START, STOP as LL_STOP, Splitter as LinkSplitter, isLink, parse as linkParse } from "./link.js";

export class LiveLinkDevice {
  constructor(fixtures, { clock = globalThis, deaf = false, mute = false } = {}) {
    const frames = (list) => list.map((h) => Uint8Array.from(Buffer.from(h, "hex")));
    this.f = { hello: frames(fixtures.hello), palette: frames(fixtures.palette), font: frames(fixtures.font),
      keyframe: frames(fixtures.keyframe), beat: frames(fixtures.beat), stopped: frames(fixtures.stopped) };
    this.clock = clock;
    this.now = clock.now ? () => clock.now() : () => Date.now();
    this.deaf = deaf;
    this.mute = mute;
    this.on = false;
    this.heard = 0;
    this.got = [];
    this.panels = [];
    this.splitter = new LinkSplitter();
    this.timer = clock.setInterval(() => this.tick(), 50);
  }
  attach(receive) { this.toHost = receive; return (bytes) => this.receive(bytes); }
  close() { this.clock.clearInterval(this.timer); }
  reply(list) {
    if (this.mute) return;
    for (const f of list) queueMicrotask(() => this.toHost?.(f));
  }
  tick() {
    if (this.on && this.now() - this.heard >= 3000) {
      this.on = false;
      this.reply(this.f.stopped);
    }
  }
  receive(bytes) {
    for (const f of this.splitter.push(bytes)) {
      assert.ok(isLink(f), "only Live Link frames go to the device here");
      const p = linkParse(f);
      assert.ok(p, `a frame that fails the checksum or its packing: ${Buffer.from(f).toString("hex")}`);
      this.got.push(p.cmd);
      if (p.cmd === LL_START) {
        assert.deepEqual([...p.payload], [1], "START carries the protocol version");
        this.on = true;
        this.heard = this.now();
        this.reply([...this.f.hello, ...this.f.keyframe]);
        continue;
      }
      if (p.cmd === LL_HEARTBEAT && this.deaf) continue;
      if (!this.on) { this.reply(this.f.stopped); continue; }
      if (p.cmd === LL_STOP) { this.on = false; this.reply(this.f.stopped); continue; }
      assert.ok(p.cmd === LL_PANEL || p.payload.length === 0, "only PANEL has a payload");
      if (p.cmd === LL_HEARTBEAT) { this.heard = this.now(); this.reply(this.f.beat); }
      else if (p.cmd === LL_PALETTE) this.reply(this.f.palette);
      else if (p.cmd === LL_FONT) this.reply(this.f.font);
      else if (p.cmd === LL_KEYFRAME) this.reply(this.f.keyframe);
      else if (p.cmd === LL_PANEL) {
        const d = p.payload;
        assert.ok(d.length >= 4 && d.length <= 4 + 7, "PANEL: held, pressed, at most 7 encoders");
        this.panels.push({ held: d[0] | (d[1] << 8), pressed: d[2] | (d[3] << 8), steps: Array.from(d.slice(4), (s) => (s << 24) >> 24), at: this.now() });
      } else assert.fail(`an unknown command ${p.cmd}`);
    }
  }
}
