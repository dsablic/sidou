// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
export const LINK_KEY = "#patch=";
export const BANK_KEY = "#bank=";
export const LINK_MAX = 8192;

const QUIET = 1;

const cstr = (mem, ptr) => {
  const b = new Uint8Array(mem.buffer, ptr);
  const end = b.indexOf(0);
  return new TextDecoder().decode(b.subarray(0, end < 0 ? 0 : end));
};

export async function loadPatcher(source) {
  const bytes = source instanceof ArrayBuffer || ArrayBuffer.isView(source) ? source : await (await fetch(source)).arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return new Patcher(instance.exports);
}

export class Patcher {
  constructor(x) {
    this.x = x;
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

  check(text) {
    const x = this.x;
    const bytes = new TextEncoder().encode(text);
    if (bytes.length > x.sp_in_max()) return { ok: false, error: `the text is too long (${bytes.length} bytes): a patch is a few KB` };
    new Uint8Array(x.memory.buffer, x.sp_in(), bytes.length).set(bytes);
    const r = x.sp_patch(bytes.length);
    const message = cstr(x.memory, x.sp_error());
    if (r < 0) return { ok: false, error: message || `the patch was refused (${r})` };
    const count = x.sp_bank();
    return {
      ok: true,
      quiet: r === QUIET ? message : "",
      name: cstr(x.memory, x.sp_name()),
      sounds: count ? Array.from({ length: count }, (_, k) => cstr(x.memory, x.sp_bank_name(k))) : [cstr(x.memory, x.sp_name())],
      bank: count > 0,
      sng: new Uint8Array(x.memory.buffer, x.sp_out(), x.sp_song_bytes()).slice(),
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
  const key = JSON.parse(compact).format === "sidou-bank/1" ? BANK_KEY : LINK_KEY;
  return `${page}${key}${encodeURIComponent(compact)}`;
}

export function fromLink(hash) {
  const key = [LINK_KEY, BANK_KEY].find((k) => hash.startsWith(k));
  if (!key) return null;
  const raw = hash.slice(key.length);
  if (raw.length > LINK_MAX) {
    return { error: `This patch link is too long (${raw.length} characters; a patch link is under ${LINK_MAX / 1024} KB). Paste the patch's JSON in the box instead.` };
  }
  try {
    return { text: decodeURIComponent(raw) };
  } catch {
    return { error: "This patch link is damaged: a %-code in it is broken. Paste the patch's JSON in the box instead, or ask for the link again." };
  }
}
