// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The Library page's work on the FM-1's store, over sysex.js's Client: the listing (DIR) and free
// space (INQ), download (GET), send with an overwrite check (the same name with the same length and
// CRC is left alone; a different one is replaced only when asked), delete (DEL), rename (GET, PUT
// under the new name, DEL the old), and a backup of every file as one .zip with a manifest that
// keeps each file's store name and type, and its restore. No DOM and no Web MIDI in here:
// web/library.test.mjs runs it in Node against a simulated FM-1 (web/fake-fm1.mjs).
// The store's private files (the reserved MIDI SONG and MIDI INSTRUMENT, SAVE AS's staging
// files) are never listed by the FM-1, so nothing here can read, back up or touch them.
import { SysexError, crc32, validName } from "./sysex.js";
import { unzip, zip } from "./zip.js";

export const MANIFEST = "sidou-backup.json";
export const MAX_FILE = 65536;

// The store's file types (sysex.md): 1 a song, 2 a PSID .sid tune, 3 a GTI5 instrument.
export const TYPES = {
  1: { label: "song", ext: ".sng" },
  2: { label: ".sid tune", ext: ".sid" },
  3: { label: "instrument", ext: ".ins" },
};
export const typeLabel = (t) => TYPES[t]?.label ?? `type ${t}`;
export const extOf = (t) => TYPES[t]?.ext ?? ".bin";

export class LibraryError extends Error {
  constructor(message, code = null) {
    super(message);
    this.name = "LibraryError";
    this.code = code;
  }
}

// A file's store type from its content first, then its name: GoatTracker's song idents (GTS2-5) and
// instrument idents (GTI2-5), PSID/RSID; null when it is none of them.
export function typeOfFile(filename, bytes) {
  const magic = String.fromCharCode(...bytes.subarray(0, 4));
  if (/^GTS[2-5]$/.test(magic)) return 1;
  if (magic === "PSID" || magic === "RSID") return 2;
  if (/^GTI[2-5]$/.test(magic)) return 3;
  const ext = /\.([^.]+)$/.exec(filename)?.[1]?.toLowerCase();
  return { sng: 1, sid: 2, ins: 3, gti: 3 }[ext] ?? null;
}

// A store name from a file name, as `sidou-sysex sid` names tunes: the stem uppercased, characters
// outside 20-5F as "?", no leading space, 16 at most.
export function storeName(filename) {
  const stem = filename.replace(/^.*[\\/]/, "").replace(/\.[^.]*$/, "");
  const name = [...stem.toUpperCase()].map((c) => (c >= " " && c <= "_" ? c : "?")).join("").replace(/^ +/, "").slice(0, 16).replace(/ +$/, "");
  return validName(name) ? name : "FILE";
}

// A file name for a store name inside a backup (characters file systems refuse become "_").
const fileName = (name, type, taken) => {
  const stem = name.replace(/[\\/:*?"<>|]/g, "_").replace(/^\.+/, "_");
  let file = `${stem}${extOf(type)}`;
  for (let k = 2; taken.has(file.toLowerCase()); k++) file = `${stem}-${k}${extOf(type)}`;
  taken.add(file.toLowerCase());
  return file;
};

export class Library {
  constructor(client) {
    this.client = client;
    this.info = null;
    this.files = [];
  }

  // INQ (free space, version) and DIR (the files).
  async refresh() {
    this.info = await this.client.inquire();
    if (this.info.protocol !== 1) throw new LibraryError(`it speaks SysEx protocol ${this.info.protocol}; this page speaks 1`);
    this.files = await this.client.dir();
    return this;
  }

  find(name) {
    return this.files.find((f) => f.name === name) ?? null;
  }

  // What sending bytes as name would do: "new", "same" (identical there) or "replace".
  status(name, bytes) {
    const there = this.find(name);
    if (!there) return "new";
    return there.length === bytes.length && there.crc === crc32(bytes) ? "same" : "replace";
  }

  async download(name, { onProgress } = {}) {
    if (!this.find(name)) throw new LibraryError(`the FM-1 has no file named ${name}`, "missing");
    return this.client.get({ target: 0, name }, { onProgress });
  }

  // PUT bytes as name: "same" when an identical file is there (nothing sent), "sent" otherwise. A
  // different file under that name is replaced only with replace: true, else LibraryError "exists".
  async send(name, bytes, { type = 1, replace = false, onProgress, onBusy, onWait } = {}) {
    if (!validName(name)) throw new LibraryError(`"${name}" is not a valid FM-1 file name (1 to 16 characters: A-Z, 0-9, space and ! " # $ % & ' ( ) * + , - . / : ; < = > ? @ [ \\ ] ^ _; no leading space)`, "name");
    if (bytes.length > MAX_FILE) throw new LibraryError(`${name} is ${bytes.length} bytes; the FM-1 holds files up to ${MAX_FILE}`, "size");
    const s = this.status(name, bytes);
    if (s === "same") return "same";
    if (s === "replace" && !replace) throw new LibraryError(`${name} is already on the FM-1 with different content`, "exists");
    await this.client.put(name, bytes, { type, onProgress, onBusy, onWait });
    await this.refresh();
    return "sent";
  }

  async remove(name, opts = {}) {
    await this.client.del(name, opts);
    await this.refresh();
  }

  // GET, PUT under the new name (the same type), then DEL the old one; the old file stays until the
  // new one has committed. A file already called `to` is replaced only with replace: true.
  async rename(from, to, { replace = false, onProgress, onBusy, onWait } = {}) {
    const file = this.find(from);
    if (!file) throw new LibraryError(`the FM-1 has no file named ${from}`, "missing");
    if (from === to) return;
    if (!validName(to)) throw new LibraryError(`"${to}" is not a valid FM-1 file name`, "name");
    if (this.find(to) && !replace) throw new LibraryError(`${to} is already on the FM-1`, "exists");
    const bytes = await this.client.get({ target: 0, name: from }, { onProgress });
    await this.client.put(to, bytes, { type: file.type, onBusy, onWait });
    await this.client.del(from, { onBusy, onWait });
    await this.refresh();
  }

  // Every listed file, read and zipped with a manifest: { bytes, count, name }.
  async backup({ onProgress = () => {}, date = new Date() } = {}) {
    const taken = new Set([MANIFEST]);
    const entries = [], manifest = [];
    for (const [k, f] of this.files.entries()) {
      onProgress(k, this.files.length, f.name);
      const bytes = await this.client.get({ target: 0, name: f.name });
      const file = fileName(f.name, f.type, taken);
      entries.push({ name: file, bytes });
      manifest.push({ name: f.name, type: f.type, file, length: bytes.length, crc: crc32(bytes) });
    }
    onProgress(this.files.length, this.files.length, "");
    const json = JSON.stringify({ "sidou-backup": 1, made: date.toISOString(), files: manifest }, null, 1);
    entries.unshift({ name: MANIFEST, bytes: new TextEncoder().encode(json) });
    const stamp = date.toISOString().slice(0, 10);
    return { bytes: zip(entries, { date }), count: manifest.length, name: `fm1-backup-${stamp}.zip` };
  }

  // What a backup .zip holds, as the files restore() would send: [{ name, type, bytes }], and the
  // entries it leaves out with the reason. With SIDOU's manifest the names and types are the
  // FM-1's; a .zip without one (made by hand) names each .sng, .sid or .ins by its file name.
  static async readBackup(zipBytes) {
    const entries = await unzip(zipBytes);
    const byName = new Map(entries.map((e) => [e.name, e.bytes]));
    const files = [], skipped = [];
    const m = byName.get(MANIFEST);
    if (m) {
      let manifest;
      try { manifest = JSON.parse(new TextDecoder().decode(m)); } catch { throw new LibraryError("the backup's manifest is damaged", "manifest"); }
      for (const f of manifest.files ?? []) {
        const bytes = byName.get(f.file);
        if (!bytes) skipped.push({ name: f.name, why: "missing from the .zip" });
        else if (bytes.length !== f.length || crc32(bytes) !== f.crc) skipped.push({ name: f.name, why: "damaged (its checksum does not match)" });
        else if (!validName(f.name) || !(f.type >= 1 && f.type <= 127)) skipped.push({ name: f.name, why: "not a valid FM-1 file name or type" });
        else files.push({ name: f.name, type: f.type, bytes });
      }
    } else {
      for (const e of entries) {
        const type = typeOfFile(e.name, e.bytes);
        if (type) files.push({ name: storeName(e.name), type, bytes: e.bytes });
        else skipped.push({ name: e.name, why: "not a song, .sid tune or instrument" });
      }
    }
    for (const f of files.filter((x) => x.bytes.length > MAX_FILE)) skipped.push({ name: f.name, why: "over 64 KiB" });
    return { files: files.filter((x) => x.bytes.length <= MAX_FILE), skipped };
  }

  // Sends a backup's files: identical ones are left alone; replace(names) is asked once about the
  // ones the FM-1 holds with different content (true replaces them all, false keeps the FM-1's).
  // Stops at the first refusal (FULL, I/O), with what was done so far in the error's `done`.
  async restore(zipBytes, { replace = async () => false, onProgress = () => {}, onBusy, onWait } = {}) {
    const { files, skipped } = await Library.readBackup(zipBytes);
    const conflicts = files.filter((f) => this.status(f.name, f.bytes) === "replace").map((f) => f.name);
    const ok = conflicts.length ? await replace(conflicts) : false;
    const done = { sent: [], same: [], kept: [], skipped };
    for (const [k, f] of files.entries()) {
      onProgress(k, files.length, f.name);
      const s = this.status(f.name, f.bytes);
      if (s === "same") { done.same.push(f.name); continue; }
      if (s === "replace" && !ok) { done.kept.push(f.name); continue; }
      try {
        await this.client.put(f.name, f.bytes, { type: f.type, onBusy, onWait });
      } catch (e) {
        if (e instanceof SysexError) e.done = done;
        await this.refresh().catch(() => {});
        throw e;
      }
      done.sent.push(f.name);
      this.files = this.files.filter((x) => x.name !== f.name).concat({ name: f.name, type: f.type, length: f.bytes.length, crc: crc32(f.bytes) });
    }
    onProgress(files.length, files.length, "");
    await this.refresh();
    return done;
  }
}
