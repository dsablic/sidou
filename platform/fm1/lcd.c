/* SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-only
 * The panel protocol, adapted from Felucca's firmware/src/lcd.c (the init sequence, the
 * transfer left running until the next access): hal/fm1_lcd_hw.h drives the pins and SPI1.
 * The only HAL headers here are fm1_lcd_hw.h and fm1_time.h, which hold no interrupt code
 * (platform/fm1/VENDORED.md). */
#include "lcd.h"

#include "fm1_lcd_hw.h"
#include "fm1_time.h"

/* SPI1 clock = 60 MHz / (BAUD + 1): 4 = 12 MHz. */
#define LCD_BAUD 4u

static uint16_t buffers[2][LCD_BUFFER_PIXELS] __attribute__((aligned(4)));
static uint8_t next_buffer; /* lcd_buffer's; the other one may be in transfer */
static uint8_t small[8] __attribute__((aligned(4)));
static uint8_t busy; /* a data transfer may still run; CS is low */
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
    lcd_sync(); /* small is the source of the transfer before */
    for (uint32_t i = 0; i < n; i++) {
        small[i] = bytes[i];
    }
    data(small, n);
}

static void window(uint32_t x0, uint32_t y0, uint32_t x1, uint32_t y1) {
    const uint8_t cols[4] = {(uint8_t)(x0 >> 8), (uint8_t)x0, (uint8_t)(x1 >> 8), (uint8_t)x1};
    const uint8_t rows[4] = {(uint8_t)(y0 >> 8), (uint8_t)y0, (uint8_t)(y1 >> 8), (uint8_t)y1};
    cmd_data(0x2A, cols, 4); /* CASET */
    cmd_data(0x2B, rows, 4); /* RASET */
    cmd(0x2C);               /* RAMWR */
}

void lcd_init(void (*during)(void)) {
    static const uint8_t colmod = 0x55; /* RGB565 */
    static const uint8_t madctl = 0x00; /* top-left origin, RGB order */
    fm1_lcd_hw_init();
    fm1_delay_ms(80);
    cmd(0x01); /* SWRESET */
    {
        /* The boot's own work in the reset's wait (M2c Task 6: the song's region), so the
         * boot's timeline stays as it was. */
        uint32_t start = fm1_ticks();
        if (during) {
            during();
        }
        while ((uint32_t)(fm1_ticks() - start) < 150u * 1000u * FM1_TICKS_PER_US) {
        }
    }
    cmd(0x11); /* SLPOUT */
    fm1_delay_ms(120);
    cmd_data(0x3A, &colmod, 1);
    cmd_data(0x36, &madctl, 1);
    cmd(0x21); /* INVON: the IPS panel's native mode */
    cmd(0x13); /* NORON */
    fm1_lcd_baud(LCD_BAUD);
}

void lcd_on(void) {
    cmd(0x29); /* DISPON */
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
