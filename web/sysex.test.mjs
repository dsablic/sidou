// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The Songs page's SysEx code (web/sysex.js) in plain Node: the vectors Rust's sidou-sysex checks
// too (web/sysex-vectors.txt), a PUT's frames against Python's reference encoder
// (tools/sysex_roundtrip.py's packing), and whole INQ, DIR and PUT runs against a simulated device
// that follows docs/reference/sysex.md (stop-and-wait, ACKs, NAKs). Opens no MIDI port.
//   node web/sysex.test.mjs        (tools/web-demo --check runs it)
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { crc32 as zlibCrc32 } from "node:zlib";
import { fileURLToPath } from "node:url";
import {
  ACK, ABORT, Client, DATA, busyWaits, DIR, END, INQ, NAK, PUT, crc32, decodeFrame, encodeFrame, nakMessage, pack, unpack,
  validName,
} from "./sysex.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const hex = (b) => Buffer.from(b).toString("hex");
const unhex = (s) => (s === "-" ? new Uint8Array() : Uint8Array.from(Buffer.from(s, "hex")));
let n = 0;
const test = async (name, fn) => { await fn(); n++; console.log(`ok - ${name}`); };

await test("vectors from sidou-sysex: encode and decode", () => {
  const rows = readFileSync(`${here}sysex-vectors.txt`, "utf8").split("\n").filter((l) => l && !l.startsWith("#"));
  assert.equal(rows.length, 11);
  for (const row of rows) {
    const [label, cmd, seq, data, frame] = row.split(" ");
    assert.equal(hex(encodeFrame(parseInt(cmd, 16), Number(seq), unhex(data))), frame, `${label}: encode`);
    const d = decodeFrame(unhex(frame));
    assert.deepEqual([d.command, d.sequence, hex(d.data)], [parseInt(cmd, 16), Number(seq), data === "-" ? "" : data], label);
  }
  assert.equal(hex(encodeFrame(INQ, 0)), "f07d534401007ff7"); // sysex.md's example
});

await test("CRC-32 is zlib's; packing round-trips every length and refuses damage", () => {
  assert.equal(crc32(Buffer.from("123456789")), 0xcbf43926);
  const data = Uint8Array.from({ length: 300 }, (_, i) => (i * 131 + 7) & 0xff);
  assert.equal(crc32(data), zlibCrc32(data));
  for (let len = 0; len <= 256; len++) {
    const p = pack(data.slice(0, len));
    assert.ok(p.every((b) => b < 0x80));
    assert.deepEqual(unpack(p), data.slice(0, len));
  }
  const frame = encodeFrame(DATA, 3, data.slice(0, 40));
  const bad = Uint8Array.from(frame);
  bad[8] ^= 1;
  assert.throws(() => decodeFrame(bad), /checksum/);
  assert.equal(decodeFrame(Uint8Array.from([0xf0, 0x7e, 0x7f, 6, 2, 0xf7])), null, "a foreign frame is ignored");
  assert.deepEqual(decodeFrame(Uint8Array.from([0xf8, ...frame.slice(0, 5), 0xfe, ...frame.slice(5)])).data, data.slice(0, 40), "real-time bytes are dropped");
});

await test("names follow the store's rules", () => {
  for (const ok of ["CHROME COMET", "A", "0123456789ABCDEF", "SIDOU LOOP", "!A_"]) assert.ok(validName(ok), ok);
  for (const bad of ["", " LEAD", "lower", "SEVENTEEN CHARS!!", "MIDI SONG", "MIDI INSTRUMENT", "TAB\t"]) assert.ok(!validName(bad), bad);
});

// A device that follows sysex.md: it checks every frame it gets as the real one would and answers
// into the client. `fail` names a NAK to send for a given command: { DATA: [index, reason, pool] }.
class Device {
  constructor({ files = [], free = 40000, nak = null, mute = null } = {}) {
    this.files = files.map((f) => ({ ...f }));
    this.free = free;
    this.nak = nak;
    this.mute = mute;
    this.sent = [];
    this.upload = null;
    this.expect = null;
    this.stored = new Map();
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
    if (f.command === ACK) return this.dirNext();
    if (f.command === ABORT) { this.upload = null; return; }
    if (this.nak?.command === f.command && this.nak.at === (f.command === DATA ? this.nData++ : 0) && this.nak.times !== 0) {
      if (this.nak.times) this.nak.times--;
      this.upload = null;
      return this.reply(NAK, f.sequence, Uint8Array.from([this.nak.reason, this.nak.pool ?? 0]));
    }
    if (f.command === PUT) {
      const d = f.data;
      assert.equal(d.length, 2 + 16 + 1 + 4 + 4);
      assert.deepEqual([d[0], d[1]], [0, 0], "target 0, instrument 0");
      const name = Buffer.from(d.slice(2, 18)).toString("latin1").replace(/\0+$/, "");
      assert.ok(validName(name));
      assert.ok(d.slice(2 + name.length, 18).every((b) => b === 0), "NUL padding");
      this.upload = { name, type: d[18], length: le(d, 19), crc: le(d, 23), data: [], expect: (f.sequence + 1) & 0x7f };
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
      return this.reply(ACK, f.sequence);
    }
    assert.equal(f.command, END);
    assert.equal(u.data.length, u.length, "exact length");
    assert.equal(crc32(Uint8Array.from(u.data)), u.crc, "CRC");
    this.files = this.files.filter((x) => x.name !== u.name).concat({ name: u.name, type: u.type, length: u.length, crc: u.crc });
    this.stored.set(u.name, Uint8Array.from(u.data));
    this.upload = null;
    this.reply(ACK, f.sequence);
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
const connect = (opts, copts = { timeout: 300, ackTimeout: 300, busy: [] }) => {
  const device = new Device(opts);
  const client = new Client(null, copts);
  client.send = device.attach(client);
  return { device, client };
};

await test("a PUT's frames are byte for byte Python's reference encoder's", async () => {
  const data = Uint8Array.from({ length: 20 }, (_, i) => (i * 7 + 200) & 0xff);
  assert.equal(hex(data), "c8cfd6dde4ebf2f900070e151c232a31383f464d");
  assert.equal(crc32(data), 1917856991);
  const { device, client } = connect();
  client.seq = 1;
  await client.put("TEST SONG", data);
  assert.deepEqual(device.sent.slice(0, 3), [
    "f07d53440401000000544553542000534f4e4700000000000000000114000400005f2c50727ef7",
    "f07d534405027f484f565d646b72017900070e151c23002a31383f464d27f7",
    "f07d5344060377f7",
  ]);
  assert.equal(device.sent.length, 3);
});

await test("INQ, DIR and a whole multi-packet PUT against the device", async () => {
  const { device, client } = connect({ files: [{ name: "OLD SONG", type: 1, length: 99, crc: 0x80000001 }] });
  const info = await client.inquire();
  assert.deepEqual(info, { protocol: 1, version: [0, 2, 0, 1], store: 0x1234, free: 40000, pool: 6, windowed: true });
  assert.deepEqual(await client.dir(), [{ name: "OLD SONG", type: 1, length: 99, crc: 0x80000001 }]);
  const song = Uint8Array.from({ length: 5744 }, (_, i) => (i * 29 + (i >> 8)) & 0xff);
  const seen = [];
  await client.put("CHROME COMET", song, { onProgress: (d, t) => seen.push([d, t]) });
  assert.deepEqual(device.stored.get("CHROME COMET"), song);
  assert.deepEqual(seen[0], [0, 5744]);
  assert.deepEqual(seen.at(-1), [5744, 5744]);
  assert.equal(seen.length, 1 + 23);
  assert.equal(device.sent.length, 1 + 1 + 2 + 1 + 23 + 1, "INQ, DIR, 2 ACKs, PUT, 23 DATA, END");
  assert.deepEqual((await client.dir()).map((f) => f.name), ["OLD SONG", "CHROME COMET"]);
  // overwriting: the same name again replaces it
  await client.put("CHROME COMET", song.slice(0, 100));
  assert.equal(device.stored.get("CHROME COMET").length, 100);
});

await test("the sequence wraps modulo 128 over a long run", async () => {
  const { device, client } = connect();
  await client.inquire();
  for (let i = 0; i < 5; i++) await client.put(`SONG ${i}`, new Uint8Array(256 * 30 + 3).fill(i));
  assert.equal(device.stored.size, 5);
});

await test("NAKs end the transfer with the protocol's reasons and what to do", async () => {
  let c = connect({ nak: { command: PUT, at: 0, reason: 4, pool: 2 } });
  await assert.rejects(c.client.put("A", new Uint8Array(10)), (e) => e.nak === 4 && e.pool === 2 && /busy.*2 clean.*try again/.test(e.message));
  assert.ok(!c.device.sent.some((h) => h.startsWith("f07d534408")), "no ABORT after a NAK");
  c = connect({ nak: { command: DATA, at: 1, reason: 5 } });
  await assert.rejects(c.client.put("A", new Uint8Array(600)), (e) => e.nak === 5 && /store is full.*delete a song/.test(e.message));
  c = connect({ nak: { command: END, at: 0, reason: 7 } });
  await assert.rejects(c.client.put("A", new Uint8Array(30)), (e) => e.nak === 7 && /did not accept/.test(e.message));
  assert.match(nakMessage(2), /checksum/);
  assert.match(nakMessage(9), /aborted/);
});

await test("busyWaits backs off 1, 2, 4, 5 s up to the total", () => {
  assert.deepEqual(busyWaits().slice(0, 5), [1000, 2000, 4000, 5000, 5000]);
  assert.equal(busyWaits().reduce((a, b) => a + b), 60000);
  assert.deepEqual(busyWaits(0), []);
});

await test("NAK BUSY waits and sends the whole PUT again, then succeeds", async () => {
  const waits = [];
  const data = new Uint8Array(600).fill(9);
  for (const nak of [{ command: PUT, at: 0, reason: 4, pool: 1, times: 2 }, { command: DATA, at: 1, reason: 4, pool: 1, times: 1 }]) {
    const { device, client } = connect({ nak }, { timeout: 300, ackTimeout: 300, busy: [10, 20, 40], sleep: async (ms) => { waits.push(ms); } });
    const seen = [];
    await client.put("BUSY SONG", data, { onBusy: (ms, n) => seen.push([ms, n]) });
    assert.deepEqual(device.stored.get("BUSY SONG"), data);
    assert.ok(seen.length > 0 && seen[0][1] === 1);
  }
  assert.deepEqual(waits, [10, 20, 10]);
  const { client } = connect({ nak: { command: PUT, at: 0, reason: 4, pool: 1, times: 5 } }, { timeout: 300, ackTimeout: 300, busy: [1, 1], sleep: async () => {} });
  await assert.rejects(client.put("A", new Uint8Array(10)), (e) => e.nak === 4);
  const f = connect({ nak: { command: DATA, at: 0, reason: 5 } }, { timeout: 300, ackTimeout: 300, busy: [1, 1], sleep: async () => { throw new Error("must not wait"); } });
  await assert.rejects(f.client.put("A", new Uint8Array(10)), (e) => e.nak === 5);
});

await test("a silent device times out and gets an ABORT", async () => {
  const { client } = connect({ mute: "inq" });
  await assert.rejects(client.inquire(), /no answer/);
  const sent = [];
  const c = new Client((b) => sent.push(hex(b)), { timeout: 50, ackTimeout: 50 });
  await assert.rejects(c.put("A", new Uint8Array(4)), /no answer/);
  assert.equal(sent.length, 2);
  assert.equal(decodeFrame(unhex(sent[1])).command, ABORT);
});

await test("the site's songs are valid PUTs", () => {
  const dir = `${here}../assets/songs/`;
  const songs = readdirSync(dir).filter((f) => f.endsWith(".sng"));
  assert.ok(songs.length >= 4);
  const page = readFileSync(`${here}songs.js`, "utf8");
  for (const f of songs.filter((s) => s !== "sidou-loop.sng")) {
    assert.ok(page.includes(`"${f}"`), `${f} is listed on the Songs page`);
    const bytes = readFileSync(dir + f);
    assert.equal(bytes.subarray(0, 4).toString("latin1"), "GTS5");
    assert.ok(bytes.length <= 65536);
  }
  for (const m of page.matchAll(/name: "([^"]+)"/g)) assert.ok(validName(m[1]), m[1]);
});

console.log(`sysex.test.mjs: ${n} tests passed`);
