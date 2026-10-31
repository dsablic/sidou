/* SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-only */
#include "lcd.h"

#include "fm1_lcd_hw.h"
#include "fm1_time.h"

#define LCD_BAUD 4u

static uint16_t buffers[2][LCD_BUFFER_PIXELS] __attribute__((aligned(4)));
static uint8_t next_buffer;
static uint8_t small[8] __attribute__((aligned(4)));
static uint8_t busy;
lcd_counters lcd_count;

void lcd_sync(void) {
    if (!busy) {
        return;
    }
    fm1_lcd_wait();
    fm1_lcd_deselect();
    busy = 0;
}

int lcd_busy(void) {
    return busy && !(FM1_LCD_SPI_CON & 0x8000u);
}

static void cmd(uint8_t c) {
    lcd_sync();
    fm1_lcd_send_cmd(c);
    lcd_count.bytes++;
    fm1_lcd_wait();
    fm1_lcd_deselect();
}

static void data(const void* p, uint32_t n) {
    lcd_sync();
    fm1_lcd_send_data(p, n);
    lcd_count.bytes += n;
    busy = 1;
}

static void cmd_data(uint8_t c, const uint8_t* bytes, uint32_t n) {
    cmd(c);
    lcd_sync();
    for (uint32_t i = 0; i < n; i++) {
        small[i] = bytes[i];
    }
    data(small, n);
}

static void window(uint32_t x0, uint32_t y0, uint32_t x1, uint32_t y1) {
    const uint8_t cols[4] = {(uint8_t)(x0 >> 8), (uint8_t)x0, (uint8_t)(x1 >> 8), (uint8_t)x1};
    const uint8_t rows[4] = {(uint8_t)(y0 >> 8), (uint8_t)y0, (uint8_t)(y1 >> 8), (uint8_t)y1};
    cmd_data(0x2A, cols, 4);
    cmd_data(0x2B, rows, 4);
    cmd(0x2C);
}

void lcd_init(void (*during)(void), void (*while_waking)(void)) {
    static const uint8_t colmod = 0x55;
    static const uint8_t madctl = 0x00;
    fm1_lcd_hw_init();
    fm1_delay_ms(80);
    cmd(0x01);
    {
        uint32_t start = fm1_ticks();
        if (during) {
            during();
        }
        while ((uint32_t)(fm1_ticks() - start) < 150u * 1000u * FM1_TICKS_PER_US) {
        }
    }
    cmd(0x11);
    uint32_t woke = fm1_ticks();
    if (while_waking) {
        while_waking();
    }
    while ((uint32_t)(fm1_ticks() - woke) < 120000u * FM1_TICKS_PER_US) {
    }
    cmd_data(0x3A, &colmod, 1);
    cmd_data(0x36, &madctl, 1);
    cmd(0x21);
    cmd(0x13);
    fm1_lcd_baud(LCD_BAUD);
}

void lcd_on(void) {
    cmd(0x29);
}

uint16_t* lcd_buffer(void) {
    return buffers[next_buffer];
}

void lcd_send(uint32_t x, uint32_t y, uint32_t w, uint32_t h) {
    window(x, y, x + w - 1u, y + h - 1u);
    data(buffers[next_buffer], w * h * 2u);
    next_buffer ^= 1u;
    lcd_count.windows++;
}
