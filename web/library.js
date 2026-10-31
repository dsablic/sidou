// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { CoreError, loadCore } from "./core.js";
import { Library, extOf, storeName, typeLabel, typeOfFile } from "./store.js";
import { Client } from "./sysex.js";

const DEVICE_PORT = "SIDOU FM-1";

const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...kids) => {
  const e = Object.assign(document.createElement(tag), props);
  e.append(...kids);
  return e;
};
const button = (text, onclick, props = {}) => {
  const b = el("button", { type: "button", textContent: text, ...props });
  b.addEventListener("click", onclick);
  return b;
};
const say = (node, text, kind = "") => { node.textContent = text; node.dataset.state = kind; };
const version = (v) => `${v[0]}.${v[1]}.${v[2]}${v[3] ? ` (revision ${v[3]})` : ""}`;
const hex32 = (n) => n.toString(16).toUpperCase().padStart(8, "0");
const kb = (n) => (n < 10240 ? `${n} bytes` : `${(n / 1024).toFixed(1)} KiB`);
const stemOf = (name) => name.replace(/\.[^.]*$/, "").replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "") || "file";

function save(bytes, filename, type = "application/octet-stream") {
  const url = URL.createObjectURL(new Blob([bytes], { type }));
  const a = el("a", { href: url, download: filename });
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
  state.saved.push({ filename, bytes });
}

const state = { access: null, lib: null, busy: false, core: null, song: null, saved: [] };
window.__library = state;

function setBusy(busy) {
  state.busy = busy;
  const ready = !!state.lib && !busy;
  $("connect").disabled = busy;
  $("check").disabled = busy || !state.access || !$("out").value || !$("in").value;
  for (const b of document.querySelectorAll("#store button, #store input, #song .needs-fm1")) b.disabled = !ready;
  $("progress").hidden = !busy;
}

function fillPorts() {
  for (const [id, ports] of [["out", state.access.outputs], ["in", state.access.inputs]]) {
    const select = $(id);
    const previous = select.value;
    const names = [...ports.values()];
    select.replaceChildren(el("option", { value: "", textContent: "choose…" }),
      ...names.map((p) => el("option", { value: p.id, textContent: p.name })));
    const keep = names.find((p) => p.id === previous) ?? names.find((p) => p.name === DEVICE_PORT);
    select.value = keep ? keep.id : "";
    select.disabled = false;
  }
}

function forget() {
  state.lib = null;
  $("store").hidden = true;
  $("files").replaceChildren();
  setBusy(false);
}

async function connect() {
  if (!navigator.requestMIDIAccess) return;
  setBusy(true);
  try {
    state.access = await navigator.requestMIDIAccess({ sysex: true });
  } catch (e) {
    say($("device"), `The browser refused MIDI access with SysEx (${e.message || e.name}). Allow it for this site and press Connect again.`, "error");
    setBusy(false);
    return;
  }
  state.access.onstatechange = () => { fillPorts(); forget(); say($("device"), "A MIDI device changed: pick the ports and press Check this pair.", ""); };
  fillPorts();
  setBusy(false);
  const out = $("out").selectedOptions[0]?.textContent, inn = $("in").selectedOptions[0]?.textContent;
  if (out === DEVICE_PORT && inn === DEVICE_PORT) await check();
  else say($("device"), `No port named "${DEVICE_PORT}" found. Is the FM-1 plugged in and running SIDOU? Pick its output and input and press Check this pair; nothing is sent to other ports until you do.`, "error");
}

async function check() {
  const output = state.access.outputs.get($("out").value);
  const input = state.access.inputs.get($("in").value);
  if (!output || !input) return;
  forget();
  setBusy(true);
  for (const p of state.access.inputs.values()) p.onmidimessage = null;
  const client = new Client((bytes) => output.send(bytes));
  input.onmidimessage = (e) => client.push(e.data);
  say($("device"), `Asking ${output.name}…`);
  try {
    const lib = new Library(client);
    await lib.refresh();
    state.lib = lib;
    state.port = output.name;
    showFiles();
  } catch (e) {
    say($("device"), e.message, "error");
  }
  setBusy(false);
}

function showFiles() {
  const { lib } = state;
  say($("device"), `SIDOU ${version(lib.info.version)} on ${state.port}: ${lib.files.length} file${lib.files.length === 1 ? "" : "s"}, ${kb(lib.info.free)} free.`, "ok");
  const rows = lib.files.map((f) => {
    const acts = el("td", { className: "acts" });
    if (f.type === 1) acts.append(button("Open", () => openFromFm1(f.name)), button("Edit", () => editFromFm1(f.name)));
    acts.append(button("Download", () => download(f)), button("Rename", () => rename(f)), button("Delete", () => remove(f)));
    const tr = el("tr", {}, el("td", { className: "name", textContent: f.name }), el("td", { textContent: typeLabel(f.type) }),
      el("td", { className: "num", textContent: kb(f.length) }), el("td", { className: "crc", textContent: hex32(f.crc) }), acts);
    tr.dataset.name = f.name;
    return tr;
  });
  $("files").replaceChildren(...(rows.length ? rows : [el("tr", {}, el("td", { colSpan: 5, textContent: "The FM-1's store has no files yet." }))]));
  $("store").hidden = false;
}

async function run(what, fn) {
  if (state.busy || !state.lib) return;
  setBusy(true);
  const note = $("store-state");
  say(note, `${what}…`);
  try {
    const done = await fn((d, t) => { $("progress").value = t ? d / t : 0; });
    if (done) say(note, done, "ok");
    else say(note, "");
  } catch (e) {
    say(note, `${what} failed: ${e.message}.`, "error");
    await state.lib.refresh().catch(() => {});
  } finally {
    if (state.lib) showFiles();
    setBusy(false);
  }
}

const waits = (note) => ({
  onBusy: (ms) => say(note, `The FM-1 is tidying its store, waiting ${Math.round(ms / 1000)} s…`),
  onWait: () => say(note, "Waiting for the FM-1 to be quiet: stop the song playing on it (or the recording of its USB audio)…"),
});

function download(f) {
  return run(`Reading ${f.name}`, async (progress) => {
    const bytes = await state.lib.download(f.name, { onProgress: progress });
    save(bytes, `${stemOf(f.name)}${extOf(f.type)}`);
    return `${f.name} downloaded (${kb(bytes.length)}).`;
  });
}

function remove(f) {
  if (!window.confirm(`Delete ${f.name} from the FM-1? This cannot be undone (back it up first if you may want it again).`)) return;
  return run(`Deleting ${f.name}`, async () => {
    await state.lib.remove(f.name, waits($("store-state")));
    return `${f.name} deleted.`;
  });
}

function rename(f) {
  const to = window.prompt(`A new name for ${f.name} (up to 16 characters: A-Z, 0-9, space and most punctuation):`, f.name)?.trim().toUpperCase();
  if (!to || to === f.name) return;
  const replace = !!state.lib.find(to);
  if (replace && !window.confirm(`${to} is already on the FM-1. Replace it with ${f.name}?`)) return;
  return run(`Renaming ${f.name}`, async (progress) => {
    await state.lib.rename(f.name, to, { replace, onProgress: progress, ...waits($("store-state")) });
    return `${f.name} is now ${to}.`;
  });
}

async function sendFiles(list) {
  for (const file of list) {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const type = typeOfFile(file.name, bytes);
    if (!type) { say($("store-state"), `${file.name} is not a song (.sng), a .sid tune or an instrument (.ins): not sent.`, "error"); return; }
    if (type === 1) {
      try {
        (await core()).read(bytes);
      } catch (e) {
        say($("store-state"), `${file.name} was not sent: the FM-1 could not load it (${e.message}).`, "error");
        return;
      }
    }
    const name = window.prompt(`The name for ${file.name} on the FM-1 (up to 16 characters):`, storeName(file.name))?.trim().toUpperCase();
    if (!name) return;
    await sendBytes(name, bytes, type);
  }
}

async function sendBytes(name, bytes, type) {
  const s = state.lib.status(name, bytes);
  if (s === "replace" && !window.confirm(`${name} is already on the FM-1 with different content. Replace it?`)) {
    say($("store-state"), `${name}: not replaced.`);
    return;
  }
  await run(`Sending ${name}`, async (progress) => {
    const r = await state.lib.send(name, bytes, { type, replace: s === "replace", onProgress: progress, ...waits($("store-state")) });
    return r === "same" ? `${name} is already on the FM-1, identical: left alone.` : `${name} sent (${kb(bytes.length)}).${type === 1 ? " On the FM-1: menu, SONG, LOAD." : ""}`;
  });
}

function backup() {
  return run("Backing up", async (progress) => {
    const b = await state.lib.backup({ onProgress: (k, n, name) => { progress(k, n); if (name) say($("store-state"), `Backing up: reading ${name} (${k + 1} of ${n})…`); } });
    save(b.bytes, b.name, "application/zip");
    return `${b.count} file${b.count === 1 ? "" : "s"} backed up in ${b.name}.`;
  });
}

async function restore(file) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  await run(`Restoring ${file.name}`, async (progress) => {
    const done = await state.lib.restore(bytes, {
      replace: async (names) => window.confirm(`${names.join(", ")} ${names.length > 1 ? "are" : "is"} on the FM-1 with different content. Replace ${names.length > 1 ? "them" : "it"} with the backup's? (Cancel keeps the FM-1's.)`),
      onProgress: (k, n, name) => { progress(k, n); if (name) say($("store-state"), `Restoring ${name} (${k + 1} of ${n})…`); },
      ...waits($("store-state")),
    });
    const parts = [`${done.sent.length} sent`];
    if (done.same.length) parts.push(`${done.same.length} already there`);
    if (done.kept.length) parts.push(`${done.kept.length} kept as the FM-1 has ${done.kept.length > 1 ? "them" : "it"}`);
    if (done.skipped.length) parts.push(`left out: ${done.skipped.map((s) => `${s.name} (${s.why})`).join(", ")}`);
    return `Restored: ${parts.join("; ")}.`;
  });
}

async function core() {
  if (!state.core) state.core = await loadCore("sidou-core.wasm");
  return state.core;
}

async function openSong(bytes, source) {
  $("song-error").textContent = "";
  let info;
  try {
    info = (await core()).read(bytes);
  } catch (e) {
    $("song-panel").hidden = true;
    state.song = null;
    $("song-error").textContent = e instanceof CoreError
      ? `The FM-1 would refuse this song: ${e.message}.`
      : `Could not open the song: ${e.message}.`;
    return;
  }
  state.song = { bytes, source, info, model: state.core.song() };
  showSong();
}

function openFromFm1(name) {
  return run(`Reading ${name}`, async (progress) => {
    const bytes = await state.lib.download(name, { onProgress: progress });
    await openSong(bytes, { name, fm1: true });
    $("song").scrollIntoView({ block: "nearest" });
    return "";
  });
}

function editSong(bytes, source) {
  try {
    sessionStorage.setItem("sidou-edit", JSON.stringify({ hex: Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join(""), source }));
  } catch {}
  state.edited = source.name;
  location.href = "editor.html";
}

function editFromFm1(name) {
  return run(`Reading ${name}`, async (progress) => {
    const bytes = await state.lib.download(name, { onProgress: progress });
    editSong(bytes, { name, fm1: true });
    return "";
  });
}

function openCurrent() {
  return run("Reading the current song", async (progress) => {
    const bytes = await state.lib.client.get({ target: 2 }, { onProgress: progress });
    await openSong(bytes, { name: "the current song", fm1: true, current: true });
    return "";
  });
}

const TRAILER_TEXT = {
  none: "a plain GoatTracker song (no SIDOU settings yet)",
  read: "with SIDOU's settings (the chips' setup, effect slots, locks and the rest)",
  dropped: "SIDOU's settings were damaged and are left out",
  "locks-dropped": "its parameter locks no longer matched the song and are left out",
};

function showSong() {
  const { model: s, info, source } = state.song;
  $("s-name").textContent = s.name || source.name;
  const rows = [
    ["From", source.fm1 ? `the FM-1 (${source.name})` : source.name],
    ["Author", s.author || "-"],
    ["Format", `GoatTracker ${s.channels === 6 ? "Stereo, 6 channels" : "2, 3 channels"}; ${TRAILER_TEXT[info.trailer] ?? info.trailer}`],
    ["Contents", `${s.subtunes} subtune${s.subtunes > 1 ? "s" : ""}, ${s.instruments.length} instruments, ${s.patterns.length} patterns`],
    ["Size", `${kb(s.savedBytes)} as the FM-1 saves it`],
  ];
  if (info.message) rows.push(["Note", info.message]);
  $("s-info").replaceChildren(...rows.flatMap(([k, v]) => [el("dt", { textContent: k }), el("dd", { textContent: v })]));
  const stem = stemOf(s.name || source.name);
  $("s-plain-note").textContent = "";
  $("export-sng").onclick = () => save(state.core.write().bytes, `${stem}.sng`);
  $("export-plain").onclick = () => {
    const { bytes, warning } = state.core.write({ trailer: false });
    const lost = info.trailer === "read" ? " SIDOU's settings (the chips' setup, effect slots, locks, MIDI and the rest) are left out of it." : "";
    if (warning) {
      const other = /STEREO/.test(warning) ? "a 6-channel GoatTracker Stereo song" : "a 3-channel song";
      if (!window.confirm(`GoatTracker will read this plain file as ${other}, not as the ${s.channels}-channel song it is: without SIDOU's trailer nothing tells the two apart (${warning}). Download it anyway?`)) return;
      $("s-plain-note").textContent = `Warning: GoatTracker reads this file as ${other}. Keep the .sng with SIDOU's settings for the FM-1.${lost}`;
    } else {
      $("s-plain-note").textContent = `Plain GoatTracker .sng saved.${lost}`;
    }
    save(bytes, `${stem}-goattracker.sng`);
  };
  $("edit-song").onclick = () => editSong(state.song.bytes, source);
  $("send-song").onclick = () => {
    const name = window.prompt("The name for this song on the FM-1 (up to 16 characters):", source.fm1 && !source.current ? source.name : storeName(s.name || source.name))?.trim().toUpperCase();
    if (name) sendBytes(name, state.core.write().bytes, 1);
  };
  const ins = s.instruments.map((i) => {
    const b = button("GTI5", () => {
      try {
        save(state.core.exportInstrument(i.index), `${stem}-${String(i.index).padStart(2, "0")}-${stemOf(i.name || "instrument")}.ins`);
      } catch (e) {
        b.textContent = "cannot export";
        b.disabled = true;
        b.title = e.message;
      }
    }, { title: "Download as a GoatTracker instrument (.ins), its tables with it" });
    return el("tr", {}, el("td", { className: "num", textContent: i.index.toString(16).toUpperCase().padStart(2, "0") }),
      el("td", { className: "name", textContent: i.name || "(no name)" }),
      el("td", { className: "crc", textContent: i.bytes.slice(0, 2).map((x) => x.toString(16).toUpperCase().padStart(2, "0")).join(" ") }),
      el("td", { className: "acts" }, b));
  });
  $("instruments").replaceChildren(...ins);
  $("song-panel").hidden = false;
  setBusy(state.busy);
}

async function pickSong(file) {
  if (!file) return;
  await openSong(new Uint8Array(await file.arrayBuffer()), { name: file.name });
}

$("file").addEventListener("change", (e) => { pickSong(e.target.files[0]); e.target.value = ""; });
const drop = $("drop");
for (const type of ["dragenter", "dragover"]) drop.addEventListener(type, (e) => { e.preventDefault(); drop.classList.add("over"); });
for (const type of ["dragleave", "drop"]) drop.addEventListener(type, (e) => { e.preventDefault(); drop.classList.remove("over"); });
drop.addEventListener("drop", (e) => pickSong(e.dataTransfer.files[0]));

if (!navigator.requestMIDIAccess) {
  $("support").hidden = false;
  $("connect").disabled = true;
  say($("device"), "This browser has no Web MIDI: the FM-1's store needs Chrome or Edge. Songs from files work here.", "error");
} else {
  $("connect").addEventListener("click", connect);
  $("check").addEventListener("click", check);
  $("out").addEventListener("change", forget);
  $("in").addEventListener("change", forget);
  $("refresh").addEventListener("click", () => run("Listing", async () => { await state.lib.refresh(); return ""; }));
  $("backup").addEventListener("click", backup);
  $("current").addEventListener("click", openCurrent);
  $("restore").addEventListener("change", (e) => { const f = e.target.files[0]; e.target.value = ""; if (f) restore(f); });
  $("send").addEventListener("change", (e) => { const fs = [...e.target.files]; e.target.value = ""; sendFiles(fs); });
}
setBusy(false);
