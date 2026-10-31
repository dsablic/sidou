// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The song editor's work in plain Node (web/song-edit.js and the wasm core's edits and previews,
// web/core.js), against what the host build of the same C makes (web/core-fixtures.json's
// "editor", host/crates/sidou-tests/tests/web_core.rs): a script of edits through the FM-1's own
// primitives, refusals among them, each step's .sng byte for byte; both save-size refusals; the
// parameter locks following the rows; undo and redo; the previews' samples (the song from an
// orderlist position through the effect slots, a pattern on one voice, an instrument at a note);
// the FM-1's words for cells, orderlist entries and table rows; then saving to a simulated FM-1
// (web/fake-fm1.mjs: PUT target 0 with the overwrite check, PUT target 2 and SAVE AS FIRST).
// Opens no MIDI port.
//   node web/editor.test.mjs [sidou-core.wasm]    (tools/web-demo --check runs it)
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { EDIT, EditError, loadCore } from "./core.js";
import { connect } from "./fake-fm1.mjs";
import { resample } from "./preview.js";
import { KEYOFF, REST, SongSession, cellText, orderText, parseNote, parseOrder, tableMeaning } from "./song-edit.js";
import { Library } from "./store.js";
import { crc32 } from "./sysex.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = `${here}../`;
const wasm = process.argv[2] ?? `${root}build/wasm-core/sidou-core.wasm`;
const core = await loadCore(readFileSync(wasm));
const fx = JSON.parse(readFileSync(`${here}core-fixtures.json`, "utf8")).editor;
const sum = (b) => createHash("sha256").update(b).digest("hex").slice(0, 16);
const fp = (b) => [b.length, sum(b)];
const pcm = (s, ch = 1) => [s.length / ch, sum(Buffer.from(s.buffer, s.byteOffset, s.byteLength))];
const plus = Uint8Array.from(Buffer.from(fx.song, "hex"));
const P = fx.pattern;
let n = 0;
const test = async (name, fn) => { await fn(); n++; console.log(`ok - ${name}`); };
const code = (fn) => {
  try { fn(); return 0; } catch (e) { assert.ok(e instanceof EditError, e.message); return e.code; }
};

await test("each edit through the wasm core is the host build's, refusals and all", () => {
  core.read(plus);
  for (const [[op, a, b, c, d], result, added, len, hash, saved] of fx.steps) {
    const before = core.write().bytes;
    const r = code(() => core.edit(op, a, b, c, d));
    assert.equal(r, result, `op ${op} (${a}, ${b}, ${c}, ${d})`);
    if (op === EDIT.PATTERN_ADD && r === 0) assert.equal(core.x.sc_added(), added);
    const after = core.write().bytes;
    assert.deepEqual(fp(after), [len, hash], `op ${op} (${a}, ${b}, ${c}, ${d}): the .sng`);
    assert.equal(core.savedBytes, saved);
    if (r !== 0) assert.deepEqual(after, before, "a refused edit changes nothing");
  }
});

await test("the save-size limits: the song's memory (FULL) and the 64 KiB save (SONG FULL)", () => {
  const [arenaCount, arena, saveCount, save, len, hash] = fx.fill;
  core.read(plus);
  let k = 0;
  for (;;) {
    const r = code(() => core.addPattern(128));
    if (r) { assert.equal(r, arena); break; }
    k++;
  }
  assert.equal(k, arenaCount);
  assert.match(new EditError(arena).message, /^SONG FULL: no room/);
  core.read(plus);
  while (core.addPattern(1) !== 207);
  core.setOrder(0, 0, 0, 207);
  for (let p = 51; p < 207; p++) core.removePattern(p);
  k = 0;
  for (;;) {
    const r = code(() => core.addPattern(128));
    if (r) { assert.equal(r, save); break; }
    k++;
  }
  assert.equal(k, saveCount);
  assert.match(new EditError(save).message, /^SONG FULL: .*over 64 KiB/);
  assert.deepEqual(fp(core.write().bytes), [len, hash]);
  assert.ok(core.savedBytes <= 65536);
});

await test("parameter locks follow the rows: INS, DEL, a row inserted or deleted, a cut", () => {
  core.read(plus);
  const at = (row) => core.locks(P, row).map((l) => `${l.name}=${l.value}`).join(",");
  assert.equal(at(4), "CUTOFF=900,AGE=100");
  assert.equal(at(10), "PULSE=2048");
  core.pushRow(P, 2); // TRACKER's INS above them: they move down a row
  assert.deepEqual([at(4), at(5), at(11)], ["", "CUTOFF=900,AGE=100", "PULSE=2048"]);
  core.pullRow(P, 5); // DEL on the locked row: its locks go, the rest move up
  assert.deepEqual([at(5), at(10), at(11)], ["", "PULSE=2048", ""]);
  core.insertRow(P, 0);
  assert.equal(at(11), "PULSE=2048");
  core.deleteRow(P, 0);
  assert.equal(at(10), "PULSE=2048");
  core.resizePattern(P, 8); // cut: the locks past the end go
  assert.equal(at(10), "");
  core.resizePattern(P, 32);
  assert.equal(at(10), "", "and do not come back");
});

await test("undo and redo give back the song byte for byte; a refused change is no step", () => {
  const core2 = core;
  const s = new SongSession(core2);
  s.open(plus, { name: "COMET PLUS" });
  const v0 = s.bytes();
  s.apply("instrument", (c) => c.setInstrument(1, 0, 0x0f));
  const v1 = s.bytes();
  s.apply("row", (c) => c.setRow(P, 0, [parseNote("C-4"), 1, 0, 0]));
  s.apply("ins", (c) => c.pushRow(P, 0));
  const v3 = s.bytes();
  assert.throws(() => s.apply("bad", (c) => c.resizePattern(P, 200)), (e) => e instanceof EditError && e.code === 4);
  assert.throws(() => s.apply("half", (c) => { c.setInstrument(2, 0, 1); c.removePattern(P); }), EditError);
  assert.deepEqual(s.bytes(), v3, "a refusal halfway is undone at once");
  assert.equal(s.undoStack.length, 3);
  assert.ok(s.dirty);
  s.undo(); s.undo();
  assert.deepEqual(s.bytes(), v1);
  s.redo();
  s.redo();
  assert.deepEqual(s.bytes(), v3);
  s.undo(); s.undo(); s.undo();
  assert.deepEqual(s.bytes(), v0);
  assert.equal(s.canUndo, false);
  s.redo();
  s.apply("other", (c) => c.setTable(0, 0, 1, 0x81));
  assert.equal(s.canRedo, false, "a new change drops the redo");
});

await test("the previews' samples are the host build's", () => {
  core.read(plus);
  core.play(0, 2);
  assert.deepEqual(pcm(core.renderOut(2), 2), fx.out_pcm, "the song from position 2 through the effect slots");
  assert.deepEqual(pcm(core.renderPattern(P, { channel: 1, seconds: 2 })), fx.pattern_pcm, "a pattern on voice 2");
  assert.deepEqual(pcm(core.renderInstrument(1)), fx.instrument_pcm, "instrument 1 at C-4");
  const before = core.write().bytes;
  core.renderPattern(P, { channel: 0, seconds: 0.1 });
  assert.deepEqual(core.write().bytes, before, "a pattern's preview leaves the song as it was");
  // an effect slot is heard: the output's two sides differ (the chorus spreads it)
  core.play(0, 0);
  const lr = core.renderOut(1);
  let diff = 0;
  for (let i = 0; i < lr.length; i += 2) diff += Math.abs(lr[i] - lr[i + 1]);
  assert.ok(diff > 0, "stereo out of the slots");
});

await test("resampling from 44,118 Hz: the same rate passes, another keeps the pitch", () => {
  const src = new Float32Array(44118).map((_, i) => Math.sin((2 * Math.PI * 441 * i) / 44118));
  const r0 = resample(src, 44118, 44118);
  assert.deepEqual(Array.from(r0.slice(0, 100)), Array.from(src.slice(0, 100)));
  const r1 = resample(src, 44118, 48000);
  assert.ok(Math.abs(r1.length - 48000) <= 1);
  let crossings = 0;
  for (let i = 1; i < r1.length; i++) if (r1[i - 1] < 0 && r1[i] >= 0) crossings++;
  assert.ok(Math.abs(crossings - 441) <= 1, `441 Hz stays 441 Hz (${crossings})`);
});

await test("the FM-1's words: cells, orderlist entries, table rows", () => {
  for (const [h, want] of fx.cells) assert.equal(cellText(Array.from(Buffer.from(h, "hex"))), want, h);
  fx.orders.forEach((want, e) => {
    assert.equal(orderText(e), want, `entry ${e}`);
    assert.equal(parseOrder(want), e, `parse ${want}`);
  });
  for (const [t, l, r, want] of fx.meanings) assert.equal(tableMeaning(t, l, r), want, `${t} ${l} ${r}`);
  assert.equal(parseNote("C-4"), 0x60 + 48);
  assert.equal(parseNote("c#4"), 0x60 + 49);
  assert.equal(parseNote("Db4"), 0x60 + 49);
  assert.equal(parseNote("G#7"), 0xbc);
  assert.equal(parseNote("A-7"), null);
  assert.equal(parseNote("---"), REST);
  assert.equal(parseNote("==="), KEYOFF);
  assert.equal(parseNote("x"), null);
});

await test("saving to the FM-1: a named file (the overwrite check), and Load now (PUT target 2)", async () => {
  const s = new SongSession(core);
  s.open(plus, { name: "COMET PLUS" });
  s.apply("row", (c) => c.setRow(P, 0, [parseNote("E-3"), 2, 0, 0]));
  const { device, client } = connect({ files: [{ name: "COMET", type: 1, bytes: readFileSync(`${root}assets/songs/chrome-comet.sng`) }], current: plus });
  const lib = new Library(client);
  await lib.refresh();
  assert.equal(await s.saveToFm1(lib, "COMET", { confirmReplace: async () => false }), "kept");
  assert.equal(device.stored.get("COMET").length, readFileSync(`${root}assets/songs/chrome-comet.sng`).length, "not replaced");
  assert.ok(s.dirty);
  const asked = [];
  assert.equal(await s.saveToFm1(lib, "COMET", { confirmReplace: async (name) => { asked.push(name); return true; } }), "sent");
  assert.deepEqual(asked, ["COMET"]);
  assert.deepEqual(device.stored.get("COMET"), s.bytes());
  assert.equal(s.dirty, false);
  assert.equal(await s.saveToFm1(lib, "COMET"), "same", "identical: nothing sent");
  assert.equal(await s.saveToFm1(lib, "NEW ONE"), "sent");
  assert.equal(lib.find("NEW ONE").crc, crc32(s.bytes()));
  // Load now: the FM-1's current song from now on, as GET target 2 reads it
  s.apply("instrument", (c) => c.setInstrument(1, 0, 0x22));
  await s.loadNow(client);
  assert.deepEqual(device.current, s.bytes());
  assert.deepEqual(await client.get({ target: 2 }), s.bytes());
  assert.equal(device.sent.filter((h) => h.startsWith("f07d534404")).length, 3, "three PUTs in all");
  // SAVE AS FIRST: BUSY at once, no retry
  device.saveAsFirst = true;
  const puts = device.sent.length;
  await assert.rejects(() => s.loadNow(client), (e) => e.nak === 4);
  assert.equal(device.sent.length, puts + 1, "one PUT, no DATA, no retry");
});

console.log(`editor.test.mjs: ${n} tests passed`);
