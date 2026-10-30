// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The wasm core (sidou-core.wasm, tools/wasm-core; web/core.js) in plain Node, against what the
// host build of the same C makes (web/core-fixtures.json, host/crates/sidou-tests/tests/web_core.rs):
// for SIDOU's songs and GoatTracker's examples the read, the .sng written back with and without
// SIDOU's trailer (and the plain file's check), every instrument's GTI5, and the device's player's
// samples for the song and an instrument at a note, byte for byte; refusals with the device's
// messages. Opens no MIDI port.
//   node web/core.test.mjs [sidou-core.wasm]    (tools/web-demo --check runs it)
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { CoreError, TRAILER, loadCore, noteName } from "./core.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = `${here}../`;
const wasm = process.argv[2] ?? `${root}build/wasm-core/sidou-core.wasm`;
const core = await loadCore(readFileSync(wasm));
const fixtures = JSON.parse(readFileSync(`${here}core-fixtures.json`, "utf8")).songs;
const sum = (b) => createHash("sha256").update(b).digest("hex").slice(0, 16);
const fp = (b) => [b.length, sum(b)];
const pcm = (s) => [s.length, sum(Buffer.from(s.buffer, s.byteOffset, s.byteLength))];
let n = 0;
const test = async (name, fn) => { await fn(); n++; console.log(`ok - ${name}`); };

const fileOf = (f) => (f.hex ? Uint8Array.from(Buffer.from(f.hex, "hex")) : readFileSync(`${root}${f.name}`));

await test(`the wasm core is ${readFileSync(wasm).length} bytes and imports nothing`, async () => {
  const m = await WebAssembly.compile(readFileSync(wasm));
  assert.deepEqual(WebAssembly.Module.imports(m), []);
  assert.ok(readFileSync(wasm).length <= 160 * 1024, "the size budget: 160 KiB");
});

for (const f of fixtures) {
  await test(`${f.name}: as the host build`, () => {
    const bytes = fileOf(f);
    let info;
    try {
      info = core.read(bytes);
    } catch (e) {
      assert.ok(e instanceof CoreError);
      assert.equal(f.read.ok, false, `refused: ${e.message}`);
      assert.equal(e.message, f.read.message);
      assert.equal(core.loaded, false);
      assert.throws(() => core.write(), /no song loaded/);
      return;
    }
    assert.equal(f.read.ok, true);
    assert.deepEqual([info.message, info.version, info.channels, info.trailer],
      [f.read.message, f.read.version, f.read.channels, TRAILER[f.read.trailer]]);
    const sng = core.write();
    assert.deepEqual(fp(sng.bytes), f.sng, ".sng with the trailer");
    assert.equal(sng.warning, "");
    const plain = core.write({ trailer: false });
    assert.deepEqual(fp(plain.bytes), f.plain, "plain .sng");
    assert.equal(plain.warning, f.check[0] === 0 ? "" : f.check[1], "the plain file's check");
    assert.ok(f.goattracker, "GoatTracker 2.77 loads and saves the plain file unchanged");
    f.gti5.forEach((want, k) => {
      if (want.length === 1) assert.throws(() => core.exportInstrument(k + 1), (e) => e.code === -want[0]);
      else assert.deepEqual(fp(core.exportInstrument(k + 1)), want, `instrument ${k + 1}`);
    });
    if (f.song_pcm) assert.deepEqual(pcm(core.renderSong({ seconds: 2 })), f.song_pcm, "the song's samples");
    if (f.instrument_pcm) assert.deepEqual(pcm(core.renderInstrument(1)), f.instrument_pcm, "instrument 1 at C-4");
  });
}

await test("SIDOU's songs write back byte for byte, and the plain file reads as GoatTracker's", () => {
  for (const f of fixtures.filter((x) => x.name.startsWith("assets/songs/") && x.read.trailer === 1)) {
    const bytes = readFileSync(`${root}${f.name}`);
    core.read(bytes);
    assert.deepEqual(core.write().bytes, Uint8Array.from(bytes), f.name);
    const plain = core.write({ trailer: false }).bytes;
    const info = core.read(plain);
    assert.equal(info.trailer, "none");
    assert.deepEqual(core.write({ trailer: false }).bytes, plain);
  }
});

await test("the song for display: strings, instruments, tables, orderlists, patterns", () => {
  core.read(readFileSync(`${root}assets/songs/chrome-comet.sng`));
  const s = core.song();
  assert.equal(s.name, "CHROME COMET");
  assert.equal(s.channels, 3);
  assert.equal(s.subtunes, 1);
  assert.equal(s.instruments.length, 12);
  assert.ok(s.instruments.every((i) => i.bytes.length === 9 && typeof i.name === "string"));
  assert.deepEqual(s.tables.map((t) => t.name), ["wave", "pulse", "filter", "speed"]);
  assert.ok(s.tables[0].rows > 0 && s.tables[0].left.length === s.tables[0].rows);
  assert.equal(s.orders.length, 1);
  assert.equal(s.orders[0].length, 3);
  assert.ok(s.orders[0].every((o) => o.entries.length > 0));
  assert.equal(s.patterns.length, 51);
  const p = s.patterns[s.orders[0][0].entries.find((e) => e < 0xd0)];
  assert.ok(p.held && p.cells.length === p.rows && p.cells.every((c) => c.length === 4));
  assert.equal(noteName(48), "C-4");
  assert.equal(noteName(13), "C#1");
});

await test("a 6-channel GoatTracker Stereo song is read and kept", () => {
  const f = fixtures.find((x) => x.name === "ambiguous, stereo");
  const info = core.read(fileOf(f));
  assert.equal(info.channels, 6);
  assert.equal(core.song().orders[0].length, 6);
  assert.deepEqual(core.write().bytes, fileOf(f));
});

await test("a refusal leaves no song, and the next read works", () => {
  core.read(readFileSync(`${root}assets/songs/iron-rite.sng`));
  assert.throws(() => core.read(Uint8Array.from([0x47, 0x54, 0x53])), (e) => e instanceof CoreError && /^SNG: /.test(e.message));
  assert.throws(() => core.song(), /no song loaded/);
  core.read(readFileSync(`${root}assets/songs/iron-rite.sng`));
  assert.equal(core.song().name, "IRON RITE");
});

console.log(`${n} tests passed`);
