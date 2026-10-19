// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SYSEX_H
#define SIDOU_SYSEX_H

#include "store.h"

#define SYSEX_PACKET_MAX 256u
#define SYSEX_TIMEOUT_MS 2000u
enum { SYSEX_USB = 0, SYSEX_TRS = 1 };
enum { SYSEX_PENDING = -1 };
enum {
    SYSEX_MALFORMED = 1, SYSEX_CHECKSUM, SYSEX_SEQUENCE, SYSEX_BUSY,
    SYSEX_FULL, SYSEX_NOT_FOUND, SYSEX_IO, SYSEX_TIMEOUT, SYSEX_ABORTED,
    SYSEX_RECEIVING = 10, SYSEX_SAVED = 11
};

/* All hooks run in the main loop. Errors use the positive SYSEX_* reasons.
 * open freezes/prepares target 1/2 (index is 1..63 for instruments), before a
 * claim or read; length is the declared upload size, zero for downloads.
 * Every successful open is paired with close, including errors and lost input.
 * size/read describe the frozen download; read returns exactly n, or a negative
 * error. Indexed reads allow a bounded CRC prepass, without buffering the dump.
 * complete accepts a committed temporary file (MIDI INSTRUMENT / MIDI SONG).
 * It returns zero on success, a positive SYSEX_* refusal without modifying on
 * refusal, or SYSEX_PENDING while bounded import/load work continues. Pending
 * calls repeat once per poll with the same committed file, keeping ownership,
 * the source open and the quiet veto until acceptance/refusal (no sender timeout).
 * After abort/reset/lost, pending work still drains before close; it no longer
 * produces SAVED or an END ACK. New requests cannot replace its pinned file.
 * send accepts one USB-MIDI event packet, returns 1 accepted / 0 backpressure.
 * Its bytes must be copied before returning. toast receives a SYSEX_* reason/status.
 * writing(1) follows a successful PUT claim (or accepted DEL), before any ACK;
 * writing(0) releases program ownership after completion or failure. Ownership
 * covers any irrevocable commit still draining after abort/reset. It permits
 * explicit program holds, never maintenance erases. open/read/close are unchanged.
 * available checks the shared save/load freeze before any PUT/DEL claim, including
 * target 0. Zero permits the claim, a positive SYSEX_* refuses without ownership.
 */
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
} sysex;

_Static_assert(sizeof(sysex) <= 1024, "SysEx state includes its one packet buffer");

/* State may live in POOL. windows reports the platform's write-window capability;
 * zero still accepts explicit PUT/DEL through writing's program ownership.
 * Version bytes are seven-bit. Nonfinal upload DATA lengths are multiples of 16,
 * and each accepted DATA starts at a 16-byte-aligned file offset. Only the final
 * DATA may be short; packing itself supports every length from zero to 256.
 * poll does protocol work only: the platform independently calls store_poll.
 * now_ms is a wrapping monotonic millisecond clock, also passed to byte.
 * Sender idle time starts at the accepted final USB ACK packet (TRS: completion);
 * storage/import work and transmit backpressure do not consume that interval.
 * Feed raw MIDI bytes, including F0/F7; real-time bytes are ignored everywhere.
 */
void sysex_init(sysex* s, store* storage, const sysex_hooks* hooks,
                const uint8_t version[4], int windows);
void sysex_byte(sysex* s, uint8_t port, uint8_t byte, uint32_t now_ms);
void sysex_poll(sysex* s, uint32_t now_ms);
/* reset cancels our own work only. lost cancels only the affected input owner.
 * Irrevocable commits and pending imports stay active, keeping ownership until drained. */
void sysex_reset(sysex* s);
void sysex_lost(sysex* s, uint8_t port);
int sysex_active(const sysex* s);

#endif
