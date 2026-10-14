// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The ST7789 240 x 240 panel, drawn in full-width strips of LCD_STRIP_ROWS rows from one
 * RAM buffer (the SPI DMA reads RAM only), so there is no framebuffer.
 *
 * Pixels are RGB565 in the panel's byte order: the high byte first (LCD_RGB). A strip's
 * transfer is left running; lcd_strip_buffer waits for it before handing the buffer out
 * again, and lcd_busy tells the main loop whether that would wait. */
#ifndef SIDOU_FM1_LCD_H
#define SIDOU_FM1_LCD_H

#include <stdint.h>

#define LCD_W 240u
#define LCD_H 240u
#define LCD_STRIP_ROWS 24u
#define LCD_STRIPS (LCD_H / LCD_STRIP_ROWS)

/* RGB565 from 8-bit r, g, b, byte-swapped for the panel. */
#define LCD_RGB565(r, g, b) ((uint16_t)((((r) & 0xF8u) << 8) | (((g) & 0xFCu) << 3) | ((b) >> 3)))
#define LCD_SWAP(c) ((uint16_t)(((c) >> 8) | ((c) << 8)))
#define LCD_RGB(r, g, b) LCD_SWAP(LCD_RGB565(r, g, b))

/* The panel's pins, SPI1, reset and init sequence; the display stays off (lcd_on).
 * Interrupts must be off: the backlight pin PA2 is also the key matrix's /OE, which the
 * TIMER5 scan writes on the same port. */
void lcd_init(void);
/* DISPON, once the first picture is in the panel's RAM. */
void lcd_on(void);
/* 1 while a strip transfer is still running. */
int lcd_busy(void);
/* The strip buffer, LCD_W * LCD_STRIP_ROWS pixels, once the last transfer from it is done. */
uint16_t* lcd_strip_buffer(void);
/* Sends the buffer as strip s (rows s * LCD_STRIP_ROWS ..) and leaves the transfer running. */
void lcd_strip_send(uint32_t s);
/* Waits for the running transfer. */
void lcd_sync(void);

#endif
