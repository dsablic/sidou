// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "song_io.h"

#include <string.h>

#include "song/gt2_kit.h"
#include "song/gt2_stream.h"

_Static_assert(GT2_SAVED_MAX == STORE_FILE_MAX, "device song edits keep the store's file cap");

enum { LOAD_FILE = 1, LOAD_NEW, LOAD_ABOUT, LOAD_RESTORE, LOAD_BUILTIN };
enum { ROLLBACK_SNAPSHOT = 1, ROLLBACK_APPLIED = 2 };

static void text(char out[32], const char* in) {
    uint32_t i = 0;
    while (i < 31u && in[i]) {
        out[i] = in[i];
        i++;
    }
    out[i] = 0;
}

static int name_copy(char out[STORE_NAME_MAX + 1u], const char* in) {
    char next[STORE_NAME_MAX + 1u] = {0};
    if (!in || !in[0] || in[0] == ' ') {
        return 0;
    }
    uint32_t i = 0;
    for (; i < STORE_NAME_MAX && in[i]; i++) {
        if ((uint8_t)in[i] < 0x20u || (uint8_t)in[i] > 0x5fu) {
            return 0;
        }
        next[i] = in[i];
    }
    if (in[i] || !strcmp(next, STORE_SOUNDS_NAME)) {
        return 0; /* LAB's USER sounds' file is no song's to save, load or delete */
    }
    memcpy(out, next, sizeof next);
    return 1;
}

void song_io_init(song_io* io, song_ctl* ctl, store* st, settings* set, void* workspace,
                  const song_io_hooks* hooks) {
    memset(io, 0, sizeof *io);
    io->ctl = ctl;
    io->st = st;
    io->set = set;
    io->work = workspace;
    if (hooks) {
        io->hooks = *hooks;
    }
    if (ctl) {
        io->saved_revision = ctl->song->revision;
    }
    if (io->hooks.generation) {
        io->saved_live = io->hooks.generation(io->hooks.ctx);
    }
}

int song_io_changed(const song_io* io) {
    if (io->away) {
        return 0; /* the model is a tune's memory: nothing of the song's is there to lose */
    }
    return io->dirty || (io->ctl && io->ctl->song->revision != io->saved_revision) ||
           (io->hooks.generation && io->hooks.generation(io->hooks.ctx) != io->saved_live);
}

int song_io_edited(const song_io* io) {
    if (io->away) {
        return 0;
    }
    return io->dirty || (io->ctl && io->ctl->song->revision != io->saved_revision);
}

int song_io_busy(const song_io* io) {
    return io->state != SONG_IO_IDLE;
}

int song_io_saving(const song_io* io) {
    return io->state >= SONG_IO_SAVE_WAIT && io->state <= SONG_IO_REMEMBER;
}

int song_io_listed(const store_file* file) {
    return file->type == STORE_FILE_SONG && memcmp(file->name, "MIDI SONG", 10u) != 0;
}

static int ready(song_io* io, int confirmed) {
    if (io->away) {
        text(io->message, "SID PLAYING");
        return 0;
    }
    if (song_io_busy(io) || io->about || io->external || (io->ctl && io->ctl->song->frozen)) {
        text(io->message, song_io_saving(io) ? "SAVING" : "LOADING");
        return 0;
    }
    if (song_io_changed(io) && !confirmed) {
        text(io->message, "DISCARD CHANGES?");
        return 0;
    }
    return 1;
}

int song_io_load(song_io* io, const char* name, int confirmed) {
    if (!ready(io, confirmed)) {
        return 0;
    }
    if (!io->st || !name_copy(io->target, name) ||
        store_file_open(io->st, name, &io->file) != STORE_OK ||
        io->file.type != STORE_FILE_SONG) {
        text(io->message, "SONG NOT FOUND");
        return 0;
    }
    io->action = LOAD_FILE;
    io->restoring = 0;
    io->failure[0] = 0;
    io->state = SONG_IO_STOP;
    return 1;
}

int song_io_load_builtin(song_io* io, int confirmed) {
    if (!ready(io, confirmed)) {
        return 0;
    }
    if (!io->hooks.builtin) {
        text(io->message, "SONG NOT FOUND");
        return 0;
    }
    memset(io->target, 0, sizeof io->target); /* read_done names it: none */
    io->action = LOAD_BUILTIN;
    io->restoring = 0;
    io->failure[0] = 0;
    io->state = SONG_IO_STOP;
    return 1;
}

int song_io_program(song_io* io, uint32_t program) {
    if (io->away) {
        text(io->message, "SID PLAYING");
        return 0;
    }
    if ((song_io_busy(io) && io->state != SONG_IO_INDEX) || io->about || io->external ||
        (io->ctl && io->ctl->song->frozen)) {
        text(io->message, song_io_saving(io) ? "SAVING" : "LOADING");
        return 0;
    }
    if (song_io_changed(io)) {
        text(io->message, "SAVE YOUR SONG FIRST");
        return 0;
    }
    if (!io->ctl || !io->st) {
        text(io->message, "SONG NOT FOUND");
        return 0;
    }
    io->index_remaining = (uint8_t)(program < STORE_KEYS_MAX ? program : STORE_KEYS_MAX);
    io->file.name[0] = 0;
    io->state = SONG_IO_INDEX;
    return 1;
}

int song_io_new(song_io* io, int confirmed) {
    if (!ready(io, confirmed)) {
        return 0;
    }
    io->action = LOAD_NEW;
    io->restoring = 0;
    io->failure[0] = 0;
    io->state = SONG_IO_STOP;
    return 1;
}

int song_io_save(song_io* io, const char* name, uint32_t now_ms) {
    if (io->away) {
        text(io->message, "SID PLAYING");
        return 0;
    }
    if (song_io_busy(io) || io->about || io->external || (io->ctl && io->ctl->song->frozen)) {
        text(io->message, song_io_saving(io) ? "SAVING" : "LOADING");
        return 0;
    }
    if (!io->ctl || !io->st || !name_copy(io->target, name)) {
        text(io->message, "SONG: BAD NAME");
        return 0;
    }
    io->asked_at = now_ms;
    if (io->hooks.snapshot) {
        io->hooks.snapshot(io->hooks.ctx, io->data);
        io->snapshot = 1;
    }
    io->captured_live = io->hooks.generation ? io->hooks.generation(io->hooks.ctx) : 0u;
    io->ctl->song->frozen = 1;
    io->state = SONG_IO_SAVE_WAIT;
    return 1;
}

int song_io_delete(song_io* io, const char* name, int confirmed) {
    if (!confirmed) {
        text(io->message, "DELETE SONG?");
        return 0;
    }
    if (io->away || song_io_busy(io) || io->about || io->external ||
        (io->ctl && io->ctl->song->frozen) || !io->st || !name_copy(io->target, name)) {
        return 0;
    }
    io->state = SONG_IO_DELETE;
    return 1;
}

int song_io_about(song_io* io, int opening) {
    if (opening) {
        if (io->away || !io->ctl || io->external || io->ctl->song->frozen || song_io_busy(io) || song_io_changed(io) ||
            song_ctl_state(io->ctl) == SONG_PLAYING || song_ctl_state(io->ctl) == SONG_LOADING ||
            io->ctl->auditioning || !io->hooks.builtin) {
            return 0;
        }
        io->about = 1;
        io->about_close = 0;
        io->about_play = 0;
        io->action = LOAD_ABOUT;
        io->state = SONG_IO_STOP;
        return 1;
    }
    if (io->about) {
        io->about_close = 1;
        io->about_play = 0;
        if (!song_io_busy(io)) {
            io->action = LOAD_RESTORE;
            io->state = SONG_IO_STOP;
        }
    }
    return 1;
}

int song_io_away(song_io* io, int on) {
    if (on) {
        if (io->away) {
            return 1;
        }
        if (song_io_busy(io) || io->about || io->external || !io->ctl || io->ctl->song->frozen) {
            return 0;
        }
        /* 2: NEW's song, never saved (no name, unsaved from its start); 1 a file's or the
         * built-in (the one the boot reads has no name either, and no unsaved start) */
        io->away = (uint8_t)(!io->name[0] && !io->builtin && io->dirty ? 2u : 1u);
        io->dirty = 0; /* its edits go with the model */
        return 1;
    }
    if (!io->away) {
        return 1;
    }
    /* The song back as ABOUT's close brings it (its file by name, or the built-in it was), or
     * NEW's empty song. */
    io->action = io->away == 2u ? LOAD_NEW : LOAD_RESTORE;
    io->keeping = io->away == 1u; /* the file's song, the keys' runtime as the tune left it */
    io->away = 0;
    io->message[0] = 0; /* a refusal while away is old news */
    io->restoring = 0;
    io->rollback = 0;
    io->failure[0] = 0;
    io->state = SONG_IO_STOP;
    return 1;
}

int song_io_keeping(const song_io* io) {
    return io->keeping;
}

int song_io_returning(const song_io* io) {
    if (io->restoring || io->action == LOAD_RESTORE) {
        return !io->builtin; /* the file, not the built-in standing in for a lost one */
    }
    return io->action == LOAD_ABOUT;
}

static uint32_t read_file(void* ctx, uint32_t off, uint8_t* p, uint32_t n) {
    song_io* io = ctx;
    int got = store_file_read(io->st, &io->file, off, p, n);
    return got > 0 ? (uint32_t)got : 0u;
}

/* The template is a plain GTS5 stream: one subtune, three 64-row patterns, a pulse lead and a
 * bass (instruments 1 and 2), then the SID drum kit (core/song/gt2_kit.h) as instruments 3-10,
 * its table rows first (the kit's pointers are rows from 1), the lead's and the bass's after,
 * and the digi drums 11-14, each a copy of its chip drum (the fallback GoatTracker plays;
 * read_done adds the DIGI chunk that marks them). Producing its bytes instead of holding
 * another model keeps one content region. */
_Static_assert(SONG_IO_KIT == 3u, "the kit follows the lead and the bass");
_Static_assert(SONG_IO_DIGI == SONG_IO_KIT + GT2_KIT_DRUMS, "the digi drums follow the kit");
#define DIGI_DRUMS (SONG_DIGI_KINDS - 1u)
/* Each digi drum's chip drum and name, in SONG_DIGI_* order. */
static const uint8_t template_digi_kit[DIGI_DRUMS] = {GT2_KIT_KICK, GT2_KIT_SNARE, GT2_KIT_CLAP,
                                                      GT2_KIT_CLOSED_HAT};
static const char template_digi_name[DIGI_DRUMS][9] = {"DG KICK", "DG SNARE", "DG CLAP",
                                                       "DG HAT"};
static uint8_t template_digi(uint32_t at) {
    uint32_t d = at / GT2_INSTR_BYTES, b = at % GT2_INSTR_BYTES;
    if (b < GT2_INS_NAME) {
        return gt2_kit_instr[template_digi_kit[d]].head[b];
    }
    b -= GT2_INS_NAME;
    return b < sizeof template_digi_name[0] ? (uint8_t)template_digi_name[d][b] : 0u;
}
/* The bass's pulse rows after the kit's: the width $400 (a quarter), then the table stops. */
#define BASS_PULSE_ROWS 2u
static const uint8_t template_bass_pulse[2][BASS_PULSE_ROWS] = {{0x84, 255}, {0x00, 0}};
#define LEAD_WAVE (GT2_KIT_WAVE_ROWS + 1u)
/* The lead and the bass attack at 1 (8 ms), not 0: at the gate the chip compares the
 * envelope's rate counter with the decay's period for a cycle, and one phase in nine the
 * counter stepped past attack 0's 9 and the note waited out its wrap, 33 ms (the ADSR delay
 * bug; decision record 0009, as the kit's drums). */
static const uint8_t template_orders[] = {1, 2, 0, 255, 0, 2, 1, 255, 0, 2, 2, 255, 0};
static const uint8_t template_lead[2][GT2_INSTR_BYTES] = {
    {0x19, 0xa8, LEAD_WAVE, 1, 0, 0, 0, 2, 0x41, 'P', 'U', 'L', 'S', 'E'},
    {0x12, 0x98, LEAD_WAVE + 2u, GT2_KIT_PULSE_ROWS + 1u, 0, 0, 0, 2, 0x41, 'B', 'A', 'S', 'S'}};
/* The lead's and the bass's rows: a gated pulse at the note played (the right column's
 * relative note 0; $80 there would keep the frequency unchanged, so never set it: silence),
 * then a jump to 0, which stops the table and leaves the pitch to the pattern's commands. The
 * bass's relative note is -12 ($74: GoatTracker's $60-$7F are negative), an octave down. */
static const uint8_t template_lead_wave[2][4] = {{0x41, 255, 0x41, 255}, {0, 0, 0x74, 0}};
static const uint8_t template_counts[] = {2u + GT2_KIT_DRUMS + DIGI_DRUMS,
                                          GT2_KIT_WAVE_ROWS + 4u,
                                          GT2_KIT_PULSE_ROWS + BASS_PULSE_ROWS,
                                          0, 0, 3}; /* filter, speed; the patterns */
typedef struct {
    const uint8_t* p;
    uint16_t n;
} template_piece;
#define PIECE(x) {(const uint8_t*)(x), (uint16_t)sizeof(x)}
static const template_piece template_pieces[] = {
    {template_orders, sizeof template_orders},
    {template_counts, 1},
    PIECE(template_lead),
    PIECE(gt2_kit_instr),
    {NULL, DIGI_DRUMS * GT2_INSTR_BYTES}, /* template_digi */
    {template_counts + 1, 1},
    {gt2_kit_wave[0], GT2_KIT_WAVE_ROWS},
    {template_lead_wave[0], 4},
    {gt2_kit_wave[1], GT2_KIT_WAVE_ROWS},
    {template_lead_wave[1], 4},
    {template_counts + 2, 1},
    {gt2_kit_pulse[0], GT2_KIT_PULSE_ROWS},
    {template_bass_pulse[0], BASS_PULSE_ROWS},
    {gt2_kit_pulse[1], GT2_KIT_PULSE_ROWS},
    {template_bass_pulse[1], BASS_PULSE_ROWS},
    {template_counts + 3, 3},
};
#define TEMPLATE_HEAD 100u
#define TEMPLATE_PATTERNS (13u + 1u + (2u + GT2_KIT_DRUMS + DIGI_DRUMS) * GT2_INSTR_BYTES + 1u + \
                           2u * (GT2_KIT_WAVE_ROWS + 4u) + 1u + 2u * (GT2_KIT_PULSE_ROWS + BASS_PULSE_ROWS) + 3u)
#define TEMPLATE_BYTES (TEMPLATE_HEAD + TEMPLATE_PATTERNS + 3u * (1u + (GT2_DEFAULT_ROWS + 1u) * 4u))

static uint8_t template_byte(uint32_t at) {
    if (at < 4u) {
        return (uint8_t)"GTS5"[at];
    }
    if (at < TEMPLATE_HEAD) {
        static const char title[] = "NEW SONG";
        return at - 4u < sizeof title - 1u ? (uint8_t)title[at - 4u] : 0u;
    }
    at -= TEMPLATE_HEAD;
    for (uint32_t i = 0; i < sizeof template_pieces / sizeof template_pieces[0]; i++) {
        if (at < template_pieces[i].n) {
            return template_pieces[i].p ? template_pieces[i].p[at] : template_digi(at);
        }
        at -= template_pieces[i].n;
    }
    at %= 1u + (GT2_DEFAULT_ROWS + 1u) * 4u;
    if (at == 0u) {
        return GT2_DEFAULT_ROWS + 1u;
    }
    at--;
    return at % 4u == 0u ? (at / 4u == GT2_DEFAULT_ROWS ? GT2_ENDPATT : GT2_REST) : 0u;
}

uint32_t song_io_template(void* ctx, uint32_t off, uint8_t* p, uint32_t n) {
    (void)ctx;
    if (off >= TEMPLATE_BYTES) {
        return 0u;
    }
    if (n > TEMPLATE_BYTES - off) {
        n = TEMPLATE_BYTES - off;
    }
    for (uint32_t i = 0; i < n; i++) {
        p[i] = template_byte(off + i);
    }
    return n;
}

static void force_set(song_io* io, int on) {
    if (io->hooks.force) {
        io->hooks.force(on);
    } else if (io->set && io->set->force) {
        io->set->force(on);
    }
}

static void force_end(song_io* io) {
    if (io->forcing) {
        force_set(io, 0);
    }
    io->forcing = 0;
}

static void finish_save(song_io* io, int ok) {
    if (ok && io->set && !io->remembering) {
        settings_set_song(io->set, io->target);
        /* A prior explicit settings write must finish before this generation is asked. */
        io->remembering = 1;
        io->state = SONG_IO_REMEMBER;
        return;
    }
    io->ctl->song->frozen = 0;
    io->snapshot = 0;
    io->state = SONG_IO_IDLE;
    if (ok) {
        memcpy(io->name, io->target, sizeof io->name);
        io->fid = io->file.fid; /* SONG_IO_COMMIT opened it */
        io->saved_revision = io->ctl->song->revision;
        io->saved_live = io->captured_live;
        io->dirty = 0;
        if (io->set) {
            settings_set_song(io->set, io->name);
        }
        text(io->message, io->forcing ? "SONG SAVED (GAPS)" : "SONG SAVED");
    } else {
        text(io->message, "SONG: SAVE FAILED");
    }
    force_end(io);
    io->remembering = 0;
}

static void forget_player(song_io* io) {
    song_ctl* sc = io->ctl;
    sc->state = SONG_IDLE;
    sc->shapes_used = 0;
    sc->player.shape = NULL;
    sc->bg_on = sc->bg_dirty = sc->widened = sc->ff_on = sc->deriving = 0;
    sc->want_handover = sc->spare_wait = 0;
    sc->hold_next = 0;
    sc->error = 0;
}

static void start_read(song_io* io) {
    gt2_read_fn read = read_file;
    void* ctx = io;
    if ((io->restoring || io->action == LOAD_RESTORE) && io->rollback) {
        /* Restore inherited metadata before VERIFY reuses the shared byte buffer. */
        if (io->rollback & ROLLBACK_APPLIED) {
            io->hooks.restore(io->hooks.ctx, io->data);
        }
        io->rollback = 0;
    }
    io->builtin = 0;
    if (io->action == LOAD_NEW) {
        read = song_io_template;
    } else if (io->action == LOAD_ABOUT || io->action == LOAD_BUILTIN) {
        read = io->hooks.builtin;
        ctx = io->hooks.ctx;
        io->builtin = 1;
    } else if (io->restoring || io->action == LOAD_RESTORE) {
        if (!io->name[0] || store_file_open(io->st, io->name, &io->file) != STORE_OK) {
            read = io->hooks.builtin;
            ctx = io->hooks.ctx;
            io->builtin = 1;
        }
    }
    io->invalidated = 0;
    gt2_caps caps = {SONG_MULTISPEED_MAX};
    gt2_stream_read_begin(io->work, io->ctl->song, read, ctx, &caps);
    io->offset = io->read_crc = 0;
    io->state = read == read_file ? SONG_IO_VERIFY : SONG_IO_READ;
}

static void read_failed(song_io* io, const char* reason, int touched) {
    io->keeping = 0;
    if (io->restoring || io->action == LOAD_RESTORE) {
        text(io->message, "SONG: RELOAD FAILED");
        io->state = SONG_IO_IDLE;
        io->dirty = 1;
        io->about = io->about_close = 0;
        io->restoring = 0;
    } else if (!touched) {
        text(io->message, reason);
        io->state = SONG_IO_IDLE;
        io->rollback = 0;
        if (io->action == LOAD_ABOUT) {
            io->about = io->about_close = 0;
        }
    } else {
        text(io->failure, reason);
        io->restoring = 1;
        start_read(io);
    }
}

static void read_done(song_io* io) {
    if (io->hooks.loaded) {
        if (!io->hooks.loaded(io->hooks.ctx)) {
            read_failed(io, io->failure[0] ? io->failure : "SNG: SAVED FORM TOO BIG", 1);
            return;
        }
        if (io->rollback & ROLLBACK_SNAPSHOT) {
            io->rollback |= ROLLBACK_APPLIED;
        }
    }
    if (io->hooks.prepare && !io->hooks.prepare(io->hooks.ctx)) {
        read_failed(io, io->failure[0] ? io->failure : "SNG: SAVED FORM TOO BIG", 1);
        return;
    }
    if (io->action == LOAD_ABOUT && !io->restoring) {
        io->about_play = !io->about_close;
    } else if (!io->restoring && io->action != LOAD_RESTORE) {
        if (io->action == LOAD_NEW) {
            memset(io->name, 0, sizeof io->name);
            io->dirty = 1;
            /* the digi drums: instruments SONG_IO_DIGI on, the built-in samples in order */
            song_digi digi = {DIGI_DRUMS, {{0}}};
            for (uint32_t d = 0; d < DIGI_DRUMS; d++) {
                digi.entry[d][0] = (uint8_t)(SONG_IO_DIGI + d);
                digi.entry[d][1] = (uint8_t)(SONG_DIGI_KICK + d);
            }
            (void)song_ext_set_digi(io->ctl->song, &digi);
        } else {
            memcpy(io->name, io->target, sizeof io->name);
            io->dirty = 0;
        }
        if (io->set) {
            settings_set_song(io->set, io->name);
        }
    }
    io->saved_revision = io->ctl->song->revision;
    if (io->hooks.generation && !io->keeping) {
        /* (kept: the runtime is what it was before the tune, and so is its unsaved mark) */
        io->saved_live = io->hooks.generation(io->hooks.ctx);
    }
    io->keeping = 0;
    /* The model is now what was read: a stored file (start_read's read_file), or none. */
    io->fid = io->builtin || io->action == LOAD_NEW ? 0u : io->file.fid;
    io->state = SONG_IO_IDLE;
    if (io->action != LOAD_ABOUT || io->restoring) {
        io->rollback = 0;
    }
    if (io->failure[0]) {
        text(io->message, io->failure);
        io->failure[0] = 0;
    }
    io->restoring = 0;
    if (io->action == LOAD_RESTORE) {
        io->about = io->about_close = 0;
    } else if (io->about_close) {
        io->action = LOAD_RESTORE;
        io->state = SONG_IO_STOP;
    }
}

void song_io_poll(song_io* io, uint32_t now_ms) {
    song_ctl* sc = io->ctl;
    if (!sc || io->away) {
        return;
    }
    if (song_io_saving(io) && !io->forcing && now_ms - io->asked_at >= SETTINGS_FORCE_MS &&
        (io->hooks.force || (io->set && io->set->force))) {
        io->forcing = 1;
        force_set(io, 1);
    }
    switch (io->state) {
    case SONG_IO_IDLE:
        if (io->about_play && song_ctl_play(sc, 0u, 0u)) {
            io->about_play = 0;
        }
        break;
    case SONG_IO_INDEX:
        if (song_io_changed(io)) {
            text(io->message, "SAVE YOUR SONG FIRST");
            io->state = SONG_IO_IDLE;
        } else if (!store_dir_next(io->st, &io->file)) {
            text(io->message, "SONG NOT FOUND");
            io->state = SONG_IO_IDLE;
        } else if (song_io_listed(&io->file)) {
            if (io->index_remaining) {
                io->index_remaining--;
            } else {
                memcpy(io->target, io->file.name, sizeof io->target);
                io->action = LOAD_FILE;
                io->restoring = 0;
                io->failure[0] = 0;
                io->state = SONG_IO_STOP;
            }
        }
        break;
    case SONG_IO_STOP:
        if (sc->auditioning) {
            (void)song_ctl_audition_end(sc);
        }
        if (song_ctl_state(sc) == SONG_PLAYING) {
            (void)song_ctl_stop(sc);
            break;
        }
        if (song_ctl_state(sc) == SONG_LOADING || sc->isr_playing || sc->audition ||
            sc->cmd_head != sc->cmd_tail || sc->inflight_n || sc->handback ||
            sc->song->pending || sc->song->pending_loose) {
            break;
        }
        start_read(io);
        break;
    case SONG_IO_READ: {
        gt2_stream* w = io->work;
        if (!io->rollback && !io->restoring && io->action != LOAD_RESTORE &&
            io->hooks.snapshot && io->hooks.restore) {
            io->hooks.snapshot(io->hooks.ctx, io->data);
            io->rollback = ROLLBACK_SNAPSHOT;
        }
        if (w->wipe_at && !io->invalidated) {
            if (!io->restoring && io->action != LOAD_RESTORE && io->hooks.generation &&
                io->hooks.generation(io->hooks.ctx) != io->saved_live) {
                io->dirty = 1;
            }
            forget_player(io);
            io->invalidated = 1;
        }
        int r = gt2_stream_read_step(io->work);
        if (r < 0) {
            read_failed(io, w->info.msg, io->invalidated);
        } else if (r > 0) {
            read_done(io);
        }
        break;
    }
    case SONG_IO_VERIFY:
        if (io->offset < io->file.length) {
            uint32_t n = io->file.length - io->offset;
            n = n > sizeof io->data ? (uint32_t)sizeof io->data : n;
            if (read_file(io, io->offset, io->data, n) != n) {
                read_failed(io, "SNG: READ ERROR", 0);
                break;
            }
            io->read_crc = store_crc32(io->read_crc, io->data, n);
            io->offset += n;
        } else if (io->read_crc != io->file.crc) {
            read_failed(io, "SNG: FILE CRC", 0);
        } else {
            io->state = SONG_IO_READ;
        }
        break;
    case SONG_IO_SAVE_WAIT: {
        if (sc->song->pending || sc->song->pending_loose) {
            break;
        }
        sc->song->frozen = 0; /* the main loop's internal capture, with no edit caller interleaved */
        if (io->hooks.prepare && !io->hooks.prepare(io->hooks.ctx)) {
            text(io->message, io->failure[0] ? io->failure : "SONG FULL");
            io->failure[0] = 0;
            io->state = SONG_IO_IDLE;
            io->snapshot = 0;
            force_end(io);
            break;
        }
        sc->song->frozen = 1;
        /* Unchanged, and the file it would replace is still the one it was loaded or saved as
         * (io->file is free while saving: SONG_IO_COMMIT opens it again). */
        if (!io->dirty && sc->song->revision == io->saved_revision &&
            io->captured_live == io->saved_live &&
            memcmp(io->name, io->target, sizeof io->name) == 0 &&
            store_file_open(io->st, io->target, &io->file) == STORE_OK && io->fid &&
            io->file.fid == io->fid) {
            text(io->message, "SONG SAVED");
            io->state = SONG_IO_IDLE;
            sc->song->frozen = 0;
            io->snapshot = 0;
            force_end(io);
            break;
        }
        io->length = gt2_saved_size(sc->song);
        int r = store_file_begin(io->st, io->target, STORE_FILE_SONG, io->length);
        if (r == STORE_OK) {
            io->snapshot = 0; /* the metadata is in the model; reuse data for streamed bytes */
            io->fill = io->sent = 0;
            io->offset = 0;
            gt2_stream_write_begin(io->work, sc->song, 1);
            io->state = SONG_IO_WRITE;
        } else if (r == STORE_BUSY) {
            if (!io->forcing && now_ms - io->asked_at >= SETTINGS_FORCE_MS &&
                (io->hooks.force || (io->set && io->set->force))) {
                io->forcing = 1;
                force_set(io, 1);
            }
        } else {
            text(io->message, r == STORE_FULL ? "STORE FULL" : "SONG: SAVE FAILED");
            io->state = SONG_IO_IDLE;
            sc->song->frozen = 0;
            io->snapshot = 0;
            force_end(io);
        }
        break;
    }
    case SONG_IO_WRITE:
        if (store_file_result(io->st) != STORE_BUSY) {
            finish_save(io, 0);
            break;
        }
        if (io->sent < io->fill) {
            io->sent += (uint16_t)store_file_write(io->st, io->data + io->sent,
                                                 io->fill - io->sent);
            break;
        }
        {
            int n = gt2_stream_write_step(io->work, io->data, sizeof io->data);
            if (n < 0) {
                store_file_abort(io->st);
                finish_save(io, 0);
            } else if (n == 0) {
                if (io->offset != io->length || store_file_end(io->st) != STORE_OK) {
                    store_file_abort(io->st);
                    finish_save(io, 0);
                } else {
                    io->state = SONG_IO_COMMIT;
                }
            } else {
                io->fill = (uint16_t)n;
                io->sent = 0;
                io->offset += (uint32_t)n;
            }
        }
        break;
    case SONG_IO_COMMIT: {
        int r = store_file_result(io->st);
        if (r != STORE_BUSY) {
            if (r != STORE_OK || store_file_open(io->st, io->target, &io->file) != STORE_OK) {
                io->file.fid = 0; /* unknown: the next SAVE writes */
            }
            finish_save(io, r == STORE_OK);
        }
        break;
    }
    case SONG_IO_DELETE: {
        int r = store_file_delete(io->st, io->target);
        if (r == STORE_OK) {
            io->state = SONG_IO_DELETE_COMMIT;
        } else if (r != STORE_BUSY) {
            text(io->message, "SONG: DELETE FAILED");
            io->state = SONG_IO_IDLE;
        }
        break;
    }
    case SONG_IO_DELETE_COMMIT:
        if (!store_busy(io->st)) {
            store_file found;
            if (store_file_open(io->st, io->target, &found) == STORE_OK) {
                text(io->message, "SONG: DELETE FAILED");
                io->state = SONG_IO_IDLE;
                break;
            }
            if (memcmp(io->name, io->target, sizeof io->name) == 0) {
                memset(io->name, 0, sizeof io->name);
                io->dirty = 1;
                if (io->set) {
                    settings_set_song(io->set, "");
                }
            }
            text(io->message, "SONG DELETED");
            io->state = SONG_IO_IDLE;
        }
        break;
    case SONG_IO_REMEMBER:
        settings_tick(io->set, now_ms);
        if (!io->set->asked) {
            uint8_t rec[SETTINGS_BYTES];
            int n = store_get(io->st, SETTINGS_KEY, rec, sizeof rec, NULL);
            if (n == (int)sizeof rec && rec[0] >= 4u &&
                memcmp(rec + SETTINGS_V3_BYTES, io->target, STORE_NAME_MAX) == 0) {
                (void)settings_take_result(io->set);
                finish_save(io, 1);
            } else if (io->remembering == 1u) {
                (void)settings_take_result(io->set);
                settings_save_now(io->set, now_ms);
                io->remembering = 2u;
            } else {
                finish_save(io, 0);
            }
        }
        break;
    default: break;
    }
}

int song_io_take_message(song_io* io, char out[32]) {
    if (!io->message[0]) {
        return 0;
    }
    text(out, io->message);
    io->message[0] = 0;
    return 1;
}
