// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { SIDE, loadArtCore, overSky, parseTrace, refusalWords } from "./art-core.js";
import { CHANGED, NO_FM1, clientFor, fillPorts, pairIsTheFm1, refusedWords } from "./ports.js";
import {
  ArtError, FIRST_FRAME, PAGE, PALETTE, SLOTS, decodePack, encodePack, framesFromSheet, nameRefusal,
  sheetFromFrames, sheetRefusal, sheetSize, slotFrames, toPalette, toRgba,
} from "./sidart.js";
import { ART_NAME, Library, STORE_FILE_ART } from "./store.js";

const FRAME_MS = 40;
const SHOWN_WIDTH = 480;
const MOST_ZOOM = 4;
const SKY = SLOTS.find((s) => s.sky);
const SKY_FIRST = FIRST_FRAME[SKY.id - 1];

const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...kids) => {
  const e = Object.assign(document.createElement(tag), props);
  e.append(...kids);
  return e;
};
const say = (node, text, kind = "") => { node.textContent = text; node.dataset.state = kind; };
const bytesText = (n) => `${n.toLocaleString("en")} bytes`;
const fileStem = (name) => name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "art";

const state = {
  core: null, trace: [], skies: [], defaults: [], frames: [], name: "", pack: null, cards: new Map(),
  playing: true, at: 0, access: null, lib: null, port: "", busy: false, saved: [],
};
window.__art = state;

function save(blob, filename) {
  const url = URL.createObjectURL(blob);
  const a = el("a", { href: url, download: filename });
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
  state.saved.push({ filename, blob });
}

const painted = (frames) => frames.map((px, i) => (i >= SKY_FIRST && i < SKY_FIRST + state.skies.length ? overSky(px, state.skies[i - SKY_FIRST]) : px));

function draw(canvas, rgba, w, h) {
  canvas.width = w;
  canvas.height = h;
  canvas.getContext("2d").putImageData(new ImageData(rgba, w, h), 0, 0);
}

function showAt(canvas, slot, w) {
  const screenWide = w * slot.gx;
  const zoom = Math.max(1, Math.min(MOST_ZOOM, Math.floor(SHOWN_WIDTH / screenWide)));
  canvas.style.width = `${screenWide * zoom}px`;
}

function sheetOf(slot) {
  return sheetFromFrames(slot, slotFrames(state.frames, slot));
}

function same(a, b) {
  return a.length === b.length && a.every((v, k) => v === b[k]);
}

const changed = (slot) => slotFrames(state.frames, slot).some((f, k) => !same(f, state.defaults[FIRST_FRAME[slot.id - 1] + k]));

function buildCard(slot) {
  const { w, h } = sheetSize(slot);
  const sheet = el("canvas", { ariaLabel: `${slot.name}: its frames` });
  const status = el("p", { role: "status" });
  const input = el("input", { type: "file", accept: "image/png" });
  const reset = el("button", { type: "button", textContent: "Back to SIDOU's" });
  const template = el("button", { type: "button", textContent: "Template PNG" });
  const yours = el("canvas"), reduced = el("canvas");
  const compare = el("div", { className: "compare", hidden: true },
    el("figure", {}, yours, el("figcaption", { textContent: "Your PNG" })),
    el("figure", {}, reduced, el("figcaption", { textContent: "In the screen's 16 colours" })));
  const scale = slot.gx > 1 ? `, each ${slot.gx} screen pixels wide` : "";
  const card = el("li", { className: "slot" },
    el("div", { className: "head" },
      el("h3", { textContent: `${slot.id}. ${slot.name}` }),
      el("span", { className: "meta", textContent: `${slot.w} × ${slot.h} stored pixels${scale} · ${slot.frames.length} frame${slot.frames.length > 1 ? "s" : ""} · PNG ${w} × ${h}` })),
    el("div", { className: "sheet" }, sheet),
    el("ol", { className: "frames" }, ...slot.frames.map((f) => el("li", { textContent: f }))),
    el("div", { className: "acts" }, template, el("label", { className: "button", textContent: "Replace… " }, input), reset),
    compare, status);
  card.dataset.slot = slot.file;
  template.addEventListener("click", () => downloadTemplate(slot));
  input.addEventListener("change", () => { if (input.files[0]) replace(slot, input.files[0]); input.value = ""; });
  reset.addEventListener("click", () => resetSlot(slot));
  state.cards.set(slot.id, { card, sheet, status, reset, compare, yours, reduced });
  return card;
}

function showSlot(slot) {
  const c = state.cards.get(slot.id);
  const { w, h } = sheetSize(slot);
  draw(c.sheet, toRgba(sheetOf(slot)), w, h);
  showAt(c.sheet, slot, w);
  const differs = changed(slot);
  c.card.dataset.changed = String(differs);
  c.reset.disabled = !differs;
}

function downloadTemplate(slot) {
  const { w, h } = sheetSize(slot);
  const canvas = el("canvas");
  draw(canvas, toRgba(sheetOf(slot)), w, h);
  canvas.toBlob((blob) => save(blob, `${slot.file}.png`), "image/png");
}

async function pixelsOf(file) {
  const bitmap = await createImageBitmap(file, { colorSpaceConversion: "none", premultiplyAlpha: "none" });
  const canvas = el("canvas", { width: bitmap.width, height: bitmap.height });
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  ctx.drawImage(bitmap, 0, 0);
  return { w: bitmap.width, h: bitmap.height, rgba: ctx.getImageData(0, 0, bitmap.width, bitmap.height).data };
}

function buildPack(frames, name) {
  const pack = encodePack(frames, name, { skies: state.skies });
  const why = refusalWords(state.core.check(pack));
  if (why) throw new ArtError(`the FM-1 would refuse it: ${why}`);
  return pack;
}

async function replace(slot, file) {
  const c = state.cards.get(slot.id);
  let image;
  try {
    image = await pixelsOf(file);
  } catch {
    say(c.status, `${file.name} does not read as a PNG.`, "error");
    return;
  }
  const why = sheetRefusal(slot, image.w, image.h);
  if (why) { say(c.status, why, "error"); return; }
  const { px, moved, clear } = toPalette(image.rgba);
  const frames = state.frames.slice();
  frames.splice(FIRST_FRAME[slot.id - 1], slot.frames.length, ...framesFromSheet(slot, px));
  try {
    state.pack = buildPack(frames, state.name);
  } catch (e) {
    if (state.pack) state.core.check(state.pack);
    say(c.status, `${file.name} was not used: ${e.message}.`, "error");
    return;
  }
  state.frames = frames;
  draw(c.yours, image.rgba, image.w, image.h);
  draw(c.reduced, toRgba(px), image.w, image.h);
  for (const canvas of [c.yours, c.reduced]) showAt(canvas, slot, image.w);
  c.compare.hidden = false;
  const notes = [moved ? `${moved.toLocaleString("en")} pixels moved to the nearest of the 16 colours` : "every pixel already one of the 16 colours",
    clear ? `${clear.toLocaleString("en")} transparent` : "none transparent"];
  say(c.status, `${file.name}: ${notes.join(", ")}.`, "ok");
  showSlot(slot);
  packBuilt();
}

function resetSlot(slot) {
  const c = state.cards.get(slot.id);
  const at = FIRST_FRAME[slot.id - 1];
  state.frames.splice(at, slot.frames.length, ...state.defaults.slice(at, at + slot.frames.length));
  c.compare.hidden = true;
  say(c.status, "SIDOU's art again.");
  showSlot(slot);
  rebuild();
}

function rebuild() {
  try {
    state.pack = buildPack(state.frames, state.name);
    packBuilt();
  } catch (e) {
    state.pack = null;
    say($("pack-state"), `Not built: ${e.message}.`, "error");
    setReady();
  }
}

function packBuilt() {
  const pages = Math.ceil(state.pack.length / PAGE);
  const mine = SLOTS.filter(changed).length;
  const what = mine ? `${mine} slot${mine > 1 ? "s" : ""} of yours` : "SIDOU's art";
  say($("pack-state"), `${state.name}: ${what}, ${bytesText(state.pack.length)} in ${pages} pages of 1,248; the FM-1's own checks accept it.`, "ok");
  state.at = 0;
  state.core.start();
  setReady();
}

function setReady() {
  const ready = !!state.pack;
  $("download").disabled = !ready;
  $("play").disabled = !ready;
  $("send").disabled = !ready || !state.lib || state.busy;
}

function setName(text) {
  const name = text.toUpperCase();
  $("name").value = name;
  const why = nameRefusal(name);
  say($("name-why"), why ? `The name: ${why}.` : "", why ? "error" : "");
  if (why) return;
  state.name = name;
  rebuild();
}

function useFrames(frames, name) {
  state.frames = frames;
  for (const slot of SLOTS) {
    const c = state.cards.get(slot.id);
    c.compare.hidden = true;
    say(c.status, "");
    showSlot(slot);
  }
  setName(name);
}

async function openPack(file) {
  try {
    const { name, frames } = decodePack(new Uint8Array(await file.arrayBuffer()));
    useFrames(painted(frames), name);
  } catch (e) {
    say($("pack-state"), `${file.name} was not opened: ${e instanceof ArtError ? e.message : "it does not read"}.`, "error");
  }
}

const LUT = new Uint32Array(PALETTE.map(([r, g, b]) => (0xff000000 | (b << 16) | (g << 8) | r) >>> 0));

function showScreen(px) {
  const rgba = new Uint32Array(SIDE * SIDE);
  for (let k = 0; k < rgba.length; k++) rgba[k] = LUT[px[k]];
  $("screen").getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer), SIDE, SIDE), 0, 0);
}

function tick() {
  if (state.playing && state.pack) {
    if (state.at === state.trace.length) { state.at = 0; state.core.start(); }
    showScreen(state.core.play(state.trace[state.at++]));
    $("frame").textContent = `Frame ${state.at} of ${state.trace.length}`;
  }
}

function setBusy(busy) {
  state.busy = busy;
  $("connect").disabled = busy;
  $("check").disabled = busy || !state.access || !$("out").value || !$("in").value;
  $("progress").hidden = !busy;
  setReady();
}

function forget() {
  state.lib = null;
  setBusy(false);
}

async function connect() {
  setBusy(true);
  try {
    state.access = await navigator.requestMIDIAccess({ sysex: true });
  } catch (e) {
    say($("device"), refusedWords(e), "error");
    setBusy(false);
    return;
  }
  state.access.onstatechange = () => { fillPorts(state.access, $("out"), $("in")); forget(); say($("device"), CHANGED); };
  fillPorts(state.access, $("out"), $("in"));
  setBusy(false);
  if (pairIsTheFm1($("out"), $("in"))) await check();
  else say($("device"), NO_FM1, "error");
}

async function check() {
  const pair = clientFor(state.access, $("out"), $("in"));
  if (!pair) return;
  forget();
  setBusy(true);
  say($("device"), `Asking ${pair.port}…`);
  try {
    const lib = await new Library(pair.client).refresh();
    state.lib = lib;
    state.port = pair.port;
    const v = lib.info.version;
    const art = lib.find(ART_NAME);
    say($("device"), `SIDOU ${v[0]}.${v[1]}.${v[2]} on ${pair.port}: ${art ? `RED MOON RITE's art there (${bytesText(art.length)})` : "no RED MOON RITE art yet"}.`, "ok");
  } catch (e) {
    say($("device"), e.message, "error");
  }
  setBusy(false);
}

async function send() {
  const { lib, pack } = state;
  const status = lib.status(ART_NAME, pack);
  if (status === "same") { say($("device"), `${state.name} is already on the FM-1, identical: left alone.`, "ok"); return; }
  if (status === "replace" && !window.confirm(`The FM-1 has other RED MOON RITE art. Replace it with ${state.name}?`)) return;
  setBusy(true);
  say($("device"), `Sending ${state.name}…`);
  try {
    await lib.send(ART_NAME, pack, {
      type: STORE_FILE_ART, replace: true,
      onProgress: (d, t) => { $("progress").value = t ? d / t : 0; },
      onBusy: (ms) => say($("device"), `The FM-1 is tidying its store, waiting ${Math.round(ms / 1000)} s…`),
      onWait: () => say($("device"), "Waiting for the FM-1 to be quiet: stop the song playing on it (or the recording of its USB audio)…"),
    });
    say($("device"), `${state.name} sent (${bytesText(pack.length)}). On the FM-1: menu, SCENE, RED MOON RITE.`, "ok");
  } catch (e) {
    say($("device"), `Sending failed: ${e.message}.`, "error");
  }
  setBusy(false);
}

async function start() {
  const fetched = async (path) => {
    const r = await fetch(path);
    if (!r.ok) throw new Error(`${path} could not be fetched (HTTP ${r.status})`);
    return r;
  };
  const [wasm, shipped, trace] = await Promise.all([
    fetched("sidou-art.wasm").then((r) => r.arrayBuffer()),
    fetched("art/red-moon-rite.sidart").then((r) => r.arrayBuffer()),
    fetched("art/rmr-pattern.tsv").then((r) => r.text()),
  ]);
  state.core = await loadArtCore(wasm);
  state.skies = state.core.skies();
  state.trace = parseTrace(trace);
  const { name, frames } = decodePack(new Uint8Array(shipped));
  state.defaults = painted(frames);
  $("slots").replaceChildren(...SLOTS.map(buildCard));
  useFrames(state.defaults.slice(), name);
  $("name").addEventListener("input", () => setName($("name").value));
  $("open").addEventListener("change", () => { if ($("open").files[0]) openPack($("open").files[0]); $("open").value = ""; });
  $("restart").addEventListener("click", () => useFrames(state.defaults.slice(), name));
  $("download").addEventListener("click", () => save(new Blob([state.pack], { type: "application/octet-stream" }), `${fileStem(state.name)}.sidart`));
  $("play").addEventListener("click", () => { state.playing = !state.playing; $("play").textContent = state.playing ? "Pause" : "Play"; });
  setInterval(tick, FRAME_MS);
  if (!navigator.requestMIDIAccess) {
    $("support").hidden = false;
    $("connect").disabled = true;
    return;
  }
  $("connect").addEventListener("click", connect);
  $("check").addEventListener("click", check);
  $("send").addEventListener("click", send);
  for (const id of ["out", "in"]) $(id).addEventListener("change", forget);
}

start().catch((e) => say($("pack-state"), `The page could not start: ${e.message}.`, "error"));
