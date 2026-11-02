// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "psid_lift.h"

#include <string.h>

#include "player/gt2_play.h"
#include "song/gt2_song.h"

#define KEEP 0xffu
#define ROWS PSID_LIFT_ROWS
enum { PLAIN, NOTE, CMD };
enum { STEP_WAVE, STEP_FIT };

typedef struct {
    psid_lift* l;
    uint32_t n;
    uint32_t pos;
    uint32_t rows;
} build;

#define VOICE_REGS 7u
#define SHARED 0x15u
#define CONTROL 4u
#define GATE 1u
#define EDGE PSID_LIFT_CHIP_REGS
#define BUSY (PSID_LIFT_CHIP_REGS + 1u)
#define CHANGED(v) (8u << (v))
#define SHARED_BUSY 8u

void psid_lift_init(psid_lift* l, psid_lift_past* past, uint32_t chips)
{
    memset(l, 0, sizeof *l);
    l->past = past;
    l->chips = (uint8_t)(chips == 2u ? 2u : 1u);
    psid_lift_forget(l);
}

void psid_lift_forget(psid_lift* l)
{
    l->past->head = l->past->filled = 0;
}

void psid_lift_write(psid_lift* l, uint32_t chip, uint32_t reg, uint32_t value)
{
    if (chip >= l->chips || reg >= PSID_LIFT_CHIP_REGS)
        return;
    uint32_t old = l->regs[chip][reg], v = reg / VOICE_REGS;
    l->regs[chip][reg] = (uint8_t)value;
    if (l->count[chip][reg] < 255u)
        l->count[chip][reg]++;
    if (reg < SHARED && reg % VOICE_REGS == CONTROL && ((old ^ value) & GATE))
        l->edge[chip] |= (uint8_t)(CHANGED(v) | (value & GATE) << v);
}

static uint32_t busy(const uint8_t* count)
{
    uint32_t b = 0;
    for (uint32_t r = 0; r < PSID_LIFT_CHIP_REGS; r++)
        if (count[r] > PSID_LIFT_DIGI)
            b |= r < SHARED ? 1u << (r / VOICE_REGS) : SHARED_BUSY;
    return b;
}

static void record(psid_lift* l, uint32_t chip)
{
    psid_lift_past* p = l->past;
    uint8_t* r = p->rec[p->head];
    memcpy(r, l->regs[chip], PSID_LIFT_CHIP_REGS);
    r[EDGE] = l->edge[chip];
    r[BUSY] = (uint8_t)busy(l->count[chip]);
    l->edge[chip] = 0;
    memset(l->count[chip], 0, PSID_LIFT_CHIP_REGS);
    p->head = (uint8_t)((p->head + 1u) % PSID_LIFT_PAST);
    if (p->filled < PSID_LIFT_PAST)
        p->filled++;
}

static uint32_t calls_kept(const psid_lift* l)
{
    return l->past->filled / l->chips;
}

static const uint8_t* at(const psid_lift* l, uint32_t back, uint32_t chip)
{
    uint32_t n = back * l->chips + (l->chips - chip);
    return l->past->rec[(l->past->head + PSID_LIFT_PAST - n) % PSID_LIFT_PAST];
}

static int starts(const psid_lift* l, const uint8_t* r)
{
    return (r[EDGE] >> l->voice & 1u) && (r[l->voice * VOICE_REGS + CONTROL] & GATE);
}

static int keep(psid_lift* l, const uint8_t* r)
{
    uint8_t* f = l->frame[l->frames++];
    memcpy(f, r + l->voice * VOICE_REGS, VOICE_REGS);
    memcpy(f + VOICE_REGS, r + SHARED, PSID_LIFT_REGS - VOICE_REGS);
    l->digi |= (r[BUSY] & (1u << l->voice | SHARED_BUSY)) != 0;
    return l->frames < PSID_LIFT_FRAMES;
}

static void follow(psid_lift* l, uint32_t back)
{
    const uint8_t* r = at(l, back, l->chip);
    if (l->frames && (r[EDGE] & CHANGED(l->voice))) {
        l->state = PSID_LIFT_TAKEN;
        return;
    }
    if (!keep(l, r))
        l->state = PSID_LIFT_TAKEN;
}

static void begin(psid_lift* l, uint32_t back, uint32_t chip)
{
    const uint8_t* r = at(l, back, chip);
    const uint8_t* b = back + 1u < calls_kept(l) ? at(l, back + 1u, chip) : NULL;
    uint32_t f = l->voice * VOICE_REGS;
    l->chip = (uint8_t)chip;
    l->frames = l->digi = l->cut = l->step = 0;
    l->before[0] = (uint8_t)(b ? b[f] : ~r[f]);
    l->before[1] = (uint8_t)(b ? b[f + 1u] : ~r[f + 1u]);
    l->state = PSID_LIFT_TAKING;
    for (uint32_t k = back + 1u; k-- > 0u && l->state == PSID_LIFT_TAKING;)
        follow(l, k);
}

static int find(psid_lift* l, uint32_t calls)
{
    for (uint32_t back = 0; back < calls; back++)
        for (uint32_t chip = 0; chip < l->chips; chip++)
            if (starts(l, at(l, back, chip))) {
                begin(l, back, chip);
                return 1;
            }
    return 0;
}

uint32_t psid_lift_take(psid_lift* l, uint32_t voice)
{
    l->state = PSID_LIFT_OFF;
    if (voice >= PSID_LIFT_NO_VOICE)
        return l->state;
    l->voice = (uint8_t)voice;
    if (!find(l, calls_kept(l)))
        l->state = PSID_LIFT_ARMED;
    return l->state;
}

void psid_lift_call(psid_lift* l)
{
    for (uint32_t chip = 0; chip < l->chips; chip++)
        record(l, chip);
    if (l->state == PSID_LIFT_ARMED)
        (void)find(l, 1u);
    else if (l->state == PSID_LIFT_TAKING)
        follow(l, 0u);
}

void psid_lift_stop(psid_lift* l)
{
    l->state = l->state == PSID_LIFT_TAKING || l->state == PSID_LIFT_TAKEN ? PSID_LIFT_TAKEN
                                                                          : PSID_LIFT_OFF;
}

static uint32_t note_freq(uint32_t n)
{
    return gt2_freq_lo[n] | (uint32_t)gt2_freq_hi[n] << 8;
}

static uint32_t pal_freq(const uint8_t* f, int ntsc)
{
    uint32_t x = f[0] | (uint32_t)f[1] << 8;
    return ntsc ? x + (x * 37479u + 492624u) / 985248u : x;
}

static uint32_t nearest(uint32_t f)
{
    uint32_t lo = 0, hi = GT2_NOTES - 1u;
    while (lo < hi) {
        uint32_t m = (lo + hi) / 2u;
        if (note_freq(m) < f)
            lo = m + 1u;
        else
            hi = m;
    }
    if (lo && 2u * f < note_freq(lo - 1u) + note_freq(lo))
        lo--;
    return lo;
}

static void row(build* b, uint32_t left, uint32_t right)
{
    if (b->rows < ROWS) {
        b->l->row[0][b->rows] = (uint8_t)left;
        b->l->row[1][b->rows] = (uint8_t)right;
    }
    b->rows++;
}

static int put_table(build* b, int end)
{
    uint8_t* f = b->l->file;
    if (end && b->rows)
        row(b, 0xffu, 0u);
    uint32_t n = b->rows;
    if (n >= ROWS || b->pos + 1u + 2u * n > PSID_LIFT_FILE_MAX)
        return 0;
    f[b->pos++] = (uint8_t)n;
    memcpy(f + b->pos, b->l->row[0], n);
    memcpy(f + b->pos + n, b->l->row[1], n);
    b->pos += 2u * n;
    b->rows = 0;
    return 1;
}

static void idle(build* b, uint32_t m)
{
    while (m) {
        uint32_t take = m < 16u ? m : 16u;
        row(b, take - 1u, 0x80u);
        m -= take;
    }
}

static uint32_t note_byte(uint32_t note, uint32_t base)
{
    int32_t r = (int32_t)note - (int32_t)base;
    if (r >= 0 && r <= 0x5f)
        return (uint32_t)r;
    if (r < 0 && r >= -32)
        return (uint32_t)r & 0x7fu;
    return 0x80u + (note ? note : 1u);
}

static void wave_rows(build* b, uint32_t base)
{
    const psid_lift* l = b->l;
    uint32_t ctrl = l->frame[0][PSID_LIFT_CTRL], note = KEEP, m = 0;
    uint32_t ad = l->frame[0][PSID_LIFT_AD], sr = l->frame[0][PSID_LIFT_SR];
    for (uint32_t k = 1; k < b->n; k++) {
        const uint8_t* f = l->frame[k];
        uint32_t want = b->l->note[k];
        b->l->kind[k] = PLAIN;
        if (f[PSID_LIFT_AD] != ad || f[PSID_LIFT_SR] != sr) {
            idle(b, m);
            m = 0;
            int a = f[PSID_LIFT_AD] != ad;
            ad = f[PSID_LIFT_AD];
            if (!a)
                sr = f[PSID_LIFT_SR];
            row(b, a ? 0xf5u : 0xf6u, a ? ad : sr);
            b->l->kind[k] = CMD;
            continue;
        }
        int w = f[PSID_LIFT_CTRL] != ctrl, n = want != KEEP && want != note;
        if (!w && !n) {
            m++;
            continue;
        }
        idle(b, m);
        m = 0;
        uint32_t c = f[PSID_LIFT_CTRL];
        uint32_t left = c < 0x10u || c >= 0xe0u ? 0xe0u | (c & 0x0fu) : c;
        row(b, w ? left : 0u, n ? note_byte(want, base) : 0x80u);
        ctrl = c;
        if (n) {
            note = want;
            b->l->kind[k] = NOTE;
        }
    }
}

static uint32_t vibrato_error(const build* b, int ntsc, uint32_t speed, uint32_t shift,
                              uint32_t delay, uint32_t best)
{
    uint32_t freq = 0, note = 0, time = 0, err = 0;
    for (uint32_t k = 1; k < b->n && err < best; k++) {
        if (b->l->kind[k] == NOTE) {
            note = b->l->note[k];
            freq = note_freq(note);
            time = 0;
        } else if (b->l->kind[k] == PLAIN && note + 1u < GT2_NOTES) {
            if (delay) {
                delay--;
            } else {
                uint32_t t = ((note_freq(note + 1u) - note_freq(note)) & 0xffffu) >> shift;
                uint32_t a = time;
                if (!(a & 0x80u) && a > speed)
                    a = ~a & 0xffu;
                time = (a + 2u) & 0xffu;
                freq = (time & 1u ? freq - t : freq + t) & 0xffffu;
            }
        }
        uint32_t want = pal_freq(b->l->frame[k], ntsc);
        err += freq > want ? freq - want : want - freq;
    }
    return err;
}

static uint32_t width(const uint8_t* f)
{
    return (f[PSID_LIFT_PW_HI] & 0x0fu) << 8 | f[PSID_LIFT_PW_LO];
}

static void runs(build* b, int pulse)
{
    const psid_lift* l = b->l;
    int32_t speed = 0;
    uint32_t count = 0;
    for (uint32_t k = 1; k < b->n; k++) {
        const uint8_t* f = l->frame[k];
        const uint8_t* p = l->frame[k - 1u];
        uint32_t v = pulse ? width(f) : f[PSID_LIFT_FC_HI];
        int32_t d = pulse ? (int32_t)((width(f) - width(p)) & 0xfffu)
                          : (int32_t)((f[PSID_LIFT_FC_HI] - p[PSID_LIFT_FC_HI]) & 0xffu);
        d -= pulse ? (d >= 0x800 ? 0x1000 : 0) : (d >= 0x80 ? 0x100 : 0);
        uint32_t mode = f[PSID_LIFT_MODE] & 0x70u;
        uint32_t res = (f[PSID_LIFT_RES] & 0xf0u) | 1u << l->voice;
        int set = k == 1u || (pulse ? d > 127 || d < -127
                                    : mode != (p[PSID_LIFT_MODE] & 0x70u) ||
                                          (f[PSID_LIFT_RES] & 0xf0u) != (p[PSID_LIFT_RES] & 0xf0u));
        if (!set && d == speed && count && count < 127u) {
            count++;
            continue;
        }
        if (count)
            row(b, count, (uint32_t)speed & 0xffu);
        count = 0;
        if (set) {
            if (pulse) {
                row(b, 0x80u | v >> 8, v & 0xffu);
            } else {
                row(b, 0x80u | mode, res);
                row(b, 0u, v);
            }
        } else {
            speed = d;
            count = 1;
        }
    }
    if (count && speed)
        row(b, count, (uint32_t)speed & 0xffu);
}

static int wave_step(build* b, int ntsc)
{
    psid_lift* l = b->l;
    uint32_t n = b->n;
    memset(l->file, 0, 4u + GT2_INSTR_BYTES);
    memcpy(l->file, "GTI5", 4);
    b->pos = 4u + GT2_INSTR_BYTES;
    b->rows = 0;
    uint32_t f0 = l->frame[0][0] | (uint32_t)l->frame[0][1] << 8;
    uint32_t base = nearest(pal_freq(l->frame[f0 == (l->before[0] | (uint32_t)l->before[1] << 8)], ntsc));
    l->base = (uint8_t)base;
    uint32_t turns = 0, off = 0, k0 = 0, first = 0, last = 0;
    int32_t dir = 0;
    for (uint32_t k = 1; k < n; k++) {
        uint32_t f = pal_freq(l->frame[k], ntsc), q = nearest(f);
        int32_t d = (int32_t)f - (int32_t)pal_freq(l->frame[k - 1u], ntsc);
        l->note[k] = (uint8_t)q;
        if (f != note_freq(q)) {
            off++;
            k0 = k0 ? k0 : k;
        }
        if (d && k > 1u) {
            int32_t nd = d > 0 ? 1 : -1;
            if (dir && nd != dir) {
                first = first ? first : k;
                last = k;
                turns++;
            }
            dir = nd;
        }
    }
    uint32_t centre = 0, s = 0;
    if (turns >= 3u && off >= 3u) {
        uint32_t sum = 0;
        for (uint32_t k = k0; k < n; k++)
            sum += pal_freq(l->frame[k], ntsc);
        centre = nearest(sum / (n - k0));
        s = k0 > 1u ? k0 - 1u : 1u;
        for (uint32_t k = s; k < n; k++)
            if (l->note[k] + 1u < centre || l->note[k] > centre + 1u)
                s = 0;
        for (uint32_t k = s ? s : n; k < n; k++)
            l->note[k] = (uint8_t)(k == s ? centre : KEEP);
    }
    wave_rows(b, base);
    l->fit_next = 1u;
    l->fit_last = 0u;
    if (!put_table(b, 1))
        return 0;
    if (s && centre + 1u < GT2_NOTES) {
        uint32_t h = (last - first) / (turns - 1u), delay = 0;
        for (uint32_t k = 1; k < k0 && delay < 254u; k++)
            delay += l->kind[k] == PLAIN;
        l->centre = (uint8_t)centre;
        l->start = (uint8_t)s;
        l->delay = (uint8_t)delay;
        l->best = 0xffffffffu;
        l->fit_speed = l->fit_shift = 0;
        l->fit_next = (uint8_t)(h > 3u ? h - 3u : 0u);
        l->fit_last = (uint8_t)(h < 0x7fu ? h : 0x7fu);
    }
    return 1;
}

static void fit_step(build* b, int ntsc)
{
    psid_lift* l = b->l;
    uint32_t sp = l->fit_next++;
    for (uint32_t sh = 0; sh < 8u; sh++) {
        uint32_t e = vibrato_error(b, ntsc, sp, sh, l->delay, l->best);
        if (e < l->best) {
            l->best = e;
            l->fit_speed = (uint8_t)sp;
            l->fit_shift = (uint8_t)sh;
        }
    }
    if (l->fit_next <= l->fit_last)
        return;
    uint32_t held = 0, c = note_freq(l->centre);
    for (uint32_t k = l->start + 1u; k < b->n; k++) {
        uint32_t f = pal_freq(l->frame[k], ntsc);
        held += f > c ? f - c : c - f;
    }
    l->flags |= 2u * l->best <= held ? PSID_LIFT_F_VIBRATO : PSID_LIFT_F_NO_VIBRATO;
}

static uint32_t tables_step(build* b, const char* name)
{
    psid_lift* l = b->l;
    uint32_t n = b->n, voice_bit = 1u << l->voice;
    uint8_t* ins = l->file + 4;
    uint32_t pulse = 0, filter = 0;
    b->rows = 0;
    for (uint32_t k = 0; k < n; k++) {
        pulse |= l->frame[k][PSID_LIFT_CTRL] & 0x40u;
        filter |= l->frame[k][PSID_LIFT_RES] & voice_bit;
    }
    if (pulse)
        runs(b, 1);
    if (!put_table(b, 1))
        return 0;
    if (filter) {
        runs(b, 0);
        l->flags |= PSID_LIFT_F_FILTER;
    }
    if (!put_table(b, 1))
        return 0;
    int vibrato = (l->flags & PSID_LIFT_F_VIBRATO) != 0;
    if (vibrato)
        row(b, 0x80u | l->fit_speed, l->fit_shift);
    if (!put_table(b, 0))
        return 0;
    uint32_t c0 = l->frame[0][PSID_LIFT_CTRL];
    ins[GT2_INS_AD] = l->frame[0][PSID_LIFT_AD];
    ins[GT2_INS_SR] = l->frame[0][PSID_LIFT_SR];
    ins[GT2_INS_WAVE] = 1;
    ins[GT2_INS_PULSE] = (uint8_t)(pulse ? 1u : 0u);
    ins[GT2_INS_FILTER] = (uint8_t)(filter ? 1u : 0u);
    ins[GT2_INS_VIBPARAM] = (uint8_t)(vibrato ? 1u : 0u);
    ins[GT2_INS_VIBDELAY] = (uint8_t)(vibrato ? l->delay + 1u : 0u);
    ins[GT2_INS_GATETIMER] = 2;
    ins[GT2_INS_FIRSTWAVE] = (uint8_t)(c0 >= 0xfeu ? 0xf1u : c0);
    for (uint32_t i = 0; i < GT2_INSTR_NAME && name[i]; i++)
        ins[GT2_INS_NAME + i] = (uint8_t)name[i];
    return b->pos;
}

static int cut(psid_lift* l)
{
    uint32_t c = l->frames / 8u;
    l->frames = (uint8_t)(l->frames - (c ? c : 1u));
    l->cut = 1;
    l->step = STEP_WAVE;
    return l->frames > 1u ? PSID_LIFT_AGAIN : PSID_LIFT_E_EMPTY;
}

int psid_lift_convert(psid_lift* l, int ntsc, const char* name, uint32_t* flags)
{
    if (l->step == STEP_WAVE) {
        l->flags = l->cut ? PSID_LIFT_F_SHORT : 0u;
        if (l->digi)
            return PSID_LIFT_E_DIGI;
        if (!l->frames)
            return PSID_LIFT_E_EMPTY;
        if (l->frames == 1u) {
            memcpy(l->frame[1], l->frame[0], PSID_LIFT_REGS);
            l->frames = 2;
        }
    }
    build b = {l, l->frames, l->pos, 0};
    uint32_t len = 0;
    if (l->step == STEP_WAVE) {
        if (!wave_step(&b, ntsc))
            return cut(l);
        l->step = STEP_FIT;
    } else if (l->step == STEP_FIT && l->fit_next <= l->fit_last) {
        fit_step(&b, ntsc);
    } else {
        len = tables_step(&b, name);
        if (!len)
            return cut(l);
    }
    l->pos = (uint16_t)b.pos;
    *flags = l->flags;
    if (!len)
        return PSID_LIFT_AGAIN;
    l->step = STEP_WAVE;
    return (int)len;
}
