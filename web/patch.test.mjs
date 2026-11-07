// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { loadCore } from "./core.js";
import { LINK_KEY, LINK_MAX, fromLink, linkOf, loadPatcher, patchOf } from "./patch.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = `${here}../`;
const wasm = process.argv[2] ?? `${root}build/wasm-patch/sidou-patch.wasm`;
const corePath = process.argv[3] ?? `${root}build/wasm-core/sidou-core.wasm`;
const patcher = await loadPatcher(readFileSync(wasm), readFileSync(corePath));
const cases = JSON.parse(readFileSync(`${here}patch-fixtures.json`, "utf8")).patches;
const bank = JSON.parse(readFileSync(`${root}assets/sounds/sidou-sounds.json`, "utf8"));
const page = "https://dsablic.github.io/sidou/library.html";
const sum = (b) => createHash("sha256").update(b).digest("hex").slice(0, 16);
let n = 0;
const test = async (name, fn) => { await fn(); n++; console.log(`ok - ${name}`); };

await test(`the patch converter is ${readFileSync(wasm).length} bytes and imports nothing`, async () => {
  const m = await WebAssembly.compile(readFileSync(wasm));
  assert.deepEqual(WebAssembly.Module.imports(m), []);
  assert.ok(readFileSync(wasm).length <= 80 * 1024, "the size budget: 80 KiB");
});

await test("every case reads as the host build reads it: the same song, warning or refusal", () => {
  for (const c of cases) {
    const got = patcher.check(c.text);
    if (!c.ok) {
      assert.deepEqual(got, { ok: false, error: c.error }, c.text);
      continue;
    }
    assert.equal(got.ok, true, got.error);
    assert.deepEqual([got.name, got.quiet, [got.sng.length, sum(got.sng)]], [c.name, c.quiet, c.sng], c.text);
  }
});

await test("the sixteen sounds' patch files are the sixteen, each a one-instrument song the core plays", async () => {
  const core = await loadCore(readFileSync(corePath));
  for (const s of bank.sounds) {
    const got = patcher.check(readFileSync(`${root}assets/sounds/${s.patch}`, "utf8"));
    assert.equal(got.name, s.name);
    const info = core.read(got.sng);
    assert.equal(info.trailer, "read");
    const song = core.song();
    assert.deepEqual([song.name, song.instruments.length], [s.name, 1]);
    const pcm = core.renderInstrument(1);
    assert.ok(pcm.some((v) => Math.abs(v) > 512), `${s.name} is heard`);
  }
});

await test("a song's LAB sounds are written as the host writes them, and a plain instrument has none", async () => {
  const core = await loadCore(readFileSync(corePath));
  core.read(readFileSync(`${root}assets/sounds/sidou-sounds.sng`));
  for (const [i, s] of bank.sounds.entries()) {
    const kept = core.kept(i + 1);
    assert.equal(patcher.write(kept.sound, kept.name), readFileSync(`${root}assets/sounds/${s.patch}`, "utf8"), s.name);
  }
  assert.equal(core.kept(17), null);
  core.read(readFileSync(`${root}assets/songs/chrome-comet.sng`));
  assert.equal(core.kept(1), null);
});

await test("a link carries each of the sixteen and comes back as the same patch", () => {
  for (const s of bank.sounds) {
    const text = readFileSync(`${root}assets/sounds/${s.patch}`, "utf8");
    const link = linkOf(page, text);
    assert.ok(link.length < 2048, `${s.name}: ${link.length}`);
    const back = fromLink(link.slice(link.indexOf("#")));
    assert.equal(back.text, patchOf(text));
    assert.deepEqual(patcher.check(back.text).sng, patcher.check(text).sng, s.name);
  }
});

await test("a model's link of raw or encoded JSON reads; a damaged or oversized one says what to do", () => {
  const raw = `${LINK_KEY}{"format":"sidou-patch/1","name":"HI"}`;
  assert.equal(patcher.check(fromLink(raw).text).name, "HI");
  assert.match(fromLink(`${LINK_KEY}%7B%22format%22%3A%E0%A4%A`).error, /damaged.*Paste the patch's JSON/);
  assert.match(fromLink(`${LINK_KEY}${"%20".repeat(LINK_MAX)}`).error, /too long.*Paste the patch's JSON/);
  assert.equal(fromLink("#other"), null);
  assert.equal(linkOf(page, "no patch here"), null);
});

await test("the sixteen as a bank are the sixteen's song byte for byte, and fit a link", () => {
  const text = readFileSync(`${root}assets/sounds/${bank.bank}`, "utf8");
  const got = patcher.check(text);
  assert.deepEqual([got.bank, got.name, got.sounds], [true, bank.name, bank.sounds.map((s) => s.name)]);
  assert.deepEqual(got.sng, Uint8Array.from(readFileSync(`${root}assets/sounds/${bank.song}`)));
  const link = linkOf(page, text);
  assert.ok(link.includes("#bank=") && link.length - link.indexOf("#") < LINK_MAX, `${link.length}`);
  assert.deepEqual(patcher.check(fromLink(link.slice(link.indexOf("#"))).text).sng, got.sng);
  console.log(`  the sixteen's bank link: ${link.length} characters`);
});

await test("each STEP pattern is a song the core plays from its first row, and its link carries it", async () => {
  const core = await loadCore(readFileSync(corePath));
  for (const name of ["acid-line", "beat", "tune"]) {
    const text = readFileSync(`${root}assets/steps/${name}.json`, "utf8");
    const got = patcher.check(text);
    assert.equal(got.step, true, name);
    core.read(got.sng);
    const pcm = core.renderSong({ seconds: 2 });
    assert.ok(pcm.some((v) => Math.abs(v) > 512), `${name} is heard`);
    const link = linkOf(page, text);
    assert.ok(link.includes("#step="));
    assert.deepEqual(patcher.check(fromLink(link.slice(link.indexOf("#"))).text).sng, got.sng);
  }
});

await test("a whole answer around the patch is read", () => {
  const text = readFileSync(`${root}assets/sounds/${bank.sounds[1].patch}`, "utf8");
  const answer = `Here you go:\n\`\`\`json\n${text}\`\`\`\nPlay it low.`;
  assert.deepEqual(patcher.check(answer).sng, patcher.check(text).sng);
});

console.log(`patch.test.mjs: ${n} tests passed`);
