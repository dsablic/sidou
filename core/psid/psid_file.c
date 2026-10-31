/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-FileCopyrightText: 2026 Joakim Eriksson
 * SPDX-License-Identifier: GPL-3.0-or-later AND BSD-2-Clause */
#include "psid_file.h"

#include <string.h>

#define MAGIC_LEN 4u
#define SONGS_MAX 256u
#define META_OFF 0x16u
#define META_LEN 96u
#define FLAGS_OFF 0x76u
#define FLAG_MUS 0x0001u
#define FLAG_PLAYSID 0x0002u
#define FLAG_LENGTHS 0x0400u
#define FLAG_FM_OPL 0x0800u
#define SID_ADDR_MAX 15u
#define SID_END 0xDFE0u

static uint32_t be16(const uint8_t* p)
{
    return (uint32_t)p[0] << 8 | p[1];
}

static char shown(uint8_t c)
{
    return (c >= 0x20u && c < 0x7Fu) ? (char)c : '?';
}

static void put_string(char* out, const uint8_t* s, uint32_t len)
{
    uint32_t i;
    if (len > PSID_STRING_MAX - 1u)
        len = PSID_STRING_MAX - 1u;
    for (i = 0; i < len; i++)
        out[i] = shown(s[i]);
    out[len] = 0;
}

static uint32_t cstr_len(const uint8_t* s, uint32_t max)
{
    uint32_t i = 0;
    while (i < max && s[i])
        i++;
    return i;
}

static int packed_strings(const uint8_t* m, uint32_t st[3], uint32_t len[3])
{
    uint32_t pos = 0, f;
    for (f = 0; f < 3; f++) {
        if (f)
            while (pos < META_LEN && m[pos] == 0)
                pos++;
        st[f] = pos;
        len[f] = cstr_len(m + pos, META_LEN - pos);
        if (len[f] == 0)
            return 0;
        pos += len[f];
        if (f < 2 && pos >= META_LEN)
            return 0;
    }
    return 1;
}

static void read_strings(const uint8_t* b, uint32_t version, psid_header* h)
{
    const uint8_t* m = b + META_OFF;
    uint32_t st[3], len[3];
    if (version == 5u && packed_strings(m, st, len)) {
        put_string(h->name, m + st[0], len[0]);
        put_string(h->author, m + st[1], len[1]);
        put_string(h->released, m + st[2], len[2]);
        return;
    }
    put_string(h->name, m, cstr_len(m, 32));
    put_string(h->author, m + 32, cstr_len(m + 32, 32));
    put_string(h->released, m + 64, cstr_len(m + 64, 32));
}

static int sid_byte_valid(uint32_t a)
{
    return !(a & 1u) && ((a >= 0x42u && a <= 0x7Eu) || (a >= 0xE0u && a <= 0xFEu));
}

static uint32_t v5_addresses(uint32_t cfg, uint32_t start, uint32_t want, uint16_t out[SID_ADDR_MAX])
{
    static const uint16_t steps[7][2] = {{0x20, 0x20}, {0x40, 0x40}, {0x80, 0x80}, {0x100, 0x100},
                                         {0x20, 0x60}, {0x20, 0xE0}, {0x100, 0x300}};
    uint32_t addr = 0xD400u, k = 1, i = 0;
    out[0] = 0xD400u;
    if (sid_byte_valid(start)) {
        addr = 0xD000u + start * 16u;
        out[k++] = (uint16_t)addr;
    }
    while (k < want) {
        addr += steps[cfg][i++ & 1u];
        if (addr >= 0xD800u && addr < 0xDE00u)
            addr += 0x600u;
        if (addr > SID_END)
            break;
        out[k++] = (uint16_t)addr;
    }
    return k;
}

int psid_parse(const uint8_t* b, uint32_t n, psid_header* out)
{
    psid_header t;
    psid_header* h = &t;
    uint32_t version, off, flags = 0, avail, table, load;

    memset(out, 0, sizeof *out);
    memset(&t, 0, sizeof t);
    if (n < PSID_HEADER_SIZE_V1)
        return PSID_E_NOT_SID;
    if (memcmp(b, "RSID", MAGIC_LEN) == 0)
        return PSID_E_RSID;
    if (memcmp(b, "PSID", MAGIC_LEN) != 0)
        return PSID_E_NOT_SID;

    version = be16(b + 4);
    off = be16(b + 6);
    if (version < 1u || version > 5u)
        return PSID_E_HEADER;
    if (off != (version == 1u ? PSID_HEADER_SIZE_V1 : PSID_HEADER_SIZE_V2) || off > n)
        return PSID_E_HEADER;
    h->songs = (uint16_t)be16(b + 14);
    if (h->songs < 1u || h->songs > SONGS_MAX)
        return PSID_E_HEADER;
    if (version >= 2u) {
        if (n < PSID_HEADER_SIZE_V2)
            return PSID_E_HEADER;
        flags = be16(b + FLAGS_OFF);
    }
    if (flags & FLAG_MUS)
        return PSID_E_MUS;

    avail = n - off;
    if (version == 5u && (flags & FLAG_LENGTHS)) {
        table = (uint32_t)h->songs * 4u;
        if (table > avail)
            return PSID_E_HEADER;
        avail -= table;
        h->lengths_off = off + avail;
    }
    load = be16(b + 8);
    if (load == 0u) {
        if (avail < 2u)
            return PSID_E_HEADER;
        load = (uint32_t)b[off] | (uint32_t)b[off + 1u] << 8;
        off += 2u;
        avail -= 2u;
    }
    if (avail == 0u || load + avail > 0x10000u)
        return PSID_E_TOO_LONG;
    if (load < 2u)
        return PSID_E_LOAD;

    h->version = (uint16_t)version;
    h->load = (uint16_t)load;
    h->init = (uint16_t)be16(b + 10);
    if (h->init == 0u)
        h->init = h->load;
    h->play = (uint16_t)be16(b + 12);
    h->start = (uint16_t)be16(b + 16);
    if (h->start < 1u || h->start > h->songs) {
        h->start = 1u;
        h->warn |= PSID_W_START;
    }
    h->speed = (uint32_t)be16(b + 18) << 16 | be16(b + 20);
    h->flags = (uint16_t)flags;
    h->data_off = off;
    h->data_len = avail;
    h->sids = 1u;
    read_strings(b, version, h);

    h->clock = (uint8_t)(flags >> 2 & 3u);
    h->model1 = (uint8_t)(flags >> 4 & 3u);
    h->model2 = h->model3 = h->model1;
    if (flags & FLAG_PLAYSID)
        h->warn |= PSID_W_PLAYSID;
    h->speed_repeats = (uint8_t)(version == 1u || (flags & FLAG_PLAYSID));

    if (version == 3u || version == 4u) {
        uint32_t m2 = flags >> 6 & 3u, a2 = b[0x7A], a3 = b[0x7B];
        if (m2)
            h->model2 = (uint8_t)m2;
        if (sid_byte_valid(a2)) {
            h->sid2 = (uint16_t)(0xD000u + a2 * 16u);
            h->sids++;
        } else if (a2) {
            h->warn |= PSID_W_SID2;
        }
        if (version == 4u) {
            uint32_t m3 = flags >> 8 & 3u;
            if (m3)
                h->model3 = (uint8_t)m3;
            if (sid_byte_valid(a3) && a3 != a2) {
                h->sid3 = (uint16_t)(0xD000u + a3 * 16u);
                h->sids++;
            } else if (a3) {
                h->warn |= PSID_W_SID3;
            }
        }
    } else if (version == 5u) {
        uint32_t count = b[0x7A] & 15u, cfg = b[0x7A] >> 4;
        if (flags & FLAG_FM_OPL)
            h->warn |= PSID_W_FM_OPL;
        if (count >= 2u) {
            uint16_t a[SID_ADDR_MAX] = {0};
            uint32_t got, i;
            if (cfg > 6u)
                return PSID_E_HEADER;
            got = v5_addresses(cfg, b[0x7B], count, a);
            if (got < count)
                return PSID_E_HEADER;
            if (flags & FLAG_FM_OPL)
                for (i = 1; i < count; i++)
                    if (a[i] >= 0xDF00u)
                        return PSID_E_HEADER;
            h->sids = (uint8_t)count;
            h->sid2 = a[1];
            if (count >= 3u)
                h->sid3 = a[2];
            if (count > 3u)
                h->warn |= PSID_W_MORE_SIDS;
        }
    }
    *out = t;
    return PSID_OK;
}

int psid_speed_bit(const psid_header* h, uint32_t song)
{
    uint32_t bit = song ? song - 1u : 0u;
    if (bit > 31u)
        bit = h->speed_repeats ? (bit & 31u) : 31u;
    return (int)(h->speed >> bit & 1u);
}
