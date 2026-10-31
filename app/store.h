// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_STORE_H
#define SIDOU_STORE_H

#include <stdint.h>

#include "store_layout.h"

#define STORE_FORMAT 1u
#define STORE_HEADER 32u
#define STORE_RECORDS (STORE_SECTOR_USE - STORE_HEADER)
#define STORE_REC_HEADER 16u
#define STORE_REC_OVERHEAD 20u
#define STORE_VALUE_MAX 1248u
#define STORE_KEYS_MAX 48u
#define STORE_KEY_MAX 0x7FFFFFFFu
#define STORE_TYPE_MAX 0x7Fu
#define STORE_TYPE_TOMB 0x80u
#define STORE_TYPE_CHUNK 0x81u
#define STORE_TYPE_FILE 0x82u
#define STORE_KEY_FILE 0x80000000u
#define STORE_NAME_MAX 16u
#define STORE_FILE_SECTORS 18u
#define STORE_FILE_HEAD 32u
#define STORE_FILE_SONG 1u
#define STORE_FILE_SID 2u
#define STORE_FILE_GTI 3u
#define STORE_FILE_SOUNDS 4u
#define STORE_SOUNDS_NAME "USER SOUNDS"
#define STORE_WINDOW_MAX 256u
#define STORE_FILE_MAX 65536u
#define STORE_CHUNK 1248u
#define STORE_FILE_SECTOR_DATA (3u * STORE_CHUNK)
#define STORE_GC_RESERVE 2u
#define STORE_POOL_MIN ((STORE_FILE_MAX + STORE_RECORDS - 1u) / STORE_RECORDS + STORE_GC_RESERVE)
#define STORE_POOL_TARGET (STORE_POOL_MIN + 2u)
#define STORE_CAPACITY ((STORE_SECTORS - STORE_POOL_MIN) * STORE_FILE_SECTOR_DATA)
#define STORE_WEAR_SPREAD 4u
#define STORE_MOUNT_SLICE 512u
#define STORE_POLL_WORK 7168u
#define STORE_PAGE_PROGS 34u
#define STORE_FILE_IDLE_MS 5000u

_Static_assert(STORE_POOL_MIN == 20u, "ceil(65,536 / 3,808) + 2");
_Static_assert((STORE_FILE_MAX + STORE_FILE_SECTOR_DATA - 1u) / STORE_FILE_SECTOR_DATA +
                       STORE_GC_RESERVE <=
                   STORE_POOL_MIN,
               "the pool covers the largest file at the chunks' real packing");
_Static_assert(3u * (STORE_CHUNK + STORE_REC_OVERHEAD) <= STORE_RECORDS, "3 chunks a sector");
_Static_assert(STORE_VALUE_MAX + STORE_REC_OVERHEAD <= STORE_RECORDS, "a value fits a sector");
_Static_assert(STORE_FILE_SECTORS* STORE_FILE_SECTOR_DATA >= STORE_FILE_MAX, "the largest file");

enum {
    STORE_OK = 0,
    STORE_LATER = 1,
    STORE_BUSY = -1,
    STORE_FULL = -2,
    STORE_NOT_FOUND = -3,
    STORE_FOREIGN = -4,
    STORE_INVALID = -5,
    STORE_ERROR = -6,
    STORE_EXISTS = -7,
    STORE_TIMEOUT = -8,
};

typedef enum {
    STORE_STATE_UNMOUNTED,
    STORE_STATE_MOUNTING,
    STORE_STATE_BLANK,
    STORE_STATE_SIDOU,
    STORE_STATE_FOREIGN,
} store_state_id;

typedef struct {
    const uint8_t* (*map)(void* ctx, uint32_t off);
    int (*prog)(void* ctx, uint32_t off, const void* p, uint32_t n);
    int (*erase)(void* ctx, uint32_t sector_off);
    void* ctx;
} store_flash;

typedef struct {
    uint32_t erases;
    uint16_t free;
    uint16_t live;
} store_sector;

typedef struct {
    uint32_t key;
    uint16_t off;
    uint8_t sec;
    uint8_t pad;
    uint16_t stale;
    uint16_t pad2;
} store_key;

typedef struct {
    const uint8_t* src;
    uint32_t from;
    uint32_t key;
    uint32_t seq;
    uint32_t crc;
    uint16_t len;
    uint16_t off;
    uint16_t done;
    uint16_t from_off;
    uint8_t sec;
    uint8_t type;
    uint8_t phase;
    uint8_t copy;
    uint8_t from_sec;
    uint8_t pad[3];
} store_writer;

typedef struct {
    uint32_t key;
    uint32_t fid;
    uint32_t length;
    uint32_t taken;
    uint32_t crc;
    uint32_t rcrc;
    uint16_t chunk;
    uint16_t cdone;
    uint16_t pdone;
    uint16_t fill;
    uint16_t start;
    uint8_t phase;
    uint8_t closing;
    uint8_t type;
    uint8_t nsec;
    int8_t result;
    char name[STORE_NAME_MAX];
    uint8_t sectors[STORE_FILE_SECTORS];
    uint8_t flush;
    uint16_t idle;
} store_file_writer;

typedef struct {
    store_flash flash;
    uint32_t seq;
    uint32_t live;
    uint32_t corrupt;
    uint32_t errors;
    uint32_t window;
    store_sector sector[STORE_SECTORS];
    uint8_t sstate[STORE_SECTORS];
    uint8_t state;
    uint8_t head;
    uint8_t quiet;
    uint8_t nkeys;
    store_key key[STORE_KEYS_MAX];
    store_writer w;
    store_file_writer f;
    uint16_t mount_off;
    uint8_t mount_sec;
    uint8_t mount_sidou;
    uint8_t mount_blank;
    uint8_t mount_newer;
    uint8_t job;
    uint8_t job_sec;
    uint8_t job_phase;
    uint8_t job_done;
    uint16_t job_off;
    uint16_t fres_bytes;
    uint8_t fres_sec;
    uint8_t mount_phase;
    uint32_t fix_seq;
    uint16_t fix_off;
    uint8_t fix_sec;
    uint8_t fix_kind;
    uint8_t rec;
    uint8_t retired;
    uint8_t unsure;
    uint8_t file_wait;
    uint32_t work;
    uint8_t buf[STORE_WINDOW_MAX];
} store;

typedef struct {
    uint32_t free;
    uint32_t live;
    uint32_t pool;
    uint32_t dirty;
    uint32_t claimed;
    uint32_t erases_min;
    uint32_t erases_max;
    uint32_t keys;
    uint32_t corrupt;
    uint32_t errors;
    uint32_t retired;
} store_stats;

void store_init(store* s, const store_flash* flash, uint32_t window);
void store_mount(store* s);
store_state_id store_state(const store* s);
int store_poll(store* s);
int store_busy(const store* s);
void store_quiet(store* s, int quiet);
int store_erase_wanted(const store* s);

int store_get(const store* s, uint32_t key, void* buf, uint32_t cap, uint8_t* type);
int store_put(store* s, uint32_t key, uint8_t type, const void* data, uint32_t len);
int store_delete(store* s, uint32_t key);
int store_collect(store* s);
void store_format(store* s);
void store_stats_get(const store* s, store_stats* out);

typedef struct {
    uint32_t key;
    uint32_t fid;
    uint32_t length;
    uint32_t crc;
    uint16_t chunks;
    uint8_t type;
    uint8_t nsec;
    char name[STORE_NAME_MAX + 1];
    uint8_t sectors[STORE_FILE_SECTORS];
} store_file;

int store_file_begin(store* s, const char* name, uint8_t type, uint32_t length);
void store_file_wait(store* s, int on);
uint32_t store_file_write(store* s, const void* p, uint32_t n);
void store_file_flush(store* s);
int store_file_pending(const store* s);
int store_file_end(store* s);
void store_file_abort(store* s);
int store_file_result(const store* s);
void store_file_idle(store* s, uint32_t ms);
int store_file_open(const store* s, const char* name, store_file* f);
int store_file_read(const store* s, const store_file* f, uint32_t off, void* p, uint32_t n);
int store_dir_next(const store* s, store_file* f);
int store_file_delete(store* s, const char* name);
int store_file_rename(store* s, const char* from, const char* to);

uint32_t store_crc32(uint32_t crc, const void* p, uint32_t n);

#endif
