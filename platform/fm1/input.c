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
    uint32_t pressed = fm1_input_edges(0);
    /* The held keys and their press edges in one go: a press between two separate reads
     * would play twice or blip. */
    uint32_t k = fm1__lock();
    uint32_t down = fm1_in.notes;
    uint32_t note_pressed = fm1_in.notes_pressed;
    fm1_in.notes_pressed = 0;
    fm1__unlock(k);

    ui_buttons(p, pressed, fm1_in.buttons);
    panel_note_keys(p, l, down, note_pressed);
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
