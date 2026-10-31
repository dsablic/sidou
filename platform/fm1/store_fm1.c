// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "store_fm1.h"

#include <stdint.h>

#include "fm1_time.h"
#include "fm1_xip.h"

#include "audio.h"
#include "input.h"
#include "midi.h"
#include "store_layout.h"
#include "ui.h"

#define ALNK_CON2 (*(volatile uint8_t*)0x12E08u)
#define ALNK_HALF 0x80u

#define BOOT_POLLS 20000u
#define BOOT_WORK_MS 100u
#define WANT_MS 1000u

store store_fm1_store;
store_fm1_stats store_fm1_counts;
store_fm1_test store_fm1_hook;
volatile uint32_t store_fm1_window = STORE_WINDOW_BYTES;

struct {
    uint32_t now;
    uint32_t last_ms;
    uint32_t last_hold;
    uint32_t inputs;
    uint32_t input_ms;
    uint32_t wanted_at;
    uint32_t file_at;
    uint8_t wanted;
    uint8_t quiet;
    uint8_t force;
    uint8_t holding;
    uint8_t polled;
    uint8_t file;
    uint8_t sysex_turn;
    uint8_t silent;
    uint8_t programmed;
    uint8_t loud;
} store_fm1_state;
#define fm store_fm1_state

static void cli(void) {
    __asm__ volatile("cli" ::: "memory");
}

static void sti(void) {
    __asm__ volatile("csync\n\tsti" ::: "memory");
}

static const uint8_t* map(void* ctx, uint32_t off) {
    (void)ctx;
    return (const uint8_t*)FM1_XIP(off);
}

static void hold_on(void) {
    audio_hold(1);
    fm.holding = 1;
    fm.programmed = 0;
}

static void hold_off(void) {
    audio_hold(0);
    fm.holding = 0;
}

static int hold_ready(void) {
    if (!fm.quiet && !fm.force && !midi_sysex_writing) {
        return 0;
    }
    if (midi_sysex_writing && fm.loud && !fm.force) {
        if (fm.holding) {
            hold_off();
        }
        return 0;
    }
    if (midi_sysex_writing) {
        uint32_t gap = fm.holding ? STORE_PROGRAM_GAP_MS
                       : fm.silent ? STORE_HOLD_GAP_MS
                                   : STORE_SYSEX_GAP_MS;
        if (fm.now - fm.last_hold < gap) {
            return 0;
        }
        if (!fm.holding) {
            hold_on();
            return 0;
        }
        return audio_held();
    }
    if (!fm.holding) {
        hold_on();
        return 0;
    }
    uint32_t gap = fm.quiet ? STORE_HOLD_GAP_MS : STORE_FORCE_GAP_MS;
    return fm.now - fm.last_hold >= gap && audio_held();
}

static int operate(uint32_t off, const void* p, uint32_t n, int erase) {
    int rc, held = 0;
    uint32_t masked = fm1_ticks();
    if (!audio_running()) {
        cli();
    } else if (!erase && store_fm1_window) {
        cli();
        uint32_t since = fm1_ticks() - audio_block_end;
        if (since > (STORE_WINDOW_LEAD_US - STORE_WINDOW_ENTRY_US) * FM1_TICKS_PER_US ||
            (ALNK_CON2 & ALNK_HALF)) {
            sti();
            store_fm1_counts.window_later++;
            return STORE_LATER;
        }
    } else {
        if (!hold_ready()) {
            store_fm1_counts.hold_later++;
            return STORE_LATER;
        }
        held = 1;
        cli();
    }
    rc = erase ? flash_store_erase(off) : flash_store_prog(off, p, n);
    sti();
    midi_clock_held(masked, fm1_ticks());
    if (held) {
        fm.last_hold = fm.now;
        fm.programmed = 1;
        store_fm1_counts.holds++;
    }
    if (rc) {
        store_fm1_counts.errors++;
        return STORE_ERROR;
    }
    store_fm1_counts.progs += (uint32_t)!erase;
    store_fm1_counts.erases += (uint32_t)erase;
    return STORE_OK;
}

static int prog(void* ctx, uint32_t off, const void* p, uint32_t n) {
    (void)ctx;
    return operate(off, p, n, 0);
}

static int erase(void* ctx, uint32_t off) {
    (void)ctx;
    return operate(off, 0, 0, 1);
}

store* store_fm1(void) {
    return &store_fm1_store;
}

void store_fm1_force(int on) {
    fm.force = (uint8_t)(on != 0);
}

void store_fm1_boot(void (*feed)(void)) {
    static const store_flash flash = {map, prog, erase, 0};
    store_init(&store_fm1_store, &flash, store_fm1_window ? store_fm1_window : STORE_WINDOW_MAX);
    if (!flash_store_ok()) {
        return;
    }
    store_mount(&store_fm1_store);
    store_quiet(&store_fm1_store, 1);
    uint32_t start = fm1_ticks();
    for (uint32_t n = 0; n < BOOT_POLLS && store_poll(&store_fm1_store); n++) {
        feed();
        if (store_fm1_store.state != STORE_STATE_MOUNTING &&
            fm1_ticks() - start >= BOOT_WORK_MS * 1000u * FM1_TICKS_PER_US) {
            break;
        }
    }
    store_quiet(&store_fm1_store, 0);
}

static int silent(const live* l, const keys_route* kr, int sounding) {
    if (kr->song || sounding || input_buttons()) {
        return 0;
    }
    for (uint32_t v = 0; v < LIVE_VOICES; v++) {
        if (l->voice[v].gate) {
            return 0;
        }
    }
    return audio_quiet_blocks() * STORE_BLOCK_US >= STORE_QUIET_MS * 1000u;
}

static int quiet(const live* l, const keys_route* kr, int sounding) {
    uint32_t in = midi_inputs();
    if (in != fm.inputs) {
        fm.inputs = in;
        fm.input_ms = fm.now;
    }
    fm.silent = (uint8_t)silent(l, kr, sounding);
    return fm.silent && !midi_sysex_running && fm.now - fm.input_ms >= STORE_QUIET_MS;
}

static void test_hook(keys_route* kr) {
    store_fm1_test* t = &store_fm1_hook;
    if (fm.file == 1) {
        uint8_t b[64];
        uint32_t k = t->len - fm.file_at > sizeof b ? sizeof b : t->len - fm.file_at;
        for (uint32_t i = 0; i < k; i++) {
            b[i] = (uint8_t)((fm.file_at + i) * 7u + t->key);
        }
        fm.file_at += store_file_write(&store_fm1_store, b, k);
        if (fm.file_at == t->len) {
            store_file_end(&store_fm1_store);
            fm.file = 2;
        }
        return;
    }
    if (fm.file == 2) {
        if (store_file_result(&store_fm1_store) != STORE_BUSY) {
            t->result = store_file_result(&store_fm1_store);
            fm.file = 0;
            t->op = 0;
        }
        return;
    }
    if (store_busy(&store_fm1_store)) {
        return;
    }
    uint32_t len = t->len > STORE_FM1_DATA ? STORE_FM1_DATA : t->len;
    switch (t->op) {
    case STORE_FM1_PUT:
        t->result = store_put(&store_fm1_store, t->key, 1, t->data, len);
        break;
    case STORE_FM1_GET:
        t->result = store_get(&store_fm1_store, t->key, t->data, STORE_FM1_DATA, 0);
        break;
    case STORE_FM1_FILE:
        t->result = store_file_begin(&store_fm1_store, "TEST", STORE_FILE_SONG, t->len);
        if (t->result == STORE_OK) {
            fm.file = (uint8_t)(t->len ? 1u : 2u);
            fm.file_at = 0;
            if (!t->len) {
                store_file_end(&store_fm1_store);
            }
            return;
        }
        break;
    case STORE_FM1_COLLECT:
        t->result = store_collect(&store_fm1_store);
        break;
    case STORE_FM1_SONG:
        keys_route_set_song(kr, t->len != 0);
        break;
    case STORE_FM1_FORCE:
        store_fm1_force(t->len != 0);
        break;
    case STORE_FM1_KEEP:
        t->data[STORE_FM1_DATA - 1u] = 0;
        t->result = ui_keep_sound(t->key, (const char*)t->data);
        break;
    default:
        t->result = STORE_INVALID;
        break;
    }
    fm.wanted_at = fm.now - WANT_MS;
    t->op = 0;
}

uint8_t store_fm1_active;

void store_fm1_service(uint32_t now_ms, const live* l, keys_route* kr, int sounding) {
    store* s = &store_fm1_store;
    int writing = s->w.phase || s->f.phase || s->state == STORE_STATE_MOUNTING;
    if (!fm.wanted && !fm.polled && !store_fm1_hook.op && !fm.holding && !writing &&
        !midi_sysex_running &&
        now_ms - fm.wanted_at < WANT_MS) {
        fm.last_ms = now_ms;
        store_fm1_active = 0;
        return;
    }
    fm.now = now_ms;
    fm.loud = (uint8_t)(kr->song || midi_usb_recording());
    if (midi_sysex_running && (++fm.sysex_turn & 1u) && audio_work_slice()) {
        midi_sysex_service(now_ms, fm.loud && !fm.force && !store_fm1_window);
    }
    if (store_fm1_hook.op) {
        test_hook(kr);
    }
    fm.quiet = (uint8_t)quiet(l, kr, sounding);
    store_fm1_store.window = store_fm1_window ? store_fm1_window : STORE_WINDOW_MAX;
    int ask = !fm.polled && now_ms - fm.wanted_at >= WANT_MS;
    if (ask) {
        fm.wanted = (uint8_t)store_erase_wanted(&store_fm1_store);
        fm.wanted_at = now_ms;
    }
    int want_hold = fm.wanted || (!store_fm1_window && writing);
    audio_peak_scan(want_hold && !kr->song);
    if (fm.holding && (!want_hold || (!fm.quiet && !fm.force && !midi_sysex_writing))) {
        hold_off();
    }
    store_quiet(&store_fm1_store, !midi_sysex_running && (fm.quiet || fm.force) && fm.wanted);
    fm.polled = 0;
    int sliced = audio_pass_sliced();
    if (!ask && !sliced) {
        fm.polled = (uint8_t)store_poll(&store_fm1_store);
        if (!fm.polled && (fm.quiet || fm.force) && fm.wanted) {
            fm.wanted = 0;
        }
        if (fm.holding && midi_sysex_writing && fm.programmed && !fm.silent &&
            !store_file_pending(&store_fm1_store) && !s->w.phase) {
            hold_off();
        }
    }
    store_file_idle(&store_fm1_store, now_ms - fm.last_ms);
    fm.last_ms = now_ms;
    store_fm1_active = (uint8_t)(fm.polled || fm.holding || store_fm1_hook.op || writing ||
                                midi_sysex_running);
}
