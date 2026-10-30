// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The Library page's store work (web/store.js) and its .zip (web/zip.js) in plain Node, against a
// simulated FM-1 (web/fake-fm1.mjs): list, download, send with the overwrite check, delete,
// rename, back up and restore, and the refusals. Opens no MIDI port.
//   node web/library.test.mjs        (tools/web-demo --check runs it)
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { connect } from "./fake-fm1.mjs";
import { Library, LibraryError, MANIFEST, storeName, typeOfFile } from "./store.js";
import { crc32 } from "./sysex.js";
import { unzip, zip } from "./zip.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const song = (f) => Uint8Array.from(readFileSync(`${here}../assets/songs/${f}`));
const sid = Uint8Array.from(readFileSync(`${here}../assets/starter/sanction.sid`));
let n = 0;
const test = async (name, fn) => { await fn(); n++; console.log(`ok - ${name}`); };

const fm1 = (files = [], opts = {}) => {
  const { device, client } = connect({ files, ...opts });
  return { device, lib: new Library(client) };
};
const comet = song("chrome-comet.sng"), rite = song("iron-rite.sng"), loop = song("sidou-loop.sng");
const stocked = () => fm1([
  { name: "CHROME COMET", type: 1, bytes: comet },
  { name: "SANCTION", type: 2, bytes: sid },
  { name: "LIFTED SOUND", type: 3, bytes: Uint8Array.from([0x47, 0x54, 0x49, 0x35, 1, 2, 3]) },
]);

await test("the listing: every file with its type, size and CRC, and the free space", async () => {
  const { lib } = stocked();
  await lib.refresh();
  assert.deepEqual(lib.files.map((f) => [f.name, f.type, f.length, f.crc]), [
    ["CHROME COMET", 1, comet.length, crc32(comet)], ["SANCTION", 2, sid.length, crc32(sid)], ["LIFTED SOUND", 3, 7, lib.files[2].crc]]);
  assert.equal(lib.info.free, 40000);
});

await test("download: GET of any type, checked against its CRC", async () => {
  const { lib } = stocked();
  await lib.refresh();
  assert.deepEqual(await lib.download("SANCTION"), sid);
  assert.deepEqual(await lib.download("CHROME COMET"), comet);
  await assert.rejects(lib.download("NOPE"), (e) => e instanceof LibraryError && e.code === "missing");
});

await test("send: new, identical (nothing sent), different (only when asked to replace)", async () => {
  const { device, lib } = stocked();
  await lib.refresh();
  assert.equal(await lib.send("IRON RITE", rite), "sent");
  assert.deepEqual(device.stored.get("IRON RITE"), rite);
  assert.equal(lib.info.free, 40000 - rite.length, "INQ again after the send");
  const before = device.sent.length;
  assert.equal(await lib.send("CHROME COMET", comet), "same");
  assert.equal(device.sent.length, before, "an identical file is not sent");
  await assert.rejects(lib.send("CHROME COMET", rite), (e) => e.code === "exists");
  assert.deepEqual(device.stored.get("CHROME COMET"), comet, "not replaced without asking");
  assert.equal(await lib.send("CHROME COMET", rite, { replace: true }), "sent");
  assert.deepEqual(device.stored.get("CHROME COMET"), rite);
  assert.equal(await lib.send("A TUNE", sid, { type: 2 }), "sent");
  assert.equal(lib.find("A TUNE").type, 2);
});

await test("send refusals: a bad name, over 64 KiB, the store full", async () => {
  const { lib } = fm1([], { free: 3000 });
  await lib.refresh();
  await assert.rejects(lib.send("lower case", comet), (e) => e.code === "name");
  await assert.rejects(lib.send("MIDI SONG", comet), (e) => e.code === "name", "a reserved name");
  await assert.rejects(lib.send("BIG", new Uint8Array(65537)), (e) => e.code === "size");
  await assert.rejects(lib.send("TOO BIG", comet), (e) => e.nak === 5 && /store is full/.test(e.message));
});

await test("delete: DEL, then the listing and free space again; a missing file is refused", async () => {
  const { device, lib } = stocked();
  await lib.refresh();
  await lib.remove("SANCTION");
  assert.deepEqual(device.deleted, ["SANCTION"]);
  assert.deepEqual(lib.files.map((f) => f.name), ["CHROME COMET", "LIFTED SOUND"]);
  assert.equal(lib.info.free, 40000 + sid.length);
  await assert.rejects(lib.remove("SANCTION"), (e) => e.nak === 6 && /no file named SANCTION/.test(e.message));
});

await test("delete waits out BUSY and sends again, as a PUT does", async () => {
  const { device, client } = connect({ files: [{ name: "A", type: 1, bytes: loop }], nak: { command: 0x07, at: 0, reason: 4, times: 2 } },
    { timeout: 300, ackTimeout: 300, busy: [1, 1, 1], sleep: async () => {} });
  const lib = new Library(client);
  await lib.refresh();
  const busy = [];
  await lib.remove("A", { onBusy: (ms, k) => busy.push(k) });
  assert.deepEqual(busy, [1, 2]);
  assert.deepEqual(device.deleted, ["A"]);
});

await test("rename: GET, PUT under the new name with its type, DEL the old; refusals change nothing", async () => {
  const { device, lib } = stocked();
  await lib.refresh();
  await lib.rename("SANCTION", "CIA SANCTION");
  assert.deepEqual(lib.files.map((f) => [f.name, f.type]), [["CHROME COMET", 1], ["LIFTED SOUND", 3], ["CIA SANCTION", 2]]);
  assert.deepEqual(device.stored.get("CIA SANCTION"), sid);
  assert.ok(!device.stored.has("SANCTION"));
  await assert.rejects(lib.rename("CIA SANCTION", "CHROME COMET"), (e) => e.code === "exists");
  await assert.rejects(lib.rename("CIA SANCTION", "bad"), (e) => e.code === "name");
  await assert.rejects(lib.rename("GONE", "X"), (e) => e.code === "missing");
  assert.equal(lib.files.length, 3);
  await lib.rename("CIA SANCTION", "CHROME COMET", { replace: true });
  assert.deepEqual(device.stored.get("CHROME COMET"), sid);
  assert.equal(lib.find("CHROME COMET").type, 2);
  assert.equal(lib.files.length, 2);
});

await test("a failed PUT in a rename keeps the old file", async () => {
  const { device, lib } = stocked();
  device.free = 0;
  await lib.refresh();
  await assert.rejects(lib.rename("CHROME COMET", "COMET"), (e) => e.nak === 5);
  assert.deepEqual(device.stored.get("CHROME COMET"), comet);
  assert.equal(device.deleted, undefined);
});

await test("backup and restore round trip: names, types and bytes come back, private names never appear", async () => {
  const { lib } = stocked();
  await lib.refresh();
  const steps = [];
  const b = await lib.backup({ onProgress: (k, t, name) => steps.push([k, t, name]), date: new Date(2026, 9, 30, 12, 0, 0) });
  assert.equal(b.count, 3);
  assert.equal(b.name, "fm1-backup-2026-10-30.zip");
  assert.deepEqual(steps.at(-1), [3, 3, ""]);
  const entries = await unzip(b.bytes);
  assert.deepEqual(entries.map((e) => e.name), [MANIFEST, "CHROME COMET.sng", "SANCTION.sid", "LIFTED SOUND.ins"]);
  assert.ok(!entries.some((e) => /MIDI (SONG|INSTRUMENT)/.test(e.name)));
  // onto an empty FM-1
  const empty = fm1();
  await empty.lib.refresh();
  const done = await empty.lib.restore(b.bytes);
  assert.deepEqual(done.sent, ["CHROME COMET", "SANCTION", "LIFTED SOUND"]);
  for (const f of lib.files) {
    assert.deepEqual(empty.device.stored.get(f.name), await lib.download(f.name), f.name);
    assert.equal(empty.lib.find(f.name).type, f.type);
  }
  // again: all identical, nothing sent
  const before = empty.device.sent.length;
  const again = await empty.lib.restore(b.bytes);
  assert.deepEqual([again.sent, again.same], [[], ["CHROME COMET", "SANCTION", "LIFTED SOUND"]]);
  assert.equal(empty.device.sent.slice(before).filter((h) => h.slice(8, 10) === "04").length, 0, "no PUT");
  assert.ok(empty.device.sent.length > before);
});

await test("restore asks once about files that differ, and keeps the FM-1's when told to", async () => {
  const { lib } = stocked();
  await lib.refresh();
  const b = await lib.backup();
  const other = fm1([{ name: "CHROME COMET", type: 1, bytes: rite }, { name: "KEEP ME", type: 1, bytes: loop }]);
  await other.lib.refresh();
  const asked = [];
  const kept = await other.lib.restore(b.bytes, { replace: async (names) => { asked.push(names); return false; } });
  assert.deepEqual(asked, [["CHROME COMET"]]);
  assert.deepEqual([kept.sent, kept.kept], [["SANCTION", "LIFTED SOUND"], ["CHROME COMET"]]);
  assert.deepEqual(other.device.stored.get("CHROME COMET"), rite);
  const replaced = await other.lib.restore(b.bytes, { replace: async () => true });
  assert.deepEqual(replaced.sent, ["CHROME COMET"]);
  assert.deepEqual(other.device.stored.get("CHROME COMET"), comet);
  assert.ok(other.lib.find("KEEP ME"), "a restore deletes nothing");
});

await test("restore refusals: not a zip, a damaged entry, a hand-made zip of files, the store full", async () => {
  const { lib } = fm1([], { free: 100 });
  await lib.refresh();
  await assert.rejects(lib.restore(comet), /not a \.zip/);
  const good = zip([{ name: "a.sng", bytes: comet }]);
  const bad = Uint8Array.from(good);
  bad[60] ^= 0xff;
  await assert.rejects(lib.restore(bad), /damaged/);
  const plain = await Library.readBackup(zip([{ name: "songs/Iron Rite.sng", bytes: rite }, { name: "notes.txt", bytes: new Uint8Array(3) }, { name: "tune.sid", bytes: sid }]));
  assert.deepEqual(plain.files.map((f) => [f.name, f.type]), [["IRON RITE", 1], ["TUNE", 2]]);
  assert.deepEqual(plain.skipped.map((s) => s.name), ["notes.txt"]);
  await assert.rejects(lib.restore(good), (e) => e.nak === 5 && e.done.sent.length === 0);
});

await test("the .zip opens in unzip, and a deflated .zip reads too", async () => {
  const dir = mkdtempSync(join(tmpdir(), "sidou-zip-"));
  try {
    const z = zip([{ name: "CHROME COMET.sng", bytes: comet }, { name: "a/b.txt", bytes: new TextEncoder().encode("hi") }]);
    writeFileSync(join(dir, "t.zip"), z);
    let unzipped = true;
    try { execFileSync("unzip", ["-tq", join(dir, "t.zip")]); } catch (e) { if (e.code === "ENOENT") unzipped = false; else throw e; }
    if (unzipped) {
      execFileSync("unzip", ["-q", join(dir, "t.zip"), "-d", join(dir, "out")]);
      assert.deepEqual(Uint8Array.from(readFileSync(join(dir, "out", "CHROME COMET.sng"))), comet);
    }
    // deflated by Python's zipfile (what a person's own zip tool might make)
    let python = true;
    try {
      execFileSync("python3", ["-I", "-c", "import sys,zipfile\nwith zipfile.ZipFile(sys.argv[1],'w',zipfile.ZIP_DEFLATED) as z: z.write(sys.argv[2],'x.sng')", join(dir, "d.zip"), `${here}../assets/songs/chrome-comet.sng`]);
    } catch (e) { if (e.code === "ENOENT") python = false; else throw e; }
    if (python) assert.deepEqual((await unzip(Uint8Array.from(readFileSync(join(dir, "d.zip")))))[0].bytes, comet);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

await test("file types and store names from file names", () => {
  assert.equal(typeOfFile("x.bin", comet), 1);
  assert.equal(typeOfFile("x.bin", sid), 2);
  assert.equal(typeOfFile("lead.ins", new Uint8Array(4)), 3);
  assert.equal(typeOfFile("readme.txt", new Uint8Array(4)), null);
  assert.equal(storeName("chrome-comet.sng"), "CHROME-COMET");
  assert.equal(storeName("C:\\music\\Mon Ami – très.sng"), "MON AMI ? TR?S");
  assert.equal(storeName("a very long song name indeed.sng"), "A VERY LONG SONG");
  assert.equal(storeName("   .sng"), "FILE");
});

console.log(`library.test.mjs: ${n} tests passed`);
