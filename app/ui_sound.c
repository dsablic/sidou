// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "ui_sound.h"

#include <stddef.h>

/* GoatTracker's sound effects (readme 6.3): AD, SR, the pulse byte, frames, $00. Notes are
 * $80 + the note from C-0 (D-0 $82 .. B-7 $DF); $11 triangle, $21 sawtooth, $41 pulse, $81
 * noise, each with the gate. */
static const uint8_t BLIP[] = {0x00, 0xa2, 0x08, 0x11, 0xc8, 0xc8, 0x00};
static const uint8_t CONFIRM[] = {0x00, 0xa3, 0x08, 0x41, 0xc0, 0xc4, 0xc7, 0x00};
static const uint8_t CHIME[] = {0x01, 0x94, 0x08, 0x11, 0xcc, 0xc8, 0xc3, 0xc3, 0x00};
static const uint8_t BUZZ[] = {0x00, 0xb3, 0x02, 0x41, 0x98, 0x98, 0x97, 0x97, 0x00};
static const uint8_t CHUNK[] = {0x00, 0x83, 0x08, 0x81, 0xa4, 0x41, 0x90, 0x90, 0x00};
static const uint8_t* const SOUNDS[UI_SOUND_COUNT] = {BLIP, CONFIRM, CHIME, BUZZ, CHUNK};

/* The SID's release (and decay) times, in ms, by rate. */
static const uint16_t RELEASE_MS[16] = {6,   24,  48,   72,   114,  168,  204,   240,
                                        300, 750, 1500, 2400, 3000, 9000, 15000, 24000};

#define REG_RES_FILT 0x17u
#define REG_MODE_VOL 0x18u
#define TICK_MS (1000u / LIVE_TICK_HZ)

const uint8_t* ui_sound_data(ui_sound_id id) {
    return (uint32_t)id < UI_SOUND_COUNT ? SOUNDS[id] : NULL;
}

static uint16_t release_ticks(uint8_t sr) {
    return (uint16_t)(RELEASE_MS[sr & 15u] / TICK_MS + 1u);
}

void ui_sound_init(ui_sound* us, live* l, live_sink_fn sink, void* sink_ctx) {
    us->l = l;
    us->sink = sink;
    us->sink_ctx = sink_ctx;
    us->on = 0;
    us->hush = 0;
    us->id = UI_SOUND_NONE;
    us->voice = UI_SOUND_NONE;
    us->pos = 0;
    us->wave = 0;
    us->phase = 0;
    us->keys = 0;
    us->tail = 0;
    for (uint32_t v = 0; v < LIVE_VOICES; v++) {
        us->free_in[v] = 0;
        us->stamp[v] = l->voice[v].stamp;
    }
    us->live_sink = NULL;
    us->live_ctx = NULL;
}

static int keys_on_b(const live* l) {
    return l->two_chips && l->keys_chip == 1u;
}

static void put(ui_sound* us, uint8_t reg, uint8_t value) {
    us->sink(us->sink_ctx, reg, value);
}

/* live's sink while a sound holds a voice of chip B, the keys' chip: live's writes to that
 * voice, and its filter routing bit, are held back. */
static void filter(void* ctx, uint8_t reg, uint8_t value) {
    ui_sound* us = ctx;
    uint8_t u = us->l->ui_voice;
    if (u) {
        uint32_t v = u - 1u;
        if (reg < REG_RES_FILT - 2u && reg / 7u == v) {
            return;
        }
        if (reg == REG_RES_FILT) {
            value = (uint8_t)(value & ~(1u << v));
        }
    }
    us->live_sink(us->live_ctx, reg, value);
}

/* The voice back to live (its registers, and its filter bit) and live's sink back. */
static void give_back(ui_sound* us) {
    live* l = us->l;
    if (us->live_sink) {
        if (l->sink == filter && l->sink_ctx == us) {
            l->sink = us->live_sink;
            l->sink_ctx = us->live_ctx;
        }
        us->live_sink = NULL;
        if (l->ui_voice == us->voice + 1u) {
            l->ui_voice = 0;
            live_rewrite_voice(l, us->voice);
        }
    }
    us->voice = UI_SOUND_NONE;
    us->id = UI_SOUND_NONE;
    us->tail = 0;
}

void ui_sound_set(ui_sound* us, int on) {
    us->l->settings_dirty = 1;
    us->on = on ? 1u : 0u;
    if (!us->on && us->voice != UI_SOUND_NONE) {
        give_back(us);
    }
}

void ui_sound_hush(ui_sound* us, int on) {
    us->hush = on ? 1u : 0u;
    if (us->hush && us->voice != UI_SOUND_NONE) {
        give_back(us);
    }
}

int ui_sound_busy(const ui_sound* us) {
    return us->voice != UI_SOUND_NONE;
}

/* A free voice of chip B for a sound (the header), or UI_SOUND_NONE. */
static uint8_t free_voice(const ui_sound* us) {
    const live* l = us->l;
    if (!keys_on_b(l)) {
        return 2; /* chip B is all free: voice 3 */
    }
    const live_preset* p = live_preset_get(l->preset);
    if (p && p->mono_sync) {
        return UI_SOUND_NONE;
    }
    uint8_t best = UI_SOUND_NONE;
    for (uint32_t v = 0; v < LIVE_VOICES; v++) {
        if (l->voice[v].gate || us->free_in[v]) {
            continue;
        }
        /* The newest released: live's allocator takes the oldest next. */
        if (best == UI_SOUND_NONE || (int32_t)(l->voice[v].stamp - l->voice[best].stamp) > 0) {
            best = (uint8_t)v;
        }
    }
    return best;
}

/* The sound's next frame: a waveform kept, then the note; $00 the end (the gate off, then its
 * release). */
static void step(ui_sound* us) {
    const uint8_t* d = SOUNDS[us->id];
    uint8_t v7 = (uint8_t)(us->voice * 7u);
    uint8_t b = us->pos < UI_SOUND_MAX_BYTES ? d[us->pos++] : 0u;
    if (b >= 0x01u && b <= 0x81u) {
        us->wave = b;
        b = us->pos < UI_SOUND_MAX_BYTES ? d[us->pos++] : 0u;
    }
    if (b < 0x82u || b > 0xdfu) { /* $00, or a frame with no note: the end */
        put(us, (uint8_t)(v7 + 4u), (uint8_t)(us->wave & ~1u));
        us->id = UI_SOUND_NONE;
        us->tail = release_ticks(d[1]);
        return;
    }
    uint16_t f = live_freq_table[(uint32_t)(b - 0x80u) + 12u]; /* C-0 is MIDI's 12 */
    put(us, v7, (uint8_t)f);
    put(us, (uint8_t)(v7 + 1u), (uint8_t)(f >> 8));
    put(us, (uint8_t)(v7 + 4u), us->wave);
}

void ui_sound_play(ui_sound* us, ui_sound_id id) {
    live* l = us->l;
    if (!us->on || us->hush || (uint32_t)id >= UI_SOUND_COUNT) {
        return;
    }
    if (us->voice == UI_SOUND_NONE) {
        uint8_t v = free_voice(us);
        if (v == UI_SOUND_NONE) {
            return;
        }
        us->voice = v;
        us->keys = l->keys_chip;
        if (keys_on_b(l)) {
            /* Hold the voice from live: its allocator skips it, its writes stop at the filter. */
            l->ui_voice = (uint8_t)(v + 1u);
            us->live_sink = l->sink;
            us->live_ctx = l->sink_ctx;
            l->sink = filter;
            l->sink_ctx = us;
            put(us, REG_RES_FILT,
                (uint8_t)((l->resonance << 4) | (l->filter_voices & ~(1u << v))));
        } else {
            put(us, REG_RES_FILT, 0); /* chip B is the sound's: no filter */
            put(us, REG_MODE_VOL, 0x0f);
        }
    }
    const uint8_t* d = SOUNDS[id];
    uint8_t v7 = (uint8_t)(us->voice * 7u);
    us->id = (uint8_t)id;
    us->pos = 3;
    us->wave = 0;
    us->phase = 0;
    us->tail = 0;
    put(us, (uint8_t)(v7 + 4u), 0); /* the gate off first: the sound's attack from its start */
    put(us, (uint8_t)(v7 + 5u), d[0]);
    put(us, (uint8_t)(v7 + 6u), d[1]);
    put(us, (uint8_t)(v7 + 2u), d[2]);
    put(us, (uint8_t)(v7 + 3u), d[2]);
    step(us);
}

void ui_sound_tick(ui_sound* us) {
    live* l = us->l;
    uint16_t rel = release_ticks(l->sustain_release);
    for (uint32_t v = 0; v < LIVE_VOICES; v++) {
        if (l->voice[v].gate || l->voice[v].stamp != us->stamp[v]) {
            us->free_in[v] = rel;
            us->stamp[v] = l->voice[v].stamp;
        } else if (us->free_in[v]) {
            us->free_in[v]--;
        }
    }
    if (us->voice == UI_SOUND_NONE) {
        return;
    }
    if (l->keys_chip != us->keys) {
        /* The keys moved: the voice is theirs (or the chip's new player's) now. */
        us->live_sink = NULL;
        l->ui_voice = 0;
        us->voice = UI_SOUND_NONE;
        us->id = UI_SOUND_NONE;
        us->tail = 0;
        return;
    }
    if (keys_on_b(l) && l->ui_voice != us->voice + 1u) {
        /* A note took the voice back: the sound stops, writing nothing more. */
        if (l->sink == filter && l->sink_ctx == us) {
            l->sink = us->live_sink;
            l->sink_ctx = us->live_ctx;
        }
        us->live_sink = NULL;
        us->voice = UI_SOUND_NONE;
        us->id = UI_SOUND_NONE;
        us->tail = 0;
        return;
    }
    if (us->id != UI_SOUND_NONE) {
        us->phase ^= 1u;
        if (!us->phase) {
            step(us);
        }
        return;
    }
    if (us->tail && --us->tail == 0u) {
        give_back(us);
    }
}
