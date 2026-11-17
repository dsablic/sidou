// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { TuneError, loadTune } from "./tune.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = `${here}../`;
const wasm = process.argv[2] ?? `${root}build/wasm-tune/sidou-tune.wasm`;
const TUNE_BUDGET = 96 * 1024;
const tune = await loadTune(readFileSync(wasm));
const fixtures = JSON.parse(readFileSync(`${here}tune-fixtures.json`, "utf8"));
let n = 0;
const test = async (name, fn) => { await fn(); n++; console.log(`ok - ${name}`); };

const callBytes = (c) => {
  const b = Buffer.alloc(4 + c.writes.length * 7);
  b.writeUInt32LE(c.start >>> 0, 0);
  c.writes.forEach(([cycle, chip, reg, value], k) => {
    const o = 4 + k * 7;
    b.writeUInt32LE(cycle >>> 0, o);
    b[o + 4] = chip; b[o + 5] = reg; b[o + 6] = value;
  });
  return b;
};

const digests = (calls, per) => {
  const out = [];
  for (let k = 0; k < calls.length; k += per) {
    const h = createHash("sha256");
    calls.slice(k, k + per).forEach((c) => h.update(callBytes(c)));
    out.push(h.digest("hex").slice(0, 16));
  }
  return out;
};

await test("the module is within its budget", () => {
  assert.ok(readFileSync(wasm).length <= TUNE_BUDGET, `${readFileSync(wasm).length} bytes, the budget 96 KiB`);
});

for (const f of fixtures.tunes) {
  await test(`${f.file} song ${f.song}: every call's writes are the host build's psid_env's`, () => {
    tune.read(readFileSync(`${root}${f.file}`));
    const run = tune.capture({ song: f.song, seconds: fixtures.seconds });
    assert.equal(run.calls.length, f.calls);
    assert.equal(run.calls.reduce((s, c) => s + c.writes.length, 0), f.writes);
    assert.deepEqual(digests(run.calls, fixtures.callsADigest), f.digests);
  });
}

await test("refusals are in words", () => {
  assert.throws(() => tune.read(new Uint8Array(200)), (e) => e instanceof TuneError && /not a \.sid file/.test(e.message));
  const rsid = Uint8Array.from(readFileSync(`${root}assets/starter/dojo.sid`));
  rsid.set([0x52], 0);
  assert.throws(() => tune.read(rsid), /RSID/);
  tune.read(readFileSync(`${root}assets/starter/dojo.sid`));
  assert.throws(() => tune.capture({ song: 9 }), /no song 9 \(it has 4\)/);
});

console.log(`tune: ${n} tests passed`);
