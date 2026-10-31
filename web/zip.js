// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { crc32 } from "./sysex.js";

export class ZipError extends Error {
  constructor(message) {
    super(message);
    this.name = "ZipError";
  }
}

const LOCAL_HEADER = 0x04034b50, CENTRAL_HEADER = 0x02014b50, END_OF_DIRECTORY = 0x06054b50;
const VERSION_NEEDED_1_0 = 10, MADE_BY_UNIX_2_0 = 0x0314;
const FLAG_ENCRYPTED = 1, FLAG_UTF8_NAMES = 0x0800;
const METHOD_STORED = 0, METHOD_DEFLATED = 8;
const PLAIN_FILE_RW_R_R = 0o100644 << 16;

const enc = new TextEncoder();
const dec = new TextDecoder();

function dosTime(d) {
  const time = (d.getHours() << 11) | (d.getMinutes() << 5) | (d.getSeconds() >> 1);
  const date = ((Math.max(1980, d.getFullYear()) - 1980) << 9) | ((d.getMonth() + 1) << 5) | d.getDate();
  return [time, date];
}

export function zip(files, { date = new Date() } = {}) {
  const [time, day] = dosTime(date);
  const parts = [], central = [];
  let offset = 0;
  for (const f of files) {
    const name = enc.encode(f.name);
    const crc = crc32(f.bytes);
    const head = new DataView(new ArrayBuffer(30));
    head.setUint32(0, LOCAL_HEADER, true);
    head.setUint16(4, VERSION_NEEDED_1_0, true);
    head.setUint16(6, FLAG_UTF8_NAMES, true);
    head.setUint16(8, METHOD_STORED, true);
    head.setUint16(10, time, true);
    head.setUint16(12, day, true);
    head.setUint32(14, crc, true);
    head.setUint32(18, f.bytes.length, true);
    head.setUint32(22, f.bytes.length, true);
    head.setUint16(26, name.length, true);
    parts.push(new Uint8Array(head.buffer), name, f.bytes);
    const c = new DataView(new ArrayBuffer(46));
    c.setUint32(0, CENTRAL_HEADER, true);
    c.setUint16(4, MADE_BY_UNIX_2_0, true);
    c.setUint16(6, VERSION_NEEDED_1_0, true);
    c.setUint16(8, FLAG_UTF8_NAMES, true);
    c.setUint16(12, time, true);
    c.setUint16(14, day, true);
    c.setUint32(16, crc, true);
    c.setUint32(20, f.bytes.length, true);
    c.setUint32(24, f.bytes.length, true);
    c.setUint16(28, name.length, true);
    c.setUint32(38, PLAIN_FILE_RW_R_R, true);
    c.setUint32(42, offset, true);
    central.push(new Uint8Array(c.buffer), name);
    offset += 30 + name.length + f.bytes.length;
  }
  const size = central.reduce((s, p) => s + p.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, END_OF_DIRECTORY, true);
  end.setUint16(8, files.length, true);
  end.setUint16(10, files.length, true);
  end.setUint32(12, size, true);
  end.setUint32(16, offset, true);
  const all = [...parts, ...central, new Uint8Array(end.buffer)];
  const out = new Uint8Array(all.reduce((s, p) => s + p.length, 0));
  let at = 0;
  for (const p of all) { out.set(p, at); at += p.length; }
  return out;
}

async function inflate(bytes) {
  if (typeof DecompressionStream === "undefined") throw new ZipError("this browser cannot unpack compressed .zip entries");
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

export async function unzip(bytes) {
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let eocd = -1;
  for (let i = bytes.length - 22; i >= Math.max(0, bytes.length - 22 - 65535); i--) {
    if (v.getUint32(i, true) === END_OF_DIRECTORY) { eocd = i; break; }
  }
  if (eocd < 0) throw new ZipError("not a .zip file");
  const count = v.getUint16(eocd + 10, true);
  let at = v.getUint32(eocd + 16, true);
  const files = [];
  for (let k = 0; k < count; k++) {
    if (at + 46 > bytes.length || v.getUint32(at, true) !== CENTRAL_HEADER) throw new ZipError("a damaged .zip file (its directory)");
    const flags = v.getUint16(at + 8, true), method = v.getUint16(at + 10, true);
    const crc = v.getUint32(at + 16, true), csize = v.getUint32(at + 20, true), usize = v.getUint32(at + 24, true);
    const nlen = v.getUint16(at + 28, true), xlen = v.getUint16(at + 30, true), clen = v.getUint16(at + 32, true);
    const local = v.getUint32(at + 42, true);
    const name = dec.decode(bytes.subarray(at + 46, at + 46 + nlen));
    at += 46 + nlen + xlen + clen;
    if (flags & FLAG_ENCRYPTED) throw new ZipError(`${name} is encrypted`);
    if (name.endsWith("/")) continue;
    if (local + 30 > bytes.length || v.getUint32(local, true) !== LOCAL_HEADER) throw new ZipError(`a damaged .zip file (${name})`);
    const start = local + 30 + v.getUint16(local + 26, true) + v.getUint16(local + 28, true);
    const data = bytes.subarray(start, start + csize);
    if (data.length !== csize) throw new ZipError(`a damaged .zip file (${name} is cut short)`);
    let out;
    if (method === METHOD_STORED) out = data.slice();
    else if (method === METHOD_DEFLATED) out = await inflate(data);
    else throw new ZipError(`${name} is packed in a way this page cannot unpack (method ${method})`);
    if (out.length !== usize || crc32(out) !== crc) throw new ZipError(`${name} in the .zip is damaged (its checksum does not match)`);
    files.push({ name, bytes: out });
  }
  return files;
}
