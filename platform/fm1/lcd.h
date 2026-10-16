// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The ST7789 240 x 240 panel, drawn in windows (rectangles) of at most LCD_BUFFER_PIXELS
 * pixels from two RAM buffers (the SPI DMA reads RAM only), so there is no framebuffer.
 *
 * Pixels are RGB565 in the panel's byte order: the high byte first (LCD_RGB). A window's
 * transfer is left running; the other buffer can be filled meanwhile (lcd_buffer), and
 * lcd_busy tells the main loop whether the next lcd_send would wait. */
#ifndef SIDOU_FM1_LCD_H
#define SIDOU_FM1_LCD_H

#include <stdint.h>

#define LCD_W 240u
#define LCD_H 240u
/* One buffer: a full-width row of 8 x 8 cells. */
#define LCD_BUFFER_PIXELS (LCD_W * 8u)
/* SPI bytes of a window's commands besides its pixels: CASET, RASET (1 + 4 each), RAMWR. */
#define LCD_WINDOW_COMMAND_BYTES 11u

/* RGB565 from 8-bit r, g, b, byte-swapped for the panel. */
#define LCD_RGB565(r, g, b) ((uint16_t)((((r) & 0xF8u) << 8) | (((g) & 0xFCu) << 3) | ((b) >> 3)))
#define LCD_SWAP(c) ((uint16_t)(((c) >> 8) | ((c) << 8)))
#define LCD_RGB(r, g, b) LCD_SWAP(LCD_RGB565(r, g, b))

/* The panel's pins, SPI1, reset and init sequence; the display stays off (lcd_on).
 * Interrupts must be off: the backlight pin PA2 is also the key matrix's /OE, which the
 * TIMER5 scan writes on the same port. */
/* during: work run inside the panel's 150 ms reset wait (the wait takes as long as before
 * if the work is shorter), or NULL; with interrupts off. */
void lcd_init(void (*during)(void));
/* DISPON, once the first picture is in the panel's RAM. */
void lcd_on(void);
/* 1 while a window's transfer is still running. */
int lcd_busy(void);
/* The buffer the next lcd_send takes, LCD_BUFFER_PIXELS: the one the running transfer (if any)
 * is not reading, so it can be filled at once. */
uint16_t* lcd_buffer(void);
/* Sends w x h pixels from lcd_buffer()'s buffer, rows top first, to the window at (x, y), and
 * leaves the transfer running; the other buffer is lcd_buffer()'s from then on. Waits for the
 * transfer before, if it still runs. */
void lcd_send(uint32_t x, uint32_t y, uint32_t w, uint32_t h);
/* Waits for the running transfer. */
void lcd_sync(void);

/* What went out since boot: windows sent by lcd_send and SPI bytes (pixels and every command
 * byte). The emulator tests read them to estimate the SPI time per frame. */
typedef struct {
    uint32_t windows;
    uint32_t bytes;
} lcd_counters;
extern lcd_counters lcd_count;

#endif
