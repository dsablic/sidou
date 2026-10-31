// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { chromium } from "playwright";

const url = process.argv[2];
const shot = process.argv[3];
const failures = [];
const fail = (msg) => { failures.push(msg); console.error(`FAIL: ${msg}`); };

const KEY_A4 = "h", KEY_PLAY = " ", KEY_C5 = "q";
const LCD_ON_CANVAS = { x: 370, y: 42, size: 480 };
const NOTE_RELEASE_MS = 800, KEY_DEBOUNCE_MS = 150, RING_OUT_MS = 1500;

function watch() {
  const BOOT_SECTOR_BYTES = 4 + 4096, PAST_TITLE_CARD_START_MS = 500;
  const w = (window.__sidou = { frames: 0, title: null, last: null, errors: [], analyser: null, bootSectors: null });
  const Base = window.AudioWorkletNode;
  window.AudioWorkletNode = class extends Base {
    constructor(ctx, name, opts) {
      super(ctx, name, opts);
      const post = this.port.postMessage.bind(this.port);
      this.port.postMessage = (m, ...rest) => {
        if (m?.type === "boot") w.bootSectors = m.sectors ? m.sectors.byteLength / BOOT_SECTOR_BYTES : 0;
        return post(m, ...rest);
      };
      w.analyser = ctx.createAnalyser();
      w.analyser.fftSize = 2048;
      this.connect(w.analyser);
      this.port.addEventListener("message", (e) => {
        const m = e.data;
        if (m.type === "error") w.errors.push(m.message);
        if (m.type !== "frame") return;
        if (m.lcd) {
          let lit = 0, hash = 0;
          for (let i = 0; i < m.lcd.length; i++) {
            if (m.lcd[i]) lit++;
            hash = (Math.imul(hash, 31) + m.lcd[i]) | 0;
          }
          w.picture = { lit, hash };
        }
        const { lit, hash } = w.picture || { lit: 0, hash: 0 };
        w.frames++;
        w.last = { ms: m.ms, lit, hash, halted: m.halted, short: m.short, minsns: m.minsns };
        if (!w.title && m.ms >= PAST_TITLE_CARD_START_MS) w.title = w.last;
      });
    }
  };
  w.level = async (ms) => {
    const buf = new Float32Array(w.analyser.fftSize);
    let sum = 0, n = 0;
    const end = performance.now() + ms;
    while (performance.now() < end) {
      w.analyser.getFloatTimeDomainData(buf);
      for (const v of buf) { sum += v * v; n++; }
      await new Promise((r) => setTimeout(r, 20));
    }
    return Math.sqrt(sum / n);
  };
}

function fakeMidi() {
  const log = (window.__midi = { other: [], fm1: [], requested: null });
  const inputs = new Map([["i1", { id: "i1", name: "Other Synth", onmidimessage: null }],
    ["i2", { id: "i2", name: "SIDOU FM-1", onmidimessage: null }]]);
  let chain = Promise.resolve();
  const reply = async (command, sequence, data = []) => {
    const { encodeFrame } = await import("./sysex.js");
    inputs.get("i2").onmidimessage?.({ data: encodeFrame(command, sequence, Uint8Array.from(data)) });
  };
  const u32 = (v) => [v & 255, (v >>> 8) & 255, (v >>> 16) & 255, (v >>> 24) & 255];
  let upload = null, listing = null, dirSeq = 0;
  const tune = new Uint8Array(400);
  tune.set([0x50, 0x53, 0x49, 0x44, 0, 2, 0, 0x7c]);
  let files = [{ name: "IRON RITE", type: 1, length: 10, crc: 1 }, { name: "A TUNE", type: 2, length: 400, crc: 2 }];
  const stored = new Map([["A TUNE", tune]]);
  const dirNext = async () => {
    if (!listing) return;
    const e = listing.shift();
    const seq = dirSeq;
    dirSeq = (dirSeq + 1) & 127;
    if (!e) { listing = null; return reply(6, seq); }
    const name = new Array(16).fill(0);
    [...e.name].forEach((c, i) => { name[i] = c.charCodeAt(0); });
    return reply(2, seq, [...name, e.type ?? 1, ...u32(e.length), ...u32(e.crc)]);
  };
  let out = null, outSeq = 0;
  const outNext = async () => {
    const next = out.shift();
    if (!next) { out = null; return; }
    const seq = outSeq;
    outSeq = (outSeq + 1) & 127;
    return reply(next[0], seq, next[1]);
  };
  const get = async (f) => {
    const { crc32 } = await import("./sysex.js");
    const d = f.data;
    let file, name = new Array(16).fill(0), bytes, type = 1;
    if (d[0] === 2 && log.current) bytes = log.current;
    if (d[0] === 2) file = "chrome-comet.sng";
    else {
      const n = String.fromCharCode(...d.slice(2, 18)).replace(/\0+$/, "");
      const there = files.find((x) => x.name === n);
      if (!there) return reply(0x7f, f.sequence, [6, 0]);
      type = there.type;
      bytes = stored.get(n);
      file = bytes ? n : n.toLowerCase().replace(/ /g, "-") + ".sng";
      name = Array.from(d.slice(2, 18));
    }
    if (!bytes) {
      const r = await fetch(`songs/${file}`);
      if (!r.ok) return reply(0x7f, f.sequence, [6, 0]);
      bytes = new Uint8Array(await r.arrayBuffer());
    }
    log.gets = [...(log.gets || []), { target: d[0], file }];
    out = [[4, [d[0], 0, ...name, type, ...u32(bytes.length), ...u32(crc32(bytes))]]];
    for (let o = 0; o < bytes.length; o += 256) out.push([5, bytes.slice(o, o + 256)]);
    out.push([6, []]);
    outSeq = f.sequence;
    return outNext();
  };
  const device = async (bytes) => {
    const { decodeFrame } = await import("./sysex.js");
    log.fm1.push(Array.from(bytes));
    const f = decodeFrame(bytes);
    if (f.command === 1) return reply(1, f.sequence, [1, 0, 2, 0, 0, 1, 0, ...u32(30000), 6, 1]);
    if (f.command === 2) { listing = files.map((x) => ({ ...x })); dirSeq = f.sequence; return dirNext(); }
    if (f.command === 3) return get(f);
    if (f.command === 0x7e) return out ? outNext() : dirNext();
    if (f.command === 4) {
      const d = f.data;
      upload = { target: d[0], name: String.fromCharCode(...d.slice(2, 18)).replace(/\0+$/, ""), type: d[18], data: [] };
      log.puts = [...(log.puts || [])];
      return reply(0x7e, f.sequence);
    }
    if (f.command === 5) { upload.data.push(...f.data); return reply(0x7e, f.sequence); }
    if (f.command === 6) {
      const { crc32 } = await import("./sysex.js");
      const bytes = Uint8Array.from(upload.data);
      if (upload.target === 2) {
        log.current = bytes;
        log.loads = [...(log.loads || []), bytes.length];
        upload = null;
        return reply(0x7e, f.sequence);
      }
      stored.set(upload.name, bytes);
      files = files.filter((x) => x.name !== upload.name).concat({ name: upload.name, type: upload.type, length: bytes.length, crc: crc32(bytes) });
      log.puts.push(upload);
      upload = null;
      return reply(0x7e, f.sequence);
    }
    if (f.command === 7) {
      const n = String.fromCharCode(...f.data).replace(/\0+$/, "");
      if (!files.some((x) => x.name === n)) return reply(0x7f, f.sequence, [6, 0]);
      files = files.filter((x) => x.name !== n);
      stored.delete(n);
      log.dels = [...(log.dels || []), n];
      return reply(0x7e, f.sequence);
    }
  };
  const outputs = new Map([["o1", { id: "o1", name: "Other Synth", send: (b) => log.other.push(Array.from(b)) }],
    ["o2", { id: "o2", name: "SIDOU FM-1", send: (b) => { chain = chain.then(() => device(b)); } }]]);
  navigator.requestMIDIAccess = async (opts) => { log.requested = opts; return { inputs, outputs, onstatechange: null }; };
}

const browser = await chromium.launch({ args: ["--autoplay-policy=no-user-gesture-required"] });
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  page.on("console", (m) => { if (m.type() === "error") fail(`console error: ${m.text()}`); });
  page.on("pageerror", (e) => fail(`page error: ${e.message}`));
  page.on("requestfailed", (r) => fail(`request failed: ${r.url()} (${r.failure()?.errorText})`));
  page.on("response", (r) => { if (r.status() >= 400) fail(`HTTP ${r.status()}: ${r.url()}`); });
  await page.addInitScript(watch);
  await page.goto(url);

  const build = page.locator("#build");
  await page.waitForFunction(() => !/loading/.test(document.getElementById("build").textContent));
  console.log(`page: ${await page.title()} · ${await build.textContent()}`);

  const status = () => page.locator("#status").textContent();
  const start = async () => {
    await page.locator("#start").click();
    try {
      await page.waitForFunction(() => window.__sidou.last?.ms > 3000 || window.__sidou.errors.length, null, { timeout: 60_000 });
    } catch {
      fail(`no 3 s of emulated time within 60 s (status: ${await status()})`);
    }
  };
  await start();
  const first = await page.evaluate(() => window.__sidou.bootSectors);
  if (first !== 0) fail(`a fresh browser booted with ${first} saved flash sectors`);
  const s = await page.evaluate(() => window.__sidou);
  if (s.errors.length) fail(`audio thread: ${s.errors.join("; ")}`);
  if (s.last) {
    console.log(`booted: ${s.frames} frames, ${s.last.ms} ms emulated, ${s.last.lit} lit LCD pixels, ` +
      `${s.last.short} short quanta · ${await status()}`);
    if (s.last.halted) fail("the emulated CPU halted");
    if (s.last.lit < 5000) fail(`the LCD is nearly black (${s.last.lit} lit pixels)`);
    if (!s.title || s.title.hash === s.last.hash) fail("the screen did not change after the title card");
  }

  const canvasLit = await page.evaluate(({ x, y, size }) => {
    const c = document.getElementById("panel");
    const d = c.getContext("2d").getImageData(x, y, size, size).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] | d[i + 1] | d[i + 2]) n++;
    return n;
  }, LCD_ON_CANVAS);
  console.log(`canvas LCD: ${canvasLit} of ${LCD_ON_CANVAS.size ** 2} pixels not black`);
  if (canvasLit < 20_000) fail("the canvas LCD is nearly black");

  const quiet = await page.evaluate(() => window.__sidou.level(400));
  const before = await page.evaluate(() => window.__sidou.last?.hash);
  await page.keyboard.down(KEY_A4);
  await page.waitForTimeout(300);
  const note = await page.evaluate(() => window.__sidou.level(500));
  const during = await page.evaluate(() => window.__sidou.last?.hash);
  await page.keyboard.up(KEY_A4);
  console.log(`A4 held: output RMS ${quiet.toFixed(5)} -> ${note.toFixed(5)}; screen ${before === during ? "unchanged" : "changed"}`);
  if (before === during) fail("holding a note key did not change the screen");
  if (!(note > 4 * quiet + 1e-4)) fail("holding a note key made no sound");

  await page.waitForTimeout(NOTE_RELEASE_MS);
  const still = await page.evaluate(() => window.__sidou.level(300));
  const play = async () => {
    await page.keyboard.down(KEY_PLAY);
    await page.waitForTimeout(KEY_DEBOUNCE_MS);
    await page.keyboard.up(KEY_PLAY);
  };
  await play();
  await page.waitForTimeout(600);
  const song = await page.evaluate(() => window.__sidou.level(1000));
  await play();
  await page.waitForTimeout(RING_OUT_MS);
  const after = await page.evaluate(() => window.__sidou.level(400));
  console.log(`PLAY: output RMS ${still.toFixed(5)} -> ${song.toFixed(5)}, stopped ${after.toFixed(5)}`);
  if (!(song > 4 * still + 1e-4)) fail("PLAY made no sound");
  if (!(after < song / 4)) fail("PLAY again did not stop the song");

  if (shot) {
    await page.keyboard.down(KEY_A4);
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot, fullPage: false });
    await page.keyboard.up(KEY_A4);
    console.log(`screenshot: ${shot}`);
  }

  const saveStatus = () => page.locator("#save-status").textContent();
  await page.locator("#save-now").click();
  try {
    await page.waitForFunction(() => document.getElementById("save-status").textContent === "Saved in this browser",
      null, { timeout: 20_000 });
  } catch {
    fail(`Save now did not save (save status: ${await saveStatus()})`);
  }
  await page.reload();
  await page.waitForFunction(() => !/loading/.test(document.getElementById("build").textContent));
  await start();
  const kept = await page.evaluate(() => window.__sidou.bootSectors);
  console.log(`reload: booted with ${kept} saved flash sectors · save: ${await saveStatus()}`);
  if (!(kept > 0)) fail("the reload booted without the saved flash");
  const again = await page.evaluate(() => window.__sidou);
  if (again.last?.halted) fail("the emulated CPU halted after the reload");

  const songs = await browser.newPage({ viewport: { width: 900, height: 900 } });
  songs.on("console", (m) => { if (m.type() === "error") fail(`songs page console error: ${m.text()}`); });
  songs.on("pageerror", (e) => fail(`songs page error: ${e.message}`));
  songs.on("requestfailed", (r) => fail(`songs page request failed: ${r.url()}`));
  songs.on("response", (r) => { if (r.status() >= 400) fail(`songs page HTTP ${r.status()}: ${r.url()}`); });
  const asked = [];
  songs.on("dialog", (d) => { asked.push(d.message()); d.accept(); });
  await songs.addInitScript(fakeMidi);
  await songs.goto(new URL("songs.html", url).href);
  if (await songs.locator("#songs .song").count() !== 4) fail("the Songs page does not list four songs");
  if (!(await songs.locator("#send-all").isDisabled())) fail("Send all is enabled before connecting");
  await songs.waitForTimeout(300);
  const sentEarly = await songs.evaluate(() => window.__midi.fm1.length + window.__midi.other.length);
  if (sentEarly !== 0) fail("the Songs page sent MIDI before Connect");
  await songs.locator("#connect").click();
  try {
    await songs.waitForFunction(() => /SIDOU 0\.2\.0 .* 30000 bytes free/.test(document.getElementById("device").textContent), null, { timeout: 10_000 });
    await songs.waitForFunction(() => !document.getElementById("send-all").disabled, null, { timeout: 5_000 });
  } catch {
    fail(`the Songs page did not find the FM-1: ${await songs.locator("#device").textContent()}`);
  }
  if (!/IRON RITE/.test(await songs.locator("#files").textContent())) fail("the Songs page does not list the device's files");
  await songs.locator("#send-all").click();
  try {
    await songs.waitForFunction(() => /4 sent/.test(document.getElementById("all-state").textContent), null, { timeout: 20_000 });
  } catch {
    fail(`Send all did not finish: ${await songs.locator("#all-state").textContent()} / ${await songs.locator(".state").allTextContents()}`);
  }
  if (asked.length !== 1 || !/IRON RITE/.test(asked[0])) fail(`expected one replace question about IRON RITE, got ${JSON.stringify(asked)}`);
  const sent = await songs.evaluate(async () => {
    const { crc32 } = await import("./sysex.js");
    const out = [];
    for (const put of window.__midi.puts || []) {
      const file = put.name.toLowerCase().replace(/ /g, "-") + ".sng";
      const want = new Uint8Array(await (await fetch(`songs/${file}`)).arrayBuffer());
      out.push({ name: put.name, same: put.data.length === want.length && crc32(Uint8Array.from(put.data)) === crc32(want), length: want.length });
    }
    return { out, other: window.__midi.other.length, requested: window.__midi.requested };
  });
  console.log(`songs page: sent ${sent.out.map((o) => `${o.name} (${o.length} B)`).join(", ")}`);
  if (sent.out.length !== 4 || sent.out.some((o) => !o.same)) fail(`the PUTs do not carry the site's songs: ${JSON.stringify(sent.out)}`);
  if (sent.other !== 0) fail("the Songs page sent to the other port");
  if (!sent.requested?.sysex) fail("Web MIDI was not asked for SysEx");
  await songs.close();

  const midi = await browser.newPage({ viewport: { width: 390, height: 800 } });
  midi.on("console", (m) => { if (m.type() === "error") fail(`midi page console error: ${m.text()}`); });
  midi.on("pageerror", (e) => fail(`midi page error: ${e.message}`));
  midi.on("requestfailed", (r) => fail(`midi page request failed: ${r.url()}`));
  midi.on("response", (r) => { if (r.status() >= 400) fail(`midi page HTTP ${r.status()}: ${r.url()}`); });
  await midi.addInitScript(fakeMidi);
  await midi.goto(new URL("midi.html", url).href);
  if (await midi.locator("#songs .song").count() !== 4) fail("the MIDI page does not list four songs");
  if (!(await midi.locator("#result").isHidden())) fail("the MIDI page shows a result before a song is chosen");
  await midi.locator("#songs .song button").first().click();
  try {
    await midi.waitForSelector("#result:not([hidden])", { timeout: 10_000 });
  } catch {
    fail(`the MIDI page made nothing: ${await midi.locator("#error").textContent()}`);
  }
  const mid = await midi.evaluate(async () => {
    const a = document.getElementById("download");
    const bytes = new Uint8Array(await (await fetch(a.href)).arrayBuffer());
    return { name: a.download, head: String.fromCharCode(...bytes.slice(0, 4)), tracks: (bytes[10] << 8) | bytes[11], size: bytes.length,
      wide: document.documentElement.scrollWidth <= window.innerWidth };
  });
  console.log(`midi page: ${mid.name}, ${mid.size} bytes, ${mid.tracks} tracks`);
  if (mid.head !== "MThd" || mid.tracks !== 4 || mid.name !== "chrome-comet.mid" || mid.size < 5000) fail(`the MIDI page's download is wrong: ${JSON.stringify(mid)}`);
  if (!mid.wide) fail("the MIDI page scrolls sideways at phone width");

  const touched = () => midi.evaluate(() => window.__midi.fm1.length + window.__midi.other.length + (window.__midi.requested ? 1 : 0));
  if (await touched() !== 0) fail("the MIDI page used MIDI before Connect");
  await midi.locator("#connect").click();
  try {
    await midi.waitForFunction(() => /SIDOU 0\.2\.0/.test(document.getElementById("device").textContent), null, { timeout: 10_000 });
  } catch {
    fail(`the MIDI page did not find the FM-1: ${await midi.locator("#device").textContent()}`);
  }
  const cards = await midi.locator("#fm1 .song h3").allTextContents();
  if (cards.join("|") !== "The current song|IRON RITE") fail(`the MIDI page lists ${JSON.stringify(cards)} from the FM-1`);
  const readFrom = async (n) => {
    await midi.locator("#fm1 .song button").nth(n).click();
    try {
      await midi.waitForFunction((k) => document.getElementById("download").textContent.includes(k), n ? "IRON-RITE" : "fm1-current-song", { timeout: 10_000 });
    } catch {
      fail(`the MIDI page did not read song ${n} from the FM-1: ${await midi.locator("#error").textContent()}`);
    }
    return midi.evaluate(async () => {
      const a = document.getElementById("download");
      const bytes = new Uint8Array(await (await fetch(a.href)).arrayBuffer());
      return { name: a.download, head: String.fromCharCode(...bytes.slice(0, 4)), size: bytes.length, gets: window.__midi.gets };
    });
  };
  const cur = await readFrom(0), stored = await readFrom(1);
  console.log(`midi page from the FM-1: ${cur.name} ${cur.size} bytes, ${stored.name} ${stored.size} bytes`);
  if (cur.head !== "MThd" || stored.head !== "MThd" || cur.name !== "fm1-current-song.mid" || stored.name !== "IRON-RITE.mid") fail(`the MIDI page's FM-1 downloads are wrong: ${JSON.stringify([cur, stored])}`);
  if (JSON.stringify(stored.gets) !== JSON.stringify([{ target: 2, file: "chrome-comet.sng" }, { target: 0, file: "iron-rite.sng" }])) fail(`the GETs were ${JSON.stringify(stored.gets)}`);
  if (await midi.evaluate(() => window.__midi.other.length) !== 0) fail("the MIDI page sent to the other port");
  await midi.close();

  const lib = await browser.newPage({ viewport: { width: 390, height: 900 } });
  lib.on("console", (m) => { if (m.type() === "error") fail(`library page console error: ${m.text()}`); });
  lib.on("pageerror", (e) => fail(`library page error: ${e.message}`));
  lib.on("requestfailed", (r) => fail(`library page request failed: ${r.url()}`));
  lib.on("response", (r) => { if (r.status() >= 400) fail(`library page HTTP ${r.status()}: ${r.url()}`); });
  const prompts = [];
  lib.on("dialog", (d) => (d.type() === "prompt" ? d.accept(prompts.shift() ?? d.defaultValue()) : d.accept()));
  await lib.addInitScript(fakeMidi);
  await lib.goto(new URL("library.html", url).href);
  const comet = new Uint8Array(await (await fetch(new URL("songs/chrome-comet.sng", url))).arrayBuffer());
  await lib.setInputFiles("#file", { name: "chrome-comet.sng", mimeType: "application/octet-stream", buffer: Buffer.from(comet) });
  try {
    await lib.waitForSelector("#song-panel:not([hidden])", { timeout: 10_000 });
  } catch {
    fail(`the Library page did not open a song: ${await lib.locator("#song-error").textContent()}`);
  }
  const saved = () => lib.evaluate(() => window.__library.saved.map((x) => ({ filename: x.filename, length: x.bytes.length,
    head: String.fromCharCode(...x.bytes.slice(0, 4)), hex: Array.from(x.bytes, (b) => b.toString(16).padStart(2, "0")).join("") })));
  await lib.locator("#export-sng").click();
  await lib.locator("#export-plain").click();
  await lib.locator("#instruments button").first().click();
  const exported = await saved();
  console.log(`library page exports: ${exported.map((x) => `${x.filename} (${x.length} B)`).join(", ")}`);
  const cometHex = Buffer.from(comet).toString("hex");
  if (await lib.locator("#s-name").textContent() !== "CHROME COMET" || await lib.locator("#instruments tr").count() !== 12) fail("the Library page shows the wrong song");
  if (exported.length !== 3 || exported[0].filename !== "CHROME-COMET.sng" || exported[0].hex !== cometHex) fail(`the .sng export is not the FM-1's save: ${JSON.stringify(exported.map((x) => x.filename))}`);
  if (exported[1]?.filename !== "CHROME-COMET-goattracker.sng" || exported[1].head !== "GTS5" || exported[1].length >= comet.length) fail("the plain export is wrong");
  if (!/\.ins$/.test(exported[2]?.filename ?? "") || exported[2].head !== "GTI5") fail("the GTI5 export is wrong");
  if (await lib.evaluate(() => window.__midi.fm1.length + window.__midi.other.length + (window.__midi.requested ? 1 : 0)) !== 0) fail("the Library page used MIDI before Connect");
  await lib.locator("#connect").click();
  try {
    await lib.waitForFunction(() => /SIDOU 0\.2\.0 .*2 files, 29\.3 KiB free/.test(document.getElementById("device").textContent), null, { timeout: 10_000 });
  } catch {
    fail(`the Library page did not find the FM-1: ${await lib.locator("#device").textContent()}`);
  }
  const names = () => lib.locator("#files td.name").allTextContents();
  if ((await names()).join("|") !== "IRON RITE|A TUNE") fail(`the Library lists ${JSON.stringify(await names())}`);
  const settle = async (what, cond) => {
    try {
      await lib.waitForFunction(cond, null, { timeout: 10_000 });
    } catch {
      fail(`the Library page did not ${what}: ${await lib.locator("#store-state").textContent()}`);
    }
  };
  await lib.locator('#files tr[data-name="A TUNE"] button', { hasText: "Delete" }).click();
  await settle("delete", () => /A TUNE deleted/.test(document.getElementById("store-state").textContent));
  prompts.push("RITE");
  await lib.locator('#files tr[data-name="IRON RITE"] button', { hasText: "Rename" }).click();
  await settle("rename", () => /IRON RITE is now RITE/.test(document.getElementById("store-state").textContent));
  if ((await names()).join("|") !== "RITE") fail(`after delete and rename the Library lists ${JSON.stringify(await names())}`);
  await lib.locator("#backup").click();
  await settle("back up", () => /1 file backed up/.test(document.getElementById("store-state").textContent));
  const zipped = await lib.evaluate(async () => {
    const { unzip } = await import("./zip.js");
    const z = window.__library.saved.at(-1);
    return { filename: z.filename, entries: (await unzip(z.bytes)).map((e) => e.name), dels: window.__midi.dels, puts: window.__midi.puts.map((p) => p.name) };
  });
  console.log(`library page: deleted ${zipped.dels.join(", ")}, renamed by PUT ${zipped.puts.join(", ")}, backup ${zipped.filename}: ${zipped.entries.join(", ")}`);
  if (!/^fm1-backup-\d{4}-\d\d-\d\d\.zip$/.test(zipped.filename) || zipped.entries.join("|") !== "sidou-backup.json|RITE.sng") fail(`the backup is wrong: ${JSON.stringify(zipped)}`);
  if (zipped.dels.join("|") !== "A TUNE|IRON RITE" || zipped.puts.join("|") !== "RITE") fail(`the store saw ${JSON.stringify(zipped)}`);
  await lib.locator('#files tr[data-name="RITE"] button', { hasText: "Open" }).click();
  try {
    await lib.waitForFunction(() => document.getElementById("s-name").textContent === "IRON RITE", null, { timeout: 10_000 });
  } catch {
    fail(`the Library page did not open a stored song: ${await lib.locator("#song-error").textContent()}`);
  }
  if (await lib.evaluate(() => window.__midi.other.length) !== 0) fail("the Library page sent to the other port");
  if (!(await lib.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth))) fail("the Library page scrolls sideways at phone width");
  await lib.close();

  const ed = await browser.newPage({ viewport: { width: 390, height: 900 } });
  ed.on("console", (m) => { if (m.type() === "error") fail(`editor console error: ${m.text()}`); });
  ed.on("pageerror", (e) => fail(`editor page error: ${e.message}`));
  ed.on("requestfailed", (r) => fail(`editor request failed: ${r.url()}`));
  ed.on("response", (r) => { if (r.status() >= 400) fail(`editor HTTP ${r.status()}: ${r.url()}`); });
  const edPrompts = [];
  ed.on("dialog", (d) => (d.type() === "prompt" ? d.accept(edPrompts.shift() ?? d.defaultValue()) : d.accept()));
  await ed.addInitScript(fakeMidi);
  await ed.goto(new URL("editor.html", url).href);
  await ed.setInputFiles("#file", { name: "chrome-comet.sng", mimeType: "application/octet-stream", buffer: Buffer.from(comet) });
  try {
    await ed.waitForSelector("#editor:not([hidden])", { timeout: 10_000 });
  } catch {
    fail(`the editor did not open a song: ${await ed.locator("#song-error").textContent()}`);
  }
  const edBytes = () => ed.evaluate(() => Array.from(window.__editor.session.bytes(), (b) => b.toString(16).padStart(2, "0")).join(""));
  if (await ed.locator("#s-name").textContent() !== "CHROME COMET" || await edBytes() !== cometHex) fail("the editor opened the wrong song, or not as the FM-1 saves it");
  await ed.locator('[data-key="i0"]').fill("0A");
  await ed.locator('[data-key="i0"]').press("Enter");
  await ed.locator(".tabs button", { hasText: "Patterns" }).click();
  const edP = await ed.evaluate(() => window.__editor.pattern);
  await ed.locator(`[data-key="p${edP}.0.0"]`).focus();
  await ed.keyboard.press(KEY_C5);
  const edited = await ed.evaluate((p) => ({ ad: window.__editor.core.instrument(1)[0], cell: window.__editor.core.pattern(p)[0], status: document.getElementById("status").textContent, undo: window.__editor.session.undoStack.length }), edP);
  console.log(`editor: ${edited.status}; ${edited.undo} edits`);
  if (edited.ad !== 0x0a || edited.cell[0] !== 0x60 + 60 || edited.cell[1] !== 1 || edited.undo !== 2) fail(`the editor's edits are wrong: ${JSON.stringify(edited)}`);
  await ed.locator("#undo").click();
  await ed.locator("#undo").click();
  if (await edBytes() !== cometHex) fail("undo did not give the song back byte for byte");
  await ed.locator("#redo").click();
  if (await ed.evaluate(() => window.__editor.core.instrument(1)[0]) !== 0x0a) fail("redo did not make the edit again");
  await ed.locator("#play").click();
  try {
    await ed.waitForFunction(() => /Playing the song · [1-9]/.test(document.getElementById("pv-state").textContent), null, { timeout: 10_000 });
  } catch {
    fail(`the editor's preview did not play: ${await ed.locator("#pv-state").textContent()}`);
  }
  if (!(await ed.evaluate(() => window.__editor.preview.worklet))) fail("the editor's preview has no AudioWorklet");
  await ed.locator("#stop").click();
  if (await ed.evaluate(() => window.__midi.fm1.length + window.__midi.other.length + (window.__midi.requested ? 1 : 0)) !== 0) fail("the editor used MIDI before Connect");
  await ed.locator("#connect").click();
  try {
    await ed.waitForFunction(() => /SIDOU 0\.2\.0 /.test(document.getElementById("device").textContent), null, { timeout: 10_000 });
  } catch {
    fail(`the editor did not find the FM-1: ${await ed.locator("#device").textContent()}`);
  }
  await ed.locator("#save-name").fill("COMET EDIT");
  await ed.locator("#save-fm1").click();
  await ed.waitForFunction(() => /Saved as COMET EDIT/.test(document.getElementById("save-state").textContent), null, { timeout: 10_000 })
    .catch(async () => fail(`the editor did not save to the FM-1: ${await ed.locator("#save-state").textContent()}`));
  await ed.locator("#load-now").click();
  await ed.waitForFunction(() => /has loaded the song/.test(document.getElementById("save-state").textContent), null, { timeout: 10_000 })
    .catch(async () => fail(`the editor did not load the song on the FM-1: ${await ed.locator("#save-state").textContent()}`));
  const edLog = await ed.evaluate(() => ({ puts: (window.__midi.puts || []).map((p) => `${p.target}:${p.name}:${p.data.length}`), loads: window.__midi.loads || [], other: window.__midi.other.length,
    size: window.__editor.session.bytes().length }));
  console.log(`editor and the FM-1: PUT ${edLog.puts.join(", ")}; loaded ${edLog.loads.join(", ")} bytes`);
  if (edLog.puts.join("|") !== `0:COMET EDIT:${edLog.size}` || edLog.loads.join("|") !== String(edLog.size) || edLog.other) fail(`the editor's saves are wrong: ${JSON.stringify(edLog)}`);
  await ed.locator("#dl-sng").click();
  if (await ed.evaluate(() => window.__editor.saved.at(-1)?.filename) !== "CHROME-COMET.sng") fail("the editor's .sng download is missing");
  if (!(await ed.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth))) fail("the editor scrolls sideways at phone width");
  await ed.close();
  const handoff = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  handoff.on("pageerror", (e) => fail(`library to editor page error: ${e.message}`));
  await handoff.goto(new URL("library.html", url).href);
  await handoff.setInputFiles("#file", { name: "chrome-comet.sng", mimeType: "application/octet-stream", buffer: Buffer.from(comet) });
  await handoff.waitForSelector("#song-panel:not([hidden])", { timeout: 10_000 }).catch(() => fail("the Library page did not open the song to hand over"));
  await handoff.locator("#edit-song").click();
  await handoff.waitForSelector("#editor:not([hidden])", { timeout: 10_000 }).catch(() => fail("the Library's Edit did not open the song in the editor"));
  if (await handoff.locator("#s-name").textContent() !== "CHROME COMET") fail("the Library's Edit opened the wrong song");
  await handoff.close();

  const sheet = await browser.newPage({ viewport: { width: 390, height: 844 } });
  sheet.on("pageerror", (e) => fail(`cheat sheet error: ${e.message}`));
  sheet.on("requestfailed", (r) => fail(`cheat sheet request failed: ${r.url()}`));
  sheet.on("response", (r) => { if (r.status() >= 400) fail(`cheat sheet HTTP ${r.status()}: ${r.url()}`); });
  await sheet.goto(new URL("cheatsheet.html", url).href, { waitUntil: "load" });
  const cs = await sheet.evaluate(() => ({ sections: document.querySelectorAll("section h2").length,
    footer: document.querySelector("footer")?.textContent || "", logo: document.querySelector("img")?.naturalWidth || 0,
    wide: document.documentElement.scrollWidth <= window.innerWidth }));
  console.log(`cheat sheet: ${cs.sections} sections`);
  if (cs.sections < 8 || !/SIDOU \d+\.\d+\.\d+/.test(cs.footer) || !cs.logo) fail(`the cheat sheet is incomplete: ${JSON.stringify(cs)}`);
  if (!cs.wide) fail("the cheat sheet scrolls sideways at phone width");
  await sheet.close();

  {
    const { readFileSync } = await import("node:fs");
    const linkFx = JSON.parse(readFileSync(new URL("link-fixtures.json", import.meta.url), "utf8"));
    const link = await browser.newPage({ viewport: { width: 390, height: 900 } });
    link.on("console", (m) => { if (m.type() === "error") fail(`link page console error: ${m.text()}`); });
    link.on("pageerror", (e) => fail(`link page error: ${e.message}`));
    link.on("requestfailed", (r) => fail(`link page request failed: ${r.url()}`));
    link.on("response", (r) => { if (r.status() >= 400) fail(`link page HTTP ${r.status()}: ${r.url()}`); });
    await link.addInitScript(fakeLinkMidi, linkFx);
    await link.goto(new URL("link.html", url).href);
    const used = () => link.evaluate(() => window.__midi.fm1.length + window.__midi.other.length);
    if (!(await link.locator("#start").isDisabled())) fail("the Live Link page's Start is enabled before Connect");
    await link.locator("#connect").click();
    try {
      await link.waitForFunction(() => !document.getElementById("start").disabled, null, { timeout: 5_000 });
    } catch {
      fail(`the Live Link page did not pick the FM-1's ports: ${await link.locator("#device").textContent()}`);
    }
    if (await used() !== 0) fail("the Live Link page sent MIDI before Start");
    await link.locator("#start").click();
    const canvasCrc = () => link.evaluate(async () => {
      const { crc32 } = await import("./link.js");
      const d = document.getElementById("screen").getContext("2d").getImageData(0, 0, 240, 240).data;
      const px = new Uint8Array(240 * 240 * 2);
      for (let i = 0; i < 240 * 240; i++) {
        const v = ((d[4 * i] >> 3) << 11) | ((d[4 * i + 1] >> 2) << 5) | (d[4 * i + 2] >> 3);
        px[2 * i] = v & 255;
        px[2 * i + 1] = v >> 8;
      }
      return crc32(px);
    });
    try {
      await link.waitForFunction(() => /Live: SIDOU FIXTURE/.test(document.getElementById("status").textContent), null, { timeout: 5_000 });
      await link.waitForTimeout(300);
    } catch {
      fail(`the Live Link session did not start: ${await link.locator("#status").textContent()}`);
    }
    const crc = await canvasCrc();
    console.log(`link page: ${await link.locator("#status").textContent()}; canvas CRC ${crc} (the panel's ${linkFx.picture_crc})`);
    if (crc !== linkFx.picture_crc) fail("the Live Link page's canvas is not the FM-1's screen");
    const home = link.locator(".btn", { hasText: "HOME" });
    await home.hover();
    await link.mouse.down();
    await link.waitForTimeout(700);
    await link.mouse.up();
    await link.locator(".enc", { hasText: "K1" }).locator("button", { hasText: "+" }).click();
    await link.waitForTimeout(200);
    const panels = await link.evaluate(() => window.__midi.panels);
    const homes = panels.filter((p) => p.held === 1 << 8);
    if (!homes.length || homes[0].pressed !== 1 << 8 || homes.length < 3) fail(`HOME held did not go as PANEL frames, again while held: ${JSON.stringify(panels)}`);
    const after = panels.slice(panels.indexOf(homes.at(-1)) + 1);
    if (!after.length || after[0].held !== 0) fail(`HOME let go did not send a PANEL with nothing held: ${JSON.stringify(panels)}`);
    if (!panels.some((p) => p.held === 0 && JSON.stringify(p.steps) === "[0,0,1]")) fail(`K1's + did not send a step: ${JSON.stringify(panels)}`);
    if (await link.evaluate(() => window.__midi.bad)) fail("the Live Link page sent a frame with a bad checksum or magic");
    await link.evaluate(() => window.dispatchEvent(new Event("pagehide")));
    await link.waitForTimeout(100);
    const last = await link.evaluate(() => window.__midi.fm1.at(-1));
    if (last?.[6] !== 0x03) fail(`the page going away did not send STOP: ${JSON.stringify(last)}`);
    if (await link.evaluate(() => window.__midi.on)) fail("the simulated FM-1 still has a session after the page went away");
    if (await link.evaluate(() => window.__midi.other.length) !== 0) fail("the Live Link page sent to the other port");
    if (!(await link.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth))) fail("the Live Link page scrolls sideways at phone width");
    await link.close();
  }
} finally {
  await browser.close();
}
if (failures.length) {
  console.error(`smoke test FAILED (${failures.length})`);
  process.exit(1);
}
console.log("smoke test passed");

function fakeLinkMidi(fx) {
  const START = 0x01, HEARTBEAT = 0x02, STOP = 0x03, KEYFRAME = 0x04, FONT = 0x05, PALETTE = 0x06, PANEL = 0x07;
  const AUTO_STOP_MS = 3000;
  const unpack = (packed) => {
    const d = [];
    for (let i = 0; i < packed.length; i += 8) packed.slice(i + 1, i + 8).forEach((x, k) => d.push(x | (((packed[i] >> k) & 1) << 7)));
    return d;
  };
  const bytes = (h) => Uint8Array.from(h.match(/../g), (x) => parseInt(x, 16));
  const f = Object.fromEntries(["hello", "palette", "font", "keyframe", "beat", "stopped"].map((k) => [k, fx[k].map(bytes)]));
  const log = (window.__midi = { other: [], fm1: [], requested: null, panels: [], notes: [], bad: 0, on: false });
  const inputs = new Map([["i1", { id: "i1", name: "Other Synth", onmidimessage: null }],
    ["i2", { id: "i2", name: "SIDOU FM-1", onmidimessage: null }]]);
  let heard = 0;
  const reply = (list) => { for (const x of list) setTimeout(() => inputs.get("i2").onmidimessage?.({ data: x }), 0); };
  setInterval(() => { if (log.on && performance.now() - heard >= AUTO_STOP_MS) { log.on = false; reply(f.stopped); } }, 100);
  const device = (b) => {
    b = Array.from(b);
    log.fm1.push(b);
    if (b[0] !== 0xf0) { log.notes.push(b); return; }
    const magic = [0x7d, 0x46, 0x4d, 0x31, 0x4c];
    if (!magic.every((x, i) => b[i + 1] === x) || b.at(-1) !== 0xf7 || b.slice(6, -1).reduce((s, x) => s + x, 0) & 0x7f) { log.bad++; return; }
    const cmd = b[6];
    if (cmd === START) { log.on = true; heard = performance.now(); return reply([...f.hello, ...f.keyframe]); }
    if (!log.on) return reply(f.stopped);
    if (cmd === HEARTBEAT) { heard = performance.now(); return reply(f.beat); }
    if (cmd === STOP) { log.on = false; return reply(f.stopped); }
    if (cmd === KEYFRAME) return reply(f.keyframe);
    if (cmd === FONT) return reply(f.font);
    if (cmd === PALETTE) return reply(f.palette);
    if (cmd === PANEL) {
      const d = unpack(b.slice(7, -2));
      log.panels.push({ held: d[0] | (d[1] << 8), pressed: d[2] | (d[3] << 8), steps: d.slice(4).map((s) => (s << 24) >> 24) });
    }
  };
  const outputs = new Map([["o1", { id: "o1", name: "Other Synth", send: (b) => log.other.push(Array.from(b)) }],
    ["o2", { id: "o2", name: "SIDOU FM-1", send: device }]]);
  navigator.requestMIDIAccess = async (opts) => { log.requested = opts; return { inputs, outputs, onstatechange: null }; };
}
