// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SYSEX_H
#define SIDOU_SYSEX_H

#include "store.h"

#define SYSEX_PACKET_MAX 256u
#define SYSEX_TIMEOUT_MS 2000u
#define SYSEX_WAIT_EVERY_MS 2000u
#define SYSEX_QUIET_MAX_MS 60000u
enum { SYSEX_USB = 0, SYSEX_TRS = 1 };
enum { SYSEX_PENDING = -1 };
enum {
    SYSEX_MALFORMED = 1, SYSEX_CHECKSUM, SYSEX_SEQUENCE, SYSEX_BUSY,
    SYSEX_FULL, SYSEX_NOT_FOUND, SYSEX_IO, SYSEX_TIMEOUT, SYSEX_ABORTED,
    SYSEX_RECEIVING = 10, SYSEX_SAVED = 11,
    SYSEX_RSID = 12, SYSEX_MUS, SYSEX_BROKEN, SYSEX_TOO_LONG,
    SYSEX_WAITING = 16
};

typedef struct {
    void* ctx;
    int (*open)(void* ctx, uint8_t target, uint8_t index, int writing, uint32_t length);
    uint32_t (*size)(void* ctx);
    int (*read)(void* ctx, uint32_t offset, void* buf, uint32_t n);
    int (*complete)(void* ctx, const store_file* file);
    void (*close)(void* ctx);
    int (*send)(void* ctx, const uint8_t packet[4]);
    void (*toast)(void* ctx, uint8_t reason);
    void (*writing)(void* ctx, int on);
    int (*available)(void* ctx);
} sysex_hooks;

typedef struct {
    uint32_t last;
    uint16_t length;
    uint16_t packed;
    uint8_t stage, command, sequence, sum, previous, mask, group, bad;
} sysex_parser;

typedef struct {
    store* storage;
    sysex_hooks hooks;
    store_file file;
    sysex_parser parser[2];
    uint32_t now, last, length, offset, crc, expected_crc;
    uint16_t used, written, tx_length, tx_position;
    uint8_t buffer[SYSEX_PACKET_MAX];
    uint8_t version[4];
    uint8_t owner, state, sequence, target, index, opened, claimed, windows;
    uint8_t programming;
    uint8_t tx_command, tx_sequence, tx_sum, tx_active, tx_identity, tx_wait;
    uint8_t packet[4], packet_length, packet_final;
    uint8_t error, error_sequence, refused;
    uint8_t checking, held;
    uint8_t loud, waiting;
    uint32_t wait_start, wait_sent;
} sysex;

_Static_assert(sizeof(sysex) <= 1024, "SysEx state includes its one packet buffer");

void sysex_init(sysex* s, store* storage, const sysex_hooks* hooks,
                const uint8_t version[4], int windows);
void sysex_byte(sysex* s, uint8_t port, uint8_t byte, uint32_t now_ms);
void sysex_poll(sysex* s, uint32_t now_ms);
void sysex_reset(sysex* s);
void sysex_lost(sysex* s, uint8_t port);
int sysex_active(const sysex* s);

#endif
