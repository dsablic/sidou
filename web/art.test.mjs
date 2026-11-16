// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";
import { SIDE, loadArtCore, overSky, parseTrace, refusalWords } from "./art-core.js";
import {
  ArtError, CLEAR, FIRST_FRAME, FRAMES, PAGE, PALETTE, SLOTS, decodePack, encodePack, framesFromSheet,
  nameRefusal, packRefusal, sheetFromFrames, sheetRefusal, toPalette, withinAPage,
} from "./sidart.js";
import { artRefusal } from "./store.js";
import { crc32 } from "./sysex.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = `${here}../`;
const wasm = process.argv[2] ?? `${root}build/wasm-art/sidou-art.wasm`;
const core = await loadArtCore(readFileSync(wasm));
const shipped = Uint8Array.from(readFileSync(`${root}assets/art/red-moon-rite.sidart`));
const skies = core.skies();
let n = 0;
const test = async (name, fn) => { await fn(); n++; console.log(`ok - ${name}`); };

const POSES = ["guard", "guard2", "raise", "chop", "block", "spinA", "spinB", "hit"];
const UNARMED = ["guard", "guard2", "hit", "reach"].map((p) => `${p}-unarmed`);
const PNG_NAMES = [
  ...[0, 1].flatMap((who) => POSES.map((p) => `barb-${who}-${p}`)),
  ...[0, 1].flatMap((who) => UNARMED.map((p) => `barb-${who}-${p}`)),
  ...Array.from({ length: 8 }, (_, k) => `sword-${k}`), "sword-stuck",
  "sky-0", "sky-1", "sky-2", "arena", "hud", "hud-blade-empty", "hud-blade-full", "hud-blade-tip",
];

function readIndexedPng(bytes) {
  const chunks = {};
  for (let at = 8; at < bytes.length;) {
    const len = bytes.readUInt32BE(at);
    const type = bytes.toString("latin1", at + 4, at + 8);
    chunks[type] = Buffer.concat([chunks[type] ?? Buffer.alloc(0), bytes.subarray(at + 8, at + 8 + len)]);
    at += 12 + len;
  }
  const w = chunks.IHDR.readUInt32BE(0), h = chunks.IHDR.readUInt32BE(4), depth = chunks.IHDR[8];
  assert.equal(chunks.IHDR[9], 3, "an indexed PNG");
  const raw = inflateSync(chunks.IDAT);
  const stride = Math.ceil((w * depth) / 8);
  const lines = [];
  let prior = new Uint8Array(stride);
  for (let y = 0; y < h; y++) {
    const filter = raw[y * (stride + 1)];
    const line = Uint8Array.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    for (let i = 0; i < stride; i++) {
      const a = i ? line[i - 1] : 0, b = prior[i], c = i ? prior[i - 1] : 0;
      const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
      const pred = [0, a, b, (a + b) >> 1, pa <= pb && pa <= pc ? a : pb <= pc ? b : c][filter];
      line[i] = (line[i] + pred) & 255;
    }
    lines.push(line);
    prior = line;
  }
  const trns = chunks.tRNS ?? Buffer.alloc(0);
  const px = new Uint8Array(w * h);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const bit = x * depth;
      const entry = (lines[y][bit >> 3] >> (8 - depth - (bit & 7))) & ((1 << depth) - 1);
      const rgb = [...chunks.PLTE.subarray(3 * entry, 3 * entry + 3)];
      px[y * w + x] = trns[entry] === 0 ? CLEAR : PALETTE.findIndex((p) => p.every((v, i) => v === rgb[i]));
    }
  }
  return px;
}

const defaultPngs = () => PNG_NAMES.map((name) => readIndexedPng(readFileSync(`${root}assets/scene/${name}.png`)));
const skyFrames = FIRST_FRAME[SLOTS.findIndex((s) => s.sky)];
const asPainted = (frames) => frames.map((px, i) => (i >= skyFrames && i < skyFrames + skies.length ? overSky(px, skies[i - skyFrames]) : px));
const build = (frames, name = "RED MOON RITE") => encodePack(frames, name, { skies });
const decoded = asPainted(decodePack(shipped).frames);
const frameOf = (slotId, k = 0) => FIRST_FRAME[slotId - 1] + k;

const FREE = shipped[32 + 4 * SLOTS.length + 5];

function seeded(seed) {
  let s = seed;
  const next = () => { s = (s * 1103515245 + 12345) >>> 0; return s >>> 16; };
  next.colour = () => { const c = next() % 15; return c >= FREE ? c + 1 : c; };
  return next;
}

function scribbled(frames, at, paint) {
  const out = frames.map((f) => f.slice());
  out[at] = out[at].map(paint);
  return out;
}

function firmwareSees(pack) {
  assert.equal(refusalWords(core.check(pack)), null, "the firmware's own loader takes the pack");
  return core.frames();
}

function everyReadWithinAPage(pack) {
  const tables = 32 + 4 * SLOTS.length;
  for (let i = 0; i < FRAMES; i++) {
    const e = tables + 12 * i;
    const [w, h] = [pack[e + 2], pack[e + 3]];
    const table = pack[e + 10] | (pack[e + 11] << 8);
    assert.ok(withinAPage(table, 2 * h), `frame ${i}'s row table crosses a page`);
    for (let r = 0; r < h; r++) {
      const at = pack[table + 2 * r] | (pack[table + 2 * r + 1] << 8);
      let len = (w - 1) >> 5, x = 0;
      while (x < w) x += (pack[at + len++] >> 4) + 1;
      assert.ok(withinAPage(at, len), `frame ${i} row ${r}: its packet at ${at} (${len} bytes) crosses page ${Math.floor(at / PAGE)}`);
    }
  }
}

await test(`the art core is ${readFileSync(wasm).length} bytes and imports nothing`, async () => {
  const m = await WebAssembly.compile(readFileSync(wasm));
  assert.deepEqual(WebAssembly.Module.imports(m), []);
  assert.ok(readFileSync(wasm).length <= 32 * 1024, "the size budget: 32 KiB");
});

await test("the shipped pack decodes to the pixels the firmware's own decoder draws, frame by frame", () => {
  const stored = decodePack(shipped);
  assert.equal(stored.name, "RED MOON RITE");
  assert.deepEqual(firmwareSees(shipped), stored.frames);
});

await test("built in the browser from assets/scene's PNGs, the pack is the shipped .sidart byte for byte", () => {
  assert.deepEqual(build(defaultPngs()), shipped);
});

await test("decoded and built again unchanged, the default pack is byte for byte the shipped one", () => {
  assert.deepEqual(build(decoded), shipped);
});

await test("a replaced frame is what the firmware draws, every other frame unchanged", () => {
  const next = seeded(7);
  const arena = frameOf(8);
  const frames = scribbled(decoded, arena, (p, k) => (k % 240 < 40 ? next.colour() : p));
  const pack = build(frames, "NOISY ARENA");
  assert.equal(decodePack(pack).name, "NOISY ARENA");
  assert.deepEqual(asPainted(firmwareSees(pack)), frames);
  everyReadWithinAPage(pack);
});

await test("an unarmed pose drawn as its armed pose is coded as a one-pixel patch over it", () => {
  const unarmed = frameOf(3), armed = frameOf(1);
  const frames = decoded.map((f, i) => (i === unarmed ? decoded[armed].slice() : f));
  const pack = build(frames);
  const e = 32 + 4 * SLOTS.length + 12 * unarmed;
  assert.deepEqual([...pack.subarray(e, e + 4)], [0, 0, 1, 1]);
  assert.equal(pack[e + 8], armed);
  assert.deepEqual(firmwareSees(pack)[unarmed], decoded[armed]);
});

await test("every row table and row packet stays within one 1,248-byte page, the default and a crowded pack", () => {
  everyReadWithinAPage(shipped);
  const next = seeded(11);
  const crowded = [frameOf(8), frameOf(7, 1), frameOf(7, 2)].reduce((f, at) => scribbled(f, at, (p) => (next() % 12 ? p : next.colour())), decoded);
  const pack = build(crowded);
  assert.ok(pack.length > shipped.length + PAGE, `crowded: ${pack.length} bytes`);
  everyReadWithinAPage(pack);
  assert.deepEqual(asPainted(firmwareSees(pack)), crowded);
});

await test("a pack over the store's 65,536 bytes is refused in words", () => {
  const next = seeded(3);
  const noise = SLOTS.reduce((f, s) => (s.w === 240 ? s.frames.reduce((g, _, k) => scribbled(g, frameOf(s.id, k), () => next.colour()), f) : f), decoded);
  assert.throws(() => build(noise), (e) => e instanceof ArtError && /over the store's 65,536 bytes/.test(e.message));
});

await test("the CRC closes the pack: its residue checks, and one changed byte is refused", () => {
  assert.equal(crc32(shipped), 0x2144df1c);
  const broken = shipped.slice();
  broken[5000] ^= 1;
  assert.equal(packRefusal(broken), "its CRC does not match its bytes");
  assert.equal(artRefusal(broken), packRefusal(broken));
  assert.equal(refusalWords(core.check(broken)), "its CRC does not match its bytes");
});

await test("with every colour in the pack, each frame codes transparency with a colour it does not use; a frame of all 16 is refused", () => {
  const bar = frameOf(9), fighter = frameOf(1);
  const every = scribbled(scribbled(decoded, fighter, (p, k) => (k < 15 ? k + (k >= FREE) : p)), bar, (p, k) => (k ? p : FREE));
  const pack = build(every);
  assert.notEqual(pack[32 + 4 * SLOTS.length + 12 * fighter + 5], pack[32 + 4 * SLOTS.length + 12 * bar + 5]);
  assert.deepEqual(asPainted(firmwareSees(pack)), every);
  const full = scribbled(every, bar, (p, k) => (k < 16 ? k : p));
  assert.throws(() => build(full), /The status bar, status bar: uses all 16 colours; leave one out/);
});

await test("the pack's name is 1 to 16 characters of the FM-1's font", () => {
  for (const [name, why] of [["", /1 to 16/], ["A NAME TOO LONG HERE", /1 to 16/], [" LEADING", /start with a space/], ["lower", /"l" is not in the FM-1's font/]]) {
    assert.match(nameRefusal(name), why);
    assert.throws(() => build(decoded, name), why);
  }
  assert.equal(nameRefusal("MY RITE #2"), null);
});

await test("a sheet of the wrong size is refused in words: the size, the frame layout, and a screen-width hint", () => {
  const fighter = SLOTS[0], sky = SLOTS[6], tip = SLOTS[11];
  assert.equal(sheetRefusal(fighter, 512, 124), null);
  assert.equal(sheetRefusal(fighter, 64, 124),
    "The left fighter, armed takes a 512 × 124 PNG, its 8 frames of 64 × 124 side by side in a row; this one is 64 × 124.");
  assert.match(sheetRefusal(fighter, 1024, 124), /drawn at the screen's width: this slot is stored at 1\/2 width/);
  assert.match(sheetRefusal(sky, 240, 372), /720 × 124 PNG, its 3 frames of 240 × 124 side by side/);
  assert.match(sheetRefusal(tip, 6, 6), /6 × 5 PNG, one frame of 6 × 5; this one is 6 × 6\.$/);
});

await test("a sheet splits into its frames in a row and joins back", () => {
  for (const slot of SLOTS) {
    const frames = decoded.slice(FIRST_FRAME[slot.id - 1], FIRST_FRAME[slot.id]);
    assert.deepEqual(framesFromSheet(slot, sheetFromFrames(slot, frames)), frames, slot.name);
  }
  const fighter = SLOTS[0];
  const sheet = sheetFromFrames(fighter, decoded.slice(0, 8));
  assert.equal(sheet[fighter.w * 3 + 1], decoded[3][1], "frame 3 starts 3 frames' widths in");
});

await test("the palette reduction keeps the art's colours, takes the nearest for others, and alpha under half is transparent", () => {
  const rgba = Uint8Array.from([
    ...PALETTE[5], 255,
    0xb0, 0xc0, 0x70, 255,
    250, 250, 250, 200,
    9, 9, 9, 127,
    0, 0, 0, 0,
  ]);
  const r = toPalette(rgba);
  assert.deepEqual([...r.px], [5, 7, 1, CLEAR, CLEAR]);
  assert.equal(r.moved, 2);
  assert.equal(r.clear, 2);
});

await test("a pack the page has not built is checked in words: the header, the slots, the name", () => {
  const withByte = (at, v) => { const b = shipped.slice(); b[at] = v; return b; };
  assert.equal(packRefusal(shipped), null);
  assert.equal(packRefusal(withByte(0, 88)), "not a SIDOU art pack (.sidart)");
  assert.match(packRefusal(withByte(33, 65)), /slots are not RED MOON RITE's/);
  assert.match(packRefusal(withByte(16, 0x61)), /pack name/);
  assert.throws(() => decodePack(withByte(0, 88)), ArtError);
});

await test("the preview plays the pack it is given: a repainted arena is on the screen the firmware's code draws", () => {
  const trace = parseTrace(readFileSync(`${root}tests/golden/scene/rmr-trace.tsv`, "utf8"));
  assert.ok(trace.length >= 700);
  assert.deepEqual(trace[6].beats, [{ s: 2, bar: 0, hits: 4, bass: 1 }]);
  const screens = (pack) => {
    firmwareSees(pack);
    core.start();
    return trace.slice(0, 50).map((f) => core.play(f)).at(-1);
  };
  const before = screens(shipped);
  const BOTTOM_LEFT = (SIDE - 1) * SIDE;
  const white = scribbled(decoded, frameOf(8), (p, k) => (p !== CLEAR ? 1 : p));
  const after = screens(build(white));
  assert.notEqual(before[BOTTOM_LEFT], 1);
  assert.equal(after[BOTTOM_LEFT], 1);
  assert.notDeepEqual(before, after);
});

console.log(`art.test.mjs: ${n} tests passed (${wasm})`);
