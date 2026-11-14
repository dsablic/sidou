// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
export const LINK_KEY = "#patch=";
export const BANK_KEY = "#bank=";
export const STEP_KEY = "#step=";
export const SONG_KEY = "#song=";
const STEP_TEXT = 2;
const SONG_TEXT = 3;
export const LINK_MAX = 8192;

const QUIET = 1;
const USER_SLOTS = 16;

const cstr = (mem, ptr) => {
  const b = new Uint8Array(mem.buffer, ptr);
  const end = b.indexOf(0);
  return new TextDecoder().decode(b.subarray(0, end < 0 ? 0 : end));
};

import { loadCore } from "./core.js";

const LINK_KEYS = { "sidou-bank/1": BANK_KEY, "sidou-step/1": STEP_KEY, "sidou-song/1": SONG_KEY };
const LEVEL = { patch: 0, bank: 1, step: 2 };

export async function loadPatcher(source, coreSource) {
  const bytes = source instanceof ArrayBuffer || ArrayBuffer.isView(source) ? source : await (await fetch(source)).arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return new Patcher(instance.exports, await loadCore(coreSource));
}

export class Patcher {
  constructor(x, core) {
    this.x = x;
    this.core = core;
  }

  heard(sng, first, count, style) {
    const c = this.core;
    c.read(sng);
    const r = c.x.sc_level(first, count, style);
    const message = cstr(c.mem, c.x.sc_out());
    return { r, message };
  }

  write(sound, name) {
    const x = this.x;
    const at = new Uint8Array(x.memory.buffer, x.sp_in(), sound.length + 12);
    at.fill(0);
    at.set(sound);
    at.set(new TextEncoder().encode(name).subarray(0, 12), sound.length);
    const n = x.sp_write();
    return new TextDecoder().decode(new Uint8Array(x.memory.buffer, x.sp_out(), n));
  }

  userSounds(file) {
    const x = this.x;
    const bytes = file.subarray(0, Math.min(file.length, x.sp_in_max()));
    new Uint8Array(x.memory.buffer, x.sp_in(), bytes.length).set(bytes);
    const used = x.sp_user_open(bytes.length);
    return Array.from({ length: USER_SLOTS }, (_, slot) => {
      if (!((used >>> slot) & 1)) return null;
      const n = x.sp_user_patch(slot);
      return { name: cstr(x.memory, x.sp_user_name()), text: new TextDecoder().decode(new Uint8Array(x.memory.buffer, x.sp_out(), n)) };
    });
  }

  check(text) {
    const x = this.x;
    const bytes = new TextEncoder().encode(text);
    if (bytes.length > x.sp_in_max()) return { ok: false, error: `the text is too long (${bytes.length} bytes): a patch is a few KB` };
    new Uint8Array(x.memory.buffer, x.sp_in(), bytes.length).set(bytes);
    const r = x.sp_patch(bytes.length);
    const message = cstr(x.memory, x.sp_error());
    if (r < 0) return { ok: false, error: message || `the patch was refused (${r})` };
    const count = x.sp_bank();
    const sng = new Uint8Array(x.memory.buffer, x.sp_out(), x.sp_song_bytes()).slice();
    const song = x.sp_kind() === SONG_TEXT;
    const step = x.sp_kind() === STEP_TEXT, instr = x.sp_step_instr();
    const level = song ? { r: 0, message: "" } : step ? (instr ? this.heard(sng, instr, 1, LEVEL.step) : { r: 0, message: "" })
      : this.heard(sng, 1, count || 1, count ? LEVEL.bank : LEVEL.patch);
    if (level.r < 0) return { ok: false, error: level.message };
    return {
      ok: true,
      quiet: level.r === QUIET ? level.message : "",
      name: cstr(x.memory, x.sp_name()),
      sounds: count ? Array.from({ length: count }, (_, k) => cstr(x.memory, x.sp_bank_name(k))) : [cstr(x.memory, x.sp_name())],
      bank: count > 0,
      step,
      song,
      bpm10: x.sp_song_bpm10(),
      sng,
    };
  }
}

export function patchOf(text) {
  const start = text.indexOf("{"), end = text.lastIndexOf("}");
  if (start < 0 || end < start) return null;
  try {
    return JSON.stringify(JSON.parse(text.slice(start, end + 1)));
  } catch {
    return null;
  }
}

export function linkOf(page, text) {
  const compact = patchOf(text);
  if (compact === null) return null;
  const format = JSON.parse(compact).format;
  const key = Object.hasOwn(LINK_KEYS, format) ? LINK_KEYS[format] : LINK_KEY;
  return `${page}${key}${encodeURIComponent(compact)}`;
}

export function shareableLink(page, text) {
  const link = linkOf(page, text);
  if (link === null) return null;
  const hash = link.slice(link.indexOf("#"));
  const length = hash.length - hash.indexOf("=") - 1;
  if (length > LINK_MAX) {
    return { error: `This is too long for a link (${length} characters; a link is under ${LINK_MAX / 1024} KB). Copy the JSON itself and paste it where you share it instead.` };
  }
  return { link };
}

export function fromLink(hash) {
  const key = [LINK_KEY, BANK_KEY, STEP_KEY, SONG_KEY].find((k) => hash.startsWith(k));
  if (!key) return null;
  const raw = hash.slice(key.length);
  if (raw.length > LINK_MAX) {
    return { error: `This link is too long (${raw.length} characters; a link is under ${LINK_MAX / 1024} KB). Paste the JSON in the box instead.` };
  }
  try {
    return { text: decodeURIComponent(raw) };
  } catch {
    return { error: "This patch link is damaged: a %-code in it is broken. Paste the patch's JSON in the box instead, or ask for the link again." };
  }
}
