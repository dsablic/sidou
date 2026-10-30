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
  ACK, ABORT, Client, DATA, GET, busyWaits, DIR, END, INQ, NAK, PUT, WAIT, crc32, decodeFrame, encodeFrame, nakMessage, pack,
  unpack, songsIn, validName,
} from "./sysex.js";
import { SongError, songToMidi } from "./songmidi.js";

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
  constructor({ files = [], free = 40000, nak = null, mute = null, quiet = null, current = null, refuse = null, chunk = 256, flip = null } = {}) {
    this.flip = flip; // flip a bit of the first DATA packet's byte at this index
    this.current = current; // the current song's bytes, for GET target 2
    this.refuse = refuse; // a NAK reason for every GET
    this.chunk = chunk;
    this.out = null;
    this.files = files.map((f) => ({ ...f }));
    this.quiet = quiet; // { at: a DATA's index, waits: WAITs before its answer, every: ms, give_up: NAK BUSY then }
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
    if (f.command === GET) return this.get(f);
    if (f.command === ACK) return this.out ? this.outNext() : this.dirNext();
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
      const q = this.quiet;
      u.packets = (u.packets ?? 0) + 1;
      if (q && q.at === u.packets - 1) {
        this.quiet = null;
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
    this.files = this.files.filter((x) => x.name !== u.name).concat({ name: u.name, type: u.type, length: u.length, crc: u.crc });
    this.stored.set(u.name, Uint8Array.from(u.data));
    this.upload = null;
    this.reply(ACK, f.sequence);
  }
  // GET: the metadata (a PUT frame), the DATA packets and END, one at a time, each after the ACK of the one before
  get(f) {
    const d = f.data;
    assert.equal(d.length, 18, "GET is target, instrument and a name");
    assert.equal(d[1], 0, "no instrument");
    if (this.refuse) return this.reply(NAK, f.sequence, Uint8Array.from([this.refuse, 3]));
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
    if (this.flip !== null) this.out[1][1] = this.out[1][1].map((b, i) => (i === this.flip ? b ^ 1 : b));
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

// one of the site's songs
const songBytes = (name) => {
  const a = Array.from(readFileSync(`${here}../assets/songs/${name}`));
  return Uint8Array.from(a);
};

await test("the Songs to MIDI flow: DIR lists the songs, GET reads one, the file converts like a picked one", async () => {
  const comet = songBytes("chrome-comet.sng"), iron = songBytes("iron-rite.sng");
  const tune = Uint8Array.from({ length: 300 }, (_, i) => i);
  const files = [
    { name: "CHROME COMET", type: 1, length: comet.length, crc: crc32(comet) },
    { name: "A TUNE", type: 2, length: tune.length, crc: crc32(tune) },
    { name: "IRON RITE", type: 1, length: iron.length, crc: crc32(iron) },
  ];
  const { device, client } = connect({ files, current: iron }, { timeout: 300, ackTimeout: 300, busy: [] });
  device.stored.set("CHROME COMET", comet);
  device.stored.set("A TUNE", tune);
  device.stored.set("IRON RITE", iron);
  await client.inquire();
  const listed = songsIn(await client.dir());
  assert.deepEqual(listed.map((f) => f.name), ["CHROME COMET", "IRON RITE"], "the .sid tune is not a song");
  // a stored song, in 100-byte packets (any length up to 256 is allowed), is the file's bytes
  device.chunk = 100;
  const seen = [];
  const got = await client.get({ target: 0, name: listed[0].name }, { onProgress: (d, t) => seen.push([d, t]) });
  assert.deepEqual(got, comet);
  assert.deepEqual(seen.at(-1), [comet.length, comet.length]);
  const direct = songToMidi(comet);
  const viaFm1 = songToMidi(got);
  assert.deepEqual(viaFm1.midi.bytes, direct.midi.bytes, "the same .mid as the picked file's");
  // the current song (target 2), GET's frame byte for byte: target 2, instrument 0, sixteen zeros
  device.chunk = 256;
  const cur = await client.get({ target: 2 });
  assert.deepEqual(cur, iron);
  assert.ok(device.sent.some((h) => h.startsWith("f07d534403") && decodeFrame(unhex(h)).data.length === 18 && decodeFrame(unhex(h)).data[0] === 2));
  assert.equal(songToMidi(cur).song.name, "IRON RITE");
  // the sequence goes on after a GET: a DIR and another GET still work
  assert.equal((await client.dir()).length, 3);
  assert.deepEqual(await client.get({ target: 0, name: "IRON RITE" }), iron);
});

await test("GET refusals: busy, no such song, full; a damaged transfer is an error and an ABORT", async () => {
  const comet = songBytes("chrome-comet.sng");
  for (const [reason, text] of [[4, /busy.*try again/], [5, /too big/], [6, /no such song/], [7, /I\/O error/]]) {
    const { device, client } = connect({ current: comet, refuse: reason });
    await assert.rejects(client.get({ target: 2 }), (e) => e.nak === reason && text.test(e.message));
    assert.ok(!device.sent.some((h) => h.startsWith("f07d534408")), "no ABORT after a NAK");
  }
  // a stored name the device does not hold
  let c = connect({ files: [{ name: "GONE", type: 1, length: 1, crc: 0 }] });
  await assert.rejects(c.client.get({ target: 0, name: "GONE" }), (e) => e.nak === 6 && /Check this pair/.test(e.message));
  // a song that arrives with a wrong CRC is not handed on
  c = connect({ current: comet, flip: 5 });
  await assert.rejects(c.client.get({ target: 2 }), /damaged/);
  assert.equal(decodeFrame(unhex(c.device.sent.at(-1))).command, ABORT);
  // silence: the page's wait ends with a sentence, not a hang
  c = connect({ current: comet }, { timeout: 50, ackTimeout: 50, busy: [] });
  c.device.get = () => {};
  await assert.rejects(c.client.get({ target: 2 }), /no answer/);
  // a GET of another target or an invalid name is refused before anything is sent
  await assert.rejects(connect().client.get({ target: 1 }), /GET reads/);
  await assert.rejects(connect().client.get({ target: 0, name: "lower" }), /valid SIDOU file name/);
  // a file that is not a song reaches the converter's own refusal
  assert.throws(() => songToMidi(Uint8Array.from({ length: 200 }, () => 7)), SongError);
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

await test("WAIT keeps a PUT waiting past its ACK timeout, says so, and the upload goes on", async () => {
  // Three WAITs 200 ms apart, then the ACK: 600 ms against an ACK timeout of 300.
  const data = Uint8Array.from({ length: 700 }, (_, i) => i & 0xff);
  const { device, client } = connect({ quiet: { at: 1, waits: 3, every: 200 } });
  const seen = [];
  await client.put("QUIET SONG", data, { onWait: () => seen.push("wait"), onProgress: (d) => seen.push(d) });
  assert.deepEqual(device.stored.get("QUIET SONG"), data);
  assert.deepEqual(seen, [0, 256, "wait", "wait", "wait", 512, 700]);
});

await test("a WAIT that ends in BUSY is not retried and says to stop the song", async () => {
  const { client } = connect({ quiet: { at: 0, waits: 2, every: 100, give_up: true } },
    { timeout: 300, ackTimeout: 300, busy: [1, 1], sleep: async () => { throw new Error("must not retry"); } });
  let waits = 0;
  await assert.rejects(client.put("GAVE UP", new Uint8Array(300), { onWait: () => waits++ }),
    (e) => e.nak === 4 && /quiet.*stop the song/.test(e.message));
  assert.equal(waits, 2);
});

await test("a WAIT for another packet is no ACK", async () => {
  const c = new Client(() => {}, { timeout: 300, ackTimeout: 300 });
  const p = c.expectAck(5);
  c.push(encodeFrame(WAIT, 4));
  await assert.rejects(p, /unexpected answer 7d/);
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
