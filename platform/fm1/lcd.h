// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_LCD_H
#define SIDOU_FM1_LCD_H

#include <stdint.h>

#define LCD_W 240u
#define LCD_H 240u
#define LCD_BUFFER_PIXELS (LCD_W * 8u)
#define LCD_WINDOW_COMMAND_BYTES 11u

#define LCD_RGB565(r, g, b) ((uint16_t)((((r) & 0xF8u) << 8) | (((g) & 0xFCu) << 3) | ((b) >> 3)))
#define LCD_SWAP(c) ((uint16_t)(((c) >> 8) | ((c) << 8)))
#define LCD_RGB(r, g, b) LCD_SWAP(LCD_RGB565(r, g, b))

void lcd_init(void (*during)(void), void (*while_waking)(void));
void lcd_on(void);
int lcd_busy(void);
uint16_t* lcd_buffer(void);
void lcd_send(uint32_t x, uint32_t y, uint32_t w, uint32_t h);
void lcd_sync(void);

typedef struct {
    uint32_t windows;
    uint32_t bytes;
} lcd_counters;
extern lcd_counters lcd_count;

#endif
