// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* USB audio out on the host: Felucca's src/usb.c (GPL-3.0-only) built as midi.c builds it
 * (platform/fm1/usb_config.h: FELUCCA_UAC on, no CDC, SIDOU's strings), with SIDOU's TIMER5
 * side (platform/fm1/usb_audio.c) after it. The tests
 * (tests/usb_audio.rs) play both interrupts and the host's packets in time; the SIE is never
 * touched (uac_service and usb_poll are not called: uac_h_packet is the packet uac_service
 * would send). */
#include <stdint.h>
#include <string.h>

#define AUDIO_HALF_FRAMES 128u
#include "usb_config.h"
/* usb_audio.c's buffer is in the device's .pool; Mach-O section names need a segment. */
#define USB_AUDIO_POOL
static void fm1_delay_ms(uint32_t ms) {
    (void)ms;
}
#pragma GCC diagnostic ignored "-Wint-to-pointer-cast" /* SIE register macros (never touched) */
#include "src/usb.c"
/* usb.c's update-protocol hooks (FELUCCA_OTA), which midi.c's ota.c defines: unused here. */
static uint32_t ota_now_ms(void) {
    return 0;
}
static void ota_idle(void) {
}
/* usb_audio_isr's uac_service talks to the SIE (and its pi32v2 sync instructions): not here. */
static void uac_h_no_sie(void) {
}
#define uac_service uac_h_no_sie
#include "usb_audio.c"
#undef uac_service

/* usb.c's descriptor for GET_DESCRIPTOR's wValue; its length, or 0 when it stalls. */
uint32_t uac_h_desc(uint32_t wvalue, const uint8_t** d) {
    uint16_t l = 0;
    if (!get_desc(wvalue, d, &l)) {
        return 0;
    }
    return l;
}

/* A configured device, no stream; the hand-over idle. */
void uac_h_reset(void) {
    memset(&usb, 0, sizeof usb);
    memset(&uac, 0, sizeof uac);
    memset(&rs, 0, sizeof rs);
    memset(ua_ring, 0, sizeof ua_ring);
    memset(&usb_audio_in, 0, sizeof usb_audio_in);
    ua_w = ua_r = 0;
    usb.up = 1;
    usb.config = 1;
}

/* SET_INTERFACE on the audio streaming interface (alt 0 or 1), SET_CUR's rate. */
void uac_h_stream(uint32_t alt) {
    uac_stream(alt);
}
void uac_h_rate(uint32_t hz) {
    uac_rate_set(hz);
}
/* The host takes our packets (uac_service sets it once the first has gone). */
void uac_h_flowing(uint32_t on) {
    uac.flowing = (uint8_t)on;
}

/* TIMER5's calls (usb_audio.h). */
void uac_h_take(const int32_t* half) {
    usb_audio_take(half);
}
void uac_h_skip(void) {
    usb_audio_skip();
}
void uac_h_tick(void) {
    usb_audio_tick();
}

/* The next packet, as uac_service queues it: its frames into d (at least UA_MAXF), the count. */
uint32_t uac_h_packet(uint32_t* d) {
    return uac_packet(d);
}

/* Felucca's producer straight from int32 blocks of 32 frames, as Felucca's audio.c calls it
 * (render start, then each block): the reference the hand-over must match. */
void uac_h_direct_start(void) {
    uac_render_start();
}
void uac_h_direct_tap(const int32_t* out, uint32_t n) {
    uac_tap(out, n);
}

/* The ring's frames as they are, oldest first, up to max: the reference's read side. */
uint32_t uac_h_drain(uint32_t* d, uint32_t max) {
    uint32_t n = ua_w - ua_r;
    if (n > max) {
        n = max;
    }
    for (uint32_t i = 0; i < n; i++) {
        d[i] = ua_ring[(ua_r + i) & (UA_N - 1u)];
    }
    ua_r += n;
    return n;
}

typedef struct {
    uint32_t want, left, blocks, skipped;
    uint32_t go, feed, r48, fill_min, fill_lo, fill_hi;
    uint32_t underruns, overruns, adj_up, adj_down, pkts, frames;
    uint32_t ring_w, ring_r, maxf, prime, prime48, lo, hi, lo48, hi48, ring_n;
} uac_h_state;

uac_h_state uac_h_get(void) {
    uac_h_state s = {
        usb_audio_in.want, usb_audio_in.left,    usb_audio_in.blocks, usb_audio_in.skipped,
        uac.go,            uac.feed,             uac.r48,             uac.fill_min,
        uac.fill_lo,       uac.fill_hi,          uac.underruns,       uac.overruns,
        uac.adj_up,        uac.adj_down,         uac.pkts,            uac.frames,
        ua_w,              ua_r,                 UA_MAXF,             UA_PRIME,
        UA_PRIME48,        UA_LO,                UA_HI,               UA_LO48,
        UA_HI48,           UA_N,
    };
    return s;
}
