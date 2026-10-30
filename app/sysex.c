// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sysex.h"

#include <string.h>

#include "psid/psid_file.h"

enum {
    IDLE, UPLOAD, PROGRAM, COMMIT, DELETE, PREPASS, META, DOWNLOAD, FINISH, DIRECTORY, WAIT,
    DRAIN, IMPORT, IMPORT_DRAIN, OPEN, UNLINK
};
/* STILL is WAIT on the wire (sysex.h); WAIT names a state above. */
enum { INQ = 1, DIR, GET, PUT, DATA, END, DEL, ABORT, STILL = 0x7D, ACK, NAK };

static const char instrument_name[] = "MIDI INSTRUMENT";
static const char song_name[] = "MIDI SONG";

static uint32_t le32(const uint8_t* p) {
    return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24;
}

static void put32(uint8_t* p, uint32_t value) {
    for (unsigned i = 0; i < 4; ++i) p[i] = (uint8_t)(value >> (8 * i));
}

static uint8_t reason(int result) {
    switch (result) {
    case STORE_BUSY: return SYSEX_BUSY;
    case STORE_FULL: return SYSEX_FULL;
    case STORE_NOT_FOUND: return SYSEX_NOT_FOUND;
    case STORE_INVALID: return SYSEX_MALFORMED;
    default: return SYSEX_IO;
    }
}

/* The files SysEx keeps to itself, and LAB's USER sounds (app/user_sound.h): never listed,
 * fetched, replaced or deleted over SysEx. */
static int reserved(const char* name) {
    return !strcmp(name, instrument_name) || !strcmp(name, song_name) ||
           !strcmp(name, STORE_SOUNDS_NAME);
}

static int name_read(const uint8_t* p, char* name) {
    unsigned i = 0;
    if (!p[0] || p[0] == ' ') return 0;
    while (i < 16 && p[i]) {
        if (p[i] < 0x20 || p[i] > 0x5F) return 0;
        name[i] = (char)p[i];
        ++i;
    }
    name[i] = 0;
    while (i < 16) if (p[i++]) return 0;
    return !reserved(name);
}

static int zeroes(const uint8_t* p, unsigned n) {
    for (unsigned i = 0; i < n; ++i) if (p[i]) return 0;
    return 1;
}

static void release(sysex* s) {
    s->tx_wait = 0;
    s->waiting = 0;
    s->checking = s->held = 0;
    if (s->state == OPEN || s->state == UNLINK) store_file_wait(s->storage, 0);
    if (s->state == IMPORT || s->state == IMPORT_DRAIN) {
        s->state = IMPORT_DRAIN;
        return;
    }
    if (s->claimed) {
        if (s->storage->f.fid == s->file.fid) {
            store_file_abort(s->storage);
            if (store_file_result(s->storage) == STORE_BUSY) {
                s->state = DRAIN;
                return;
            }
        }
    } else if ((s->state == DELETE || s->state == DRAIN) && store_busy(s->storage)) {
        s->state = DRAIN;
        return;
    }
    s->claimed = 0;
    if (s->programming && s->hooks.writing) s->hooks.writing(s->hooks.ctx, 0);
    s->programming = 0;
    if (s->opened && s->hooks.close) s->hooks.close(s->hooks.ctx);
    s->opened = 0;
    s->state = IDLE;
}

static void program_owner(sysex* s) {
    s->programming = 1;
    if (s->hooks.writing) s->hooks.writing(s->hooks.ctx, 1);
    if (s->hooks.toast) s->hooks.toast(s->hooks.ctx, SYSEX_RECEIVING);
}

static void fail(sysex* s, uint8_t why, uint8_t sequence) {
    release(s);
    if (s->owner == SYSEX_TRS) s->refused = 1;
    if (s->hooks.toast) s->hooks.toast(s->hooks.ctx, why);
    if (s->owner == SYSEX_USB && !s->error) {
        s->error = why;
        s->error_sequence = sequence;
    }
}

static int upload_failed(sysex* s) {
    if (!s->claimed || s->state == DRAIN) return 0;
    int result = store_file_result(s->storage);
    if (s->storage->f.fid != s->file.fid ||
        (result != STORE_BUSY && !(s->state == COMMIT && result == STORE_OK))) {
        fail(s, s->storage->f.fid != s->file.fid ? SYSEX_IO : reason(result), s->sequence);
        return 1;
    }
    return 0;
}

static void queue(sysex* s, uint8_t command, uint8_t sequence, uint16_t n, int wait) {
    s->tx_command = command;
    s->tx_sequence = sequence;
    s->tx_length = n;
    s->tx_position = 0;
    s->tx_sum = (uint8_t)(command + sequence);
    s->tx_identity = 0;
    s->tx_active = 1;
    s->tx_wait = (uint8_t)wait;
}

static void acknowledge(sysex* s, uint8_t sequence) {
    if (s->owner == SYSEX_USB) queue(s, ACK, sequence, 0, 0);
    else s->last = s->now;
}

/* STORE_BUSY only because a record is being written (a save, a collection's copy), which
 * waits for quiet while the device sounds: the transfer owns the programs, so its holds finish
 * the record, and it waits for it (OPEN, UNLINK; store_file_wait). The pool's BUSY is final. */
static int record_busy(const sysex* s, int result) {
    return result == STORE_BUSY && store_busy(s->storage) &&
           store_file_result(s->storage) != STORE_BUSY;
}

/* Waits for the record, as the owner of its programs, at most SYSEX_TIMEOUT_MS. */
static void wait_record(sysex* s, uint8_t state) {
    program_owner(s);
    store_file_wait(s->storage, 1);
    s->state = state;
    s->last = s->now;
}

/* A PUT's sectors claimed (store_file_begin said yes): ACK its metadata; a .sid's first DATA,
 * held in buffer, is programmed (its ACK once it is). */
static void claim(sysex* s) {
    uint8_t seq = s->sequence;
    s->claimed = 1;
    s->file.fid = s->storage->f.fid;
    if (s->held) {
        s->held = 0;
        s->state = PROGRAM;
        return;
    }
    s->state = UPLOAD;
    s->sequence = (seq + 1) & 0x7F;
    acknowledge(s, seq);
}

/* A .sid's first DATA (n bytes from the file's start, in buffer): the header checked by
 * psid_parse (the first 128 bytes and the declared length are all it reads), then the sectors
 * claimed as a PUT's are and the packet programmed. A refusal comes before any claim, so
 * nothing of a refused tune is ever programmed. */
static void sid_first(sysex* s, unsigned n, uint8_t seq) {
    psid_header h;
    s->checking = 0;
    if (n < (s->length < 128u ? s->length : 128u)) {
        fail(s, SYSEX_MALFORMED, seq);
        return;
    }
    int why = psid_parse(s->buffer, s->length, &h);
    if (why != PSID_OK) {
        fail(s, why == PSID_E_RSID ? SYSEX_RSID : why == PSID_E_MUS ? SYSEX_MUS :
                why == PSID_E_TOO_LONG ? SYSEX_TOO_LONG : SYSEX_BROKEN, seq);
        return;
    }
    int result = store_file_begin(s->storage, s->file.name, s->file.type, s->length);
    s->held = 1;
    if (record_busy(s, result)) wait_record(s, OPEN);
    else if (result) fail(s, reason(result), seq);
    else {
        program_owner(s);
        claim(s);
    }
}

static void accept(sysex* s) {
    int draining = s->state == IMPORT_DRAIN;
    int result = 0;
    if (s->target) {
        result = s->hooks.complete(s->hooks.ctx, &s->file);
        if (result == SYSEX_PENDING) return;
    }
    s->state = COMMIT;
    if (draining) {
        release(s);
        return;
    }
    if (result) {
        fail(s, result < 0 ? reason(result) : (uint8_t)result, s->sequence);
        return;
    }
    if (s->hooks.toast) s->hooks.toast(s->hooks.ctx, SYSEX_SAVED);
    release(s);
    acknowledge(s, s->sequence);
}

/* Generate the packed wire representation directly from the one decoded buffer. */
static uint8_t output_byte(sysex* s) {
    unsigned pos = s->tx_position++;
    if (s->tx_identity) {
        static const uint8_t prefix[] = {0xF0, 0x7E, 0x7F, 6, 2, 0x7D, 0x53, 0x44, 1, 0};
        if (pos < sizeof(prefix)) return prefix[pos];
        return pos < 14 ? s->version[pos - 10] : 0xF7;
    }
    static const uint8_t prefix[] = {0xF0, 0x7D, 0x53, 0x44};
    if (pos < 4) return prefix[pos];
    if (pos == 4) return s->tx_command;
    if (pos == 5) return s->tx_sequence;
    unsigned packed = s->tx_length + (s->tx_length + 6u) / 7u;
    if (pos == 6 + packed) return (uint8_t)(-s->tx_sum & 0x7F);
    if (pos > 6 + packed) return 0xF7;
    unsigned at = pos - 6;
    unsigned group = at / 8;
    unsigned item = at % 8;
    uint8_t byte = 0;
    if (!item) {
        for (unsigned i = 0; i < 7 && group * 7 + i < s->tx_length; ++i)
            byte |= (uint8_t)((s->buffer[group * 7 + i] >> 7) << i);
    } else byte = s->buffer[group * 7 + item - 1] & 0x7F;
    s->tx_sum = (uint8_t)(s->tx_sum + byte);
    return byte;
}

/* As much of the frame as send takes, to its F7: a 256-byte DATA is 101 event packets, and one
 * a poll held each reply to the poll's rate (about 200 ms a packet on the FM-1, whose USB ring
 * takes 64 and sends 16 a poll). send's back-pressure bounds a call's work. */
static void transmit(sysex* s) {
    if (!s->hooks.send) return;
    while (s->tx_active) {
        if (!s->packet_length) {
            unsigned total = s->tx_identity ? 15 : 8 + s->tx_length + (s->tx_length + 6u) / 7u;
            unsigned n = total - s->tx_position;
            if (n > 3) n = 3;
            s->packet_final = s->tx_position + n == total;
            s->packet[0] = (uint8_t)(s->packet_final ? 4 + n : 4);
            memset(s->packet + 1, 0, 3);
            for (unsigned i = 0; i < n; ++i) s->packet[1 + i] = output_byte(s);
            s->packet_length = (uint8_t)n;
        }
        if (!s->hooks.send(s->hooks.ctx, s->packet)) return;
        s->packet_length = 0;
        if (s->packet_final) {
            s->tx_active = 0;
            s->tx_identity = 0;
            s->last = s->now;
        }
    }
}

void sysex_init(sysex* s, store* storage, const sysex_hooks* hooks,
                const uint8_t version[4], int windows) {
    memset(s, 0, sizeof(*s));
    s->storage = storage;
    if (hooks) s->hooks = *hooks;
    for (unsigned i = 0; i < 4; ++i) s->version[i] = version[i] & 0x7F;
    s->windows = windows != 0;
}

void sysex_reset(sysex* s) {
    release(s);
    memset(s->parser, 0, sizeof(s->parser));
    s->tx_active = s->packet_length = s->error = 0;
    s->refused = 0;
}

void sysex_lost(sysex* s, uint8_t port) {
    if (port > SYSEX_TRS) return;
    if ((s->state || s->tx_active || s->parser[port].stage) && s->owner == port) {
        s->parser[port].stage = 0;
        fail(s, SYSEX_ABORTED, s->sequence);
    }
}

int sysex_active(const sysex* s) {
    return s->state || s->tx_active || s->error || s->parser[0].stage || s->parser[1].stage;
}

static int target_read(sysex* s, int writing, uint32_t length) {
    const uint8_t* p = s->buffer;
    s->target = p[0];
    s->index = p[1];
    if (!s->target) {
        if (s->index || !name_read(p + 2, s->file.name)) return SYSEX_MALFORMED;
    } else {
        if (s->target > 2 || !zeroes(p + 2, 16) ||
            (s->target == 1 ? !s->index || s->index > 63 : s->index != 0))
            return SYSEX_MALFORMED;
        if (!s->hooks.open || !s->hooks.close ||
            (writing ? !s->hooks.complete : !s->hooks.size || !s->hooks.read)) return SYSEX_BUSY;
        int result = s->hooks.open(s->hooks.ctx, s->target, s->index, writing, length);
        if (result) return result;
        s->opened = 1;
        strcpy(s->file.name, s->target == 1 ? instrument_name : song_name);
    }
    return 0;
}

static void metadata(sysex* s) {
    memset(s->buffer, 0, 27);
    s->buffer[0] = s->target;
    s->buffer[1] = s->index;
    if (!s->target) memcpy(s->buffer + 2, s->file.name, 16);
    s->buffer[18] = s->file.type;
    put32(s->buffer + 19, s->length);
    put32(s->buffer + 23, s->expected_crc);
    queue(s, PUT, s->sequence, 27, 1);
    s->state = WAIT;
}

static void command(sysex* s, sysex_parser* p) {
    unsigned n = p->length;
    uint8_t cmd = p->command;
    uint8_t seq = p->sequence;
    if (s->owner == SYSEX_TRS && cmd != PUT && cmd != DATA && cmd != END && cmd != ABORT) return;
    if (cmd == ABORT && !n) {
        fail(s, SYSEX_ABORTED, seq);
        return;
    }
    if (s->tx_active || s->error) {
        fail(s, SYSEX_BUSY, seq);
        return;
    }
    if (s->tx_wait) {
        if (cmd != ACK || n || seq != s->tx_sequence) {
            fail(s, cmd == ACK ? SYSEX_SEQUENCE : SYSEX_BUSY, seq);
            return;
        }
        s->tx_wait = 0;
        s->sequence = (seq + 1) & 0x7F;
        s->last = s->now;
        if (s->tx_command == END) release(s);
        else if (s->tx_command == DIR) s->state = DIRECTORY;
        else s->state = s->offset == s->length ? FINISH : DOWNLOAD;
        return;
    }
    if (s->state) {
        if (s->state != UPLOAD) {
            fail(s, SYSEX_BUSY, seq);
            return;
        }
        if (seq != s->sequence) {
            fail(s, SYSEX_SEQUENCE, seq);
            return;
        }
        if (cmd == DATA && n && n <= s->length - s->offset && !(s->offset & 15u) &&
            (!(n & 15u) || n == s->length - s->offset)) {
            s->used = (uint16_t)n;
            s->written = 0;
            s->crc = store_crc32(s->crc, s->buffer, n);
            s->offset += n;
            if (s->checking) sid_first(s, n, seq);
            else s->state = PROGRAM;
            return;
        }
        if (cmd == END && !n && s->offset == s->length && s->crc == s->expected_crc) {
            int result = store_file_end(s->storage);
            if (result) fail(s, reason(result), seq);
            else s->state = COMMIT;
            return;
        }
        fail(s, cmd == END && !n ? SYSEX_IO : SYSEX_MALFORMED, seq);
        return;
    }
    s->sequence = seq;
    if ((cmd == PUT || cmd == DEL) && s->hooks.available) {
        int result = s->hooks.available(s->hooks.ctx);
        if (result) {
            fail(s, (uint8_t)result, seq);
            return;
        }
    }
    if (cmd == INQ && !n) {
        store_stats stats;
        store_stats_get(s->storage, &stats);
        s->buffer[0] = 1;
        memcpy(s->buffer + 1, s->version, 4);
        s->buffer[5] = STORE_FORMAT;
        s->buffer[6] = 0;
        put32(s->buffer + 7, stats.free);
        s->buffer[11] = (uint8_t)stats.pool;
        s->buffer[12] = s->windows;
        queue(s, INQ, seq, 13, 0);
    } else if (cmd == DIR && !n) {
        memset(&s->file, 0, sizeof(s->file));
        s->state = DIRECTORY;
        s->last = s->now;
    } else if ((cmd == GET && n == 18) || (cmd == PUT && n == 27)) {
        int writing = cmd == PUT;
        uint32_t length = writing ? le32(s->buffer + 19) : 0;
        uint32_t crc = writing ? le32(s->buffer + 23) : 0;
        uint8_t type = writing ? s->buffer[18] : 1;
        if (writing && (!type || type > 127 || (s->buffer[0] && type != 1))) {
            fail(s, SYSEX_MALFORMED, seq);
            return;
        }
        if (length > STORE_FILE_MAX) {
            fail(s, SYSEX_FULL, seq);
            return;
        }
        int result = target_read(s, writing, length);
        if (result) {
            fail(s, (uint8_t)result, seq);
            return;
        }
        s->offset = s->crc = 0;
        s->length = length;
        s->expected_crc = crc;
        if (writing) {
            /* GTI5 staging must not appear among the store's playable songs. */
            s->file.type = s->target == 1 ? STORE_FILE_GTI : type;
            if (!s->target && type == STORE_FILE_SID) {
                /* A .sid: acknowledged now, claimed once its first DATA's header checks. */
                if (length < PSID_HEADER_SIZE_V1) {
                    fail(s, SYSEX_BROKEN, seq);
                    return;
                }
                s->checking = 1;
                s->state = UPLOAD;
                s->sequence = (seq + 1) & 0x7F;
                acknowledge(s, seq);
                s->last = s->now;
                return;
            }
            result = store_file_begin(s->storage, s->file.name, s->file.type, length);
            if (record_busy(s, result)) wait_record(s, OPEN);
            else if (result) {
                fail(s, reason(result), seq);
                return;
            } else {
                program_owner(s);
                claim(s);
            }
        } else if (!s->target) {
            result = store_file_open(s->storage, s->file.name, &s->file);
            if (result) fail(s, reason(result), seq);
            else {
                s->length = s->file.length;
                s->expected_crc = s->file.crc;
                s->state = META;
            }
        } else {
            s->length = s->hooks.size(s->hooks.ctx);
            s->file.type = 1;
            if (s->length > STORE_FILE_MAX) fail(s, SYSEX_FULL, seq);
            else s->state = PREPASS;
        }
        s->last = s->now;
    } else if (cmd == DEL && n == 16) {
        if (!name_read(s->buffer, s->file.name)) fail(s, SYSEX_MALFORMED, seq);
        else {
            s->expected_crc = s->storage->errors;
            int result = store_file_delete(s->storage, s->file.name);
            if (record_busy(s, result)) wait_record(s, UNLINK);
            else if (result) fail(s, reason(result), seq);
            else {
                program_owner(s);
                s->state = DELETE;
            }
        }
    } else fail(s, SYSEX_MALFORMED, seq);
}

/* Keep one packed byte back: only F7 identifies the checksum, not its value. */
static void packed_byte(sysex* s, sysex_parser* p, uint8_t byte) {
    ++p->packed;
    if (!p->group) {
        p->mask = byte;
        p->group = 1;
        return;
    }
    if (p->length == SYSEX_PACKET_MAX) {
        p->bad = SYSEX_MALFORMED;
        return;
    }
    if (!s->tx_active && s->state != PROGRAM && s->state != COMMIT && !s->held)
        s->buffer[p->length] = byte | (uint8_t)((p->mask & 1) << 7);
    else p->bad = SYSEX_BUSY;
    ++p->length;
    p->mask >>= 1;
    if (++p->group == 8) p->group = 0;
}

/* The clock only moves forward: sysex_poll's now_ms comes from the start of the caller's
 * main-loop pass, and the bytes taken in that pass may carry a later millisecond, so a smaller
 * now_ms than the last one seen is kept out (else now - last wraps and times the transfer out). */
static void advance(sysex* s, uint32_t now_ms) {
    if ((int32_t)(now_ms - s->now) > 0) s->now = now_ms;
}

static int expired(sysex* s) {
    for (unsigned i = 0; i < 2; ++i) {
        sysex_parser* p = &s->parser[i];
        if (p->stage && s->now - p->last >= SYSEX_TIMEOUT_MS) {
            uint8_t stage = p->stage;
            p->stage = 0;
            if ((stage >= 4 && stage < 20) || s->state) {
                fail(s, SYSEX_TIMEOUT, p->sequence);
                return 1;
            }
        }
    }
    if (s->state && !s->tx_active && s->state != PROGRAM && s->state != COMMIT && s->state != DELETE &&
        s->state != PREPASS && s->state != DRAIN && s->state != IMPORT && s->state != IMPORT_DRAIN &&
        s->state != OPEN && s->state != UNLINK && s->now - s->last >= SYSEX_TIMEOUT_MS) {
        fail(s, SYSEX_TIMEOUT, s->sequence);
        return 1;
    }
    return 0;
}

void sysex_byte(sysex* s, uint8_t port, uint8_t byte, uint32_t now_ms) {
    if (port > SYSEX_TRS || byte >= 0xF8) return;
    advance(s, now_ms);
    if (sysex_active(s) && s->owner != port) return;
    if (upload_failed(s)) return;
    if (expired(s)) return;
    sysex_parser* p = &s->parser[port];
    if (byte == 0xF0) {
        if (p->stage >= 4 && p->stage < 20) fail(s, SYSEX_MALFORMED, p->sequence);
        memset(p, 0, sizeof(*p));
        if (s->state == PROGRAM || s->state == COMMIT || s->tx_active) p->bad = SYSEX_BUSY;
        p->stage = 1;
        p->last = now_ms;
        s->owner = port;
        return;
    }
    if (!p->stage) return;
    p->last = now_ms;
    if (byte == 0xF7) {
        uint8_t stage = p->stage;
        p->stage = 0;
        if (stage == 24) {
            if (port == SYSEX_USB && !s->state && !s->tx_active && !s->error) {
                queue(s, 0, 0, 0, 0);
                s->tx_identity = 1;
            }
        } else if (stage >= 4 && stage < 20) {
            if (p->command == ABORT && !p->length && p->bad == SYSEX_BUSY) p->bad = 0;
            if (stage != 7 || p->group == 1 || p->mask || p->bad)
                fail(s, p->bad ? p->bad : SYSEX_MALFORMED, p->sequence);
            else if (p->sum & 0x7F) fail(s, SYSEX_CHECKSUM, p->sequence);
            else command(s, p);
        }
        return;
    }
    if (byte & 0x80) {
        if (p->stage >= 4 && p->stage < 20) fail(s, SYSEX_MALFORMED, p->sequence);
        p->stage = 0;
        return;
    }
    switch (p->stage) {
    case 1: p->stage = byte == 0x7D ? 2 : byte == 0x7E ? 20 : 0; break;
    case 2: p->stage = byte == 0x53 ? 3 : 0; break;
    case 3: p->stage = byte == 0x44 ? 4 : 0; break;
    case 4:
        p->command = byte;
        if (port == SYSEX_TRS && s->refused && (byte == DATA || byte == END)) {
            p->stage = 0;
            break;
        }
        if (port == SYSEX_USB || byte == PUT || byte == ABORT) s->refused = 0;
        if (port == SYSEX_TRS && byte != PUT && byte != DATA && byte != END && byte != ABORT) {
            p->stage = 0;
            break;
        }
        p->sum = byte;
        p->stage = 5;
        break;
    case 5: p->sequence = byte; p->sum = (uint8_t)(p->sum + byte); p->stage = 6; break;
    case 6: p->previous = byte; p->sum = (uint8_t)(p->sum + byte); p->stage = 7; break;
    case 7:
        if (p->packed < 294) packed_byte(s, p, p->previous);
        else p->bad = SYSEX_MALFORMED;
        p->previous = byte;
        p->sum = (uint8_t)(p->sum + byte);
        break;
    case 20: p->stage = byte == 0 || byte == 0x7F ? 21 : 0; break;
    case 21: p->stage = byte == 6 ? 22 : 0; break;
    case 22: p->stage = byte == 1 ? 24 : 0; break;
    default: p->stage = 0; break;
    }
}

void sysex_poll(sysex* s, uint32_t now_ms) {
    advance(s, now_ms);
    upload_failed(s);
    expired(s);
    if (s->claimed && s->storage->f.fid == s->file.fid && (s->state == PROGRAM || s->tx_active))
        s->storage->f.idle = 0;
    if (s->state == IMPORT_DRAIN) accept(s);
    if (s->tx_active) {
        transmit(s);
        return;
    }
    if (s->error) {
        store_stats stats;
        store_stats_get(s->storage, &stats);
        s->buffer[0] = s->error;
        s->buffer[1] = (uint8_t)stats.pool;
        s->error = 0;
        queue(s, NAK, s->error_sequence, 2, 0);
        transmit(s);
        return;
    }
    if (s->tx_wait) return;
    /* Loud (sysex.h): a write whose programs are due waits, and says so. */
    if (s->loud && s->programming &&
        (s->state == PROGRAM || s->state == COMMIT || s->state == DELETE || s->state == OPEN ||
         s->state == UNLINK)) {
        if (!s->waiting) {
            s->waiting = 1;
            s->wait_start = s->now;
            s->wait_sent = s->now - SYSEX_WAIT_EVERY_MS;
        }
        if (s->now - s->wait_start >= SYSEX_QUIET_MAX_MS) {
            fail(s, SYSEX_BUSY, s->sequence);
            return;
        }
        s->last = s->now; /* OPEN's and UNLINK's two seconds count no wait */
        if (s->now - s->wait_sent >= SYSEX_WAIT_EVERY_MS) {
            s->wait_sent = s->now;
            if (s->hooks.toast) s->hooks.toast(s->hooks.ctx, SYSEX_WAITING);
            if (s->owner == SYSEX_USB) {
                queue(s, STILL, s->sequence, 0, 0);
                transmit(s);
                return;
            }
        }
        if (s->state == PROGRAM) return; /* nothing more written until quiet */
    } else if (s->waiting) {
        s->waiting = 0;
        if (s->hooks.toast) s->hooks.toast(s->hooks.ctx, SYSEX_RECEIVING);
    }
    if (s->state == OPEN || s->state == UNLINK) {
        /* Tried again until the record commits; still busy after SYSEX_TIMEOUT_MS: BUSY. */
        int result = s->state == OPEN ?
            store_file_begin(s->storage, s->file.name, s->file.type, s->length) :
            store_file_delete(s->storage, s->file.name);
        if (!result) {
            store_file_wait(s->storage, 0);
            if (s->state == OPEN) claim(s);
            else s->state = DELETE;
        } else if (!record_busy(s, result) || s->now - s->last >= SYSEX_TIMEOUT_MS) {
            fail(s, reason(result), s->sequence);
        }
        return;
    }
    if (s->state == PROGRAM || s->state == COMMIT) {
        int result = store_file_result(s->storage);
        if (result != STORE_BUSY && !(s->state == COMMIT && result == STORE_OK)) {
            fail(s, reason(result), s->sequence);
            return;
        }
        if (s->state == PROGRAM) {
            s->written += (uint16_t)store_file_write(s->storage, s->buffer + s->written,
                                                   s->used - s->written);
            if (s->written == s->used) store_file_flush(s->storage);
            if (s->written == s->used && !store_file_pending(s->storage)) {
                s->state = UPLOAD;
                acknowledge(s, s->sequence);
                s->sequence = (s->sequence + 1) & 0x7F;
            }
        } else if (result == STORE_OK) {
            s->claimed = 0;
            if (s->target) {
                result = store_file_open(s->storage, s->file.name, &s->file);
                if (result) {
                    fail(s, reason(result), s->sequence);
                    return;
                }
            }
            s->state = IMPORT;
            accept(s);
        }
    } else if (s->state == DELETE) {
        if (!store_busy(s->storage)) {
            if (s->storage->errors != s->expected_crc) fail(s, SYSEX_IO, s->sequence);
            else {
                if (s->hooks.toast) s->hooks.toast(s->hooks.ctx, SYSEX_SAVED);
                release(s);
                acknowledge(s, s->sequence);
            }
        }
    } else if (s->state == IMPORT) accept(s);
    else if (s->state == DRAIN) release(s);
    else if (s->state == PREPASS || s->state == DOWNLOAD) {
        uint32_t n = s->length - s->offset;
        if (n > SYSEX_PACKET_MAX) n = SYSEX_PACKET_MAX;
        int result = s->target ? s->hooks.read(s->hooks.ctx, s->offset, s->buffer, n) :
            store_file_read(s->storage, &s->file, s->offset, s->buffer, n);
        if (result != (int)n) {
            fail(s, SYSEX_IO, s->sequence);
            return;
        }
        s->offset += n;
        if (s->state == PREPASS) {
            s->crc = store_crc32(s->crc, s->buffer, n);
            if (s->offset == s->length) {
                s->expected_crc = s->crc;
                s->offset = 0;
                s->state = META;
                s->last = s->now;
            }
        } else {
            queue(s, DATA, s->sequence, (uint16_t)n, 1);
            s->state = WAIT;
        }
    } else if (s->state == META) metadata(s);
    else if (s->state == FINISH) {
        queue(s, END, s->sequence, 0, 1);
        s->state = WAIT;
    } else if (s->state == DIRECTORY) {
        int result;
        do result = store_dir_next(s->storage, &s->file);
        while (result == 1 && reserved(s->file.name));
        if (result < 0) fail(s, SYSEX_IO, s->sequence);
        else if (!result) {
            queue(s, END, s->sequence, 0, 1);
            s->state = WAIT;
        } else {
            memcpy(s->buffer, s->file.name, 16);
            s->buffer[16] = s->file.type;
            put32(s->buffer + 17, s->file.length);
            put32(s->buffer + 21, s->file.crc);
            queue(s, DIR, s->sequence, 25, 1);
            s->state = WAIT;
        }
    }
}
