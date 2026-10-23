// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SONG_IO_H
#define SIDOU_SONG_IO_H

#include "player/song_ctl.h"
#include "settings.h"
#include "song/gt2_sng.h"
#include "store.h"

#define SONG_IO_BUFFER_BYTES 160u

typedef struct {
    /* Capture the song's own setup (not a character lock) and live keys/slots before SAVE. */
    int (*prepare)(void* ctx);
    /* Validate before applying runtime metadata; 0 refuses the loaded song. */
    int (*loaded)(void* ctx);
    gt2_read_fn builtin;
    void* ctx;
    uint32_t (*generation)(void* ctx);
    void (*force)(int on);
    /* SONG_IO_BUFFER_BYTES bytes: before SAVE freezes, or before a destructive READ. */
    void (*snapshot)(void* ctx, uint8_t* storage);
    void (*restore)(void* ctx, const uint8_t* storage);
} song_io_hooks;

enum {
    SONG_IO_IDLE,
    SONG_IO_STOP,
    SONG_IO_READ,
    SONG_IO_SAVE_WAIT,
    SONG_IO_WRITE,
    SONG_IO_COMMIT,
    SONG_IO_REMEMBER,
    SONG_IO_DELETE,
    SONG_IO_DELETE_COMMIT,
    SONG_IO_VERIFY,
    SONG_IO_INDEX
};

typedef struct {
    song_ctl* ctl;
    store* st;
    settings* set;
    void* work; /* one gt2_stream workspace, shared by reads and writes */
    song_io_hooks hooks;
    store_file file;
    char name[STORE_NAME_MAX + 1u];
    char target[STORE_NAME_MAX + 1u];
    char message[32];
    char failure[32];
    uint32_t saved_revision;
    uint32_t saved_live, captured_live;
    uint32_t asked_at;
    uint32_t length;
    uint32_t offset;
    uint32_t read_crc;
    /* The stored file the model is (store_file.fid), loaded or saved as `name`; 0 for none. An
     * unchanged SAVE is skipped only while `target` is still that file: a SysEx DEL or PUT may
     * have deleted or replaced it meanwhile. */
    uint32_t fid;
    uint16_t fill, sent;
    uint8_t state;
    uint8_t dirty;
    uint8_t action;
    uint8_t restoring;
    uint8_t forcing;
    uint8_t about;
    uint8_t about_close;
    uint8_t about_play;
    uint8_t builtin;
    uint8_t remembering;
    uint8_t invalidated;
    uint8_t snapshot;
    uint8_t index_remaining;
    uint8_t rollback;
    uint8_t data[SONG_IO_BUFFER_BYTES];
    /* A main-loop workspace lease: internal loading advances, new file actions refuse. */
    uint8_t external;
} song_io;

void song_io_init(song_io* io, song_ctl* ctl, store* st, settings* set, void* workspace,
                  const song_io_hooks* hooks);
int song_io_changed(const song_io* io);
int song_io_busy(const song_io* io);
int song_io_saving(const song_io* io);
/* User-facing SONG/bank directory: the MIDI loader's staging file is private. */
int song_io_listed(const store_file* file);
/* 1 accepted, 0 refused (message); confirmation is required before losing edits. */
int song_io_load(song_io* io, const char* name, int confirmed);
/* 1 lookup accepted, 0 refused (message). Load the zero-based song in name order, stopped.
 * Refuses unsaved edits; advances one directory entry per poll. A newer program may replace
 * an unfinished lookup; a missing index reports SONG NOT FOUND without stopping. */
int song_io_program(song_io* io, uint32_t program);
/* 1 accepted, 0 refused (message): the built-in song (hooks.builtin, SIDOU LOOP in the image)
 * loaded as a file is, stopped; unnamed and unchanged after it, nothing remembered for the next
 * power-on (which plays the built-in anyway) and no stored file a SAVE would replace. */
int song_io_load_builtin(song_io* io, int confirmed);
int song_io_new(song_io* io, int confirmed);
int song_io_save(song_io* io, const char* name, uint32_t now_ms);
int song_io_delete(song_io* io, const char* name, int confirmed);
/* 1 if About's music starts; 0 if it must stay silent. Closing always restores stopped. */
int song_io_about(song_io* io, int opening);
/* From the loaded hook: 1 when this read is About's tune or the song it stood in for coming
 * back from its file (About's close, or a failed load's rollback), so the song views keep their
 * places; 0 for a song that replaces the player's (a load, NEW, a fallback to the built-in). */
int song_io_returning(const song_io* io);
/* One bounded slice, only on passes without chip preparation or song analysis. */
void song_io_poll(song_io* io, uint32_t now_ms);
int song_io_take_message(song_io* io, char out[32]);

#endif
