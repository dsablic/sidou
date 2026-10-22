// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The demo's smoke test, in headless Chromium (Playwright): load the page, press Start, wait for
// SIDOU to boot past its title card, check the LCD shows something and nothing went to the
// console as an error, then hold a note key and check the screen changes and the audio thread
// makes sound, and press PLAY (Space, button 12) and check the song sounds and PLAY again
// stops it. Then save, reload and Start again, and check the boot got the saved flash back
// (fm1-emu's persistence.js: the sectors SIDOU wrote, kept in IndexedDB per package). Exits
// non-zero on any failure. Run by web/check.sh (tools/web-demo --check).
//   node check.mjs http://127.0.0.1:8642/ [screenshot.png]
import { chromium } from "playwright";

const url = process.argv[2];
const shot = process.argv[3];
const failures = [];
const fail = (msg) => { failures.push(msg); console.error(`FAIL: ${msg}`); };

// Watches the page's AudioWorkletNode (main.js creates one on Start): every frame message the
// audio thread posts, summarised, and an analyser on its output.
function watch() {
  const w = (window.__sidou = { frames: 0, title: null, last: null, errors: [], analyser: null, bootSectors: null });
  const Base = window.AudioWorkletNode;
  window.AudioWorkletNode = class extends Base {
    constructor(ctx, name, opts) {
      super(ctx, name, opts);
      // the flash sectors main.js hands the audio thread with a boot (packed: 4 + 4096 bytes each)
      const post = this.port.postMessage.bind(this.port);
      this.port.postMessage = (m, ...rest) => {
        if (m?.type === "boot") w.bootSectors = m.sectors ? m.sectors.byteLength / 4100 : 0;
        return post(m, ...rest);
      };
      w.analyser = ctx.createAnalyser();
      w.analyser.fftSize = 2048;
      this.connect(w.analyser);
      this.port.addEventListener("message", (e) => {
        const m = e.data;
        if (m.type === "error") w.errors.push(m.message);
        if (m.type !== "frame") return;
        // fm1-emu sends the picture only when it changed (lcd is null otherwise): keep the last
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
        if (!w.title && m.ms >= 500) w.title = w.last;   // the short boot's title card: 0..1.2 s
      });
    }
  };
  // the output level over `ms` milliseconds of wall time (RMS of the analyser's window)
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
  // past the title card: 3 s of emulated time (the card is 1.2 s, then the status screen wipes in)
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

  // the LCD as drawn on the canvas (fm1-live's panel: the LCD at 2x from (370, 42))
  const canvasLit = await page.evaluate(() => {
    const c = document.getElementById("panel");
    const d = c.getContext("2d").getImageData(370, 42, 480, 480).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] | d[i + 1] | d[i + 2]) n++;
    return n;
  });
  console.log(`canvas LCD: ${canvasLit} of ${480 * 480} pixels not black`);
  if (canvasLit < 20_000) fail("the canvas LCD is nearly black");

  // a note: H is A4. The LIVE view draws the voice; the audio thread plays it.
  const quiet = await page.evaluate(() => window.__sidou.level(400));
  const before = await page.evaluate(() => window.__sidou.last?.hash);
  await page.keyboard.down("h");
  await page.waitForTimeout(300);
  const note = await page.evaluate(() => window.__sidou.level(500));
  const during = await page.evaluate(() => window.__sidou.last?.hash);
  await page.keyboard.up("h");
  console.log(`A4 held: output RMS ${quiet.toFixed(5)} -> ${note.toFixed(5)}; screen ${before === during ? "unchanged" : "changed"}`);
  if (before === during) fail("holding a note key did not change the screen");
  if (!(note > 4 * quiet + 1e-4)) fail("holding a note key made no sound");

  // the song: Space is PLAY (button 12), which plays SIDOU's first song; PLAY again stops it
  await page.waitForTimeout(800); // A4's release
  const still = await page.evaluate(() => window.__sidou.level(300));
  const play = async () => {
    await page.keyboard.down(" ");
    await page.waitForTimeout(150); // past the key scan's debounce
    await page.keyboard.up(" ");
  };
  await play();
  await page.waitForTimeout(600);
  const song = await page.evaluate(() => window.__sidou.level(1000));
  await play();
  await page.waitForTimeout(1500); // the releases ring out
  const after = await page.evaluate(() => window.__sidou.level(400));
  console.log(`PLAY: output RMS ${still.toFixed(5)} -> ${song.toFixed(5)}, stopped ${after.toFixed(5)}`);
  if (!(song > 4 * still + 1e-4)) fail("PLAY made no sound");
  if (!(after < song / 4)) fail("PLAY again did not stop the song");

  if (shot) {
    await page.keyboard.down("h");
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot, fullPage: false });
    await page.keyboard.up("h");
    console.log(`screenshot: ${shot}`);
  }

  // the save: SIDOU stored its settings 5 s after the first boot; Save now keeps the flash in
  // this browser, and the next visit boots with it
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
} finally {
  await browser.close();
}
if (failures.length) {
  console.error(`smoke test FAILED (${failures.length})`);
  process.exit(1);
}
console.log("smoke test passed");
