/* SPDX-License-Identifier: GPL-3.0-only
 * Copyright (C) 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments */
/* USB full-speed device on USB0: a
 * class-compliant USB-MIDI interface. All SIE traffic happens in one place,
 * usb_poll(), called from the TIMER5 ISR at 2 kHz, so INDEX is never shared
 * and no USB IRQ is needed. MIDI in goes to a ring the main loop drains;
 * MIDI out comes from a ring nothing fills yet (midi_out_event).
 * VID 0x1209 / PID 0x0001 is the pid.codes
 * TEST id: fine for the bench, must be replaced before any release.
 *
 * SIDOU: adapted from Felucca's firmware/src/usb.c; platform/fm1/VENDORED.md lists every
 * change. In short:
 * - USB-MIDI only: the CDC-ACM console (FELUCCA_CDC), the USB audio input (FELUCCA_UAC)
 *   and their descriptor layouts are gone;
 * - the stock soft key F0 22 24 35 7D F7 (UBOOT) is neither recognised nor acted on:
 *   SIDOU never handles it (a safety rule);
 * - MIDI in: every event packet goes as it is to the includer's midi_enqueue, which stamps
 *   it and rings it to the main loop (platform/fm1/midi.c); midi_in_room() is the room left;
 * - usb_retry does the 25 ms reset without blocking the main loop, and an attach gives up
 *   at the third SIE timeout instead of timing out every write;
 * - usb_detach masks interrupts in the app;
 * - the app's strings name SIDOU.
 * The update loader is not built from this copy: it builds, unmodified, from Felucca's own
 * src/usb.c (platform/fm1/src/), so its FELUCCA_LOADER code here is unused.
 * The includer includes fm1_time.h and defines RING_PUBLISH(), FELUCCA_OTA,
 * midi_enqueue(pkt, source) and midi_in_room() first. The update protocol's SysEx frames
 * and SysEx TX queue (FELUCCA_OTA, which the app turns on) are as they were. */
#include "../hal/fm1_usb.h"   /* registers; relative, so the loader and the host tests find it too */
enum { S_FADDR = 0, S_POWER = 1, S_INTRTX1 = 2, S_INTRTX2 = 3, S_INTRRX1 = 4, S_INTRRX2 = 5, S_INTRUSB = 6,
       S_INTRTX1E = 7, S_INTRTX2E = 8, S_INTRRX1E = 9, S_INTRRX2E = 10, S_INTRUSBE = 11, S_FRAME1 = 12,
       S_FRAME2 = 13, S_INDEX = 14,
       S_TXMAXP = 16, S_CSR0 = 17, S_TXCSR1 = 17, S_TXCSR2 = 18, S_RXMAXP = 19, S_RXCSR1 = 20, S_RXCSR2 = 21,
       S_COUNT0 = 22, S_RXCOUNT1 = 22, S_RXCOUNT2 = 23 };

static uint8_t ep0buf[64 + 4] __attribute__((aligned(4)));
static uint8_t ep1tx[64] __attribute__((aligned(4)));
static uint8_t ep1rx[64 + 4] __attribute__((aligned(4)));


static struct {
    uint8_t up, config, pend_addr, has_pend_addr, e0_tx, e0_zlp;
    const uint8_t *e0_src;
    uint16_t e0_left;
    uint32_t resets, setups, rx_pkts, tx_pkts, sof_seen, timeouts, no_sof;
    uint32_t suspends, max_gap, retries, frame_stalls;   /* diagnostics: suspend events, longest run of polls without SOF */
    uint16_t frame, frame_same;  /* last SOF frame number and how long it has not moved (x 10 ms) */
    uint8_t detached;            /* usb_detach() was called: no retry */
    volatile uint8_t suspended;  /* bus idle > 3 ms (unplugged or host asleep): the UI hides "USB" */
    uint8_t last_setup[8];
    uint8_t sysex[16];
    uint8_t sx_len, sx_on;
    uint8_t rx_pend;             /* EP1 OUT packet seen, not yet taken (the MIDI ring was too full) */
    volatile uint8_t ota_req;    /* F0 22 24 35 7F F7: M-UPGRADE upgrade command (FELUCCA_OTA) */
} usb;

#if FELUCCA_OTA
/* M-UPGRADE SysEx (ota.c): one received frame at a time (7-bit bytes between
 * F0 and F7; frames arriving while one is pending are dropped, the host
 * retries), and a TX ring of SysEx event packets sent before any MIDI. */
static uint8_t sx_frame[640];
static uint32_t sx_pos;
static volatile uint32_t sx_frame_len;
static volatile uint8_t sx_ready, sx_collect, sx_busy;
#define SXQ 64u
static uint32_t sx_out_q[SXQ];
static volatile uint32_t so_w, so_r;
#endif

/* MIDI out ring: 4-byte USB-MIDI event packets. (SIDOU: MIDI in goes to the includer's
 * midi_enqueue; Felucca's in ring, its stamps and its overflow flag are gone.) */
#define MQ 64u
static uint32_t midi_out_q[MQ];
static volatile uint32_t mo_w, mo_r;

static __attribute__((unused)) void midi_out_event(uint32_t pkt)            /* from the audio ISR */
{
    if (usb.config && mo_w - mo_r < MQ) {
        midi_out_q[mo_w % MQ] = pkt;
        RING_PUBLISH();
        mo_w++;
    }
}

/* ------------------------------------------------------- descriptors --- */
/* One function: audio control (interface 0) and MIDI streaming (interface 1) on EP1 OUT and
 * IN, device class 0, no IAD: Felucca's USB-MIDI-only build (FELUCCA_CDC and FELUCCA_UAC
 * off), bcdDevice 3.00. */
#ifndef FELUCCA_USB_PID
#define FELUCCA_USB_PID 0x0001   /* the update loader is 0x0002 */
#endif

#define D_CFG(len, nif) 9, 2, (len) & 0xFF, (len) >> 8, (nif), 1, 0, 0x80, 50
#define D_AC(a) 9, 4, (a), 0, 0, 1, 1, 0, 0, 9, 0x24, 1, 0x00, 0x01, 9, 0, 1, (a) + 1,
#define D_MIDI(m)                                                                                    \
    9, 4, (m), 0, 2, 1, 3, 0, 0,                                                                     \
    7, 0x24, 1, 0x00, 0x01, 0x41, 0x00,                                                              \
    6, 0x24, 2, 1, 1, 0,                                                                             \
    6, 0x24, 2, 2, 2, 0,                                                                             \
    9, 0x24, 3, 1, 3, 1, 2, 1, 0,                                                                    \
    9, 0x24, 3, 2, 4, 1, 1, 1, 0,                                                                    \
    9, 5, 0x01, 2, 64, 0, 0, 0, 0,                                                                   \
    5, 0x25, 1, 1, 1,                                                                                \
    9, 5, 0x81, 2, 64, 0, 0, 0, 0,                                                                   \
    5, 0x25, 1, 1, 3,
#define D_AUDIO(a) D_AC(a) D_MIDI((a) + 1)   /* audio control + MIDI from interface a */
#define AUD_NIF 2
#define AUD_LEN 92
#define D_DEV(bcdusb, cls, sub, proto)                                                               \
    18, 1, (bcdusb) & 0xFF, (bcdusb) >> 8, (cls), (sub), (proto), 64, 0x09, 0x12, FELUCCA_USB_PID & 0xFF, \
    FELUCCA_USB_PID >> 8, 0x00, 0x03, 1, 2, 0, 1

#define CFG_LEN_PLAIN (9 + AUD_LEN)
static const uint8_t DEV_DESC_PLAIN[18] = {D_DEV(0x0110, 0, 0, 0)};
static const uint8_t CFG_DESC_PLAIN[] = {D_CFG(CFG_LEN_PLAIN, AUD_NIF), D_AUDIO(0)};
typedef char cfg_plain_len_ok[sizeof CFG_DESC_PLAIN == CFG_LEN_PLAIN ? 1 : -1];
static const uint8_t STR0[4] = {4, 3, 0x09, 0x04};
static const uint8_t STR1[] = {38, 3, 'S', 0, 'I', 0, 'D', 0, 'O', 0, 'U', 0, ' ', 0, 'c', 0, 'o', 0, 'n', 0, 't', 0,
                               'r', 0, 'i', 0, 'b', 0, 'u', 0, 't', 0, 'o', 0, 'r', 0, 's', 0};
#ifdef FELUCCA_LOADER
static const uint8_t STR2[] = {30, 3, 'F', 0, 'e', 0, 'l', 0, 'u', 0, 'c', 0, 'c', 0, 'a', 0, ' ', 0, 'U', 0, 'p', 0,
                               'd', 0, 'a', 0, 't', 0, 'e', 0};
#else
static const uint8_t STR2[] = {12, 3, 'S', 0, 'I', 0, 'D', 0, 'O', 0, 'U', 0};
#endif

static int get_desc(uint32_t wvalue, const uint8_t **d, uint16_t *l)
{
    switch (wvalue >> 8) {
    case 1:
        *d = DEV_DESC_PLAIN;
        *l = 18;
        return 1;
    case 2:
        *d = CFG_DESC_PLAIN;
        *l = (uint16_t)sizeof CFG_DESC_PLAIN;
        return 1;
    case 3:
        switch (wvalue & 0xFFu) {
        case 0:
            *d = STR0;
            *l = sizeof STR0;
            return 1;
        case 1:
            *d = STR1;
            *l = sizeof STR1;
            return 1;
        case 2:
            *d = STR2;
            *l = sizeof STR2;
            return 1;
        default:
            return 0;
        }
    default:
        return 0;
    }
}

/* ---------------------------------------------------------- the SIE --- */
static void sie_wr(uint32_t r, uint32_t v)
{
    uint32_t n = 20000;
    if (!fm1_usb_sie_on())
        return;
    fm1_usb_sie_wr_start(r, v);
    while (!fm1_usb_sie_done() && --n)
        ;
    if (n)
        usb.timeouts = 0;                               /* consecutive failures only */
    else if (++usb.timeouts > 50u)
        usb.up = 0;                                     /* SIE dead (no clock?): stop polling it */
}

static uint32_t sie_rd(uint32_t r)
{
    uint32_t n = 20000;
    if (!fm1_usb_sie_on())
        return 0;
    fm1_usb_sie_rd_start(r);
    while (!fm1_usb_sie_done())
        if (!--n) {
            if (++usb.timeouts > 50u)
                usb.up = 0;
            return 0;
        }
    usb.timeouts = 0;
    return fm1_usb_sie_data();
}


static void ep1_config(void)
{
    fm1_usb_ep_txbuf(1, ep1tx);
    fm1_usb_ep_rxbuf(1, ep1rx);
    sie_wr(S_INDEX, 1);
    sie_wr(S_TXMAXP, 0xFF);
    sie_wr(S_TXCSR1, 0x48);
    sie_wr(S_TXCSR2, 0);
    sie_wr(S_RXMAXP, 0xFF);
    sie_wr(S_RXCSR1, 0x90);
    sie_wr(S_RXCSR2, 0);
    sie_wr(S_INTRRX1E, 0x02);
    fm1_usb_ep_enable(1u << 1);
}

static void e0_chunk(void)
{
    uint32_t n = usb.e0_left > 64u ? 64u : usb.e0_left, i;
    int last;
    for (i = 0; i < n; i++)
        ep0buf[i] = usb.e0_src[i];
    fm1_usb_ep0_send(ep0buf, n);
    usb.e0_src += n;
    usb.e0_left = (uint16_t)(usb.e0_left - n);
    last = usb.e0_left == 0 && !(n == 64u && usb.e0_zlp);
    if (usb.e0_left == 0 && n == 64u)
        usb.e0_zlp = 0;
    sie_wr(S_INDEX, 0);
    sie_wr(S_CSR0, last ? 0x0A : 0x02);
    if (last)
        usb.e0_tx = 0;
}

static void e0_send(const uint8_t *p, uint16_t len, uint16_t wlen)
{
    if (len > wlen)
        len = wlen;
    usb.e0_src = p;
    usb.e0_left = len;
    usb.e0_zlp = (len < wlen) && (len % 64u == 0);
    usb.e0_tx = 1;
    sie_wr(S_INDEX, 0);
    sie_wr(S_CSR0, 0x40);
    e0_chunk();
}

static void ep0_service(void)
{
    static const uint8_t zero2[2];
    uint32_t csr, i;
    uint16_t wvalue, wlength;
    uint8_t *s = usb.last_setup;
    if (usb.has_pend_addr) {
        sie_wr(S_FADDR, usb.pend_addr);
        usb.has_pend_addr = 0;
    }
    sie_wr(S_INDEX, 0);
    csr = sie_rd(S_CSR0);
    if (csr & 0x04u) {                                  /* SentStall */
        sie_wr(S_CSR0, 0);
        usb.e0_tx = 0;
        return;
    }
    if (csr & 0x10u) {                                  /* SetupEnd: the host abandoned the transfer */
        sie_wr(S_CSR0, 0x80);
        usb.e0_tx = 0;
    }
    if (usb.e0_tx) {
        if (!(csr & 0x02u))
            e0_chunk();
        return;
    }
    if (!(csr & 0x01u))
        return;
    fm1_usb_rx_sync();
    for (i = 0; i < 8u; i++)
        s[i] = ep0buf[i];
    usb.setups++;
    wvalue = (uint16_t)(s[2] | s[3] << 8);
    wlength = (uint16_t)(s[6] | s[7] << 8);
    switch ((uint32_t)s[0] << 8 | s[1]) {
    case 0x0005:                                        /* SET_ADDRESS */
        usb.pend_addr = s[2] & 0x7Fu;
        usb.has_pend_addr = 1;
        goto ack;
    case 0x8006: {                                      /* GET_DESCRIPTOR */
        const uint8_t *d;
        uint16_t l;
        if (get_desc(wvalue, &d, &l)) {
            if (!wlength)
                goto ack;
            e0_send(d, l, wlength);
            return;
        }
        goto stall;
    }
    case 0x0009:                                        /* SET_CONFIGURATION: 0 or 1 only */
        if (s[2] > 1u)
            goto stall;
        usb.config = s[2];
        if (usb.config == 1u)
            ep1_config();
        else
            sie_wr(S_INTRRX1E, 0);
        goto ack;
    case 0x8008:
        e0_send(&usb.config, 1, wlength);
        return;
    case 0x8000:
    case 0x8100:
    case 0x8200:
        e0_send(zero2, 2, wlength);
        return;
    case 0x010B:                                        /* SET_INTERFACE: alt 0 only (SIDOU: no audio stream) */
        if (wvalue == 0)
            goto ack;
        goto stall;
    case 0x810A:
        e0_send(zero2, 1, wlength);
        return;
    case 0x0201: {                                      /* CLEAR_FEATURE(ENDPOINT_HALT): data toggle reset */
        uint32_t ep = s[4] & 0x0Fu, last = 1u;
        if (wvalue != 0 || ep > last)
            goto stall;                                 /* not an endpoint we have */
        if (ep) {                                       /* (EP0 has no halt to clear) */
            sie_wr(S_INDEX, ep);
            if (s[4] & 0x80u)
                sie_wr(S_TXCSR1, 0x40);
            else
                sie_wr(S_RXCSR1, 0x80);
        }
        goto ack;
    }
    default:
        goto stall;
    }
ack:
    sie_wr(S_INDEX, 0);
    sie_wr(S_CSR0, 0x48);
    return;
stall:
    sie_wr(S_INDEX, 0);
    sie_wr(S_CSR0, 0x60);
}

#if FELUCCA_OTA
/* SysEx assembly: only short commands matter here. (SIDOU: built only with FELUCCA_OTA, for
 * the update protocol's frames; the stock UBOOT soft key is not recognised.) */
static void sysex_byte(uint8_t b)
{
    if (b >= 0xF8u)
        return;                                        /* realtime may occur anywhere in SysEx */
    if ((b & 0x80u) && b != 0xF0u && b != 0xF7u) {
        usb.sx_on = 0;
        sx_collect = 0;
        return;                                        /* another status aborts the unfinished frame */
    }
    if (b == 0xF0) {
        usb.sx_on = 1;
        usb.sx_len = 0;
        sx_collect = !sx_ready;
        sx_pos = 0;
    }
    if (!usb.sx_on)
        return;
    if (sx_collect && b != 0xF0 && b != 0xF7) {
        if (sx_pos < sizeof sx_frame)
            sx_frame[sx_pos++] = b;
        else
            sx_collect = 0;                             /* too long: not ours */
    }
    if (usb.sx_len < sizeof usb.sysex)
        usb.sysex[usb.sx_len++] = b;
    if (b == 0xF7) {
        if (usb.sx_len == 6u && usb.sysex[1] == 0x22 && usb.sysex[2] == 0x24 && usb.sysex[3] == 0x35 &&
            usb.sysex[4] == 0x7F)
            usb.ota_req = 1;
        else if (sx_collect && sx_pos) {
            sx_frame_len = sx_pos;
            RING_PUBLISH();
            sx_ready = 1;                               /* main loop: ota_take() / ed_service() */
        }
        sx_collect = 0;
        usb.sx_on = 0;
    }
}
#endif

/* one USB-MIDI event packet. (SIDOU: every packet goes to the main loop as it is, source 1 =
 * USB; app/midi_usb.c turns it into MIDI bytes there. With FELUCCA_OTA the SysEx is also
 * assembled here for the update protocol, as Felucca does.) */
static void midi_in_event(uint32_t pkt)
{
#if FELUCCA_OTA
    uint32_t cin = pkt & 15u, st = (pkt >> 8) & 0xFFu;
    if (cin >= 4u && cin <= 7u) {
        uint32_t k, nb = cin == 4u || cin == 7u ? 3u : cin == 6u ? 2u : 1u;
        for (k = 0; k < nb; k++)
            sysex_byte((uint8_t)(pkt >> (8u * (k + 1u))));
    } else if (st >= 0x80u && st < 0xF8u) {
        usb.sx_on = 0;                                 /* channel/system common also ends SysEx */
        sx_collect = 0;
    }
#endif
    midi_enqueue(pkt, 1u);
}

/* one EP1 OUT packet (<= 16 events) into the MIDI ring, or 0: fewer than 16 + 8 slots free, so the packet
 * stays (the host is NAKed) and is retried next poll; a burst never overflows the ring, and
 * 8 slots stay for TRS MIDI, which shares the ring and cannot wait (31250 baud: 2 bytes a poll).
 * The update loader never drains the ring (it wants SysEx only): no back-pressure there.
 * (SIDOU: midi_in_room() is the room in the includer's ring.) */
#define EP1_ROOM (16u + 8u)
static int ep1_take(const uint8_t *b, uint32_t n)
{
    uint32_t i;
#ifndef FELUCCA_LOADER
    if (midi_in_room() < EP1_ROOM)
        return 0;
#endif
    for (i = 0; i + 3u < n; i += 4u)
        midi_in_event((uint32_t)b[i] | (uint32_t)b[i + 1] << 8 | (uint32_t)b[i + 2] << 16 | (uint32_t)b[i + 3] << 24);
    return 1;
}

static void ep1_rx(void)                                /* leaves the packet (NAK) while the ring is too full */
{
    uint32_t csr, n;
    sie_wr(S_INDEX, 1);
    csr = sie_rd(S_RXCSR1) | (sie_rd(S_RXCSR2) << 8);
    if (!(csr & 1u)) {
        usb.rx_pend = 0;
        return;
    }
    n = sie_rd(S_RXCOUNT1) | (sie_rd(S_RXCOUNT2) << 8);
    if (n > 64u)
        n = 64u;
    fm1_usb_rx_sync();
    if (!ep1_take(ep1rx, n))
        return;                                         /* rx_pend stays: retried next poll */
    usb.rx_pend = 0;
    usb.rx_pkts++;
    csr = (csr & ~0x164u) | 0x10u;
    sie_wr(S_RXCSR1, csr & 0xFFu);
    sie_wr(S_RXCSR2, csr >> 8);
}

#if FELUCCA_OTA
/* ---- SysEx frames for the main loop (ota.c / editor.c hooks; felucca.c and the
 * update loader supply ota_now_ms / ota_idle) ---- */
static uint32_t ota_now_ms(void);
static void ota_idle(void);

static int ota_wire_send(const uint8_t *p, uint32_t n)   /* F0..F7 -> USB-MIDI SysEx packets */
{
    uint32_t i = 0, t0 = ota_now_ms();
    sx_busy = 1;
    while (i < n) {
        uint32_t k = n - i >= 3u ? 3u : n - i, pkt;
        uint32_t cin = k == 3u && i + 3u < n ? 4u : k == 3u ? 7u : 4u + k;   /* 4 continues; 5/6/7 end */
        pkt = cin | (uint32_t)p[i] << 8 | (k > 1u ? (uint32_t)p[i + 1] << 16 : 0u) |
              (k > 2u ? (uint32_t)p[i + 2] << 24 : 0u);
        while (so_w - so_r >= SXQ) {
            if (!*(volatile uint8_t *)&usb.config || ota_now_ms() - t0 > 200u) {
                sx_busy = 0;
                return -1;
            }
            ota_idle();
        }
        sx_out_q[so_w % SXQ] = pkt;
        RING_PUBLISH();
        so_w++;
        i += k;
    }
    sx_busy = 0;
    return 0;
}
static int ota_frame_get(const uint8_t **p, uint32_t *n)
{
    if (!sx_ready)
        return 0;
    RING_PUBLISH();                                     /* read the frame only after the flag */
    *p = sx_frame;
    *n = sx_frame_len;
    return 1;
}
static void ota_frame_done(void)
{
    RING_PUBLISH();                                     /* done with the frame before the ISR may refill it */
    sx_ready = 0;
}
#endif

static void ep1_tx(void)
{
    uint32_t csr, n = 0;
#if FELUCCA_OTA
    if (mo_w == mo_r && so_w == so_r)
        return;
#else
    if (mo_w == mo_r)
        return;
#endif
    sie_wr(S_INDEX, 1);
    csr = sie_rd(S_TXCSR1);
    if (csr & 0x01u)
        return;                                         /* previous packet still pending */
    if (csr & 0x80u)
        sie_wr(S_TXCSR1, csr & ~0x80u);
#if FELUCCA_OTA
    while (so_r != so_w && n < 64u) {                   /* SysEx first, never split by notes */
        uint32_t pkt = sx_out_q[so_r % SXQ];
        ep1tx[n] = (uint8_t)pkt;
        ep1tx[n + 1] = (uint8_t)(pkt >> 8);
        ep1tx[n + 2] = (uint8_t)(pkt >> 16);
        ep1tx[n + 3] = (uint8_t)(pkt >> 24);
        n += 4u;
        so_r++;
    }
    while (!sx_busy && so_r == so_w && mo_r != mo_w && n < 64u) {
#else
    while (mo_r != mo_w && n < 64u) {
#endif
        uint32_t pkt;
        RING_PUBLISH();                                 /* the audio ISR (producer) can preempt us: */
        pkt = midi_out_q[mo_r % MQ];                    /* slot read strictly between the index checks */
        ep1tx[n] = (uint8_t)pkt;
        ep1tx[n + 1] = (uint8_t)(pkt >> 8);
        ep1tx[n + 2] = (uint8_t)(pkt >> 16);
        ep1tx[n + 3] = (uint8_t)(pkt >> 24);
        n += 4u;
        RING_PUBLISH();
        mo_r++;
    }
    if (!n)
        return;
    fm1_usb_ep_send(1, ep1tx, n);
    sie_wr(S_TXCSR1, sie_rd(S_TXCSR1) | 0x01u);
    usb.tx_pkts++;
}



static void usb_poll(void)                              /* TIMER5 ISR, 2 kHz */
{
    uint32_t iu, it, ir;
    if (!usb.up)
        return;
    if (fm1_usb_sof_take()) {                           /* SOF pending: NOT a reliable "host is there" */
                                                        /* (it keeps firing with the cable out) */
        usb.sof_seen++;
        usb.no_sof = 0;
    } else if (++usb.no_sof > usb.max_gap) {
        usb.max_gap = usb.no_sof;
    }
    if (usb.config) {                                   /* the frame number only advances with a real host */
        static uint32_t tick;
        if (++tick >= 20u) {                            /* every 20 polls = 10 ms */
            uint16_t f;
            tick = 0;
            f = (uint16_t)(sie_rd(S_FRAME1) | (sie_rd(S_FRAME2) & 7u) << 8);
            if (f != usb.frame) {
                usb.frame = f;
                usb.frame_same = 0;
                usb.suspended = 0;
            } else if (++usb.frame_same >= 10u && !usb.suspended) {   /* 100 ms frozen: unplugged / host asleep */
                usb.suspended = 1;
                usb.frame_stalls++;
            }
        }
    }
    iu = sie_rd(S_INTRUSB);
#ifdef FELUCCA_LOADER
    it = sie_rd(S_INTRTX1) | sie_rd(S_INTRTX2) << 8;    /* the update loader reads both */
    ir = sie_rd(S_INTRRX1) | sie_rd(S_INTRRX2) << 8;
#else
    it = sie_rd(S_INTRTX1);                             /* EP0..7 only: skipping INTR*2 saves */
    ir = sie_rd(S_INTRRX1);                             /* 2 SIE round trips per poll */
#endif
    if (iu & 0x01u) {                                   /* suspend: the bus went idle */
        usb.suspends++;
        usb.suspended = 1;
    }
    if (iu & 0x02u)                                     /* resume */
        usb.suspended = 0;
    if (iu & 0x04u) {                                   /* bus reset */
        usb.resets++;
        sie_wr(S_FADDR, 0);
        usb.config = 0;
        usb.suspended = 0;
        mo_r = mo_w;                                    /* nothing stale for the next host */
        usb.sx_on = 0;
#if FELUCCA_OTA
        so_r = so_w;
        sx_collect = 0;
#endif
        usb.e0_tx = 0;
        usb.has_pend_addr = 0;
        usb.rx_pend = 0;
        fm1_usb_ep0_buf(ep0buf);
        sie_wr(S_INTRUSBE, 0x07);
        sie_wr(S_INTRTX1E, 0x01);
        sie_wr(S_INTRRX1E, 0);
    }
    if (it & 0x01u)
        ep0_service();
    if (ir & 0x02u)
        usb.rx_pend = 1;
    if (usb.rx_pend)                                    /* not every poll: 3 SIE round trips each */
        ep1_rx();
    if (usb.config)
        ep1_tx();
}

/* the attach after the reset. (SIDOU: split from usb_start, so usb_retry need not wait out the reset in
 * the main loop; it gives up at the third consecutive SIE timeout, which leaves USB off either way, instead
 * of timing out each remaining write, 20000 polls apiece.) */
static void usb_attach_now(void)
{
    static const uint8_t init[6][2] = {
        {S_POWER, 0x60},
        {S_INTRUSBE, 0x07},                             /* suspend, resume, reset (polled) */
        {S_INTRTX1E, 0x01},
        {S_INTRTX2E, 0},
        {S_INTRRX1E, 0},
        {S_INTRRX2E, 0},
    };
    uint32_t i;
    fm1_usb_attach(ep0buf);
    for (i = 0; i < 6u && usb.timeouts < 3u; i++)
        sie_wr(init[i][0], init[i][1]);
    fm1_usb_ep_enable(0x1Fu);
    usb.up = usb.timeouts < 3u;                         /* a dead SIE leaves USB off */
}

static void usb_start(void)                             /* boot, before the TIMER5 ISR polls */
{
    usb.timeouts = 0;
    fm1_usb_reset();                                    /* reset whatever the ROM left */
    fm1_delay_ms(25);
    usb_attach_now();
}

/* Powered on without a cable, the SIE does not answer and usb_start leaves
 * USB off: retry once a second from the main loop so a cable plugged in
 * later still enumerates. (SIDOU: the reset's 25 ms pass between two calls, not in a delay.) */
static void usb_retry(uint32_t now_ms)
{
    static uint32_t t;
    static uint8_t resetting;
    if (usb.up || usb.detached)
        return;
    if (resetting) {
        if (now_ms - t > 25u) {                         /* > 25 whole ms: at least 25 ms */
            resetting = 0;
            usb_attach_now();
        }
        return;
    }
    if (now_ms - t < 1000u)
        return;
    t = now_ms;
    usb.retries++;
    usb.timeouts = 0;
    fm1_usb_reset();
    resetting = 1;
}

/* (SIDOU: in the app, with interrupts on, masked here so that no TIMER5 usb_poll runs between up = 0
 * and the controller going off; the update loader runs with interrupts off throughout, and must not
 * turn them on.) */
static void usb_detach(void)
{
#ifndef FELUCCA_LOADER
    __asm__ volatile("cli" ::: "memory");
#endif
    usb.detached = 1;
    usb.up = 0;
    fm1_usb_off();
#ifndef FELUCCA_LOADER
    __asm__ volatile("csync\n\tsti" ::: "memory");
#endif
}

