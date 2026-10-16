// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//
// RED MOON RITE, SIDOU's first SCENE vignette: the source of its art (docs/art/README.md
// "RED MOON RITE"). Original art made for SIDOU, drawn in code in the 16 C64 colours (Pepto's
// palette, docs/art/README.md "Palette"): two barbarians with broadswords under a blood moon
// in a torch-lit ruin, a homage to late-80s fantasy sword-fighting games, not to any one game.
//
// The drawing code is the Scene Arcade's (assets/prototypes/sidou-vignettes.html), copied
// verbatim: the shared helpers (sprite, blit, rect, px, disc, hash, the 3 x 5 font and text,
// T, the 4 x 4 Bayer table, bayW, gradient, put), the painter, the puppet, the music pattern
// (patternStep, cutoffAt) and RED MOON RITE itself (id 'rite': barbSpec, DUEL, PALS, the moon,
// wisps and hills, the arena, the flames, the fighters' logic).
//
// Changed from the prototype, so that it runs under Node without a canvas and exports its art:
// - the page's IIFE, DOM, audio, cost model, its sprite sheet entries (DEV.push) and the other
//   vignettes are left out; the helpers it does not use (ring, rot90, stamp, layer, ...) are
//   kept where they sit between used ones;
// - barbSpec draws no broadsword when the pose asks for `unarmed` (the sword is skipped, the
//   rest of the figure is untouched), and barbSpr's third argument is `unarmed`, not `bare`:
//   barbarian 1 keeps his helmet in every pose. The prototype's helmet knock (HELM, `bare`,
//   `st.helm`) is removed, and the fill's caption in `maps` describes the disarm;
// - make() returns its state (`st`) and its art (`art`: the skies, the arena, the flame
//   mask, the torches) beside step, layers and stages, for the exporter and the preview.
//
// New drawing, SIDOU's own, in the painter's style:
// - the disarm (the bar-4 fill's finish): the spinning attack sends the other fighter's
//   broadsword spinning away; it sticks point-down in a flagstone, and he pulls it free in
//   bar 1. SWORDS (the sword in flight, eight rotations, each drawn by the painter: no
//   rotated pixels), STUCK (the sword standing in a flagstone, a crack round its point), the
//   `reach` pose (bending to pull the sword from the floor), both barbarians' unarmed poses,
//   and the logic in step and the sprites stage (marked "disarm");
// - the status bar's stamina meter: a broadsword blade a side (bladeMeter), replacing the
//   prototype's eight pips a side, which sat too close to the genre's best-known game's
//   (docs/art/README.md "RED MOON RITE", the originality review).
//
//   node assets/scene/red-moon-rite.js [out-dir] [--review review-dir] [--frames golden-dir]
//
// writes the indexed PNGs (4 bits a pixel and the 16 colours in the art palette's order when
// opaque, else 8 bits with a 17th, transparent entry, index 16, as metal-chrome.js writes
// them) into out-dir, assets/scene/ by default:
// - sky-0.png, sky-1.png, sky-2.png: the three sky palettes with the moon, wisps and hills,
//   240 x 124, the screen's rows 16-139 (the sky band: 16-135; rows 136-139 sit under the
//   arena's steps);
// - arena.png: the temple and floor, 240 x 240 in screen coordinates, transparent where the
//   sky shows;
// - barb-0-<pose>.png (bare-headed, dark hair, red headband) and barb-1-<pose>.png (steel
//   helmet, red plume) for the eight poses of DUEL, and barb-<n>-<pose>-unarmed.png for guard,
//   guard2, hit and reach: 64 x 124, stored at half width (gx = 2: each stored pixel is two
//   on the screen). Barbarian 0 faces right; barbarian 1 is drawn mirrored. The hip is at
//   (hx, hy) = (58, 72) in screen pixels of the unmirrored sprite; the sprite's top-left goes
//   to (x - 58, FEET - 112) for barbarian 0 and (x - 70, FEET - 112) for barbarian 1, x the
//   fighter's place and FEET = 228;
// - sword-0.png .. sword-7.png: the sword in flight, rotation k pointing k x 45 degrees from
//   straight down toward the right, 42 x 84 stored (84 x 84 on screen), the grip at the
//   centre (screen pixel (42, 42) of the sprite);
// - sword-stuck.png: the sword in the floor, 16 x 44 stored, the grip at screen pixel
//   (STUCK_GX, STUCK_GY) of the sprite;
// - flame-mask.png: the torches' flame area (white), 240 x 240, transparent elsewhere;
// - red-moon-rite-preview.png: a montage of six frames of the vignette playing the arcade's
//   pattern, the disarm among them, for the docs.
// With --review, it also writes the review's sheets into review-dir (not committed); with
// --frames golden-dir, the compositor's golden frames (goldenFrames, below).
// The output is the same on every run (no randomness but the seeded hash).

'use strict'

const W = 240, H = 240, BUDGET = 225, FULL_MS = 77, FRAME_MS = 40, WIN = 250
const PAL = [0x000000, 0xFFFFFF, 0x68372B, 0x70A4B2, 0x6F3D86, 0x588D43, 0x352879, 0xB8C76F, 0x6F4F25, 0x433900, 0x9A6759, 0x444444, 0x6C6C6C, 0x9AD284, 0x6C5EB5, 0x959595]
const PAL32 = new Uint32Array(PAL.map(c => (0xFF000000 | ((c & 0xFF) << 16) | (c & 0xFF00) | (c >> 16)) >>> 0))
const KEY = { k: 0, w: 1, r: 2, c: 3, p: 4, g: 5, b: 6, y: 7, o: 8, n: 9, R: 10, d: 11, m: 12, G: 13, B: 14, l: 15 }
const BLACK = 0, WHITE = 1, RED = 2, CYAN = 3, PURPLE = 4, GREEN = 5, BLUE = 6, YELLOW = 7, ORANGE = 8, BROWN = 9, LRED = 10, DGREY = 11, GREY = 12, LGREEN = 13, LBLUE = 14, LGREY = 15

// ---------- drawing primitives on an indexed 240x240 buffer ----------
function sprite (rows, map) {
  const m = Object.assign({}, KEY, map || {})
  const h = rows.length
  const w = Math.max(...rows.map(r => r.length))
  const d = new Int8Array(w * h).fill(-1)
  rows.forEach((r, y) => {
    for (let x = 0; x < r.length; x++) {
      const c = r[x]
      if (c === '.') continue
      const v = m[c]
      if (v === undefined) throw new Error('unknown key ' + c)
      d[y * w + x] = v
    }
  })
  return { w, h, d }
}
function rot90 (s) {
  const d = new Int8Array(s.w * s.h).fill(-1)
  for (let y = 0; y < s.h; y++) for (let x = 0; x < s.w; x++) d[x * s.h + (s.h - 1 - y)] = s.d[y * s.w + x]
  return { w: s.h, h: s.w, d }
}
function blit (p, s, x, y, o) {
  o = o || {}
  const sc = o.sc || 2
  const fx = !!o.fx
  const sy = o.sy === undefined ? 1 : o.sy
  const rc = o.rc
  const sh = o.shadow
  x = Math.round(x); y = Math.round(y)
  const dh = Math.max(1, Math.round(s.h * sc * Math.abs(sy)))
  for (let dy = 0; dy < dh; dy++) {
    const Y = y + dy
    if (Y < 0 || Y >= H) continue
    let sr = Math.min(s.h - 1, Math.floor(dy * s.h / dh))
    if (sy < 0) sr = s.h - 1 - sr
    for (let sx = 0; sx < s.w; sx++) {
      let v = s.d[sr * s.w + (fx ? s.w - 1 - sx : sx)]
      if (v < 0) continue
      if (rc && rc[v] !== undefined) v = rc[v]
      for (let k = 0; k < sc; k++) {
        const X = x + sx * sc + k
        if (X < 0 || X >= W) continue
        const i = Y * W + X
        p[i] = sh ? sh[p[i]] : v
      }
    }
  }
}
function rect (p, x, y, w, h, c) {
  x = Math.round(x); y = Math.round(y)
  const x0 = Math.max(0, x), y0 = Math.max(0, y), x1 = Math.min(W, x + w), y1 = Math.min(H, y + h)
  if (x1 <= x0) return
  for (let Y = y0; Y < y1; Y++) p.fill(c, Y * W + x0, Y * W + x1)
}
function dith (p, x, y, w, h, a, b) {
  for (let Y = Math.max(0, y); Y < Math.min(H, y + h); Y++) for (let X = Math.max(0, x); X < Math.min(W, x + w); X++) p[Y * W + X] = ((X + Y) & 1) ? b : a
}
function px (p, x, y, c) { x = Math.round(x); y = Math.round(y); if (x >= 0 && x < W && y >= 0 && y < H) p[y * W + x] = c }
function disc (p, cx, cy, r, c, maxY) {
  for (let y = -r; y <= r; y++) {
    const Y = Math.round(cy + y)
    if (maxY !== undefined && Y >= maxY) continue
    const hw = Math.floor(Math.sqrt(r * r - y * y))
    for (let x = -hw; x <= hw; x++) px(p, cx + x, Y, c)
  }
}
function ring (p, cx, cy, r, c, every) {
  const n = Math.max(8, Math.round(r * 6))
  for (let i = 0; i < n; i++) {
    if (every && (i % every)) continue
    const a = i / n * Math.PI * 2
    px(p, cx + Math.cos(a) * r, cy + Math.sin(a) * r, c)
  }
}
function hash (n) {
  n = Math.imul((n | 0) ^ 0x9e3779b9, 0x85ebca6b)
  n = Math.imul(n ^ (n >>> 13), 0xc2b2ae35)
  n ^= n >>> 16
  return (n >>> 0) / 4294967296
}
function rng (seed) {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6D2B79F5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}
const clamp = (v, a, b) => v < a ? a : v > b ? b : v

// 3x5 font, drawn at 2x
const FONT = {}
;('0 111101101101111|1 010110010010111|2 111001111100111|3 111001111001111|4 101101111001001|5 111100111001111|6 111100111101111|7 111001010010010|8 111101111101111|9 111101111001111|' +
  'A 010101111101101|B 110101110101110|C 011100100100011|D 110101101101110|E 111100110100111|F 111100110100100|G 011100101101011|H 101101111101101|I 111010010010111|J 001001001101010|K 101101110101101|L 100100100100111|M 101111111101101|' +
  'N 110101101101101|O 010101101101010|P 110101110100100|Q 010101101110011|R 110101110101101|S 011100010001110|T 111010010010010|U 101101101101111|V 101101101101010|W 101101111111101|X 101101010101101|Y 101101010010010|Z 111001010100111|' +
  ': 000010000010000|. 000000000000010|- 000000111000000|/ 001001010100100|+ 000010111010000|& 010101010101011').split('|').forEach(g => { FONT[g[0]] = g.slice(2) })
function text (p, str, x, y, c, sh) {
  str = String(str).toUpperCase()
  for (let i = 0; i < str.length; i++) {
    const g = FONT[str[i]]
    if (g) {
      for (let r = 0; r < 5; r++) for (let q = 0; q < 3; q++) {
        if (g[r * 3 + q] === '1') {
          if (sh !== undefined) rect(p, x + i * 8 + q * 2 + 1, y + r * 2 + 1, 2, 2, sh)
          rect(p, x + i * 8 + q * 2, y + r * 2, 2, 2, c)
        }
      }
    }
  }
}
const textW = s => String(s).length * 8 - 2
const pad = (n, l) => String(Math.max(0, Math.floor(n))).padStart(l, '0')

// ---------- the pattern: 124 BPM, 16 steps, 4 bars, three voices ----------
const ROOT = [45, 41, 48, 43] // A, F, C, G
const TRIAD = [[0, 3, 7], [0, 4, 7], [0, 4, 7], [0, 4, 7]]
const BASSP = [0, null, 0, 12, null, 0, 7, null, 0, null, 12, 0, null, 7, 12, null]
const KICKP = ['x.....x.x.......', 'x.....x.x.x.....', 'x.....x.x.......', 'x.....x.x.......']
const SNAREP = ['....x.......x...', '....x.......x...', '....x.......x..x', '....x.......xxxx']
const HATP = ['..x...x...x.x.x.', '..x.x.x...x...x.', '..x...x...x.x.x.', '..x.x.x...x.....']
const ARPG = 'xx.xx.xx.xx.x.xx'
const ARPWALK = [0, 1, 2, 3, 4, 5, 4, 3, 2, 1]
function patternStep (i) {
  const s = i & 15, bar = (i >> 4) & 3
  const tri = TRIAD[bar]
  const tones = [tri[0], tri[1], tri[2], tri[0] + 12, tri[1] + 12, tri[2] + 12]
  let arp = null
  if (ARPG[s] === 'x') {
    let k = 0
    for (let j = 0; j < s; j++) if (ARPG[j] === 'x') k++
    arp = ROOT[bar] + 12 + tones[ARPWALK[(k + bar * 3) % ARPWALK.length]]
  }
  return {
    i, s, bar,
    kick: KICKP[bar][s] === 'x',
    snare: SNAREP[bar][s] === 'x',
    hat: HATP[bar][s] === 'x',
    fill: bar === 3 && s >= 12,
    bass: BASSP[s] === null ? null : ROOT[bar] + BASSP[s],
    arp
  }
}
const cutoffAt = pos => 0.5 - 0.45 * Math.cos(Math.PI * 2 * pos / 128)

// =====================================================================
// LATE ERA (1988-93 style). Parallax bands, shaded multicolour sprites,
// colour cycling and raster-split skies, all in the 16 Pepto colours.
// =====================================================================
const T = 255
const BAY = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5]
const bay = (x, y) => BAY[(y & 3) * 4 + (x & 3)]
const bayW = (x, y) => BAY[(y & 3) * 4 + ((x >> 1) & 3)]
// 2x2 ordered dither on wide (2 px) pixels: repeats every 4 px, so a band that moves in 4 px steps never changes it
const bay2 = (x, y) => [0, 8, 12, 4][((x >> 1) & 1) + ((y & 1) << 1)]
const rad = a => a * Math.PI / 180
const lerp2 = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]

// Raster-split gradient: flat colour bands joined by short ordered-dither seams, the way C64 raster bars were faded.
function gradient (buf, y0, y1, stops, seam) {
  for (let y = y0; y < y1; y++) {
    let i = 0
    while (i + 1 < stops.length && y >= stops[i + 1][0]) i++
    const nx = stops[i + 1]
    for (let x = 0; x < W; x++) {
      let v = stops[i][1]
      if (nx && y >= nx[0] - seam && bayW(x, y) < (y - (nx[0] - seam) + 0.5) / seam * 16) v = nx[1]
      buf[y * W + x] = v
    }
  }
}

// A wrapping layer strip (T = transparent) and the compositor that scrolls it.
function surf (w, h) { return { w, h, d: new Uint8Array(w * h).fill(T) } }
function layer (p, s, y0, off) {
  const w = s.w, o = ((Math.floor(off) % w) + w) % w
  for (let y = 0; y < s.h; y++) {
    const Y = y0 + y
    if (Y < 0 || Y >= H) continue
    const row = y * w, out = Y * W
    let sx = o
    for (let x = 0; x < W; x++) { const v = s.d[row + sx]; if (v !== T) p[out + x] = v; if (++sx === w) sx = 0 }
  }
}
// Paste a painted sprite into a strip, wrapping at the strip's width.
function stamp (s, spr, x, y) {
  const gx = spr.gx || 1
  for (let r = 0; r < spr.h; r++) {
    const Y = y + r
    if (Y < 0 || Y >= s.h) continue
    for (let c = 0; c < spr.w * gx; c++) {
      const v = spr.d[r * spr.w + Math.floor(c / gx)]
      if (v < 0) continue
      const X = (((x + c) % s.w) + s.w) % s.w
      s.d[Y * s.w + X] = v
    }
  }
}
// Draw a painted sprite on screen; gx = 2 sprites use C64 multicolour wide pixels.
function put (p, s, x, y, o) {
  const g = s.gx || 1
  blit(p, s, x, y, Object.assign({ sc: g, sy: 1 / g }, o))
}

// ---------- the painter: shapes become shaded, outlined pixel art ----------
// Every shape is a "part" with a 4-step colour ramp [shadow, base, light, highlight].
// Shading is rim lighting on the part's silhouette, with ordered-dither half steps,
// and a part tucked behind a later part gets a cast shadow along that edge.
function painter (w, h, gx) {
  gx = gx || 2
  const cw = Math.ceil(w / gx)
  const id = new Int16Array(cw * h).fill(-1)
  const ink = new Int8Array(cw * h).fill(-1)
  const ramps = [], groups = []
  let cur = null
  function add (ramp, x0, y0, x1, y1, test) {
    const k = ramps.length
    groups.push(cur === null ? 'p' + k : cur)
    ramps.push(typeof ramp === 'number' ? [ramp, ramp, ramp, ramp] : ramp)
    const a = Math.max(0, Math.floor(x0 / gx) - 1), b = Math.min(cw - 1, Math.ceil(x1 / gx) + 1)
    const c = Math.max(0, Math.floor(y0) - 1), d = Math.min(h - 1, Math.ceil(y1) + 1)
    for (let y = c; y <= d; y++) for (let x = a; x <= b; x++) if (test(x * gx + gx / 2, y + 0.5)) id[y * cw + x] = k
    return k
  }
  const P = {
    gx, cw, h,
    group (g) { cur = g === undefined ? null : g },
    cap (ax, ay, bx, by, r0, r1, ramp) {
      const dx = bx - ax, dy = by - ay, L2 = dx * dx + dy * dy || 1e-6, rm = Math.max(r0, r1)
      return add(ramp, Math.min(ax, bx) - rm, Math.min(ay, by) - rm, Math.max(ax, bx) + rm, Math.max(ay, by) + rm, (x, y) => {
        let t = ((x - ax) * dx + (y - ay) * dy) / L2
        t = t < 0 ? 0 : t > 1 ? 1 : t
        const ex = ax + dx * t - x, ey = ay + dy * t - y, r = r0 + (r1 - r0) * t
        return ex * ex + ey * ey <= r * r
      })
    },
    ell (cx, cy, rx, ry, ramp, rot) {
      const ca = Math.cos(rad(rot || 0)), sa = Math.sin(rad(rot || 0)), m = Math.max(rx, ry)
      return add(ramp, cx - m, cy - m, cx + m, cy + m, (x, y) => {
        const u = (x - cx) * ca + (y - cy) * sa, v = -(x - cx) * sa + (y - cy) * ca
        return (u * u) / (rx * rx) + (v * v) / (ry * ry) <= 1
      })
    },
    poly (pts, ramp) {
      let x0 = 1e9, y0 = 1e9, x1 = -1e9, y1 = -1e9
      for (let i = 0; i < pts.length; i += 2) { x0 = Math.min(x0, pts[i]); x1 = Math.max(x1, pts[i]); y0 = Math.min(y0, pts[i + 1]); y1 = Math.max(y1, pts[i + 1]) }
      return add(ramp, x0, y0, x1, y1, (x, y) => {
        let inside = false
        for (let i = 0, j = pts.length - 2; i < pts.length; j = i, i += 2) {
          const xi = pts[i], yi = pts[i + 1], xj = pts[j], yj = pts[j + 1]
          if ((yi > y) !== (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi) inside = !inside
        }
        return inside
      })
    },
    box (x, y, w2, h2, ramp) { return add(ramp, x, y, x + w2, y + h2, (px2, py) => px2 >= x && px2 < x + w2 && py >= y && py < y + h2) },
    shape (x0, y0, x1, y1, ramp, test) { return add(ramp, x0, y0, x1, y1, test) },
    cut (x0, y0, x1, y1, test) {
      for (let y = Math.max(0, Math.floor(y0)); y <= Math.min(h - 1, Math.ceil(y1)); y++) for (let x = Math.max(0, Math.floor(x0 / gx)); x <= Math.min(cw - 1, Math.ceil(x1 / gx)); x++) if (test(x * gx + gx / 2, y + 0.5)) id[y * cw + x] = -1
    },
    dot (x, y, c) { x = Math.floor(x / gx); y = Math.floor(y); if (x >= 0 && x < cw && y >= 0 && y < h) ink[y * cw + x] = c },
    line (ax, ay, bx, by, c) {
      const n = Math.max(1, Math.ceil(Math.max(Math.abs(bx - ax) / gx, Math.abs(by - ay))))
      for (let i = 0; i <= n; i++) P.dot(ax + (bx - ax) * i / n, ay + (by - ay) * i / n, c)
    },
    done (o) {
      o = o || {}
      const lx = o.lx === undefined ? -1 : o.lx, ly = o.ly === undefined ? -1 : o.ly
      const out = new Int8Array(cw * h).fill(-1)
      const get = (x, y) => (x < 0 || y < 0 || x >= cw || y >= h) ? -1 : id[y * cw + x]
      for (let y = 0; y < h; y++) {
        for (let x = 0; x < cw; x++) {
          const k = id[y * cw + x]
          if (k < 0) continue
          const R = ramps[k]
          const edge = (xx, yy) => get(xx, yy) < k
          let dLx = 99, dLy = 99, dSx = 99, dSy = 99
          if (lx) { for (let s = 1; s <= 3; s++) if (edge(x + lx * s, y)) { dLx = s * gx; break } for (let s = 1; s <= 3; s++) if (edge(x - lx * s, y)) { dSx = s * gx; break } }
          if (ly) { for (let s = 1; s <= 5; s++) if (edge(x, y + ly * s)) { dLy = s; break } for (let s = 1; s <= 5; s++) if (edge(x, y - ly * s)) { dSy = s; break } }
          const dL = Math.min(dLx, dLy), dS = Math.min(dSx, dSy), ch = (x + y) & 1
          let lv
          const thick = !o.flat && Math.min(dLx + dSx, dLy + dSy) > 9
          if (thick && dLx <= gx && dLy <= 1) lv = 3
          else if (dL <= 2) lv = 2
          else if (dS <= 2) lv = 0
          else if (thick && dL <= 4) lv = ch ? 2 : 1
          else if (thick && dS <= 4) lv = ch ? 0 : 1
          else lv = 1
          const front = j => j > k && groups[j] !== groups[k]
          if (o.sep !== false && lv > 0 && (front(get(x + 1, y)) || front(get(x - 1, y)) || front(get(x, y + 1)) || front(get(x, y - 1)))) lv = 0
          out[y * cw + x] = R[lv]
          if (o.inner !== undefined) {
            const back = j => j >= 0 && j < k && groups[j] !== groups[k]
            if (back(get(x + 1, y)) || back(get(x - 1, y)) || back(get(x, y + 1)) || back(get(x, y - 1))) out[y * cw + x] = o.inner
          }
        }
      }
      for (let i = 0; i < out.length; i++) { if (ink[i] >= 0) out[i] = ink[i]; else if (ink[i] === -2) out[i] = -1 }
      if (o.outline !== -1) {
        const oc = o.outline === undefined ? BLACK : o.outline
        const src = out.slice()
        for (let y = 0; y < h; y++) {
          for (let x = 0; x < cw; x++) {
            const i = y * cw + x
            if (src[i] >= 0) continue
            if ((x > 0 && src[i - 1] >= 0) || (x < cw - 1 && src[i + 1] >= 0) || (y > 0 && src[i - cw] >= 0) || (y < h - 1 && src[i + cw] >= 0)) out[i] = oc
          }
        }
      }
      return { w: cw, h, d: out, gx }
    }
  }
  return P
}
const ERASE = -2
const DEV = []

// ---------- puppets: forward kinematics for side-on figures ----------
// Angles are in degrees from straight down; positive swings toward the facing side, 180 points straight up.
function puppet (S, Q) {
  const P = painter(S.w, S.h, S.gx)
  const hip = [S.hx + (Q.dx || 0), S.hy + (Q.dy || 0)]
  const lean = Q.lean || 0
  const go = (o, a, L) => [o[0] + Math.sin(rad(a)) * L, o[1] + Math.cos(rad(a)) * L]
  const neck = go(hip, 180 + lean, S.torso)
  const head = go(neck, 180 + lean + (Q.head || 0), S.neck)
  const shF = [neck[0] + S.sh[0], neck[1] + S.sh[1]], shB = [neck[0] - S.sh[2], neck[1] + S.sh[3]]
  const kneeB = go(hip, Q.tb, S.thigh), ankB = go(kneeB, Q.sb, S.shin)
  const kneeF = go(hip, Q.tf, S.thigh), ankF = go(kneeF, Q.sf, S.shin)
  const elB = go(shB, Q.ub, S.upper), haB = go(elB, Q.fb, S.fore)
  const elF = go(shF, Q.uf, S.upper), haF = go(elF, Q.ff, S.fore)
  const J = { hip, neck, head, shF, shB, kneeB, ankB, kneeF, ankF, elB, haB, elF, haF, lean, go }
  S.paint(P, J, Q)
  const s = P.done({ lx: S.lx, ly: S.ly, inner: S.inner })
  s.j = J
  return s
}
const poseMix = (a, b, t) => { const o = {}; for (const k in a) o[k] = typeof a[k] === 'number' ? a[k] + ((b[k] === undefined ? a[k] : b[k]) - a[k]) * t : a[k]; return o }
function cacheBy (fn) { const m = new Map(); return (k, ...a) => { let v = m.get(k); if (!v) { v = fn(...a); m.set(k, v) } return v } }

// Simple palette remap of a painted sprite (for night tints, second players, flashes).
function recolour (s, map) { const d = new Int8Array(s.d.length); for (let i = 0; i < d.length; i++) { const v = s.d[i]; d[i] = v >= 0 && map[v] !== undefined ? map[v] : v } return { w: s.w, h: s.h, d, gx: s.gx } }


const VIGNETTES = []

// ---------- 11. Barbarian sword duel ----------
const STEEL = [DGREY, GREY, LGREY, WHITE], LEATHER = [BLACK, BROWN, ORANGE, ORANGE]
const BARB = [
  { skin: [ORANGE, LRED, LRED, WHITE], skinB: [BROWN, ORANGE, ORANGE, LRED], hair: [BLACK, DGREY, GREY, GREY], cloth: [BLACK, DGREY, GREY, LGREY], helm: false },
  { skin: [BROWN, ORANGE, LRED, LRED], skinB: [BLACK, BROWN, ORANGE, ORANGE], hair: [BLACK, BROWN, ORANGE, ORANGE], cloth: [BROWN, RED, RED, LRED], helm: true }
]
function barbSpec (who) {
  const C = BARB[who]
  return {
    w: 128, h: 124, gx: 2, hx: 58, hy: 72, inner: C.skin[0] === ORANGE ? BROWN : BLACK, torso: 24, neck: 6, thigh: 17, shin: 17, upper: 13, fore: 12, sh: [1, 4, 3, 3],
    paint (P, J, Q) {
      const sk = C.skin, skB = C.skinB
      const hand = J.haF, sw = Q.sw, tip = J.go(hand, sw, 38), pommel = J.go(hand, sw + 180, 7)
      if (!C.helm || Q.bare) P.cap(J.head[0] - 3, J.head[1] - 2, J.neck[0] - 7, J.neck[1] + 6, 4, 2.5, C.hair)
      P.group('ab'); P.cap(...J.shB, ...J.elB, 4.6, 4, skB); P.cap(...J.elB, ...J.haB, 4, 3.2, skB); P.group()
      P.cap(...lerp2(J.elB, J.haB, 0.35), ...lerp2(J.elB, J.haB, 0.8), 3.6, 3.4, LEATHER)
      P.ell(J.haB[0], J.haB[1], 3.2, 3, skB)
      P.group('lb'); P.cap(...J.hip, ...J.kneeB, 6, 4.6, skB); P.cap(...J.kneeB, ...J.ankB, 4.4, 3, skB); P.group()
      P.cap(...lerp2(J.kneeB, J.ankB, 0.55), ...J.ankB, 3.6, 3.4, LEATHER)
      P.cap(J.ankB[0] - 1, J.ankB[1] + 1.5, J.ankB[0] + 6, J.ankB[1] + 2, 3, 2.4, LEATHER)
      const mid = lerp2(J.hip, J.neck, 0.45), chest = lerp2(J.hip, J.neck, 0.8)
      P.group('torso')
      P.cap(...J.hip, ...mid, 6.4, 7.2, sk)
      P.cap(...mid, ...chest, 8, 9.6, sk)
      P.cap(J.shB[0] - 2, J.shB[1] + 1, J.shF[0] + 2, J.shF[1] + 1, 5.4, 5.4, sk)
      P.group()
      // pecs, abs and the belt
      const fw = J.go(chest, 90 + J.lean, 1)
      P.line(chest[0] - 3, chest[1] + 4, fw[0] + 6, fw[1] + 4, C.skin[0])
      for (let k = 0; k < 3; k++) { const a = lerp2(J.hip, J.neck, 0.2 + k * 0.13); P.dot(a[0] + 3, a[1], C.skin[0]); P.dot(a[0] + 5, a[1] + 1, C.skin[0]) }
      P.cap(J.hip[0] - 7, J.hip[1] - 2, J.hip[0] + 7, J.hip[1] - 2, 2.6, 2.6, LEATHER)
      P.box(J.hip[0] + 1, J.hip[1] - 4, 4, 4, STEEL)
      P.poly([J.hip[0] - 7, J.hip[1], J.hip[0] + 8, J.hip[1], J.hip[0] + 7 + Q.lean * 0.1, J.hip[1] + 14, J.hip[0] + 2, J.hip[1] + 16, J.hip[0] - 4, J.hip[1] + 13], C.cloth)
      // head, face and hair or helmet
      P.group('torso'); P.ell(J.neck[0] + 1, J.neck[1] - 1, 4.2, 4, sk); P.group('head')
      P.ell(J.head[0] + 1, J.head[1], 5.6, 7, sk)
      P.ell(J.head[0] + 3.5, J.head[1] + 4.5, 3.2, 2.6, sk); P.group()
      if (C.helm && !Q.bare) {
        P.ell(J.head[0], J.head[1] - 2.5, 6.6, 5.6, STEEL)
        P.box(J.head[0] - 6, J.head[1] - 1, 13, 2.4, STEEL)
        P.box(J.head[0] + 4, J.head[1] - 1, 2, 7, STEEL)
        P.cap(J.head[0] - 2, J.head[1] - 8, J.head[0] - 5, J.head[1] - 12, 1.4, 0.8, [BROWN, RED, LRED, LRED])
      } else {
        P.ell(J.head[0] - 1.5, J.head[1] - 4, 5.6, 4, C.hair)
        if (!C.helm) P.cap(J.head[0] - 5, J.head[1] - 1, J.head[0] + 5, J.head[1] - 3, 1.2, 1.2, [BROWN, RED, LRED, LRED])
      }
      P.dot(J.head[0] + 4, J.head[1] - 0.5, BLACK)
      P.line(J.head[0] + 2.5, J.head[1] - 2, J.head[0] + 5.5, J.head[1] - 1.5, C.skin[0])
      if (!C.helm) P.line(J.head[0] + 2, J.head[1] + 5, J.head[0] + 6, J.head[1] + 5, C.hair[0])
      // front leg
      P.group('lf'); P.cap(...J.hip, ...J.kneeF, 6.4, 4.8, sk); P.cap(...J.kneeF, ...J.ankF, 4.6, 3.2, sk); P.group()
      P.cap(...lerp2(J.kneeF, J.ankF, 0.55), ...J.ankF, 3.8, 3.6, LEATHER)
      P.cap(J.ankF[0] - 1, J.ankF[1] + 1.5, J.ankF[0] + 7, J.ankF[1] + 2, 3.2, 2.6, LEATHER)
      // broadsword (changed: drawn by broadsword(), below, and not at all when unarmed)
      if (!Q.unarmed) broadsword(P, hand, sw, J.go)
      // front arm and fist
      P.group('af'); P.cap(...J.shF, ...J.elF, 5, 4.2, sk); P.cap(...J.elF, ...J.haF, 4.2, 3.4, sk); P.ell(J.shF[0], J.shF[1] + 2, 5, 4.4, sk); P.group()
      P.cap(...lerp2(J.elF, J.haF, 0.35), ...lerp2(J.elF, J.haF, 0.8), 3.8, 3.6, LEATHER)
      P.ell(hand[0], hand[1], 3.4, 3.2, sk)
      J.tip = tip
    }
  }
}
// The prototype's broadsword, as barbSpec drew it (verbatim), in a function of its own so the
// sword in flight and in the floor are drawn by the same code.
function broadsword (P, hand, sw, go) {
  const tip = go(hand, sw, 38), pommel = go(hand, sw + 180, 7)
  P.cap(...hand, ...tip, 2.4, 1.1, STEEL)
  P.line(...lerp2(hand, tip, 0.12), ...lerp2(hand, tip, 0.85), WHITE)
  P.cap(...hand, ...pommel, 1.4, 1.4, LEATHER); P.ell(pommel[0], pommel[1], 2, 2, [BROWN, ORANGE, YELLOW, WHITE])
  const gd = go(hand, sw + 90, 6), gd2 = go(hand, sw - 90, 6), gc = go(hand, sw, 3)
  P.cap(gd[0] + (gc[0] - hand[0]), gd[1] + (gc[1] - hand[1]), gd2[0] + (gc[0] - hand[0]), gd2[1] + (gc[1] - hand[1]), 1.6, 1.6, [BROWN, ORANGE, YELLOW, WHITE])
}
const BARB_S = [barbSpec(0), barbSpec(1)]
const DUEL = {
  guard: { lean: 6, tf: 28, sf: 0, tb: -22, sb: -8, uf: 35, ff: 95, sw: 140, ub: 25, fb: 85, dy: 2 },
  guard2: { lean: 10, tf: 36, sf: 8, tb: -30, sb: -14, uf: 28, ff: 82, sw: 128, ub: 18, fb: 72, dy: 5 },
  raise: { lean: -6, tf: 20, sf: 0, tb: -18, sb: -8, uf: 168, ff: 205, sw: 245, ub: 150, fb: 195, dy: 0 },
  chop: { lean: 24, tf: 48, sf: 16, tb: -36, sb: -22, uf: 92, ff: 96, sw: 72, ub: 70, fb: 96, dy: 7, dx: 6 },
  block: { lean: -4, tf: 22, sf: 0, tb: -26, sb: -12, uf: 150, ff: 170, sw: 92, ub: 130, fb: 175, dy: 3 },
  spinA: { lean: 18, tf: 40, sf: 10, tb: -30, sb: -18, uf: -40, ff: -70, sw: -95, ub: -20, fb: 20, dy: 6 },
  spinB: { lean: 8, tf: 50, sf: 20, tb: -40, sb: -30, uf: 120, ff: 105, sw: 80, ub: 60, fb: 80, dy: 8, dx: 10 },
  hit: { lean: -22, tf: 12, sf: -10, tb: -38, sb: -30, uf: 145, ff: 125, sw: 205, ub: -70, fb: -40, dx: -6, dy: 2 },
  // disarm (new): bending low to pull the sword from the floor, the front hand on its grip
  reach: { lean: 40, tf: 64, sf: -6, tb: -26, sb: -30, uf: 70, ff: 40, sw: 0, ub: 30, fb: 60, dy: 6, dx: 12 }
}
// changed: the third argument is `unarmed` (the prototype's was `bare`, for the helmet knock)
const barbSpr = cacheBy((who, k, unarmed) => puppet(BARB_S[who], Object.assign({ unarmed }, DUEL[k])))

// ---------- the disarm (new drawing, SIDOU's own, in the painter's style) ----------
// The sword in flight: eight rotations, each painted whole by the painter (not rotated
// pixels), rotation k pointing k x 45 degrees from straight down toward the right, the grip
// at the canvas's centre (SWORD_C, SWORD_C).
const SWORD_C = 42
const swordGo = (o, a, L) => [o[0] + Math.sin(rad(a)) * L, o[1] + Math.cos(rad(a)) * L]
const SWORDS = [0, 1, 2, 3, 4, 5, 6, 7].map(k => {
  const P = painter(SWORD_C * 2, SWORD_C * 2, 2)
  broadsword(P, [SWORD_C, SWORD_C], k * 45, swordGo)
  return P.done()
})
// The sword stuck point-down in a flagstone, leaning a little, the blade cut at the floor
// line, and a crack round its point. The grip is at (STUCK_GX, STUCK_GY).
const STUCK_GX = 16, STUCK_GY = 10, STUCK_FLOOR = 36
const STUCK = (() => {
  const P = painter(32, 44, 2)
  broadsword(P, [STUCK_GX, STUCK_GY], -8, swordGo)
  const s = P.done()
  // the blade cut at the floor line (the painter's cut leaves the white edge line, which is
  // ink), a black slot under it, and the crack, after the outline (stored columns: one is two
  // screen pixels)
  for (let y = STUCK_FLOOR; y < s.h; y++) for (let x = 0; x < s.w; x++) s.d[y * s.w + x] = y === STUCK_FLOOR && s.d[(y - 1) * s.w + x] >= 0 ? BLACK : -1
  const crack = [[4, 36, BLACK], [3, 36, BLACK], [2, 37, BLACK], [1, 37, BLACK], [0, 38, BLACK], [9, 36, BLACK], [10, 36, BLACK], [11, 37, BLACK], [12, 37, BLACK],
    [13, 38, BLACK], [14, 38, DGREY], [5, 37, BLACK], [6, 38, BLACK], [6, 39, BLACK], [7, 40, DGREY], [8, 37, BLACK], [3, 37, LGREY], [11, 38, LGREY], [2, 34, LGREY], [10, 33, LGREY], [12, 35, GREY]]
  for (const [x, y, c] of crack) s.d[y * s.w + x] = c
  return s
})()
// The flight: the sword's middle (14 pixels down the blade from the grip) follows a parabola
// from the hand that lost it to the place it sticks, turning a whole number of turns so it
// lands point-down (rotation 0) with its grip at `to`. `a0` is the sword's angle in the hand.
function swordFlight (from, to, a0) {
  const k0 = ((Math.round(a0 / 45) % 8) + 8) % 8
  const c0 = swordGo(from, k0 * 45, 14), c1 = [to[0], to[1] + 14]
  const vy = -6, dy = c1[1] - c0[1]
  const T = Math.round((-vy + Math.sqrt(vy * vy + 1.8 * dy)) / 0.9)
  const g = 2 * (dy - vy * T) / (T * T)
  return { from, to, c0, c1, vy, g, T, n: 0, k0, kEnd: 8 * Math.max(2, Math.round(T / 8)) }
}
// Where the flying sword's grip is at frame n, and which rotation shows.
function swordAt (f, n) {
  const k = ((Math.round(f.k0 + (f.kEnd - f.k0) * n / f.T) % 8) + 8) % 8
  const cx = f.c0[0] + (f.c1[0] - f.c0[0]) * n / f.T, cy = f.c0[1] + f.vy * n + f.g * n * n / 2
  const [gx, gy] = swordGo([cx, cy], k * 45, -14)
  return [Math.round(gx), Math.round(gy), k]
}

// ---------- the status bar's stamina meter (new, replacing the prototype's pips) ----------
// A broadsword a side: the hilt at the screen's edge, the blade pointing in toward VS in
// eight notches of BLADE_N pixels; each stamina lost takes the notch at the tip, and the tip
// moves in with what is left. What is lost stays as a dark ghost of the full blade.
// Side 0 is drawn as below; side 1 is its mirror image.
const BLADE_X = 17, BLADE_N = 11
const HILT = sprite([
  '...........ny....',
  '...........oy....',
  '...........oy....',
  '...........oy....',
  '...........oy....',
  '.ny........oy.www',
  'nyyonnonnonoymlll',
  'nyyonnonnonoymmmm',
  '.nn........oymlll',
  '...........oy.ddd',
  '...........oy....',
  '...........oy....',
  '...........oy....',
  '...........nn....'
])
const NOTCH = sprite(['wwwwwwwwww.', 'llllllllllk', 'mmmmmmmmmmk', 'lllllllllll', 'dddddddd...'])
const NOTCH_TIP = sprite(['www..', 'lllww', 'mmmmll', 'lllm.', 'dd...'])
const GHOST = sprite(['dddddddddd.', '..........k', '..........k', 'dddddddddd.', '...........'])
const GHOST_TIP = sprite(['dd...', '..dd.', '....d', '..dd.', 'd....'])
function bladeMeter (p, side, hp) {
  const at = (s, x, y) => {
    for (let r = 0; r < s.h; r++) for (let q = 0; q < s.w; q++) {
      const v = s.d[r * s.w + q]
      if (v < 0) continue
      px(p, side ? W - 1 - (x + q) : x + q, y + r, v)
    }
  }
  at(HILT, 0, 1)
  for (let k = 0; k < 8; k++) at(GHOST, BLADE_X + k * BLADE_N, 6)
  at(GHOST_TIP, BLADE_X + 8 * BLADE_N, 6)
  for (let k = 0; k < hp; k++) at(NOTCH, BLADE_X + k * BLADE_N, 6)
  if (hp > 0) at(NOTCH_TIP, BLADE_X + hp * BLADE_N, 6)
}


VIGNETTES.push({
  id: 'rite', era: 'late', title: 'RED MOON RITE', cap: 'A nod to late-80s fantasy sword-fighting games: two new barbarians trade broadsword blows in a ruined temple under a blood moon.',
  maps: [
    ['kick', 'Kick drum: the leading fighter winds up and chops overhead'],
    ['snare', 'Snare: the other fighter parries, with a spark where the blades meet'],
    ['hat', 'Hi-hats: footwork; both shift their stance'],
    ['bass', 'Bass pitch: who presses forward and who gives ground'],
    ['fill', 'Bar-4 fill: a spinning attack disarms the other fighter: his sword spins away and sticks in the flagstones; he pulls it free in bar 1'],
    ['arp', 'Arp notes: the torches flare'],
    ['cutoff', 'Filter cutoff: how dark the sky gets'],
    ['bar', 'Bars: the loser of each exchange loses stamina; empty bars refill']
  ],
  bands: [
    { id: 'hud', name: 'Status bar', kind: 'hud', y0: 0, y1: 16 },
    { id: 'sky', name: 'Raster sky and moon', kind: 'sky', y0: 16, y1: 136 },
    { id: 'arena', name: 'Temple and floor', kind: 'static', y0: 136, y1: 240 }
  ],
  make () {
    const PALS = [
      [[16, BLACK], [30, BLUE], [58, PURPLE], [100, RED], [124, LRED]],
      [[16, BLUE], [28, PURPLE], [56, RED], [92, LRED], [124, RED]],
      [[16, PURPLE], [32, RED], [62, LRED], [96, LRED], [118, ORANGE]]
    ]
    const torchAt = [[26, 104], [212, 104]]
    const flameMask = []
    const bgs = PALS.map(stops => {
      const b = new Uint8Array(W * H)
      gradient(b, 16, 140, stops, 6)
      // the blood moon: maria, a lit rim and a dithered halo
      for (let y = 16; y < 110; y++) for (let x = 60; x < 180; x++) {
        const dx = x - 120, dy = y - 60, r = Math.hypot(dx, dy)
        if (r < 28) {
          const lit = -dx * 0.6 - dy * 0.5
          let v = WHITE
          const m = hash(Math.floor((x + 3) / 6) * 31 + Math.floor((y + 2) / 5) * 17) + 0.35 * Math.sin(x / 9 + y / 13)
          if (m > 0.8 && r < 25) v = bayW(x, y) < 10 ? LRED : WHITE
          if (m > 1.0 && r < 25) v = LRED
          if (lit < -2) v = bayW(x, y) < Math.min(16, (-lit + 2) * 1.6) ? (lit < -12 ? RED : LRED) : v
          if (lit < -16) v = bayW(x, y) < 8 ? RED : LRED
          b[y * W + x] = v
        } else if (r < 38 && bayW(x, y) < (38 - r) * 1.1) b[y * W + x] = r < 31 ? LRED : (stops[stops.length - 2][1] === LRED ? RED : LRED)
      }
      // wisps across the moon
      for (const [x0, y0, w0] of [[40, 70, 110], [110, 48, 90], [10, 92, 80], [150, 88, 80]]) for (let x = x0; x < x0 + w0; x++) { const th = Math.sin(Math.PI * (x - x0) / w0) * 3; for (let y = Math.round(y0 - th); y <= y0 + th * 0.5; y++) b[y * W + x] = bayW(x, y) < 10 ? PURPLE : BLACK }
      // jagged hills on the horizon
      for (let x = 0; x < W; x++) { const h = 14 + 8 * Math.abs(Math.sin(x / 23)) + 5 * Math.sin(x / 9); for (let y = Math.round(140 - h); y < 140; y++) b[y * W + x] = bayW(x, y) < 2 ? PURPLE : BLACK }
      return b
    })
    // the arena: steps, ruined columns, an arch, flagstones
    const fg = new Uint8Array(W * H).fill(T)
    {
      const P = painter(240, 240, 2), STONE = [BLACK, DGREY, GREY, LGREY], DARK = [BLACK, BLACK, DGREY, GREY]
      P.box(0, 140, 240, 8, DARK); P.box(0, 148, 240, 8, STONE); P.box(0, 156, 240, 6, DARK)
      for (const [x, top, broken] of [[8, 36, true], [196, 54, false]]) {
        P.box(x, top + 10, 34, 150 - top, STONE)
        P.box(x - 4, top + 2, 42, 9, STONE)
        if (broken) P.poly([x - 4, top + 3, x + 6, top - 6, x + 14, top + 1, x + 24, top - 10, x + 38, top + 3], STONE)
        else P.box(x - 6, top - 4, 46, 7, STONE)
        P.box(x - 6, 150, 46, 12, STONE)
      }
      for (let k = 0; k < 7; k++) { const a = rad(180 - k * 15), cx = 120, cy = 126; P.poly([cx + Math.cos(a) * 52, cy - Math.sin(a) * 52, cx + Math.cos(a - 0.26) * 52, cy - Math.sin(a - 0.26) * 52, cx + Math.cos(a - 0.26) * 40, cy - Math.sin(a - 0.26) * 40, cx + Math.cos(a) * 40, cy - Math.sin(a) * 40], STONE) }
      P.box(62, 124, 14, 18, STONE); P.box(164, 128, 14, 14, STONE)
      P.poly([164, 128, 178, 128, 176, 120, 168, 124], STONE)
      P.ell(150, 150, 8, 5, STONE); P.ell(62, 152, 6, 4, STONE)
      for (const [tx, ty] of torchAt) { P.box(tx - 3, ty, 6, 12, [BLACK, BROWN, ORANGE, ORANGE]); P.box(tx - 6, ty - 2, 12, 4, [BLACK, DGREY, GREY, GREY]) }
      const s = P.done({ lx: -1, ly: -1 })
      put(fg, s, 0, 0)
      for (let y = 46; y < 150; y++) for (const x0 of [8, 196]) for (let x = x0 + 4; x < x0 + 30; x += 8) if (fg[y * W + x] !== T && fg[y * W + x] !== BLACK) { fg[y * W + x] = DGREY; fg[y * W + x + 1] = (x - x0) < 10 ? LGREY : GREY }
      for (let y = 162; y < 240; y++) {
        const depth = (y - 150) / 90
        for (let x = 0; x < W; x++) {
          const row = Math.floor(Math.log(1 + (y - 162) / 6) * 3.2)
          const rowY = (k) => 162 + 6 * (Math.exp(k / 3.2) - 1)
          const vx = (x - 120) / (0.4 + depth * 1.6)
          const col = Math.floor(vx / 22 + (row & 1) * 0.5)
          const fracx = vx / 22 + (row & 1) * 0.5 - col
          let v = ((row + col) & 1) ? GREY : DGREY
          if (hash(row * 31 + col * 7) < 0.3) v = bayW(x, y) < 6 ? DGREY : v
          if (y - rowY(row) < 1.2) v = BLACK
          else if (y - rowY(row) < 2.2) v = v === GREY ? LRED : GREY
          if (fracx < 0.04) v = BLACK
          if (hash(Math.floor(x / 3) * 17 + Math.floor(y / 2) * 3) < 0.012) v = BLACK
          fg[y * W + x] = v
        }
      }
      for (let x = 0; x < W; x++) for (let y = 222; y < 240; y++) if (bayW(x, y) < (y - 222) / 2) fg[y * W + x] = BLACK
    }
    for (const [tx, ty] of torchAt) for (let y = ty - 14; y < ty - 2; y++) for (let x = tx - 5; x < tx + 5; x++) flameMask.push([x, y, tx, ty])
    const st = { rowPal: new Uint8Array(30).fill(1), lead: 0, pose: ['guard', 'guard'], t: [0, 0], stance: 0, x: 120, spark: 0, sparkAt: [120, 120], flare: 0, unarmed: -1, sword: null, hp: [8, 8], spin: 0, spinWho: 0, shake: 0 }
    const FEET = 228
    // disarm (new): where fighter k's front hand is on the screen in a pose, fighter at x
    const xs0 = k => k ? st.x + 30 : st.x - 30
    const handAt = (k, pose, un, x) => {
      const s = barbSpr(k + pose + (un ? 'u' : ''), k, pose, un), ox = k === 0 ? x - 58 : x - 70, c = Math.floor(s.j.haF[0] / s.gx)
      return [k === 0 ? ox + c * 2 : ox + (s.w - 1 - c) * 2, FEET - 112 + Math.floor(s.j.haF[1])]
    }
    return {
      st,
      art: { bgs, fg, flameMask, torchAt },
      step (e, c) {
        if (e.bass !== null) st.lead = e.bass >= 50 ? 1 : 0
        const L = st.lead, O = 1 - L
        // disarm: once his sword has stuck, the disarmed fighter pulls it free on the next beat
        const U = st.unarmed
        if (st.sword && st.sword.n >= st.sword.T && st.pose[U] !== 'reach' && st.t[U] <= 0 && e.s % 4 === 0) { st.pose[U] = 'reach'; st.t[U] = 8 }
        if (e.fill && e.s === 12 && U < 0) { st.spin = 4; st.spinWho = L; st.pose[L] = 'spinA'; st.t[L] = 99 }
        if (st.spin > 0) {
          st.pose[st.spinWho] = st.spin & 1 ? 'spinB' : 'spinA'
          st.spin--
          if (st.spin === 0) {
            const V = 1 - st.spinWho
            st.pose[st.spinWho] = 'spinB'; st.t[st.spinWho] = 6
            st.pose[V] = 'hit'; st.t[V] = 10; st.hp[V] = Math.max(0, st.hp[V] - 2); st.shake = 3
            // disarm (replacing the prototype's helmet knock): his sword flies from his hand and
            // lands point-down where he will bend to pull it free; the fighters hold their ground
            // meanwhile. Either fighter can be disarmed (the leader at the fill disarms the other);
            // with the arcade's pattern the leader is always fighter 0
            if (U < 0) { st.unarmed = V; st.sword = swordFlight(handAt(V, 'hit', false, xs0(V)), handAt(V, 'reach', true, xs0(V)), V ? -DUEL.hit.sw : DUEL.hit.sw) }
          }
          return
        }
        // disarm: while one fighter is unarmed, no one attacks or parries: the other waits for him
        if (e.kick && U < 0) { st.pose[L] = 'raise'; st.t[L] = 3; st.next = 'chop' }
        if (e.snare && U < 0) { st.pose[O] = 'block'; st.t[O] = 5; st.spark = 3; st.hp[L] = Math.max(0, st.hp[L] - 1) }
        if (e.hat) { st.stance ^= 1 }
        if (e.s === 0) for (const k of [0, 1]) if (st.hp[k] === 0) st.hp[k] = 8
        if (e.arp !== null) st.flare = 3
      },
      layers (p, c) {
        const want = c.cutoff < 0.36 ? 0 : c.cutoff < 0.68 ? 1 : 2
        for (let r = 2; r < 17; r++) if (st.rowPal[r] !== want) { st.rowPal[r] = want; break }
        p.fill(BLACK, 0, 16 * W)
        for (let r = 2; r < 30; r++) p.set(bgs[st.rowPal[Math.min(r, 16)]].subarray(r * 8 * W, r * 8 * W + 8 * W), r * 8 * W)
        for (let i = 16 * W; i < W * H; i++) { const v = fg[i]; if (v !== T) p[i] = v }
        rect(p, 0, 0, W, 16, BLACK)
        // changed: a broadsword blade a side (bladeMeter), not the prototype's eight pips a side
        bladeMeter(p, 0, st.hp[0]); bladeMeter(p, 1, st.hp[1])
        text(p, 'VS', 112, 3, LRED)
        rect(p, 0, 15, W, 1, RED)
      },
      stages: [
        ['cycle', 'Colour cycling', (p, c) => {
          const big = st.flare > 0
          const FL = [YELLOW, WHITE, ORANGE, YELLOW, RED, ORANGE]
          for (const [x, y, tx, ty] of flameMask) {
            const dx = (x - tx + 0.5) / 5, dy = (ty - 2 - y) / (big ? 12 : 9)
            const wob = Math.sin(y * 0.9 + c.frame * 0.7) * 0.15
            if (Math.abs(dx + wob) > 1 - dy * 0.9 || dy > 1) continue
            p[y * W + x] = FL[((y - c.frame) % 6 + 6) % 6 + (dy < 0.3 && Math.abs(dx) < 0.4 ? 0 : 0)] === WHITE && dy > 0.5 ? YELLOW : FL[((y - c.frame) % 6 + 6) % 6]
          }
          if (big) for (const [tx, ty] of torchAt) for (let y = ty - 24; y < ty - 13; y++) for (let x = tx - 3; x < tx + 3; x++) if (((x + y + c.frame) & 3) === 0) px(p, x, y, (y & 1) ? YELLOW : ORANGE)
          if (st.flare > 0 && (c.frame & 1)) st.flare--
        }, 'Two torch flames rotate six colours'],
        ['sprites', 'Fighters and FX', (p, c) => {
          for (const k of [0, 1]) if (st.t[k] > 0 && --st.t[k] === 0) {
            if (st.pose[k] === 'raise' && st.next) { st.pose[k] = st.next; st.t[k] = 5; st.next = null } else {
              if (st.pose[k] === 'reach') { st.unarmed = -1; st.sword = null } // disarm: the sword is back in his hand
              st.pose[k] = st.stance ? 'guard2' : 'guard'
            }
          }
          for (const k of [0, 1]) if (st.t[k] <= 0) st.pose[k] = st.stance ? 'guard2' : 'guard'
          const tx = 120 + (st.lead ? -8 : 8)
          if (!st.sword) st.x += clamp(tx - st.x, -1, 1) // changed: held while the sword is out of his hand
          const sh = st.shake > 0 ? (st.shake & 1 ? 2 : -2) : 0
          if (st.shake > 0) st.shake--
          const xs = [st.x - 30, st.x + 30]
          const sw = st.sword
          if (sw && sw.n < sw.T) sw.n++ // disarm: frames 1 .. T - 1 in flight, then in the floor
          // disarm: in the floor, behind the fighters, and over the hand that pulls it free
          const stuck = () => put(p, STUCK, sw.to[0] - STUCK_GX, sw.to[1] - STUCK_GY)
          if (sw && sw.n >= sw.T && st.pose[st.unarmed] !== 'reach') stuck()
          for (const k of [0, 1]) {
            const pose = st.pose[k], un = k === st.unarmed
            const s = barbSpr(k + pose + (un ? 'u' : ''), k, pose, un)
            const ox = k === 0 ? xs[0] - 58 : xs[1] - (128 - 58)
            rect(p, xs[k] - 18, FEET + 2, 36, 2, BLACK)
            put(p, s, ox + sh, FEET - 112, { fx: k === 1 })
          }
          if (st.spark > 0) {
            const cx = st.x, cy = FEET - 92
            const R = st.spark * 4 + 4
            for (let i = 0; i < 8; i++) { const a = i * Math.PI / 4 + st.spark; for (let r = 2; r < R; r++) px(p, cx + Math.cos(a) * r, cy + Math.sin(a) * r * 0.8, r < R / 2 ? WHITE : YELLOW) }
            disc(p, cx, cy, 3, WHITE)
            st.spark--
          }
          if (st.spin > 0 || (st.pose[st.spinWho] === 'spinB' && st.t[st.spinWho] > 3)) {
            const cx = xs[st.spinWho] + (st.spinWho ? -10 : 10), cy = FEET - 60
            for (let a = 0; a < 14; a++) { const t = a / 14 * Math.PI * 1.4 + c.frame * 0.9; px(p, cx + Math.cos(t) * 44 * (st.spinWho ? -1 : 1), cy + Math.sin(t) * 12, a & 1 ? LGREY : WHITE) }
          }
          if (sw && sw.n >= sw.T && st.pose[st.unarmed] === 'reach') stuck()
          if (sw && sw.n < sw.T) { // disarm: the sword in flight, over the fighters
            const [gx, gy, k] = swordAt(sw, sw.n)
            put(p, SWORDS[k], gx - SWORD_C, gy - SWORD_C)
          }
        }]
      ]
    }
  }
})

// ---------- the exporter (new) ----------
const RITE = VIGNETTES[0]

// A painted sprite (-1 transparent) as an image of palette indexes (255 transparent).
const spriteImage = s => ({ w: s.w, h: s.h, px: Uint8Array.from(s.d, v => v < 0 ? 255 : v) })

// An indexed PNG, as assets/logo/metal-chrome.js writes them: 4 bits a pixel and the 16
// colours for an opaque image, else 8 bits and a 17th, transparent entry for index 255.
function png (img) {
  const zlib = require('zlib'); const clear = img.px.includes(255); const depth = clear ? 8 : 4
  const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0 })
  const crc = b => { let c = 0xffffffff; for (const v of b) c = crcTable[(c ^ v) & 255] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0 }
  const chunk = (type, data) => { const t = Buffer.from(type, 'ascii'), len = Buffer.alloc(4), sum = Buffer.alloc(4); len.writeUInt32BE(data.length); sum.writeUInt32BE(crc(Buffer.concat([t, data]))); return Buffer.concat([len, t, data, sum]) }
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(img.w, 0); ihdr.writeUInt32BE(img.h, 4); ihdr[8] = depth; ihdr[9] = 3
  const rgb = PAL.map(c => [c >> 16, (c >> 8) & 255, c & 255]); if (clear) rgb.push([0, 0, 0])
  const plte = Buffer.from(rgb.flat()); const stride = Math.ceil(img.w * depth / 8); const raw = Buffer.alloc((stride + 1) * img.h)
  for (let y = 0; y < img.h; y++) for (let x = 0; x < img.w; x++) { const v = img.px[y * img.w + x] === 255 ? 16 : img.px[y * img.w + x], o = y * (stride + 1) + 1; if (depth === 8) raw[o + x] = v; else raw[o + (x >> 1)] |= x & 1 ? v : v << 4 }
  const parts = [Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('PLTE', plte)]
  if (clear) { const t = Buffer.alloc(17, 255); t[16] = 0; parts.push(chunk('tRNS', t)) }
  parts.push(chunk('IDAT', zlib.deflateSync(raw, { level: 9 })), chunk('IEND', Buffer.alloc(0))); return Buffer.concat(parts)
}

// Plays the vignette to the arcade's pattern (124 BPM, 40 ms frames, as the page's tick does)
// and calls see(p, st, c) after each frame is drawn.
function play (frames, see) {
  const v = RITE.make(), p = new Uint8Array(W * H), stepSec = 60 / 124 / 4
  let pos = 0
  const ctx = frame => ({ pos, bpm: 124, cutoff: cutoffAt(pos), frame })
  for (let frame = 0; frame < frames;) {
    const from = Math.floor(pos)
    pos += 0.04 / stepSec
    for (let i = from + 1; i <= Math.floor(pos); i++) v.step(patternStep(i), ctx(frame))
    frame++
    const c = ctx(frame)
    v.layers(p, c)
    for (const s of v.stages) s[2](p, c)
    see(p, v.st, c)
  }
  return v
}

// Six frames of the vignette: the duel, a parry, the spinning attack, the sword in flight,
// the sword in the floor and the fighter pulling it free.
function previewFrames () {
  const want = [
    ['GUARD', st => st.pose[0].startsWith('guard') && st.pose[1].startsWith('guard') && !st.sword],
    ['CHOP AND PARRY', st => st.spark > 1 && st.pose.includes('block')],
    ['SPINNING ATTACK', st => st.spin > 0 && st.pose[st.spinWho] === 'spinB'],
    ['DISARMED', st => st.sword && st.sword.n > st.sword.T * 0.4 && st.sword.n < st.sword.T],
    ['SWORD IN THE FLOOR', st => st.sword && st.sword.n >= st.sword.T && st.pose[1].startsWith('guard')],
    ['PULLED FREE', st => st.pose[1] === 'reach' && st.t[1] < 5]
  ]
  const got = want.map(() => null)
  let loop = 0
  play(64 * 3 * 4, (p, st, c) => {
    loop = Math.floor(c.pos / 64)
    want.forEach(([, ok], i) => { if (!got[i] && loop >= [0, 0, 1, 1, 1, 1][i] && ok(st)) got[i] = p.slice() })
  })
  got.forEach((g, i) => { if (!g) throw new Error(`no frame for ${want[i][0]}`) })
  return want.map(([name], i) => [name, got[i]])
}

// Frames side by side, `cols` a row, each with its caption under it, on black.
function montage (frames, cols, scale) {
  const gap = 8, cap = 16, rows = Math.ceil(frames.length / cols)
  const w = gap + cols * (W * scale + gap), h = gap + rows * (H * scale + cap + gap)
  const px2 = new Uint8Array(w * h)
  const label = new Uint8Array(W * H)
  frames.forEach(([name, f], i) => {
    const X = gap + (i % cols) * (W * scale + gap), Y = gap + Math.floor(i / cols) * (H * scale + cap + gap)
    for (let y = 0; y < H * scale; y++) for (let x = 0; x < W * scale; x++) px2[(Y + y) * w + X + x] = f[Math.floor(y / scale) * W + Math.floor(x / scale)]
    label.fill(BLACK); text(label, name, 0, 0, LGREY)
    for (let y = 0; y < 10; y++) for (let x = 0; x < W; x++) if (label[y * W + x] !== BLACK) px2[(Y + H * scale + 4 + y) * w + X + x] = label[y * W + x]
  })
  return { w, h, px: px2 }
}

// Every exported sprite on a sheet at `scale`, on blue so the outlines show, with names.
function spriteSheet (named, scale) {
  const cell = 140 * scale, cap = 14, cols = 6, gap = 6
  const rows = Math.ceil(named.length / cols)
  const w = gap + cols * (cell + gap), h = gap + rows * (cell + cap + gap)
  const out = new Uint8Array(w * h).fill(BLUE)
  const label = new Uint8Array(W * H)
  named.forEach(([name, s], i) => {
    const X = gap + (i % cols) * (cell + gap), Y = gap + Math.floor(i / cols) * (cell + cap + gap)
    const sw = s.w * s.gx * scale, sh = s.h * scale, ox = X + Math.floor((cell - sw) / 2), oy = Y + cell - sh
    for (let y = 0; y < sh; y++) for (let x = 0; x < sw; x++) { const v = s.d[Math.floor(y / scale) * s.w + Math.floor(x / scale / s.gx)]; if (v >= 0) out[(oy + y) * w + ox + x] = v }
    label.fill(BLUE); text(label, name.toUpperCase(), 0, 0, WHITE)
    for (let y = 0; y < 10; y++) for (let x = 0; x < Math.min(W, cell); x++) if (label[y * W + x] !== BLUE) out[(Y + cell + 2 + y) * w + X + x] = label[y * W + x]
  })
  return { w, h, px: out }
}

// The status bar, both meters, at a few staminas, scaled up: the prototype's pips above
// (drawn as the page drew them) and the blade meter below.
function hudSheet (scale) {
  const rows = [[8, 8], [6, 3], [1, 0]], bar = 16
  const w = W * scale, h = rows.length * 2 * (bar + 4) * scale
  const out = new Uint8Array(w * h).fill(DGREY)
  const p = new Uint8Array(W * H)
  rows.forEach(([a, b], r) => [0, 1].forEach(kind => {
    p.fill(BLACK, 0, 16 * W)
    if (kind === 0) for (let k = 0; k < 8; k++) { rect(p, 6 + k * 9, 5, 7, 6, k < a ? (k < 2 ? RED : YELLOW) : DGREY); rect(p, W - 13 - k * 9, 5, 7, 6, k < b ? (k < 2 ? RED : YELLOW) : DGREY) } else { bladeMeter(p, 0, a); bladeMeter(p, 1, b) }
    text(p, 'VS', 112, 3, LRED); rect(p, 0, 15, W, 1, RED)
    const Y = (r * 2 + kind) * (bar + 4) * scale
    for (let y = 0; y < bar * scale; y++) for (let x = 0; x < w; x++) out[(Y + y) * w + x] = p[Math.floor(y / scale) * W + Math.floor(x / scale)]
  }))
  return { w, h, px: out }
}

// Frames for the compositor's goldens (tests/golden/scene/, M2f Task 3): frames of the
// vignette playing the arcade's pattern as the layers the compositor stacks, each a PNG and
// a list of its layers. A frame here is the background (the status bar's rows black, the sky
// a palette a cell row, the arena) and what the sprites stage drew over it (shadows, fighters,
// swords, sparks, the arc), recorded by wrapping put, rect and px while that stage runs; the
// torches' cycling and the status bar are left out (they are the device's code, M2f Task 4).
function goldenFrames (named) {
  const names = new Map(named.map(([n, s]) => [s, n]))
  const want = [
    ['guard', st => st.pose[0].startsWith('guard') && st.pose[1].startsWith('guard') && !st.sword],
    ['parry', st => st.spark > 1 && st.pose.includes('block')],
    ['spin', st => st.spin > 0 && st.pose[st.spinWho] === 'spinB'],
    ['flight', st => st.sword && st.sword.n > st.sword.T * 0.4 && st.sword.n < st.sword.T],
    ['stuck', st => st.sword && st.sword.n >= st.sword.T && st.unarmed >= 0 && st.pose[st.unarmed].startsWith('guard')],
    ['reach', st => st.unarmed >= 0 && st.pose[st.unarmed] === 'reach' && st.t[st.unarmed] < 5],
    ['wipe', st => new Set(st.rowPal.subarray(2, 17)).size > 1]
  ]
  let rec = null
  const [put0, rect0, px0] = [put, rect, px]
  put = (p, sp, x, y, o) => { if (rec) rec.push(`image ${names.get(sp)} ${Math.round(x)} ${Math.round(y)} ${o && o.fx ? 1 : 0}`); put0(p, sp, x, y, o) }
  rect = (p, x, y, w, h, c) => { if (rec) rec.push(`fill ${Math.round(x)} ${Math.round(y)} ${w} ${h} ${c}`); rect0(p, x, y, w, h, c) }
  px = (p, x, y, c) => { const X = Math.round(x), Y = Math.round(y); if (rec && X >= 0 && X < W && Y >= 0 && Y < H) rec.push(`mark ${X} ${Y} ${c}`); px0(p, x, y, c) }
  const v = RITE.make(), p = new Uint8Array(W * H), stepSec = 60 / 124 / 4
  const got = want.map(() => null)
  let pos = 0
  for (let frame = 0; frame < 64 * 3 * 4 && got.includes(null);) {
    const from = Math.floor(pos)
    pos += 0.04 / stepSec
    for (let i = from + 1; i <= Math.floor(pos); i++) v.step(patternStep(i), { pos, bpm: 124, cutoff: cutoffAt(pos), frame })
    frame++
    const c = { pos, bpm: 124, cutoff: cutoffAt(pos), frame }
    v.layers(p, c)
    const q = p.slice(0, W * H)
    q.fill(BLACK, 0, 16 * W)
    v.stages[0][2](p, c)
    const calls = []
    rec = calls
    v.stages[1][2](p, c)
    rec = null
    const k = want.findIndex(([, ok], i) => !got[i] && ok(v.st))
    if (k < 0) continue
    // The background, and the sprites stage's calls again over it.
    for (const line of calls) {
      const [op, ...a] = line.split(' ')
      if (op === 'image') put0(q, named.find(([n]) => n === a[0])[1], +a[1], +a[2], { fx: a[3] === '1' })
      else if (op === 'fill') rect0(q, +a[0], +a[1], +a[2], +a[3], +a[4])
      else px0(q, +a[0], +a[1], +a[2])
    }
    const text = [`# RED MOON RITE, frame ${frame} of the arcade's pattern: ${want[k][0]}`,
      `sky ${Array.from(v.st.rowPal).join(' ')}`, ...calls].join('\n') + '\n'
    got[k] = [want[k][0], { w: W, h: H, px: q }, text]
  }
  ;[put, rect, px] = [put0, rect0, px0]
  got.forEach((g, i) => { if (!g) throw new Error(`no golden frame for ${want[i][0]}`) })
  return got
}

if (typeof module !== 'undefined' && require.main === module) {
  const fs = require('fs'), path = require('path')
  const args = process.argv.slice(2), ri = args.indexOf('--review')
  const review = ri >= 0 ? args.splice(ri, 2)[1] : null
  const fi = args.indexOf('--frames')
  const golden = fi >= 0 ? args.splice(fi, 2)[1] : null
  const dir = args[0] || __dirname
  const v = RITE.make()
  const out = {}
  v.art.bgs.forEach((b, i) => { out[`sky-${i}.png`] = { w: W, h: 124, px: b.slice(16 * W, 140 * W) } })
  out['arena.png'] = { w: W, h: H, px: v.art.fg.slice() }
  const sprites = []
  for (const who of [0, 1]) for (const k in DUEL) {
    if (k === 'reach') continue
    sprites.push([`barb-${who}-${k}`, barbSpr(who + k, who, k, false)])
  }
  for (const who of [0, 1]) for (const k of ['guard', 'guard2', 'hit', 'reach']) sprites.push([`barb-${who}-${k}-unarmed`, barbSpr(who + k + 'u', who, k, true)])
  SWORDS.forEach((s, k) => sprites.push([`sword-${k}`, s]))
  sprites.push(['sword-stuck', STUCK])
  for (const [name, s] of sprites) out[`${name}.png`] = spriteImage(s)
  const mask = new Uint8Array(W * H).fill(255)
  for (const [x, y] of v.art.flameMask) mask[y * W + x] = WHITE
  out['flame-mask.png'] = { w: W, h: H, px: mask }
  const frames = previewFrames()
  out['red-moon-rite-preview.png'] = montage(frames, 3, 1)
  fs.mkdirSync(dir, { recursive: true })
  for (const [name, img] of Object.entries(out)) fs.writeFileSync(path.join(dir, name), png(img))
  console.log(`${Object.keys(out).length} PNGs in ${dir}`)
  if (golden) {
    fs.mkdirSync(golden, { recursive: true })
    for (const [name, img, text] of goldenFrames(sprites)) {
      fs.writeFileSync(path.join(golden, `rmr-${name}.png`), png(img))
      fs.writeFileSync(path.join(golden, `rmr-${name}.txt`), text)
    }
    console.log(`golden frames in ${golden}`)
  }
  if (review) {
    fs.mkdirSync(review, { recursive: true })
    const sheets = {
      'rmr-preview-2x.png': montage(frames, 3, 2),
      'rmr-sprites-2x.png': spriteSheet(sprites, 2),
      'rmr-hud-4x.png': hudSheet(4)
    }
    for (const [name, img] of Object.entries(sheets)) fs.writeFileSync(path.join(review, name), png(img))
    console.log(`${Object.keys(sheets).length} review sheets in ${review}`)
  }
}
