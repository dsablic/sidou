// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* Compile check for the vendored Felucca HAL (hal/, GPL-3.0-only): every header in one
 * translation unit, as the firmware will include them. Built as an object library, never
 * linked. fm1_irq.h needs the including file to define fm1_fault, and fm1_sys.h first (its
 * FM1_RESET_PREP). */
#include "fm1_sys.h"

#include "fm1_adc.h"
#include "fm1_audio.h"
#include "fm1_cc.h"
#include "fm1_flash.h"
#include "fm1_gpio.h"
#include "fm1_input.h"
#include "fm1_irq.h"
#include "fm1_lcd_hw.h"
#include "fm1_led_anim.h"
#include "fm1_time.h"
#include "fm1_timer.h"
#include "fm1_uart.h"
#include "fm1_usb.h"
#include "fm1_xip.h"

static void fm1_fault(const fm1_crash_t* c) {
    (void)c;
    fm1_reboot();
}
