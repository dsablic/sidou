// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The only file that includes fm1_input.h and fm1_adc.h: the scan state they define is
 * static, and the TIMER5 interrupt reaches it through input_tick_isr. Neither includes
 * fm1_irq.h (platform/fm1/VENDORED.md). */
#include "input.h"

#include "midi.h"

#include "fm1_adc.h"
#include "fm1_input.h"

#include "ui.h"

_Static_assert(INPUT_FRAME_US == FM1_NCOL * FM1_LED_TICK_US, "a scan frame, as input.h says");
_Static_assert(INPUT_PRESS_FRAMES == FM1_DEB_PRESS, "the press debounce, as input.h says");

static panel_leds shown;
static uint8_t shown_valid;

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

/* MASTER: the mean of INPUT_MASTER_READS conversions, which halves the pot's noise before
 * panel_master's thresholds see it (a timeout in any of them is the reading's: -1). Called once
 * a live tick; a conversion is a few tens of microseconds, so four cost well under 1 % of it. */
#define INPUT_MASTER_READS 4

int32_t input_master(void) {
    int32_t sum = 0;
    for (uint32_t i = 0; i < INPUT_MASTER_READS; i++) {
        int32_t v = fm1_adc_read(FM1_ADC_MASTER);
        if (v < 0) {
            return -1;
        }
        sum += v;
    }
    return (sum + INPUT_MASTER_READS / 2) / INPUT_MASTER_READS;
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

    /* A Live Link host's buttons and encoders with the panel's (midi.h; none without a
     * session, which only a host starts, after the boot's combinations were read). */
    uint32_t held = fm1_in.buttons;
    uint8_t linked = midi_link_active;
    if (linked) {
        uint32_t remote;
        held |= midi_link_buttons(&remote);
        pressed |= remote;
    }
    ui_buttons(p, pressed, held);
    ui_note_keys(p, down, note_pressed);
    for (uint32_t e = 0; e < PANEL_ENCODERS; e++) {
        int32_t steps = fm1_enc_take(e);
        if (linked) {
            steps += midi_link_steps(e);
        }
        if (steps) {
            ui_encoder(p, e, steps);
        }
    }
}

/* The glow of key id: fm1_led_dim as fm1_led_key sets fm1_led (the HAL's scan pulses it, a
 * thirtieth of lit, fm1_input.h; vendored code is not touched). */
static void dim_key(uint32_t id, int on) {
    for (uint32_t p = 0; p < FM1_NCOL; p++) {
        for (uint32_t r = 1; r < 5u; r++) {
            if (FM1_KEYMAP[r][p] == (int8_t)id) {
                if (on) {
                    fm1_led_dim[p] |= (uint8_t)(1u << r);
                } else {
                    fm1_led_dim[p] &= (uint8_t)~(1u << r);
                }
            }
        }
    }
}

static void update(uint32_t now, uint32_t was, uint32_t count, uint32_t id0,
                   void (*set)(uint32_t, int)) {
    uint32_t changed = now ^ was;
    for (uint32_t i = 0; changed && i < count; i++, changed >>= 1) {
        if (changed & 1u) {
            set(id0 + i, (int)((now >> i) & 1u));
        }
    }
}

static void led_key(uint32_t id, int on) {
    fm1_led_key(id, on);
}

/* The LEDs follow the panel at most this often: the blinks and the beat move at tens of ms. */
#define LEDS_PERIOD_MS 10u

void input_leds(const panel* p, const live* l, uint32_t now_ms) {
    static uint32_t last_ms;
    if (now_ms - last_ms < LEDS_PERIOD_MS && shown_valid) {
        return;
    }
    last_ms = now_ms;
    shown_valid = 1;
    panel_led_in in;
    ui_leds(now_ms, &in);
    panel_leds now = panel_get_leds(p, l, &in);
    update(now.buttons, shown.buttons, PANEL_KEY_ID0, 0, led_key);
    update(now.notes, shown.notes, PANEL_NOTE_KEYS, PANEL_KEY_ID0, led_key);
    update(now.buttons_dim, shown.buttons_dim, PANEL_KEY_ID0, 0, dim_key);
    update(now.notes_dim, shown.notes_dim, PANEL_NOTE_KEYS, PANEL_KEY_ID0, dim_key);
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
