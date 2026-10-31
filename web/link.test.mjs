// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The Live Link viewer's code (web/link.js, sidou-link.wasm) in plain Node: the frames against
// the spec's test vectors (docs/reference/live-link.md), the session against a simulated FM-1
// (web/fake-fm1.mjs) that answers with what SIDOU's device side sent in
// host/crates/sidou-tests/tests/web_link.rs (web/link-fixtures.json), with fake time: START,
// the heartbeat, STOP on stop and when the page is hidden, the device's auto-stop and a
// restart; the wasm viewer's picture of those frames against the panel's own (its RGB565
// CRC-32); the font fetched only when HELLO's CRC differs and cached; PANEL for held, re-sent and
// released buttons and encoder steps; nothing sent before start. Opens no MIDI port.
//   node web/link.test.mjs [sidou-link.wasm]    (web/check.sh runs it)
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { LiveLinkDevice } from "./fake-fm1.mjs";
import {
  BEAT, CELLS, FONT, HEARTBEAT, HEARTBEAT_MS, KEYFRAME, LinkClient, PALETTE, PANEL, PANEL_REPEAT_MS, START, STOP, Splitter,
  crc32, fontFrame, frame, heartbeatFrame, keyframeFrame, loadViewer, pack, panelFrame, paletteFrame, parse, parseHello,
  startFrame, stopFrame, unpack,
} from "./link.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const wasmPath = process.argv[2] ?? `${here}../build/wasm-link/sidou-link.wasm`;
const wasm = readFileSync(wasmPath);
const fx = JSON.parse(readFileSync(`${here}link-fixtures.json`, "utf8"));
const bytes = (h) => Uint8Array.from(Buffer.from(h, "hex"));
const hex = (b) => Buffer.from(b).toString("hex");
let n = 0;
const test = async (name, fn) => { await fn(); n++; console.log(`ok - ${name}`); };
const settle = () => new Promise((r) => setImmediate(r));

// Time that moves only when told: timers run in order as it passes.
class Clock {
  constructor() { this.t = 0; this.timers = new Map(); this.id = 0; }
  now() { return this.t; }
  setTimeout(fn, ms) { this.timers.set(++this.id, { fn, at: this.t + ms }); return this.id; }
  setInterval(fn, ms) { this.timers.set(++this.id, { fn, at: this.t + ms, every: ms }); return this.id; }
  clearTimeout(id) { this.timers.delete(id); }
  clearInterval(id) { this.timers.delete(id); }
  async advance(ms) {
    const end = this.t + ms;
    for (;;) {
      let next = null;
      for (const [id, t] of this.timers) if (t.at <= end && (!next || t.at < next[1].at)) next = [id, t];
      if (!next) break;
      const [id, t] = next;
      this.t = t.at;
      if (t.every) t.at += t.every; else this.timers.delete(id);
      t.fn();
      await settle();
    }
    this.t = end;
    await settle();
  }
}

// A page's window and document for watchPage.
class Page extends EventTarget {
  constructor() { super(); this.document = new EventTarget(); this.document.visibilityState = "visible"; }
  hide() { this.document.visibilityState = "hidden"; this.document.dispatchEvent(new Event("visibilitychange")); }
  show() { this.document.visibilityState = "visible"; this.document.dispatchEvent(new Event("visibilitychange")); }
}

const viewer = () => loadViewer(wasm);
const picture = (v) => { v.render(); const p = v.rgb565(); return crc32(new Uint8Array(p.buffer, p.byteOffset, p.byteLength)); };

// A client and a simulated FM-1 joined: what the client sends is logged as commands.
async function session(opts = {}) {
  const clock = new Clock();
  const device = new LiveLinkDevice(fx, { clock, ...opts.device });
  const sent = [];
  const v = opts.viewer === undefined ? await viewer() : opts.viewer;
  const states = [];
  const client = new LinkClient({ clock, viewer: v, cache: opts.cache ?? null, onChange: (s) => states.push(s), send: (b) => { sent.push({ cmd: parse(b)?.cmd, at: clock.now(), bytes: b }); toDevice(b); } });
  const toDevice = device.attach((b) => client.receive(b));
  const cmds = () => sent.map((s) => s.cmd);
  return { clock, device, client, sent, cmds, v, states };
}

await test(`the viewer is ${wasm.length} bytes and imports nothing`, async () => {
  const m = await WebAssembly.compile(wasm);
  assert.deepEqual(WebAssembly.Module.imports(m), []);
  assert.ok(wasm.length <= 32 * 1024, "the size budget: 32 KiB");
});

await test("the host frames are the spec's vectors, byte for byte", () => {
  const v = fx.vectors;
  assert.equal(hex(startFrame()), v.START);
  assert.equal(hex(heartbeatFrame()), v.HEARTBEAT);
  assert.equal(hex(stopFrame()), v.STOP);
  assert.equal(hex(keyframeFrame()), v.KEYFRAME);
  assert.equal(hex(fontFrame()), v.FONT);
  assert.equal(hex(paletteFrame()), v.PALETTE);
  // HOME (button 8) held and pressed, K1 (encoder 2) a detent clockwise
  assert.equal(hex(panelFrame(1 << 8, 1 << 8, [0, 0, 1])), v.PANEL);
  assert.equal(hex(panelFrame(1 << 8, 1 << 8, [0, 0, 1, 0, 0])), v.PANEL, "trailing zero steps are left out");
});

await test("the device's vectors parse, and packing round-trips and refuses what is not canonical", () => {
  const [a, b] = fx.device_vectors.map(bytes);
  assert.deepEqual(parse(a), { cmd: CELLS, payload: Uint8Array.from([0, 0, 0x41, 0x0e, 0]) });
  const p = parse(b);
  assert.equal(p.cmd, CELLS);
  assert.deepEqual([...p.payload], [5, 3, 0x20, 0x0e, 0x13, 0x80, 0x40, 0, 0, 0, 0, 0, 0x01, 0x01, 0x7e, 0x41]);
  for (let k = 0; k <= 48; k++) {
    const pl = Uint8Array.from({ length: k }, (_, i) => (i * 37 + 200) & 255);
    const packed = pack(pl);
    assert.equal(packed.length, k + Math.ceil(k / 7));
    assert.deepEqual(unpack(packed), pl);
    const f = frame(CELLS, pl);
    assert.deepEqual(parse(f), { cmd: CELLS, payload: pl });
    if (k) {
      const bad = Uint8Array.from(f);
      bad[7] ^= 1;
      assert.equal(parse(bad), null, "the checksum fails");
    }
  }
  assert.equal(unpack(Uint8Array.from([0x01])), null, "a mask with no byte");
  assert.equal(unpack(Uint8Array.from([0x02, 0x10])), null, "a mask bit with no byte");
  const other = startFrame();
  other[5] = 0x58;
  assert.equal(parse(other), null, "another magic");
});

await test("the splitter takes frames out of a stream with real-time bytes, cut frames and notes", () => {
  const s = new Splitter();
  const f = startFrame();
  const stream = [0x90, 0x40, 0x7f, ...f.slice(0, 4), 0xf8, ...f.slice(4), 0xf0, 0x7d, 0x46, 0x80, 0x40, 0x00, ...heartbeatFrame()];
  const out = [];
  for (const b of stream) out.push(...s.push([b]));
  assert.deepEqual(out.map(hex), [hex(f), hex(heartbeatFrame())]);
});

await test(`HELLO says what the device offers (${fx.name})`, async () => {
  const h = parseHello(parse(bytes(fx.hello[0])).payload);
  assert.equal(h.name, fx.name);
  assert.deepEqual([h.version, h.features, h.cols, h.rows, h.cellW, h.cellH, h.colours, h.fontFirst, h.fontLast, h.pieceFirst, h.pieces, h.timeoutMs, h.buttons, h.encoders],
    [1, 7, 30, 30, 8, 8, 16, 0, 0xdc, 1, 14, 3000, 14, 7]);
  assert.equal(h.fontCrc, fx.font_crc);
  assert.equal(h.paletteCrc, fx.palette_crc);
  const v = await viewer();
  assert.equal(v.fontCrc(h.fontFirst, h.fontLast), fx.font_crc, "the viewer's built-in font is SIDOU's");
  assert.equal(v.paletteCrc(), fx.palette_crc, "and its palette");
});

await test("the wasm viewer draws the device's frames as the panel shows them (RGB565 CRC-32)", async () => {
  const v = await viewer();
  for (const h of [...fx.hello, ...fx.palette, ...fx.font, ...fx.keyframe]) {
    const p = parse(bytes(h));
    assert.ok(v.apply(p.cmd, p.payload), `refused ${h}`);
  }
  assert.equal(picture(v), fx.picture_crc);
  // the keyframe alone, on the built-in font and palette, and again after a HELLO: the same
  const w = await viewer();
  for (const h of [...fx.hello, ...fx.keyframe, ...fx.hello, ...fx.keyframe]) w.apply(parse(bytes(h)).cmd, parse(bytes(h)).payload);
  assert.equal(picture(w), fx.picture_crc);
  // RGBA: the same colours, expanded
  const rgba = w.render();
  const p565 = w.rgb565();
  for (const i of [0, 1234, 30000, 57599]) {
    const c = p565[i];
    assert.deepEqual([rgba[4 * i] >> 3, rgba[4 * i + 1] >> 2, rgba[4 * i + 2] >> 3, rgba[4 * i + 3]], [c >> 11, (c >> 5) & 63, c & 31, 255]);
  }
});

await test("nothing is sent before start; START, then a HEARTBEAT every 2 s, each answered", async () => {
  const s = await session();
  await s.clock.advance(5000);
  assert.deepEqual(s.sent, []);
  s.client.start();
  await settle();
  assert.equal(s.client.state, "on");
  assert.equal(s.client.hello.name, fx.name);
  assert.deepEqual(s.cmds(), [START], "the built-in font and palette are the FM-1's: neither asked for");
  assert.equal(picture(s.v), fx.picture_crc);
  await s.clock.advance(10_000);
  const beats = s.sent.filter((x) => x.cmd === HEARTBEAT);
  assert.equal(beats.length, 5);
  for (let i = 1; i < beats.length; i++) assert.ok(beats[i].at - beats[i - 1].at <= HEARTBEAT_MS);
  assert.equal(s.client.state, "on", "the BEATs keep it on");
  assert.equal(s.device.on, true);
  s.client.stop();
  await settle();
  assert.equal(s.cmds().at(-1), STOP);
  assert.equal(s.client.state, "stopped");
  assert.equal(s.device.on, false);
  const after = s.sent.length;
  await s.clock.advance(10_000);
  assert.equal(s.sent.length, after, "nothing after STOP");
  s.device.close();
});

await test("the page hidden or closed sends STOP; shown again, it starts again", async () => {
  const s = await session();
  const page = new Page();
  s.client.watchPage(page);
  s.client.start();
  await settle();
  page.hide();
  await settle();
  assert.equal(s.cmds().at(-1), STOP);
  assert.equal(s.device.on, false);
  page.show();
  await settle();
  assert.equal(s.cmds().at(-1), START);
  assert.equal(s.client.state, "on");
  for (const type of ["pagehide", "beforeunload"]) {
    page.dispatchEvent(new Event(type));
    await settle();
    assert.equal(s.cmds().at(-1), STOP, type);
    assert.equal(s.device.on, false);
    s.client.start();
    await settle();
  }
  const count = s.cmds().filter((c) => c === STOP).length;
  page.dispatchEvent(new Event("pagehide"));
  page.dispatchEvent(new Event("beforeunload"));
  await settle();
  assert.equal(s.cmds().filter((c) => c === STOP).length, count + 1, "one STOP for both");
  s.device.close();
});

await test("the device stops by itself 3 s after the heartbeats stop reaching it; Restart starts again", async () => {
  const s = await session({ device: { deaf: true } });
  s.client.start();
  await settle();
  await s.clock.advance(2999);
  assert.equal(s.client.state, "on");
  await s.clock.advance(100);
  assert.equal(s.device.on, false);
  assert.equal(s.client.state, "ended");
  const n = s.sent.length;
  await s.clock.advance(6000);
  assert.equal(s.sent.length, n, "no heartbeats into an ended session");
  s.device.deaf = false;
  s.client.start();
  await settle();
  assert.equal(s.client.state, "on");
  assert.equal(picture(s.v), fx.picture_crc);
  s.device.close();
});

await test("a device that answers nothing is silent after 5 s", async () => {
  const s = await session({ device: { mute: true } });
  s.client.start();
  await s.clock.advance(6000);
  assert.equal(s.client.state, "silent");
  s.device.close();
});

await test("the font and palette are fetched when HELLO's CRCs differ, then cached", async () => {
  const store = new Map();
  const cache = { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, v) };
  const v = await viewer();
  v.setFont(new Uint8Array(v.fontRows().length));
  v.setPalette(new Uint8Array(32));
  const s = await session({ viewer: v, cache });
  s.client.start();
  await settle();
  assert.deepEqual(s.cmds(), [START, FONT, PALETTE]);
  assert.equal(picture(v), fx.picture_crc);
  assert.equal(store.size, 2);
  // another page, the same cache: nothing asked
  const w = await viewer();
  w.setFont(new Uint8Array(w.fontRows().length));
  w.setPalette(new Uint8Array(32));
  const t = await session({ viewer: w, cache });
  t.client.start();
  await settle();
  assert.deepEqual(t.cmds(), [START]);
  assert.equal(picture(w), fx.picture_crc);
  s.device.close();
  t.device.close();
});

await test("a frame that fails its checksum asks for a keyframe, at most once a second", async () => {
  const s = await session();
  s.client.start();
  await settle();
  const bad = bytes(fx.keyframe[3]);
  bad[8] ^= 1;
  s.client.receive(bad);
  s.client.receive(bad);
  await settle();
  assert.deepEqual(s.cmds(), [START, KEYFRAME]);
  await s.clock.advance(1000);
  s.client.receive(bad);
  await settle();
  assert.equal(s.cmds().filter((c) => c === KEYFRAME).length, 2);
  assert.equal(picture(s.v), fx.picture_crc);
  s.device.close();
});

await test("PANEL: a held button is sent, again while held, and let go; encoder steps add up", async () => {
  const s = await session();
  s.client.press(8); // nothing without a session
  assert.deepEqual(s.sent, []);
  s.client.start();
  await settle();
  s.client.press(8); // HOME
  await s.clock.advance(1000);
  s.client.release(8);
  await settle();
  const p = s.device.panels;
  assert.deepEqual(p[0], { held: 1 << 8, pressed: 1 << 8, steps: [], at: 0 }, "held and pressed as it went down");
  assert.ok(p.length >= 5, `re-sent while held: ${p.length}`);
  for (let i = 1; i < p.length; i++) assert.ok(p[i].at - p[i - 1].at <= 250, "at least every 250 ms");
  assert.ok(p.slice(1, -1).every((x) => x.held === 1 << 8 && x.pressed === 0));
  assert.deepEqual(p.at(-1), { held: 0, pressed: 0, steps: [], at: 1000 }, "one with nothing held to let go");
  const count = p.length;
  await s.clock.advance(2000);
  assert.equal(p.length, count, "nothing more while nothing is held");
  // K1 (encoder 2): +1 and +2 within the coalescing window go as one PANEL, then PRESETS -1
  s.client.turn(2, 1);
  s.client.turn(2, 2);
  await s.clock.advance(30);
  s.client.turn(6, -1);
  await s.clock.advance(30);
  assert.deepEqual(p.slice(count).map((x) => x.steps), [[0, 0, 3], [0, 0, 0, 0, 0, 0, -1]]);
  // more than a byte holds goes in two frames
  s.client.turn(0, 100);
  s.client.turn(0, 100);
  await s.clock.advance(30);
  assert.deepEqual(p.slice(count + 2).map((x) => x.steps), [[100], [100]]);
  // two buttons: held together, each let go
  s.client.press(6);
  s.client.press(9);
  s.client.release(9);
  s.client.release(6);
  await settle();
  assert.deepEqual(p.slice(-4).map((x) => [x.held, x.pressed]), [[1 << 6, 1 << 6], [(1 << 6) | (1 << 9), 1 << 9], [1 << 6, 0], [0, 0]]);
  assert.ok(PANEL_REPEAT_MS <= 250);
  assert.ok(s.cmds().every((c) => c !== BEAT));
  s.client.stop();
  s.device.close();
});

console.log(`${n} tests passed (${wasmPath})`);
