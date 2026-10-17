// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The only file that includes fm1_input.h and fm1_adc.h: the scan state they define is
 * static, and the TIMER5 interrupt reaches it through input_tick_isr. Neither includes
 * fm1_irq.h (platform/fm1/VENDORED.md). */
#include "input.h"

#include "fm1_adc.h"
#include "fm1_input.h"

#include "ui.h"

static panel_leds shown;

void input_init(void) {
    fm1_input_init();
    fm1_adc_init();
    (void)fm1_adc_read(FM1_ADC_MASTER); /* the first conversion after power-on is off */
}

/* The matrix's pins as fm1_input_init sets them, without its scan state (fm1_in, in .bss,
 * which is not set up yet when input_raw_keys runs). */
static void raw_pins(void) {
    static const uint8_t LEDP[4][2] = {{FM1_PH, 6}, {FM1_PH, 9}, {FM1_PA, 9}, {FM1_PA, 10}};
    const uint32_t rows_a = (1u << 0) | (1u << 5) | (1u << 6) | (1u << 7) | (1u << 8);
    const uint32_t sr = (1u << 1) | (1u << 3) | (1u << 4), row_b = 1u << 7;
    for (uint32_t i = 0; i < 4u; i++) {
        uint32_t p = LEDP[i][0], m = 1u << LEDP[i][1];
        FM1_PR(p, FM1_DIE) |= m;
        FM1_PR(p, FM1_OUT) &= ~m;
        FM1_PR(p, FM1_DIR) &= ~m;
    }
    FM1_PR(FM1_PA, FM1_DIE) |= rows_a;
    FM1_PR(FM1_PA, FM1_DIR) |= rows_a;
    FM1_PR(FM1_PA, FM1_PD) &= ~rows_a;
    FM1_PR(FM1_PA, FM1_PU) |= rows_a;
    FM1_PR(FM1_PB, FM1_DIE) |= row_b;
    FM1_PR(FM1_PB, FM1_DIR) |= row_b;
    FM1_PR(FM1_PB, FM1_PD) &= ~row_b;
    FM1_PR(FM1_PB, FM1_PU) |= row_b;
    FM1_PR(FM1_PA, FM1_DIE) |= sr;
    FM1_PR(FM1_PA, FM1_PU) &= ~sr;
    FM1_PR(FM1_PA, FM1_PD) &= ~sr;
    FM1_PR(FM1_PA, FM1_OUT) &= ~sr;
    FM1_PR(FM1_PA, FM1_DIR) &= ~sr;
}

#define RAW_OE (1u << 2) /* PA2: the 595's /OE, the backlight (active low) */

uint32_t input_raw_keys(void) {
    raw_pins();
    uint32_t out = FM1_PR(FM1_PA, FM1_OUT) & RAW_OE, dir = FM1_PR(FM1_PA, FM1_DIR) & RAW_OE;
    FM1_PR(FM1_PA, FM1_OUT) &= ~RAW_OE;
    FM1_PR(FM1_PA, FM1_DIR) &= ~RAW_OE;
    uint32_t keys = 0;
    for (uint32_t p = 0; p < FM1_NCOL; p++) { /* fm1_input_scan's column order and select */
        fm1__sr_word(0xFFFFu ^ (1u << p) ^ (p < 2u ? 1u << (11u + p) : 0u));
        fm1_delay_us(FM1_SETTLE_US);
        uint32_t raw = fm1__rows();
        for (uint32_t r = 1; r < 5u; r++) {
            int32_t id = FM1_KEYMAP[r][p];
            if (id >= 0 && ((raw >> r) & 1u)) {
                keys |= id < 14 ? 1u << id : INPUT_RAW_NOTES;
            }
        }
    }
    fm1__sr_word(0xFFFFu); /* no column, as fm1_input_init leaves it */
    FM1_PR(FM1_PA, FM1_OUT) = (FM1_PR(FM1_PA, FM1_OUT) & ~RAW_OE) | out;
    FM1_PR(FM1_PA, FM1_DIR) = (FM1_PR(FM1_PA, FM1_DIR) & ~RAW_OE) | dir;
    return keys;
}

void input_raw_dark(void) {
    FM1_PR(FM1_PA, FM1_OUT) |= RAW_OE;
    FM1_PR(FM1_PA, FM1_DIR) &= ~RAW_OE;
}

void input_tick_isr(void) {
    fm1_input_tick();
}

uint32_t input_frame(void) {
    return fm1_in.frames;
}

int32_t input_master(void) {
    return fm1_adc_read(FM1_ADC_MASTER);
}

void input_poll(panel* p, live* l) {
    (void)l; /* the views play the one they were given (ui_init), the same */
    uint32_t pressed = fm1_input_edges(0);
    /* The held keys and their press edges in one go: a press between two separate reads
     * would play twice or blip. */
    uint32_t k = fm1__lock();
    uint32_t down = fm1_in.notes;
    uint32_t note_pressed = fm1_in.notes_pressed;
    fm1_in.notes_pressed = 0;
    fm1__unlock(k);

    ui_buttons(p, pressed, fm1_in.buttons);
    ui_note_keys(p, down, note_pressed);
    for (uint32_t e = 0; e < PANEL_ENCODERS; e++) {
        int32_t steps = fm1_enc_take(e);
        if (steps) {
            ui_encoder(p, e, steps);
        }
    }
}

static void update(uint32_t now, uint32_t was, uint32_t count, uint32_t id0) {
    uint32_t changed = now ^ was;
    for (uint32_t i = 0; changed && i < count; i++, changed >>= 1) {
        if (changed & 1u) {
            fm1_led_key(id0 + i, (int)((now >> i) & 1u));
        }
    }
}

void input_leds(const panel* p, const live* l) {
    panel_leds now = panel_get_leds(p, l);
    update(now.buttons, shown.buttons, PANEL_KEY_ID0, 0);
    update(now.notes, shown.notes, PANEL_NOTE_KEYS, PANEL_KEY_ID0);
    shown = now;
}

void input_leds_dark(void) {
    fm1__led_lines(0);
}

uint32_t input_buttons(void) {
    return fm1_in.buttons;
}

uint32_t input_buttons_at_power_on(void) {
    /* A key held since power-on counts as pressed after FM1_DEB_PRESS frames closed in a
     * row; one more covers the frame the scan was in when this started. */
    uint32_t from = fm1_in.frames;
    while (fm1_in.frames - from < FM1_DEB_PRESS + 1u) {
        __asm__ volatile("idle" ::: "memory"); /* the next TIMER5 tick */
    }
    return fm1_in.buttons;
}
