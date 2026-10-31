// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
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
