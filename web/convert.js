// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { loadCore } from "./core.js";
import { VOICES_A_CHIP, convert, measure, previewSong, writesOf } from "./sidconv.js";
import { tuneSounds } from "./sidsound.js";
import { loadTune } from "./tune.js";

const LENGTHS = [30, 60, 90, 120], DEFAULT_LENGTH = 60, FRAME_RATE = 50, MODEL_8580 = 2;
const percent = (x) => `${Math.round(x * 100)} %`;

export function setupConvert({ $, el, say, save, send, edit, stemOf, kb, patcher, sounds }) {
  const state = { tune: null, core: null, file: null, result: null, ctx: null, source: null };
  window.__convert = state;

  const tools = async () => {
    state.tune ??= await loadTune("sidou-tune.wasm");
    state.core ??= await loadCore("sidou-core.wasm");
    return state;
  };

  const info = (node, rows) => node.replaceChildren(...rows.flatMap(([k, v]) => [el("dt", { textContent: k }), el("dd", { textContent: v })]));

  async function open(file) {
    if (!file) return;
    stop();
    $("sid-result").hidden = true;
    state.result = null;
    sounds.showTune([]);
    try {
      const { tune } = await tools();
      const h = tune.read(new Uint8Array(await file.arrayBuffer()));
      state.file = { name: file.name, header: h };
      $("sid-name").textContent = h.name || file.name;
      info($("sid-info"), [
        ["Author", h.author || "-"], ["Released", h.released || "-"],
        ["Chips", h.chips === 2 ? "two (six voices)" : "one (three voices)"],
        ["Clock and chip", `${h.clockName}, ${h.model}`],
        ["Sub-tunes", `${h.songs} (the tune starts with ${h.start})`],
      ]);
      $("sid-song").replaceChildren(...Array.from({ length: h.songs }, (_, k) => el("option", { value: k + 1, textContent: `${k + 1}`, selected: k + 1 === h.start })));
      $("sid-panel").hidden = false;
      say($("sid-state"), "Choose the sub-tune and the length, then Convert.");
    } catch (e) {
      $("sid-panel").hidden = true;
      say($("sid-state"), `${file.name}: ${e.message}`, "error");
    }
  }

  async function run() {
    stop();
    const { tune, core } = await tools();
    const song = Number($("sid-song").value), seconds = Number($("sid-seconds").value);
    say($("sid-state"), `Running sub-tune ${song} for ${seconds} s in SIDOU's 6502…`);
    await new Promise((r) => setTimeout(r));
    try {
      const h = state.file.header;
      const captured = tune.capture({ song, seconds });
      const c = convert(captured, { freqTable: core.freqTable(), name: h.name, author: h.author, released: h.released });
      const read = core.read(c.bytes);
      if (read.channels !== c.song.channels) throw new Error(`the song reads back with ${read.channels} channels`);
      const bytes = core.write().bytes;
      const chipA = c.song.channels > VOICES_A_CHIP ? previewSong(c.song) : c.bytes;
      core.read(chipA);
      const close = measure(captured, core.frames(seconds * FRAME_RATE), core.freqTable());
      state.result = { captured, c, bytes, chipA, close, seconds, song, stem: `${stemOf(h.name || state.file.name)}${h.songs > 1 ? `-${song}` : ""}` };
      show();
      say($("sid-state"), `Converted: ${kb(bytes.length)}.`, "ok");
      await listSounds(c.song);
    } catch (e) {
      $("sid-result").hidden = true;
      sounds.showTune([]);
      say($("sid-state"), `Not converted: ${e.message}.`, "error");
    }
  }

  async function listSounds(song) {
    try {
      sounds.showTune(tuneSounds(song, await patcher()));
    } catch (e) {
      sounds.status(`The tune's sounds could not be listed: ${e.message}.`, "error");
    }
  }

  function show() {
    const { c, close, bytes } = state.result;
    const s = c.song;
    info($("sid-summary"), [
      ["Tempo", s.swing ? `swing, funktempo ${s.swing.join("/")} frames a row` : `${s.tempo} frames a row${s.changes > 1 ? `, ${s.changes - 1} tempo changes` : ""}`],
      ["Voices", s.channels === 6 ? "six (GoatTracker Stereo)" : "three"],
      ["Contents", `${s.instruments.length} instruments, ${s.patterns.length} patterns, ${kb(bytes.length)}`],
      ...close.map((v, k) => [`Voice ${k + 1}`, v.original ? `${percent(v.score)} of ${v.original} notes on time in the FM-1's player` : "silent"]),
    ]);
    $("sid-notes").replaceChildren(...c.notes.map((t) => el("li", { textContent: t })));
    $("sid-result").hidden = false;
  }

  async function play(which) {
    stop();
    const { core } = await tools();
    const r = state.result;
    let pcm;
    if (which === "original") {
      const h = state.file.header;
      pcm = core.renderWrites({ writes: writesOf(r.captured), count: r.captured.chips },
        { model8580: [h.model1 === MODEL_8580, h.model2 === MODEL_8580], ntsc: r.captured.ntsc, clock: r.captured.clock, seconds: r.seconds });
    } else {
      core.read(r.chipA);
      pcm = core.renderSong({ seconds: r.seconds });
    }
    const Ctx = globalThis.AudioContext ?? globalThis.webkitAudioContext;
    state.ctx ??= new Ctx();
    await state.ctx.resume();
    const buf = state.ctx.createBuffer(1, pcm.length, core.sampleRate);
    const out = buf.getChannelData(0);
    for (let i = 0; i < pcm.length; i++) out[i] = pcm[i] / 32768;
    state.source = state.ctx.createBufferSource();
    state.source.buffer = buf;
    state.source.connect(state.ctx.destination);
    state.source.onended = () => { state.playing = null; say($("sid-play-state"), ""); };
    state.source.start();
    state.playing = which;
    say($("sid-play-state"), which === "original" ? "Playing the original." : "Playing the conversion.");
  }

  function stop() {
    if (!state.source) return;
    state.source.onended = null;
    try { state.source.stop(); } catch {}
    state.source = null;
    state.playing = null;
    say($("sid-play-state"), "");
  }

  $("sid-seconds").replaceChildren(...LENGTHS.map((s) => el("option", { value: s, textContent: `${s} s`, selected: s === DEFAULT_LENGTH })));
  $("sid-file").addEventListener("change", (e) => { const f = e.target.files[0]; e.target.value = ""; open(f); });
  const drop = $("sid-drop");
  for (const type of ["dragenter", "dragover"]) drop.addEventListener(type, (e) => { e.preventDefault(); drop.classList.add("over"); });
  for (const type of ["dragleave", "drop"]) drop.addEventListener(type, (e) => { e.preventDefault(); drop.classList.remove("over"); });
  drop.addEventListener("drop", (e) => open(e.dataTransfer.files[0]));
  $("sid-convert").addEventListener("click", run);
  $("sid-play-original").addEventListener("click", () => play("original"));
  $("sid-play-conversion").addEventListener("click", () => play("conversion"));
  $("sid-stop").addEventListener("click", stop);
  $("sid-download").addEventListener("click", () => save(state.result.bytes, `${state.result.stem}.sng`));
  $("sid-send").addEventListener("click", () => send(state.result.bytes, state.result.c.song.name || state.result.stem));
  $("sid-edit").addEventListener("click", () => edit(state.result.bytes, { name: `${state.result.stem}.sng` }));
}
