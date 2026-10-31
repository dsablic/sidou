// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stdint.h>
#include <string.h>

#define FM1_CPU1_USED 1
#include "fm1_sys.h"
#undef FM1_CPU1_USED
#define FM1_CPU1_USED 0

#include "fm1_audio.h"
#include "fm1_irq.h"
#include "fm1_time.h"
#include "fm1_timer.h"

#include "audio.h"
#include "bootguard.h"
#include "demo_song.h"
#if defined(SIDOU_FLASH_SPIKE)
#include "flash_spike.h"
#endif
#include "input.h"
#include "keys_route.h"
#include "lcd.h"
#include "live.h"
#include "midi.h"
#include "panel.h"
#include "player/song_ctl.h"
#include "psid/psid_ctl.h"
#include "psid/psid_file.h"
#include "reg_ring.h"
#include "sid_io.h"
#include "sid_transport.h"
#include "song/gt2_sng.h"
#include "song/gt2_stream.h"
#include "song/gt2_ins.h"
#include "song_io.h"
#include "store_fm1.h"
#include "ui.h"
#include "ui_sound.h"
#include "update.h"
#include "usb_audio.h"

extern uint32_t _data_start[], _data_end[], _data_load[], _bss_start[], _bss_end[];
extern uint32_t _pool_start[], _pool_end[], _rt_start[], _rt_end[], _rt_load[];
typedef void (*ctor_fn)(void);
extern ctor_fn __init_array_start[], __init_array_end[];

void isr_alnk0(void);
void isr_timer5(void);
void fm1_alnk0_irq(void);
void fm1_timer5_irq(void);
void fm1_cstart(void);

#define PRIO_ALNK0 3u
#define PRIO_TIMER5 4u
#define WDT_TIMEOUT 0x0Du
#define TICK_MS (1000u / LIVE_TICK_HZ)
#define TICK_CATCH_UP 2u

static int32_t abuf[2u * AUDIO_HALF_WORDS] __attribute__((aligned(4)));
static bootguard boot_guard __attribute__((section(".noinit")));
static volatile uint32_t uptime_ms;
static live instrument;
static keys_route keys_router;
static ui_sound ui_sounds;
static panel front;
static song_io songs;
static union {
    gt2_stream stream;
    gt2_ins_import_state instrument;
} song_stream;
static struct {
    gt2_ins_export instrument;
    uint32_t offset;
    uint8_t target, index, owned, loaded, staging_busy;
} midi_object;
enum { MIDI_OBJECT_FROZEN = 1u, MIDI_OBJECT_WORKSPACE = 2u };

_Static_assert(sizeof(sysex) + sizeof(midi_object) + 16u <= 1024u,
               "MIDI dump state, packet buffer and binding globals fit the new RAM budget");

static volatile uint8_t in_audio;
static volatile uint32_t timer_irqs;

void fm1_alnk0_irq(void) {
    in_audio = 1;
    uint8_t p = fm1_audio_pending();
    fm1_audio_ack_aux(p);
    if (p & FM1_AUDIO_HALF) {
        audio_fill(&abuf[fm1_audio_free_half() * AUDIO_HALF_WORDS]);
        fm1_audio_ack_half();
    }
    audio_block_end = fm1_ticks();
    in_audio = 0;
}

#define MIDI_POLL_TICKS 5u

void fm1_timer5_irq(void) {
    static uint32_t last, acc, sub, block_seen;
    static uint8_t usb_owed;
    uint32_t now = fm1_ticks();
    fm1_timer5_ack();
    timer_irqs++;
    input_tick_isr();
    if (sub == 0u) {
        usb_owed = 1;
    } else if (sub == 2u) {
        midi_uart_isr();
    }
    if (usb_owed && !in_audio) {
        usb_owed = 0;
        midi_usb_isr();
    } else if (sub == 0u && in_audio) {
        usb_audio_isr();
    }
    uint32_t block_end = audio_block_end;
    if (block_end != block_seen && !in_audio) {
        block_seen = block_end;
        if (fm1_audio_pending() & FM1_AUDIO_HALF) {
            usb_audio_skip();
        } else {
            usb_audio_take(&abuf[fm1_audio_free_half() * AUDIO_HALF_WORDS]);
        }
    }
    usb_audio_tick();
    if (++sub == MIDI_POLL_TICKS) {
        sub = 0;
    }
    acc += now - last;
    last = now;
    while (acc >= 1000u * FM1_TICKS_PER_US) {
        acc -= 1000u * FM1_TICKS_PER_US;
        uptime_ms++;
    }
}

static void fm1_fault(const fm1_crash_t* c) {
    (void)c;
    fm1_audio_stop();
    fm1_reboot();
}

static uint8_t song_away;

static uint32_t read_demo(void* ctx, uint32_t offset, uint8_t* buf, uint32_t len) {
    (void)ctx;
    if (offset >= demo_song_sng_bytes) {
        return 0;
    }
    uint32_t n = demo_song_sng_bytes - offset;
    n = n < len ? n : len;
    for (uint32_t i = 0; i < n; i++) {
        buf[i] = demo_song_sng[offset + i];
    }
    return n;
}

static uint8_t song_setup_on;
static chip_setup song_want;
static int song_base_get(chip_setup* setup);
static void song_base_set(const chip_setup* setup);
static void setup_decode(chip_setup* out, const song_chip_setup* chip);

static int song_chip_ready(void* ctx, const song_chip_setup* c) {
    keys_route* kr = ctx;
    if (!keys_route_ready_for_song(kr)) {
        return 0;
    }
    if (!song_setup_on) {
        if (!song_base_get(&song_want)) {
            song_want = kr->ctl[0].has;
            if (c) {
                setup_decode(&song_want, c);
            } else {
                song_want.gate_reset = 0u;
            }
        }
        keys_route_song_wants(kr, &song_want);
        song_base_set(&song_want);
        kr->song_user = 0;
        song_setup_on = 1;
    }
    if (!chip_ctl_has(&kr->ctl[0], &song_want)) {
        return 0;
    }
    song_setup_on = 0;
    return 1;
}

static uint8_t song_mine;

static void song_chip_back(void* ctx) {
    keys_route_set_song(ctx, 0);
    song_mine = 0;
}

static void song_load(void) {
    gt2_song* s = audio_song_boot();
    gt2_caps caps = {SONG_MULTISPEED_MAX};
    gt2_read_info info;
    gt2_sng_read(s, read_demo, 0, &caps, &info);
}

static chip_setup song_char_base;
static uint8_t song_base_valid;
static int16_t song_char_on[4] = {-1, -1, -1, -1};
static uint32_t song_warned;

__attribute__((noinline, unused)) static int song_base_get(chip_setup* setup) {
    if (!song_base_valid) {
        return 0;
    }
    *setup = song_char_base;
    return 1;
}

__attribute__((noinline, unused)) static void song_base_set(const chip_setup* setup) {
    song_char_base = *setup;
    song_base_valid = 1;
}

static void song_chip_character(void* ctx, uint32_t field, int32_t value) {
    keys_route* kr = ctx;
    int any = 0;
    for (uint32_t f = 0; f < 4u; f++) {
        any |= song_char_on[f] >= 0;
    }
    if (!any) {
        song_char_base = kr->song_want;
        song_base_valid = 1;
    }
    song_char_on[field] = (int16_t)value;
    chip_setup want = song_char_base;
    for (uint32_t f = 0; f < 4u; f++) {
        if (song_char_on[f] >= 0) {
            chip_setup_control(&want, f, (uint8_t)song_char_on[f]);
        }
    }
    keys_route_song_wants(kr, &want);
}

static const midi_sync* sync_state;
static int song_chip_edit(void* ctx, const chip_setup* setup, uint32_t fields) {
    keys_route* kr = ctx;
    if (song_away) {
        chip_setup want = kr->song_want;
        if (fields & KEYS_ROUTE_EDIT_PROFILE) {
            want.profile = setup->profile;
        }
        if (fields & KEYS_ROUTE_EDIT_ENVELOPE) {
            want.gate_reset = setup->gate_reset;
        }
        for (uint32_t f = 0; f < 4u; f++) {
            if (fields >> f & 1u) {
                chip_setup_control(&want, f, setup->controls[f]);
            }
        }
        for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
            if (fields & (1u << (KEYS_ROUTE_EDIT_EXPERT_FIRST + f))) {
                want.expert[f] = setup->expert[f];
            }
        }
        keys_route_song_wants(kr, &want);
        return 1;
    }
    if (audio_song()->song->frozen) {
        ui_toast("SAVING", 2000u);
        return 0;
    }
    if (!song_base_valid) {
        song_char_base = kr->song_want;
    }
    song_base_valid = 1;
    if (fields & KEYS_ROUTE_EDIT_PROFILE) {
        song_char_base.profile = setup->profile;
    }
    if (fields & KEYS_ROUTE_EDIT_ENVELOPE) {
        song_char_base.gate_reset = setup->gate_reset;
    }
    for (uint32_t f = 0; f < 4u; f++) {
        if (fields >> f & 1u) {
            chip_setup_control(&song_char_base, f, setup->controls[f]);
        }
    }
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        if (fields & (1u << (KEYS_ROUTE_EDIT_EXPERT_FIRST + f))) {
            song_char_base.expert[f] = setup->expert[f];
        }
    }
    chip_setup want = song_char_base;
    for (uint32_t f = 0; f < 4u; f++) {
        if (song_char_on[f] >= 0) {
            chip_setup_control(&want, f, (uint8_t)song_char_on[f]);
        }
    }
    keys_route_song_wants(kr, &want);
    return 1;
}

static void song_boot(void) {
    song_chip_hooks hooks = {song_chip_ready, song_chip_back, song_chip_character, &keys_router};
    song_ctl_set_hooks(audio_song(), &hooks);
    keys_router.song_edit = song_chip_edit;
    keys_router.song_edit_ctx = &keys_router;
}

static midi_route* route_state;

__attribute__((noinline, unused)) static void song_mode_setup(void) {
    if (!keys_router.player || !song_base_valid || song_away) {
        return;
    }
    chip_setup want = song_char_base;
    for (uint32_t f = 0; f < 4u; f++) {
        if (song_char_on[f] >= 0) {
            chip_setup_control(&want, f, (uint8_t)song_char_on[f]);
        }
    }
    keys_route_song_wants(&keys_router, &want);
}

static void setup_encode(song_chip_setup* out, const chip_setup* in) {
    memset(out, 0, sizeof *out);
    const sid_chip_profile* profile = sid_chip_profile_get(in->profile);
    out->model = profile ? (uint8_t)profile->model : 0u;
    out->profile = in->profile;
    out->age = in->controls[0];
    out->dac = in->controls[1];
    out->thump = in->controls[2];
    out->bleed = in->controls[3];
    out->envelope = !in->gate_reset;
    out->expert_count = SID_EXPERT_COUNT;
    memcpy(out->expert, in->expert, SID_EXPERT_COUNT);
}

static int setup_valid(const song_chip_setup* chip) {
    const sid_chip_profile* profile = sid_chip_profile_get(chip->profile);
    uint32_t controls = chip->age | chip->dac | chip->thump | chip->bleed;
    if (chip->expert_count > SONG_EXPERT_MAX) {
        return 0;
    }
    for (uint32_t f = 0; f < chip->expert_count && f < SID_EXPERT_COUNT; f++) {
        if (chip->expert[f] > SID_CHARACTER_MAX && chip->expert[f] != SID_EXPERT_DEFAULT) {
            return 0;
        }
    }
    return profile && (uint32_t)profile->model == chip->model &&
           !(controls & 0x80u) && chip->envelope <= 1u;
}

static void setup_decode(chip_setup* out, const song_chip_setup* chip) {
    out->profile = chip->profile;
    out->controls[0] = chip->age;
    out->controls[1] = chip->dac;
    out->controls[2] = chip->thump;
    out->controls[3] = chip->bleed;
    out->gate_reset = chip->envelope == 0u;
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        out->expert[f] = f < chip->expert_count ? chip->expert[f] : SID_EXPERT_DEFAULT;
    }
}

typedef struct {
    song_chip_setup chip;
    gt2_options opts;
    uint8_t param[LIVE_PARAM_COUNT];
    uint8_t wide[2];
    midi_route_setup midi;
    uint8_t preset;
} song_save_data;

_Static_assert(sizeof(song_save_data) <= SONG_IO_BUFFER_BYTES, "metadata reuses the IO buffer");

static void song_save_params(song_save_data* data, const uint16_t* param) {
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; p++) {
        data->param[p] = (uint8_t)param[p];
    }
    data->wide[0] = (uint8_t)(param[LIVE_PARAM_CUTOFF] >> 8);
    data->wide[1] = (uint8_t)(param[LIVE_PARAM_PULSE_WIDTH] >> 8);
}

static void song_saved_params(const song_save_data* data, uint16_t* out) {
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; p++) {
        out[p] = data->param[p];
    }
    out[LIVE_PARAM_CUTOFF] |= (uint16_t)data->wide[0] << 8;
    out[LIVE_PARAM_PULSE_WIDTH] |= (uint16_t)data->wide[1] << 8;
}

__attribute__((noinline, unused)) static void song_collect(song_save_data* out) {
    song_ctl* sc = audio_song();
    gt2_song* s = sc->song;
    song_chip_setup chip;
    int has_chip = song_ext_get_chip(s, SONG_TAG_CHPA, &chip);
    if (song_base_valid) {
        setup_encode(&chip, &song_char_base);
    } else if (!has_chip) {
        chip_setup keys;
        chip_setup_of(&instrument, &keys);
        keys.profile = instrument.profile_req;
        setup_encode(&chip, &keys);
        chip.envelope = 1;
    }
    gt2_options opts;
    (void)song_ext_get_options(s, &opts);
    uint16_t param[LIVE_PARAM_COUNT];
    live_params_wire(&instrument, param);
    song_save_params(out, param);
    out->param[LIVE_PARAM_VOLUME] = 0u;
    out->chip = chip;
    out->opts = opts;
    out->midi = midi_routed()->setup;
    out->preset = instrument.preset;
}

static void song_snapshot(void* ctx, uint8_t* storage) {
    (void)ctx;
    song_save_data data;
    song_collect(&data);
    memcpy(storage, &data, sizeof data);
}

typedef struct {
    uint32_t sizes[5];
    uint32_t old_sizes[5];
} song_capture_plan;

__attribute__((noinline, unused)) static int song_capture_fits(
    const gt2_song* s, uint32_t expert_count, uint32_t midi_count, song_capture_plan* plan) {
    const uint32_t tags[] = {SONG_TAG_CHPA, SONG_TAG_OPTS, SONG_TAG_SLOT, SONG_TAG_KEYS, SONG_TAG_MIDI};
    plan->sizes[0] = 9u + expert_count;
    plan->sizes[1] = SONG_OPTS_BYTES;
    plan->sizes[2] = 2u + LIVE_SLOTS * SONG_SLOT_FIELDS;
    plan->sizes[3] = 3u + LIVE_PARAM_COUNT * 2u;
    plan->sizes[4] = 4u + midi_count + SONG_MIDI_OPTION_BYTES;
    uint32_t ext = s->ext_used;
    for (uint32_t i = 0; i < 5u; i++) {
        uint32_t old = 0;
        if (song_ext_find(s, tags[i], &old)) {
            ext -= SONG_CHUNK_HEADER + old;
            plan->old_sizes[i] = SONG_CHUNK_HEADER + old;
        } else {
            plan->old_sizes[i] = 0u;
        }
        ext += SONG_CHUNK_HEADER + plan->sizes[i];
    }
    if (ext > SONG_EXT_BYTES || gt2_saved_size(s) - gt2_saved_trailer_size(s->ext_used) +
            gt2_saved_trailer_size(ext) > STORE_FILE_MAX) {
        return 0;
    }
    uint32_t at = s->ext_used;
    for (uint32_t pass = 0; pass < 2u; pass++) {
        for (uint32_t i = 0; i < 5u; i++) {
            uint32_t size = SONG_CHUNK_HEADER + plan->sizes[i];
            if ((size > plan->old_sizes[i]) != (pass != 0u)) {
                continue;
            }
            at = at - plan->old_sizes[i] + size;
            if (at > SONG_EXT_BYTES || gt2_saved_ext_check(s, at) != 0) {
                return 0;
            }
        }
    }
    return 1;
}

static int song_capture(void* ctx) {
    (void)ctx;
    gt2_song* s = audio_song()->song;
    song_save_data data;
    if (songs.snapshot) {
        memcpy(&data, songs.data, sizeof data);
    } else {
        song_collect(&data);
    }
    const song_chip_setup chip = data.chip;
    const gt2_options opts = data.opts;
    song_slots slots = {LIVE_SLOTS, {{0}}};
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        for (uint32_t f = 0; f < LIVE_SLOT_FIELDS; f++) {
            slots.slot[k][f] = (uint8_t)data.param[LIVE_PARAM_SLOT + k * LIVE_SLOT_FIELDS + f];
        }
    }
    song_keys keys = {LIVE_PARAM_COUNT, data.preset, {0}};
    song_saved_params(&data, keys.param);
    song_midi midi;
    midi_route_setup_encode(&data.midi, &midi);
    if (!setup_valid(&chip)) {
        memcpy(songs.failure, "SNG: BAD CHIP SETUP", sizeof "SNG: BAD CHIP SETUP");
        return 0;
    }
    song_capture_plan plan;
    if (!song_capture_fits(s, chip.expert_count, midi.count, &plan)) {
        return 0;
    }
    for (uint32_t pass = 0; pass < 2u; pass++) {
        for (uint32_t i = 0; i < 5u; i++) {
            if ((SONG_CHUNK_HEADER + plan.sizes[i] > plan.old_sizes[i]) != (pass != 0u)) {
                continue;
            }
            if (i == 0u) {
                (void)song_ext_set_chip(s, SONG_TAG_CHPA, &chip);
            } else if (i == 1u) {
                (void)song_ext_set_options(s, &opts);
            } else if (i == 2u) {
                (void)song_ext_set_slots(s, &slots);
            } else if (i == 3u) {
                (void)song_ext_set_keys(s, &keys);
            } else {
                (void)song_ext_set_midi(s, &midi);
            }
        }
    }
    if (!song_base_valid) {
        chip_setup base;
        setup_decode(&base, &chip);
        song_base_set(&base);
    }
    return 1;
}

static uint32_t song_generation(void* ctx) {
    (void)ctx;
    uint16_t values[LIVE_PARAM_COUNT];
    live_params_wire(&instrument, values);
    values[LIVE_PARAM_VOLUME] = 0u;
    uint32_t crc = store_crc32(0u, values, sizeof values);
    crc = store_crc32(crc, &instrument.preset, sizeof instrument.preset);
    crc = store_crc32(crc, &midi_routed()->setup, sizeof(midi_route_setup));
    song_ctl* sc = audio_song();
    song_chip_setup chip;
    (void)song_ext_get_chip(sc->song, SONG_TAG_CHPA, &chip);
    if (song_base_valid) {
        setup_encode(&chip, &song_char_base);
    }
    return store_crc32(crc, &chip, sizeof chip);
}

static void song_apply_keys(const uint16_t* param, uint32_t count, uint8_t preset) {
    if (preset < LIVE_PRESET_COUNT) {
        live_set_preset(&instrument, preset);
    }
    for (uint32_t p = 0; p < count && p < LIVE_PARAM_EXPERT; p++) {
        if (p != LIVE_PARAM_VOLUME && param[p] <= live_param_max(p)) {
            live_param_set(&instrument, p, param[p]);
        }
    }
    uint32_t changed = 0;
    instrument.expert_active = 0;
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        uint32_t p = LIVE_PARAM_EXPERT + f;
        uint32_t value = p < count ? param[p] : SID_EXPERT_DEFAULT;
        uint8_t raw = value <= SID_CHARACTER_MAX ? (uint8_t)value : SID_EXPERT_DEFAULT;
        changed |= instrument.expert[f] != raw;
        instrument.expert[f] = raw;
        instrument.expert_active |= raw != SID_EXPERT_DEFAULT;
    }
    instrument.expert_dirty = 1;
    instrument.expert_version += changed != 0;
    for (uint32_t p = LIVE_PARAM_EXPERT_END; p < count && p < LIVE_PARAM_COUNT; p++) {
        if (param[p] <= live_param_max(p)) {
            live_param_set(&instrument, p, param[p]);
        }
    }
}

static void song_apply_midi(const midi_route_setup* setup) {
    midi_route_set(midi_routed(), setup);
    midi_sync_rows(midi_synced(), setup->rows_per_beat);
    ui_song_rows(setup->rows_per_beat);
    midi_sync_song_changed(midi_synced());
}

static void song_restore(void* ctx, const uint8_t* storage) {
    (void)ctx;
    song_save_data data;
    memcpy(&data, storage, sizeof data);
    song_apply_midi(&data.midi);
    uint16_t param[LIVE_PARAM_COUNT];
    song_saved_params(&data, param);
    song_apply_keys(param, LIVE_PARAM_COUNT, data.preset);
    chip_setup base;
    setup_decode(&base, &data.chip);
    song_base_set(&base);
    keys_route_song_wants(&keys_router, &base);
    keys_router.song_user = 0;
}

static int song_loaded(void* ctx) {
    (void)ctx;
    gt2_song* s = audio_song()->song;
    song_chip_setup chip;
    int has_chip = song_ext_get_chip(s, SONG_TAG_CHPA, &chip);
    if (has_chip && !setup_valid(&chip)) {
        memcpy(songs.failure, "SNG: BAD CHIP SETUP", sizeof "SNG: BAD CHIP SETUP");
        return 0;
    }
    midi_route_setup setup;
    (void)midi_route_setup_from_song(s, &midi_routed()->setup, &setup);
    song_midi midi;
    midi_route_setup_encode(&setup, &midi);
    song_capture_plan plan;
    if (!song_capture_fits(s, SID_EXPERT_COUNT, midi.count, &plan)) {
        return 0;
    }
    song_setup_on = 0;
    song_warned = 0;
    for (uint32_t f = 0; f < 4u; f++) {
        song_char_on[f] = -1;
    }
    ui_song_loaded(song_io_returning(&songs));
    if (songs.restoring || song_io_keeping(&songs)) {
        if (!songs.restoring && song_base_valid) {
            keys_route_song_wants(&keys_router, &song_char_base);
            keys_router.song_user = 0;
        }
        ui_song_rows(midi_routed()->setup.rows_per_beat);
        midi_sync_song_changed(midi_synced());
        return 1;
    }
    song_base_valid = 0;
    song_keys keys;
    song_apply_midi(&setup);
    if (song_ext_get_keys(s, &keys)) {
        song_apply_keys(keys.param, keys.count, keys.preset);
    }
    song_slots slots;
    if (song_ext_get_slots(s, &slots)) {
        for (uint32_t k = 0; k < slots.count && k < LIVE_SLOTS; k++) {
            for (uint32_t f = 0; f < LIVE_SLOT_FIELDS; f++) {
                live_param_set(&instrument, LIVE_PARAM_SLOT + k * LIVE_SLOT_FIELDS + f,
                               slots.slot[k][f]);
            }
        }
    }
    chip_setup base;
    if (has_chip) {
        setup_decode(&base, &chip);
    } else {
        chip_setup_of(&instrument, &base);
        base.profile = instrument.profile_req;
        base.gate_reset = 0;
    }
    song_base_set(&base);
    keys_route_song_wants(&keys_router, &base);
    keys_router.song_user = 0;
    if (midi_object.target == 2u && midi_object.offset == UINT32_MAX &&
        (midi_object.owned & MIDI_OBJECT_WORKSPACE)) {
        midi_object.loaded = 1;
    }
    return 1;
}

static int midi_object_stopped(const song_ctl* sc) {
    return song_ctl_state(sc) != SONG_PLAYING && song_ctl_state(sc) != SONG_LOADING &&
           !sc->isr_playing && !sc->auditioning && !sc->audition && !sc->handback &&
           !sc->inflight_n && !sc->ff_on && !sc->deriving && !sc->spare_wait &&
           !sc->bg_on && !sc->widened && !sc->want_handover &&
           sc->stage == sc->cmd_tail;
}

static void song_after_import(song_ctl* sc) {
    sc->state = SONG_IDLE;
    sc->shapes_used = 0;
    sc->player.shape = NULL;
    sc->bg_on = sc->bg_dirty = sc->widened = sc->ff_on = sc->deriving = 0;
    sc->want_handover = sc->spare_wait = sc->hold_next = 0;
}

static int midi_object_open(void* ctx, uint8_t target, uint8_t index, int writing,
                            uint32_t length) {
    (void)ctx;
    (void)length;
    if (song_away) {
        return SYSEX_BUSY;
    }
    song_ctl* sc = audio_song();
    gt2_song* s = sc->song;
    if (writing && target == 2u && memcmp(songs.name, "MIDI SONG", 10u) == 0) {
        midi_object.staging_busy = 1;
        return SYSEX_BUSY;
    }
    if (song_io_busy(&songs) || songs.about || songs.external || s->frozen || s->pending || s->pending_loose ||
        (target == 1u && !midi_object_stopped(sc)) ||
        (writing && target == 2u && song_io_changed(&songs))) {
        return SYSEX_BUSY;
    }
    if (!writing) {
        if (target == 1u) {
            if (gt2_ins_export_begin(&midi_object.instrument, s, index) != GT2_OK) {
                return SYSEX_IO;
            }
        } else if (!song_capture(0)) {
            return SYSEX_FULL;
        }
    }
    midi_object.target = target;
    midi_object.index = index;
    midi_object.offset = 0;
    midi_object.loaded = 0;
    midi_object.owned = MIDI_OBJECT_FROZEN;
    s->frozen = 1;
    return 0;
}

static int midi_object_available(void* ctx) {
    (void)ctx;
    if (song_away) {
        return SYSEX_BUSY;
    }
    return song_io_busy(&songs) || songs.external || audio_song()->song->frozen ? SYSEX_BUSY : 0;
}

static uint32_t midi_object_size(void* ctx) {
    (void)ctx;
    return midi_object.target == 1u ? midi_object.instrument.length :
                                    gt2_saved_size(audio_song()->song);
}

static int midi_object_read(void* ctx, uint32_t offset, void* buf, uint32_t n) {
    (void)ctx;
    if (midi_object.target == 1u) {
        return (int)gt2_ins_export_read(&midi_object.instrument, offset, buf, n);
    }
    if (!offset) {
        gt2_stream_write_begin(&song_stream.stream, audio_song()->song, 1);
        midi_object.offset = 0;
    }
    if (offset != midi_object.offset) {
        return -1;
    }
    uint32_t done = 0;
    while (done < n) {
        int got = gt2_stream_write_step(&song_stream.stream, (uint8_t*)buf + done, n - done);
        if (got <= 0) {
            return -1;
        }
        done += (uint32_t)got;
    }
    midi_object.offset += done;
    return (int)done;
}

static uint32_t midi_file_read(void* ctx, uint32_t offset, uint8_t* buf, uint32_t n) {
    const store_file* file = ctx;
    int got = store_file_read(store_fm1(), file, offset, buf, n);
    return got > 0 ? (uint32_t)got : 0u;
}

static int midi_object_complete(void* ctx, const store_file* file) {
    (void)ctx;
    if (song_away) {
        return SYSEX_BUSY;
    }
    song_ctl* sc = audio_song();
    if (midi_object.target == 1u) {
        if (midi_object.offset != UINT32_MAX && !midi_object_stopped(sc)) {
            return SYSEX_BUSY;
        }
        gt2_ins_import_state* work = &song_stream.instrument;
        if (midi_object.offset != UINT32_MAX) {
            gt2_err error = gt2_ins_import_begin(work, sc->song, midi_object.index, midi_file_read,
                                                (void*)file, file->length, 1);
            if (error != GT2_OK) {
                return SYSEX_IO;
            }
            midi_object.offset = UINT32_MAX;
        }
        int result = gt2_ins_import_step(work);
        if (result == 0) {
            return SYSEX_PENDING;
        }
        if (result > 0) {
            song_after_import(sc);
        }
        return result > 0 ? 0 : work->error == GT2_ERR_FULL || work->error == GT2_ERR_SONG_FULL ?
               SYSEX_FULL : SYSEX_IO;
    }
    if (midi_object.offset != UINT32_MAX) {
        sc->song->frozen = 0;
        midi_object.owned = 0;
        if (!song_io_load(&songs, file->name, 0)) {
            return SYSEX_BUSY;
        }
        songs.external = 1;
        midi_object.owned = MIDI_OBJECT_WORKSPACE;
        midi_object.offset = UINT32_MAX;
        return SYSEX_PENDING;
    }
    if (song_io_busy(&songs)) {
        if (songs.restoring || songs.failure[0]) {
            midi_object.loaded = 0;
        }
        return SYSEX_PENDING;
    }
    return midi_object.loaded ? 0 : SYSEX_IO;
}

static void midi_object_close(void* ctx) {
    (void)ctx;
    if (midi_object.owned & MIDI_OBJECT_FROZEN) {
        audio_song()->song->frozen = 0;
    }
    if (midi_object.owned & MIDI_OBJECT_WORKSPACE) {
        songs.external = 0;
    }
    midi_object.owned = 0;
    midi_object.target = 0;
    midi_object.offset = 0;
}

static void midi_sysex_toast(void* ctx, uint8_t reason) {
    (void)ctx;
    const char* text = "MIDI: BAD PACKET";
    switch (reason) {
    case SYSEX_RECEIVING: text = "RECEIVING"; break;
    case SYSEX_WAITING: text = "UPLOAD WAITS FOR QUIET"; break;
    case SYSEX_SAVED: text = "SAVED"; break;
    case SYSEX_BUSY: text = "STORE BUSY: RESEND"; break;
    case SYSEX_FULL: text = "STORE FULL"; break;
    case SYSEX_NOT_FOUND: text = "MIDI: NOT FOUND"; break;
    case SYSEX_CHECKSUM: text = "MIDI: CHECKSUM"; break;
    case SYSEX_TIMEOUT: text = "MIDI: TIMEOUT"; break;
    case SYSEX_ABORTED: text = "MIDI: ABORTED"; break;
    case SYSEX_IO: text = "MIDI: FILE ERROR"; break;
    case SYSEX_RSID: text = sid_io_message(SID_IO_RSID); break;
    case SYSEX_MUS: text = sid_io_message(SID_IO_MUS); break;
    case SYSEX_BROKEN: text = sid_io_message(SID_IO_BROKEN); break;
    case SYSEX_TOO_LONG: text = sid_io_message(SID_IO_TOO_LONG); break;
    default: break;
    }
    if (reason == SYSEX_BUSY && midi_object.staging_busy) {
        text = "SAVE AS FIRST";
    }
    midi_object.staging_busy = 0;
    ui_toast(text, 2000u);
}

__attribute__((noinline, unused)) static void midi_objects_bind(void) {
    const sysex_hooks binding = {
        .open = midi_object_open, .size = midi_object_size, .read = midi_object_read,
        .complete = midi_object_complete, .close = midi_object_close, .toast = midi_sysex_toast,
        .available = midi_object_available
    };
    midi_sysex_bind(&binding, &uptime_ms);
}

static int16_t song_error;

static void song_tell(song_ctl* sc) {
    if (sc->error != song_error) {
        song_error = sc->error;
        if (song_error == SONG_ERR_MULTISPEED) {
            char t[32] = "SNG: MULTISPEED ";
            uint32_t k = 16u, m = sc->opts.multispeed;
            if (m >= 10u) {
                t[k++] = (char)('0' + m / 10u);
            }
            t[k++] = (char)('0' + m % 10u);
            const char* tail = " (MAX 4)";
            while (*tail) {
                t[k++] = *tail++;
            }
            t[k] = 0;
            ui_toast(t, 2000u);
        } else if (song_error == GT2_PLAY_ERR_STEREO) {
            ui_toast("SNG: STEREO, NOT PLAYED", 2000u);
        } else if (song_error != 0) {
            ui_toast("SNG: NOTHING TO PLAY", 2000u);
        }
    }
    if (song_ctl_state(sc) != SONG_PLAYING) {
        return;
    }
    uint32_t w = song_ctl_warnings(sc) & ~song_warned;
    if (w & GT2_PLAY_WARN_PITCH) {
        ui_toast("SNG: A-4 PITCH NOT PLAYED", 2000u);
    } else if (w & GT2_PLAY_WARN_PAST_B7) {
        ui_toast("SNG: NOTE PAST B-7", 2000u);
    }
    song_warned |= w;
}

typedef struct {
    volatile uint32_t op;
    uint32_t a, b;
    uint8_t cell[4];
    volatile uint32_t block;
} song_test_req;
#define SONG_TEST_REQS 4u
song_test_req song_test_hook[SONG_TEST_REQS];

static void song_test_edits(song_ctl* sc) {
    for (uint32_t i = 0; i < SONG_TEST_REQS; i++) {
        song_test_req* r = &song_test_hook[i];
        if (r->op == 0u) {
            continue;
        }
        gt2_edit e;
        gt2_err err = GT2_ERR_RANGE;
        if (r->op == 1u) {
            err = gt2_set_row(sc->song, r->a, r->b, r->cell, &e);
        } else if (r->op == 2u) {
            err = gt2_pattern_insert_row(sc->song, r->a, r->b, &e);
        } else if (r->op == 3u) {
            err = gt2_pattern_delete_row(sc->song, r->a, r->b, &e);
        }
        if (err == GT2_ERR_PENDING) {
            return;
        }
        if (err != GT2_OK) {
            ui_toast(sc->song->frozen ? "SAVING" : "SONG FULL", 2000u);
        }
        if (err == GT2_OK && !song_ctl_edit(sc, &e)) {
            gt2_edit_cancel(sc->song, &e);
            return;
        }
        r->block = sc->blocks;
        r->op = 0u;
    }
}

typedef struct {
    volatile uint32_t op;
    char name[STORE_NAME_MAX + 1u];
    int32_t result;
} song_io_test_req;
song_io_test_req song_io_test_hook;

static void song_io_test(void) {
    song_io_test_req* r = &song_io_test_hook;
    switch (r->op) {
    case 1: r->result = song_io_new(&songs, 1); break;
    case 2: r->result = song_io_save(&songs, r->name, uptime_ms); break;
    case 3: r->result = song_io_load(&songs, r->name, 1); break;
    case 4: r->result = song_io_delete(&songs, r->name, 1); break;
    case 5: r->result = song_io_about(&songs, 1); break;
    case 6: r->result = song_io_about(&songs, 0); break;
    default: return;
    }
    r->op = 0;
}

#define AUDITION_HOLD_MS 2000u

static song_status song_seen;
static uint8_t jam_wait[3];
static uint8_t jam_instr;
static uint8_t jam_wait_off;
static uint8_t jam_held;
static uint32_t jam_last_ms;

static int st_play_from(void* ctx, uint32_t sub, uint32_t pos) {
    song_ctl* sc = ctx;
    if (song_away) {
        return 1;
    }
    if (midi_object.owned && midi_object.target == 1u) {
        return 1;
    }
    if (song_io_busy(&songs) && !song_io_saving(&songs)) {
        return 1;
    }
    if (!song_ctl_play(sc, sub, pos)) {
        return 0;
    }
    sc->chance_seed = uptime_ms;
    song_warned = 0;
    song_setup_on = 0;
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        song_seen.order_pos[v] = SONG_T_NOWHERE;
        song_seen.row[v] = SONG_T_NOWHERE;
    }
    return 1;
}

static int st_play(void* ctx, uint32_t pos) {
    return st_play_from(ctx, 0u, pos);
}

static int st_stop(void* ctx) {
    if (song_away) {
        return 1;
    }
    return song_ctl_stop(ctx);
}

static int st_resume(void* ctx) {
    song_ctl* sc = ctx;
    if (song_away) {
        return 1;
    }
    if (midi_object.owned && midi_object.target == 1u) {
        return 1;
    }
    if (song_io_busy(&songs) && !song_io_saving(&songs)) {
        return 1;
    }
    song_setup_on = 0;
    return song_ctl_continue(sc);
}

static int st_mute(void* ctx, uint32_t mask) {
    if (song_away) {
        return 1;
    }
    return song_ctl_mute(ctx, mask);
}

static int st_jam(void* ctx, uint32_t voice, uint32_t note, uint32_t instr) {
    song_ctl* sc = ctx;
    if (song_away) {
        return 1;
    }
    if (midi_object.owned && midi_object.target == 1u) {
        return 1;
    }
    if (song_io_busy(&songs) && !song_io_saving(&songs)) {
        return 1;
    }
    int off = note == SONG_T_RELEASE;
    jam_last_ms = uptime_ms;
    if (song_ctl_jam_ready(sc) && !jam_wait[0]) {
        if (!song_ctl_jam(sc, voice, off ? SONG_JAM_RELEASE : note, instr)) {
            return 0;
        }
        jam_held = (uint8_t)!off;
        return 1;
    }
    if (off) {
        jam_wait_off = jam_wait[0];
        jam_held = 0;
        return 1;
    }
    if (song_ctl_state(sc) != SONG_PLAYING && !song_ctl_audition(sc)) {
        return 0;
    }
    jam_wait[0] = 1;
    jam_wait[1] = (uint8_t)voice;
    jam_wait[2] = (uint8_t)note;
    jam_instr = (uint8_t)instr;
    jam_wait_off = 0;
    jam_held = 1;
    return 1;
}

static int st_draft(void* ctx, uint32_t voice, uint32_t first, uint32_t n, const uint8_t* left,
                    const uint8_t* right) {
    return song_away ? 0 : song_ctl_draft(ctx, voice, first, n, left, right);
}

static int st_held(void* ctx, uint32_t voice, const uint8_t* locks, uint32_t n) {
    return song_away ? 0 : song_ctl_held(ctx, voice, locks, n);
}

static void song_seen_rows(const song_event* ev, uint32_t n) {
    for (uint32_t i = 0; i < n; i++) {
        uint32_t ch = SONG_EV_CHANNEL(&ev[i]);
        if ((SONG_EV_FLAGS(&ev[i]) & GT2_EV_ROW) && ch < SONG_T_VOICES) {
            song_seen.order_pos[ch] = ev[i].order_pos;
            song_seen.row[ch] = ev[i].row;
            song_seen.row_call[ch] = ev[i].frame;
        }
    }
}

#define REC_LAG_SAMPLES                                                                         \
    (2u * AUDIO_HALF_FRAMES - AUDIO_HALF_FRAMES / 2u +                                          \
     ((2u * INPUT_PRESS_FRAMES - 1u) * INPUT_FRAME_US * AUDIO_SAMPLE_RATE + 1000000u) /          \
         2000000u)
_Static_assert(REC_LAG_SAMPLES == 265u, "REC's lag, as docs/reference/controls.md says");

static void st_status(void* ctx, song_status* out) {
    song_ctl* sc = ctx;
    if (song_away) {
        memset(out, 0, sizeof *out);
        for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
            out->order_pos[v] = out->row[v] = SONG_T_NOWHERE;
        }
        return;
    }
    *out = song_seen;
    out->applied = sc->applied + sc->idle_edits;
    out->state = (uint8_t)song_ctl_shown_state(sc);
    out->subtune = sc->play_sub;
    out->again = (uint8_t)song_ctl_again(sc);
    {
        const volatile song_ctl* vc = sc;
        uint32_t samples, calls, at, q;
        do {
            samples = vc->samples;
            calls = vc->calls;
            at = vc->call_sample;
            q = vc->period.q;
        } while (samples != vc->samples);
        int32_t d = (int32_t)(samples - at);
        uint32_t frac = d > 0 && q ? (uint32_t)d * 256u / q : 0u;
        out->now_call = (uint16_t)(calls ? calls - 1u : 0u);
        out->now_frac = (uint8_t)(frac > 255u ? 255u : frac);
        out->rec_lag = (uint16_t)(q ? REC_LAG_SAMPLES * 256u / q : 0u);
    }
    static gt2_options before;
    static uint8_t frames;
    gt2_options o = sc->opts;
    if (!sc->shapes_used) {
        if (frames++ % 25u == 0u && !song_ext_get_options(sc->song, &before)) {
            gt2_options_default(&before);
        }
        o = before;
    }
    out->multispeed = o.multispeed;
    out->ntsc = o.ntsc;
    out->mute = sc->mute;
    const gt2_player* p = &sc->player;
    uint8_t t = p->shape ? p->chn[0].tempo : 0u;
    if (!p->shape) {
        out->speed[0] = out->speed[1] = 6u;
    } else if (t < 2u) {
        out->speed[0] = p->funktempo[0];
        out->speed[1] = p->funktempo[1];
    } else {
        out->speed[0] = out->speed[1] = (uint8_t)(t + 1u);
    }
    const sid* a = sc->chip;
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        out->wave[v] = sc->shadow[v * 7u + 4u];
        out->env[v] = a ? a->env[v].counter : 0u;
    }
}

static int st_hold(void* ctx, uint32_t on) {
    if (song_away) {
        return 1;
    }
    if (midi_object.owned && midi_object.target == 1u) {
        return 1;
    }
    if (song_io_busy(&songs) && !song_io_saving(&songs)) {
        return 1;
    }
    song_ctl_hold(ctx, (int)on);
    return 1;
}

static int st_align(void* ctx, uint32_t sample) {
    if (song_away) {
        return 1;
    }
    if (midi_object.owned && midi_object.target == 1u) {
        return 1;
    }
    if (song_io_busy(&songs) && !song_io_saving(&songs)) {
        return 1;
    }
    return song_ctl_align(ctx, sample);
}

static int st_period(void* ctx, uint32_t q16) {
    if (song_away) {
        return 1;
    }
    if (midi_object.owned && midi_object.target == 1u) {
        return 1;
    }
    if (song_io_busy(&songs) && !song_io_saving(&songs)) {
        return 1;
    }
    song_ctl* sc = ctx;
    song_period p;
    if (q16 == 0u) {
        p = song_period_of(sc->opts.multispeed, sc->opts.ntsc);
    } else {
        p.q = q16 >> 16;
        p.r = q16 & 0xffffu;
        p.den = 65536u;
    }
    return song_ctl_set_period(sc, p);
}

static int st_seek(void* ctx, uint32_t call) {
    if (song_away) {
        return 1;
    }
    if (midi_object.owned && midi_object.target == 1u) {
        return 1;
    }
    if (song_io_busy(&songs) && !song_io_saving(&songs)) {
        return 1;
    }
    return song_ctl_seek(ctx, call);
}

static int st_seek_further(void* ctx, uint32_t call) {
    if (song_away) {
        return 1;
    }
    if (midi_object.owned && midi_object.target == 1u) {
        return 1;
    }
    if (song_io_busy(&songs) && !song_io_saving(&songs)) {
        return 1;
    }
    return song_ctl_seek_further(ctx, call);
}

static void st_clock(void* ctx, song_clock* out) {
    const volatile song_ctl* sc = ctx;
    if (song_away) {
        memset(out, 0, sizeof *out);
        out->state = SONG_IDLE;
        return;
    }
    uint32_t samples;
    do {
        samples = sc->samples;
        out->state = (uint8_t)sc->state;
        out->seeking = sc->ff_on ? (sc->ff_sent ? 2u : 1u) : 0u;
        const volatile gt2_player* p = &sc->player;
        uint8_t t = p->shape ? p->chn[0].tempo : 0u;
        if (!sc->shapes_used || !p->shape) {
            out->speed[0] = out->speed[1] = 0u;
        } else if (t < 2u) {
            out->speed[0] = p->funktempo[0];
            out->speed[1] = p->funktempo[1];
        } else {
            out->speed[0] = out->speed[1] = (uint8_t)(t + 1u);
        }
        out->calls = song_ctl_clock_calls((const song_ctl*)sc);
        out->from_top = (uint8_t)song_ctl_from_top((const song_ctl*)sc);
        out->call_sample = sc->call_sample;
        const volatile song_period* q = &sc->period;
        float frac = (float)q->r * 65536.0f / (float)(q->den ? q->den : 1u);
        out->period_q16 = (q->q << 16) + (uint32_t)frac;
    } while (samples != sc->samples);
}

static int st_edit(void* ctx, const gt2_edit* edit, uint32_t* ticket) {
    song_ctl* sc = ctx;
    if (song_away || !song_ctl_edit(sc, edit))
        return 0;
    *ticket = sc->edits_submitted;
    return 1;
}

static int st_edit_done(void* ctx, uint32_t ticket) {
    const song_ctl* sc = ctx;
    if (song_away) {
        return 1;
    }
    return (int32_t)(sc->edits_completed - ticket) >= 0;
}

static int st_edit_with_locks(void* ctx, const gt2_edit* edit, const uint8_t* locks,
                               uint32_t nlocks, uint32_t* ticket) {
    song_ctl* sc = ctx;
    if (song_away || !song_ctl_edit_with_locks(sc, edit, locks, nlocks))
        return 0;
    *ticket = sc->edits_submitted;
    return 1;
}

static int st_song_edit_saving_and_size_guard(void* ctx, const song_edit_request* request) {
    (void)request;
    const song_ctl* sc = ctx;
    if (song_away || (song_io_busy(&songs) && !song_io_saving(&songs))) {
        return SONG_EDIT_GUARD_BUSY;
    }
    if (song_io_saving(&songs) || sc->song->frozen) {
        return SONG_EDIT_GUARD_SAVING;
    }
    return SONG_EDIT_GUARD_OK;
}

static const song_transport song_binding = {
    .play = st_play,
    .play_from = st_play_from,
    .stop = st_stop,
    .resume = st_resume,
    .mute = st_mute,
    .jam = st_jam,
    .status = st_status,
    .hold = st_hold,
    .align = st_align,
    .period = st_period,
    .seek = st_seek,
    .seek_further = st_seek_further,
    .clock = st_clock,
    .edit = st_edit,
    .edit_done = st_edit_done,
    .edit_guard = st_song_edit_saving_and_size_guard,
    .edit_with_locks = st_edit_with_locks,
    .draft = st_draft,
    .held = st_held,
};
static song_transport song_bound;

static uint8_t song_taken;

static void song_keys_take(void* ctx, uint32_t voices) {
    (void)ctx;
    if (song_away) {
        return;
    }
    song_taken = (uint8_t)(song_taken | voices);
    (void)song_ctl_take(audio_song(), song_taken);
}

__attribute__((noinline, unused)) static void song_file_pass(song_ctl* sc, uint32_t slices) {
    static uint8_t turn;
    if (!(turn++ & 1u) || slices != sc->slices || !audio_work_slice()) {
        return;
    }
    song_io_poll(&songs, uptime_ms);
    char message[32];
    if (song_io_take_message(&songs, message)) {
        ui_toast(message, 2000u);
        if (memcmp(message, "SONG SAVED", 10u) == 0) {
            ui_sound_play(&ui_sounds, UI_SOUND_CHUNK);
        }
    }
}

#define SONG_EVENTS_AT_ONCE 16u
static uint8_t song_heard_state;

__attribute__((noinline, unused)) static void song_events(song_ctl* sc) {
    song_heard_state = sc->state;
    song_event ev[SONG_EVENTS_AT_ONCE];
    uint32_t n = 0;
    for (uint32_t k = 0; k < SONG_EVENT_RING / SONG_EVENTS_AT_ONCE; k++) {
        n = song_ctl_events(sc, ev, SONG_EVENTS_AT_ONCE);
        song_seen_rows(ev, n);
        ui_song(sc, ev, n);
        if (n < SONG_EVENTS_AT_ONCE) {
            break;
        }
    }
}

static void song_pass(void) {
    song_ctl* sc = audio_song();
    if (jam_wait[0] && song_ctl_jam_ready(sc) &&
        song_ctl_jam(sc, jam_wait[1], jam_wait[2], jam_instr)) {
        jam_wait[0] = 0;
        if (jam_wait_off && song_ctl_jam(sc, jam_wait[1], SONG_JAM_RELEASE, jam_instr)) {
            jam_wait_off = 0;
        }
    }
    if (sc->auditioning && !jam_held && !jam_wait[0] &&
        uptime_ms - jam_last_ms >= AUDITION_HOLD_MS) {
        song_ctl_audition_end(sc);
    }
    uint32_t st = song_ctl_state(sc);
    int engaged = st == SONG_LOADING || st == SONG_PLAYING || sc->auditioning || sc->handback;
    if (engaged && !song_mine) {
        keys_route_set_song(&keys_router, 1);
        song_mine = 1;
    }
    song_test_edits(sc);
    if (song_io_test_hook.op) {
        song_io_test();
    }
    uint32_t slices = sc->slices;
    audio_song_poll();
    if (sc->ev_head != sc->ev_tail || sc->state != song_heard_state) {
        song_events(sc);
    }
    if (songs.state || songs.about_play || songs.message[0]) {
        song_file_pass(sc, slices);
    }
    st = song_ctl_state(sc);
    engaged = st == SONG_LOADING || st == SONG_PLAYING || sc->auditioning || sc->handback;
    if (song_mine && !engaged) {
        keys_route_set_song(&keys_router, 0);
        song_mine = 0;
    }
    song_tell(sc);
    st = song_ctl_shown_state(sc);
    front.playing = st == SONG_PLAYING || st == SONG_LOADING;
}

enum {
    TUNE_OFF,
    TUNE_SONG_STOP,
    TUNE_TO_NONE,
    TUNE_TO_PSID,
    TUNE_PREP,
    TUNE_ON,
    TUNE_RELOAD,
    TUNE_END,
    TUNE_BACK_NONE,
    TUNE_BACK_SONG,
    TUNE_BACK_READ
};
enum { TUNE_ASK_NONE, TUNE_ASK_START, TUNE_ASK_SWITCH };
static struct {
    uint8_t state;
    uint8_t chips;
    uint8_t fresh;
    uint8_t toasted;
    uint8_t clocked;
    uint16_t song;
    int16_t told;
    uint8_t turn;
    sid_io io;
    uint32_t pal_q16;
    sid* was[2];
    chip_setup want[2];
    uint8_t ask;
    uint8_t hold_stop;
    uint8_t queued;
    uint8_t mute;
    char next[STORE_NAME_MAX + 1u];
    uint32_t cycles;
    uint32_t cycles_from, cycles_ms;
    uint32_t busy, window_t;
    uint16_t load;
    uint16_t ticks_k;
    uint8_t lift;
    uint8_t lift_flags;
    uint16_t lift_len;
    uint32_t lift_ms;
} sid_tune;

static uint32_t tune_step_q16(uint32_t clock) {
    return ((clock / AUDIO_SAMPLE_RATE) << 16) |
           (((clock % AUDIO_SAMPLE_RATE) << 16) / AUDIO_SAMPLE_RATE);
}

static void tune_setup(chip_setup* out, uint32_t model) {
    chip_setup_of(&instrument, out);
    if ((model == 1u || model == 2u) && ui_settings()->sid_model == SETTINGS_SID_AUTO) {
        out->profile = (uint8_t)sid_chip_default(model == 2u ? SID_MODEL_8580 : SID_MODEL_6581);
        for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; f++) {
            out->controls[f] = 0;
        }
        for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
            out->expert[f] = SID_EXPERT_DEFAULT;
        }
    }
    out->gate_reset = 0;
}

static int tune_open(const char* name, psid_header* h) {
    int r = sid_io_open(&sid_tune.io, store_fm1(), name, h);
    if (r != SID_IO_OK) {
        ui_toast(sid_io_message(r), 2000u);
        return 0;
    }
    return 1;
}

static int tune_play(const char* name, uint32_t song) {
    psid_header h;
    sid_io probe;
    int r = sid_io_open(&probe, store_fm1(), name, &h);
    if (r != SID_IO_OK) {
        ui_toast(sid_io_message(r), 2000u);
        return 0;
    }
    sid_tune.song = (uint16_t)song;
    sid_tune.hold_stop = 0;
    if (sid_tune.state > TUNE_TO_NONE) {
        memcpy(sid_tune.next, probe.file.name, sizeof sid_tune.next);
        if (sid_tune.state >= TUNE_END) {
            sid_tune.queued = 1;
        } else {
            sid_tune.ask = TUNE_ASK_SWITCH;
        }
        return 1;
    }
    if (keys_router.player) {
        ui_toast("MIDI MODE HAS CHIP A", 2000u);
        return 0;
    }
    sid_tune.io = probe;
    sid_tune.chips = (uint8_t)(h.sid2 ? 2u : 1u);
    sid_tune.told = 0;
    sid_tune.ask = TUNE_ASK_NONE;
    sid_tune.queued = 0;
    if (sid_tune.state == TUNE_OFF) {
        sid_tune.state = TUNE_SONG_STOP;
    }
    return 1;
}

static int tune_start(uint32_t song) {
    if (sid_tune.state == TUNE_OFF || sid_tune.state >= TUNE_END) {
        return 0;
    }
    sid_tune.song = (uint16_t)song;
    sid_tune.hold_stop = 0;
    if (sid_tune.state == TUNE_ON && sid_tune.ask == TUNE_ASK_NONE) {
        sid_tune.ask = TUNE_ASK_START;
    }
    return 1;
}

static void tune_stop(void) {
    if (sid_tune.state == TUNE_ON) {
        sid_tune.ask = TUNE_ASK_NONE;
        (void)psid_ctl_stop(audio_psid());
    } else if (sid_tune.state != TUNE_OFF && sid_tune.state < TUNE_END) {
        sid_tune.hold_stop = 1;
    }
}

static void tune_end(void) {
    sid_tune.ask = TUNE_ASK_NONE;
    sid_tune.queued = 0;
    if (sid_tune.state == TUNE_SONG_STOP) {
        sid_tune.state = TUNE_OFF;
    } else if (sid_tune.state != TUNE_OFF && sid_tune.state < TUNE_END) {
        sid_tune.state = TUNE_END;
    }
}

static uint32_t tune_mutes(void) {
    return sid_tune.chips == 2u ? (uint32_t)sid_tune.mute * 9u : sid_tune.mute;
}

static void tune_meter(uint32_t cycles) {
    uint32_t now = fm1_ticks(), span = (now - sid_tune.window_t) / 1000u, busy = sid_tune.busy;
    uint32_t k = cycles / 1000u;
    sid_tune.load = (uint16_t)(span && busy ? busy / span : 0u);
    sid_tune.ticks_k = (uint16_t)(k && busy ? busy / k : 0u);
    sid_tune.busy = 0;
    sid_tune.window_t = now;
}

static void tune_tell(const psid_ctl* c) {
    if (psid_ctl_state(c) != PSID_FAILED || c->error == sid_tune.told) {
        return;
    }
    sid_tune.told = c->error;
    if (c->error == PSID_RUN_JAM) {
        static const char hex[] = "0123456789ABCDEF";
        char t[] = "6502 JAM AT $0000";
        uint32_t pc = c->w->env.cpu.pc;
        for (uint32_t k = 0; k < 4u; k++) {
            t[13u + k] = hex[pc >> (12u - 4u * k) & 15u];
        }
        ui_toast(t, 2000u);
    } else {
        ui_toast(c->error == PSID_RUN_INIT_TOO_LONG ? "INIT TOO LONG" : "PLAY HUNG", 2000u);
    }
}

static void tune_prep(psid_ctl* c) {
    const psid_header* h = audio_psid_header();
    if (sid_io_busy(&sid_tune.io) && (sid_tune.turn++ & 1u) && audio_work_slice()) {
        int r = sid_io_poll(&sid_tune.io);
        if (r < 0) {
            ui_toast(sid_io_message(-r), 2000u);
            tune_end();
            return;
        }
    }
    if (sid_tune.fresh == 0u) {
        if (!keys_route_ready_for_song(&keys_router)) {
            return;
        }
        for (uint32_t k = 0; k < sid_tune.chips; k++) {
            sid_tune.was[k] = audio_chip(k);
            keys_route_fresh(&keys_router, k);
        }
        sid_tune.fresh = 1;
    }
    if (sid_io_busy(&sid_tune.io)) {
        return;
    }
    for (uint32_t k = 0; k < sid_tune.chips && sid_tune.fresh == 1u; k++) {
        if (audio_chip(k) == sid_tune.was[k] || !chip_ctl_has(&keys_router.ctl[k], &sid_tune.want[k])) {
            return;
        }
    }
    uint32_t q16 = tune_step_q16(h->clock == 2u ? PSID_CLOCK_NTSC : PSID_CLOCK_PAL);
    if (!sid_tune.clocked || sid_tune.fresh == 1u) {
        sid_tune.pal_q16 = audio_chip(0)->cycles_per_sample_q16;
    }
    for (uint32_t k = 0; k < sid_tune.chips; k++) {
        sid_tune.was[k] = audio_chip(k);
        sid_tune.was[k]->cycles_per_sample_q16 = q16;
    }
    sid_tune.clocked = 1;
    (void)psid_ctl_mute(c, tune_mutes());
    if (!psid_ctl_play(c, sid_tune.song)) {
        ui_toast("SID: NO SUCH SONG", 2000u);
    } else {
        sid_tune.song = c->w->env.song;
    }
    if (sid_tune.hold_stop) {
        sid_tune.hold_stop = 0;
        (void)psid_ctl_stop(c);
    }
    sid_tune.cycles = 0;
    sid_tune.cycles_from = 0;
    sid_tune.cycles_ms = uptime_ms;
    sid_tune.busy = 0;
    sid_tune.window_t = fm1_ticks();
    sid_tune.load = 0;
    sid_tune.ticks_k = 0;
    sid_tune.state = TUNE_ON;
}

static void tune_reload(psid_ctl* c) {
    (void)psid_ctl_poll(c);
    if (c->stop_wait || !audio_work_slice()) {
        return;
    }
    char name[STORE_NAME_MAX + 1u];
    memcpy(name, sid_tune.ask == TUNE_ASK_SWITCH ? sid_tune.next : sid_tune.io.file.name,
           sizeof name);
    if (sid_tune.ask == TUNE_ASK_SWITCH) {
        sid_tune.told = 0;
    }
    sid_tune.ask = TUNE_ASK_NONE;
    psid_header* h = audio_psid_header();
    if (!tune_open(name, h)) {
        sid_tune.state = TUNE_END;
        return;
    }
    if ((h->sid2 ? 2u : 1u) != sid_tune.chips) {
        memcpy(sid_tune.next, name, sizeof sid_tune.next);
        sid_tune.queued = 1;
        sid_tune.state = TUNE_END;
        return;
    }
    chip_setup want[2];
    tune_setup(&want[0], h->model1);
    tune_setup(&want[1], h->model2);
    sid_tune.fresh = 2u;
    if (memcmp(&want[0], &sid_tune.want[0], sizeof want[0]) != 0 ||
        (sid_tune.chips == 2u && memcmp(&want[1], &sid_tune.want[1], sizeof want[1]) != 0)) {
        sid_tune.want[0] = want[0];
        sid_tune.want[1] = want[1];
        keys_route_song_wants(&keys_router, &sid_tune.want[0]);
        if (sid_tune.chips == 2u) {
            keys_router.tune_b_want = sid_tune.want[1];
        }
        sid_tune.fresh = 0;
    }
    (void)sid_io_load(&sid_tune.io, c, h);
    sid_tune.turn = 0;
    sid_tune.state = TUNE_PREP;
}

#define LIFT_FILE "LIFTED SOUND"
enum { LIFT_IDLE, LIFT_BEGIN };
#define LIFT_GIVE_UP_MS 3000u

static struct {
    uint8_t state;
    uint8_t instr;
    uint16_t len;
    uint32_t crc;
    store_file file;
    char name[SID_OFFER_NAME + 1u];
} lift_offer;
#define LIFT_WAITING 3u

static void lift_name(char out[SID_OFFER_NAME + 1u], uint32_t voice) {
    const char* t = audio_psid_header()->name;
    uint32_t n = 0;
    for (; n < 13u && t[n]; n++) {
        char ch = t[n];
        ch = ch >= 'a' && ch <= 'z' ? (char)(ch - 32) : ch;
        out[n] = (uint8_t)ch >= 0x20u && (uint8_t)ch < 0x60u ? ch : '?';
    }
    while (n && out[n - 1u] == ' ') {
        n--;
    }
    out[n++] = ' ';
    out[n++] = 'V';
    out[n++] = (char)('1' + voice);
    out[n] = 0;
}

static uint32_t lift_free_instr(const gt2_song* s) {
    for (uint32_t i = 0; i < GT2_MAX_INSTR; i++) {
        uint8_t any = 0;
        for (uint32_t b = 0; b < GT2_INSTR_BYTES; b++) {
            any |= s->instr[i][b];
        }
        if (!any) {
            return i + 1u;
        }
    }
    return 0;
}

static int lift_pass(int ending) {
    psid_lift* l = audio_lift();
    store* st = store_fm1();
    if (sid_tune.lift == LIFT_IDLE && l->state != PSID_LIFT_TAKEN) {
        return l->state == PSID_LIFT_TAKING;
    }
    if (!audio_work_slice()) {
        return 1;
    }
    if (sid_tune.lift == LIFT_IDLE) {
        uint32_t flags;
        lift_name(lift_offer.name, l->voice);
        int n = psid_lift_convert(l, audio_psid_header()->clock == 2u, lift_offer.name, &flags);
        if (n == PSID_LIFT_AGAIN) {
            return 1;
        }
        psid_lift_arm(l, 3u, sid_tune.chips);
        if (n < 0) {
            ui_toast(n == PSID_LIFT_E_DIGI ? "DIGI: CAN'T LIFT" : "LIFT: NOTE TOO SHORT", 2000u);
            return 0;
        }
        sid_tune.lift = LIFT_BEGIN;
        sid_tune.lift_len = (uint16_t)n;
        sid_tune.lift_flags = (uint8_t)flags;
        sid_tune.lift_ms = uptime_ms;
    }
    int rc = store_file_begin(st, LIFT_FILE, STORE_FILE_GTI, sid_tune.lift_len);
    if (rc == STORE_BUSY) {
        store_file_wait(st, store_busy(st));
        if (!ending || uptime_ms - sid_tune.lift_ms < LIFT_GIVE_UP_MS) {
            return 1;
        }
    }
    store_file_wait(st, 0);
    sid_tune.lift = LIFT_IDLE;
    if (rc == STORE_OK && store_file_write(st, l->file, sid_tune.lift_len) == sid_tune.lift_len &&
        store_file_end(st) == STORE_OK) {
        lift_offer.state = LIFT_WAITING;
        lift_offer.len = sid_tune.lift_len;
        lift_offer.crc = store_crc32(0, l->file, sid_tune.lift_len);
        char t[8u + SID_OFFER_NAME + 1u] = "LIFTED: ";
        memcpy(t + 8, lift_offer.name, SID_OFFER_NAME + 1u);
        ui_toast(sid_tune.lift_flags & PSID_LIFT_F_NO_VIBRATO ? "LIFTED, VIBRATO DROPPED"
                 : sid_tune.lift_flags & PSID_LIFT_F_SHORT    ? "LIFTED, CUT TO FIT"
                                                              : t,
                 2000u);
    } else {
        if (rc == STORE_OK) {
            store_file_abort(st);
        }
        ui_toast(rc == STORE_FULL ? "STORE FULL: NOT LIFTED"
                 : rc == STORE_BUSY ? "LIFT LOST: STORE BUSY"
                                    : "LIFT: STORE ERROR",
                 2000u);
    }
    return 0;
}

static void lift_offer_pass(void) {
    if (sid_tune.state != TUNE_OFF) {
        return;
    }
    if (lift_offer.state == LIFT_WAITING) {
        store* st = store_fm1();
        if (store_file_result(st) == STORE_BUSY) {
            return;
        }
        if (store_file_open(st, LIFT_FILE, &lift_offer.file) == STORE_OK &&
            lift_offer.file.type == STORE_FILE_GTI && lift_offer.file.crc == lift_offer.crc) {
            lift_offer.instr = (uint8_t)lift_free_instr(audio_song()->song);
            lift_offer.state = SID_OFFER_READY;
        } else {
            lift_offer.state = SID_OFFER_NONE;
            ui_toast("LIFT: NOT SAVED", 2000u);
        }
    } else if (lift_offer.state == SID_OFFER_IMPORTING) {
        song_ctl* sc = audio_song();
        int r = gt2_ins_import_step(&song_stream.instrument);
        if (r == 0) {
            return;
        }
        sc->song->frozen = 0;
        lift_offer.state = SID_OFFER_NONE;
        if (r > 0) {
            song_after_import(sc);
            char t[] = "IMPORTED AS I00";
            t[13] = "0123456789ABCDEF"[lift_offer.instr >> 4];
            t[14] = "0123456789ABCDEF"[lift_offer.instr & 15u];
            ui_toast(t, 2000u);
        } else {
            gt2_err e = song_stream.instrument.error;
            ui_toast(e == GT2_ERR_FULL || e == GT2_ERR_SONG_FULL ? "SONG FULL: NOT IMPORTED"
                                                                 : "IMPORT FAILED",
                     2000u);
        }
    }
}

static void tune_pass(void) {
    psid_ctl* c = audio_psid();
    switch (sid_tune.state) {
    case TUNE_OFF:
        return;
    case TUNE_SONG_STOP: {
        song_ctl* sc = audio_song();
        if (lift_offer.state == SID_OFFER_IMPORTING) {
            return;
        }
        if (song_ctl_state(sc) == SONG_PLAYING || song_ctl_state(sc) == SONG_LOADING) {
            (void)song_ctl_stop(sc);
            return;
        }
        if (sc->auditioning) {
            song_ctl_audition_end(sc);
            return;
        }
        if (!midi_object_stopped(sc) || song_mine || midi_object.owned || jam_wait[0] ||
            !ui_tune(1)) {
            return;
        }
        if (!song_io_away(&songs, 1)) {
            (void)ui_tune(0);
            return;
        }
        song_away = 1;
        front.playing = 0;
        audio_player(AUDIO_NONE);
        sid_tune.state = TUNE_TO_NONE;
        return;
    }
    case TUNE_TO_NONE: {
        if (audio_player_now() != AUDIO_NONE || !audio_work_slice()) {
            return;
        }
        c = audio_psid_init();
        psid_lift_init(audio_lift());
        c->lift = audio_lift();
        sid_tune.lift = LIFT_IDLE;
        psid_header* h = audio_psid_header();
        char name[STORE_NAME_MAX + 1u];
        memcpy(name, sid_tune.io.file.name, sizeof name);
        if (!tune_open(name, h)) {
            sid_tune.state = TUNE_END;
            return;
        }
        tune_setup(&sid_tune.want[0], h->model1);
        tune_setup(&sid_tune.want[1], h->model2);
        keys_route_set_song(&keys_router, 1);
        keys_route_set_tune(&keys_router, 1);
        keys_route_song_wants(&keys_router, &sid_tune.want[0]);
        if (sid_tune.chips == 2u) {
            ui_sound_hush(&ui_sounds, 1);
            keys_router.tune_b_want = sid_tune.want[1];
            keys_route_set_silent(&keys_router, 1);
            if (!sid_tune.toasted) {
                ui_toast("KEYS OFF: 2SID TUNE", 2000u);
                sid_tune.toasted = 1;
            }
        }
        (void)sid_io_load(&sid_tune.io, c, h);
        (void)psid_ctl_mute(c, tune_mutes());
        sid_tune.turn = 0;
        sid_tune.fresh = 0;
        sid_tune.clocked = 0;
        audio_player(AUDIO_PSID);
        sid_tune.state = TUNE_TO_PSID;
        return;
    }
    case TUNE_TO_PSID:
        if (audio_player_now() != AUDIO_PSID) {
            return;
        }
        if (sid_tune.chips == 2u) {
            audio_chip_b_use(AUDIO_B_TUNE, 1);
        }
        sid_tune.state = TUNE_PREP;
        return;
    case TUNE_PREP:
        tune_prep(c);
        return;
    case TUNE_ON: {
        uint32_t st = psid_ctl_state(c);
        if (st == PSID_INIT || st == PSID_PLAYING || st == PSID_PAUSED) {
            for (uint32_t k = 0; k < sid_tune.chips; k++) {
                sid* s = audio_chip(k);
                if (s != sid_tune.was[k]) {
                    sid_tune.was[k] = s;
                    sid_shadow_init(&c->w->shadow[k], s, psid_env_now(&c->w->env));
                }
            }
        }
        if (sid_tune.ask != TUNE_ASK_NONE) {
            (void)psid_ctl_stop(c);
            sid_tune.state = TUNE_RELOAD;
            return;
        }
        uint32_t t0 = fm1_ticks();
        if (audio_psid_poll()) {
            sid_tune.busy += fm1_ticks() - t0;
        }
        tune_tell(c);
        st = psid_ctl_state(c);
        if (st != PSID_INIT && st != PSID_PLAYING && st != PSID_PAUSED) {
            psid_lift_stop(audio_lift());
        }
        (void)lift_pass(0);
        front.playing = st == PSID_INIT || st == PSID_PLAYING;
        if (uptime_ms - sid_tune.cycles_ms >= 1000u) {
            uint32_t now = st == PSID_INIT || st == PSID_PLAYING ? psid_env_now(&c->w->env) : 0u;
            sid_tune.cycles = sid_tune.cycles_from && now ? now - sid_tune.cycles_from : 0u;
            tune_meter(sid_tune.cycles);
            sid_tune.cycles_from = now;
            sid_tune.cycles_ms = uptime_ms;
        }
        return;
    }
    case TUNE_RELOAD:
        tune_reload(c);
        return;
    case TUNE_END:
        (void)psid_ctl_stop(c);
        (void)psid_ctl_poll(c);
        psid_lift_stop(audio_lift());
        if (c->stop_wait || lift_pass(1)) {
            return;
        }
        if (sid_tune.clocked) {
            for (uint32_t k = 0; k < sid_tune.chips; k++) {
                audio_chip(k)->cycles_per_sample_q16 = sid_tune.pal_q16;
            }
            sid_tune.clocked = 0;
        }
        audio_chip_b_use(AUDIO_B_TUNE, 0);
        front.playing = 0;
        audio_player(AUDIO_NONE);
        sid_tune.state = TUNE_BACK_NONE;
        return;
    case TUNE_BACK_NONE:
        if (audio_player_now() != AUDIO_NONE) {
            return;
        }
        (void)audio_song_remake();
        song_boot();
        audio_player(AUDIO_SONG);
        sid_tune.state = TUNE_BACK_SONG;
        return;
    case TUNE_BACK_SONG:
        if (audio_player_now() != AUDIO_SONG) {
            return;
        }
        if (sid_tune.chips == 2u) {
            keys_route_set_silent(&keys_router, 0);
            ui_sound_hush(&ui_sounds, 0);
        }
        keys_route_set_tune(&keys_router, 0);
        keys_route_set_song(&keys_router, 0);
        song_away = 0;
        (void)song_io_away(&songs, 0);
        sid_tune.state = TUNE_BACK_READ;
        return;
    case TUNE_BACK_READ:
        if (song_io_busy(&songs)) {
            return;
        }
        (void)ui_tune(0);
        sid_tune.state = TUNE_OFF;
        if (sid_tune.queued) {
            sid_tune.queued = 0;
            (void)tune_play(sid_tune.next, sid_tune.song);
        }
        return;
    default:
        return;
    }
}

static void tune_idle(void) {
    if (sid_tune.state == TUNE_ON) {
        uint32_t t0 = fm1_ticks();
        if (psid_ctl_poll(audio_psid())) {
            sid_tune.busy += fm1_ticks() - t0;
        }
    }
}

static int tb_play(void* ctx, const char* name) {
    (void)ctx;
    return tune_play(name, 0);
}

static int tb_start(void* ctx, uint32_t song) {
    (void)ctx;
    return tune_start(song);
}

static int tb_stop(void* ctx) {
    (void)ctx;
    tune_stop();
    return sid_tune.state != TUNE_OFF;
}

static int tb_pause(void* ctx) {
    (void)ctx;
    return sid_tune.state == TUNE_ON && psid_ctl_pause(audio_psid());
}

static int tb_resume(void* ctx) {
    (void)ctx;
    return sid_tune.state == TUNE_ON && psid_ctl_resume(audio_psid());
}

static int tb_end(void* ctx) {
    (void)ctx;
    tune_end();
    return 1;
}

static int tb_mute(void* ctx, uint32_t mask) {
    (void)ctx;
    sid_tune.mute = (uint8_t)(mask & 7u);
    if (sid_tune.state >= TUNE_TO_PSID && sid_tune.state <= TUNE_END) {
        (void)psid_ctl_mute(audio_psid(), tune_mutes());
    }
    return 1;
}

static void tb_status(void* ctx, sid_status* out) {
    (void)ctx;
    uint32_t t = sid_tune.state;
    if (t == TUNE_OFF &&
        (lift_offer.state == SID_OFFER_READY || lift_offer.state == SID_OFFER_IMPORTING)) {
        out->offer = lift_offer.state;
        out->offer_instr = lift_offer.instr;
        out->offer_playing = (uint8_t)!midi_object_stopped(audio_song());
        memcpy(out->offer_name, lift_offer.name, sizeof out->offer_name);
    }
    if (t == TUNE_OFF) {
        return;
    }
    if (t >= TUNE_TO_PSID && t <= TUNE_END) {
        const psid_lift* l = audio_lift();
        out->lift = (uint8_t)(l->state == PSID_LIFT_ARMED ? SID_LIFT_ARMED
                              : l->state || sid_tune.lift ? SID_LIFT_TAKING
                                                          : SID_LIFT_OFF);
        out->lift_voice = l->voice;
    }
    const psid_ctl* c = audio_psid();
    int has = t >= TUNE_TO_PSID && t <= TUNE_END;
    out->state = (uint8_t)(t >= TUNE_END ? SID_T_ENDING : SID_T_LOADING);
    if (t == TUNE_ON && sid_tune.ask == TUNE_ASK_NONE) {
        static const uint8_t STATE[] = {SID_T_LOADING, SID_T_LOADING, SID_T_PLAYING, SID_T_PLAYING,
                                        SID_T_PAUSED,  SID_T_STOPPED, SID_T_FAILED};
        uint32_t p = psid_ctl_state(c);
        out->state = STATE[p < sizeof STATE ? p : 0u];
        out->seconds = c->clock[0] / c->w->env.clock;
        out->overruns = c->w->env.overruns;
        out->late = c->late;
        out->dropped = c->w->env.sid3_dropped;
        out->cycles = sid_tune.cycles;
        out->load = sid_tune.load;
        out->ticks_k = sid_tune.ticks_k;
    }
    out->chips = sid_tune.chips;
    out->shadow = 1u;
    out->song = sid_tune.song;
    for (uint32_t k = 0; k < 2u; k++) {
        const sid_chip_profile* pr = sid_chip_profile_get(sid_tune.want[k].profile);
        out->model[k] = (uint8_t)(pr ? pr->model : SID_MODEL_6581);
    }
    const sid* a = audio_chip(0);
    for (uint32_t v = 0; v < 3u; v++) {
        out->env[v] = t == TUNE_ON ? a->env[v].counter : 0u;
    }
    memcpy(out->name, sid_tune.ask == TUNE_ASK_SWITCH ? sid_tune.next : sid_tune.io.file.name,
           sizeof out->name);
    out->h = has && sid_tune.ask != TUNE_ASK_SWITCH ? audio_psid_header() : NULL;
}

static int tb_lift(void* ctx, uint32_t voice) {
    (void)ctx;
    if (sid_tune.state != TUNE_ON || audio_lift()->state == PSID_LIFT_TAKEN ||
        sid_tune.lift != LIFT_IDLE) {
        return 0;
    }
    psid_lift_arm(audio_lift(), voice, sid_tune.chips);
    return 1;
}

static int tb_import(void* ctx, int go) {
    (void)ctx;
    if (lift_offer.state != SID_OFFER_READY || sid_tune.state != TUNE_OFF) {
        return 0;
    }
    if (!go) {
        lift_offer.state = SID_OFFER_NONE;
        return 1;
    }
    song_ctl* sc = audio_song();
    gt2_song* s = sc->song;
    lift_offer.instr = (uint8_t)lift_free_instr(s);
    if (!lift_offer.instr) {
        ui_toast("NO FREE INSTRUMENT", 2000u);
        return 0;
    }
    if (!midi_object_stopped(sc) || song_io_busy(&songs) || songs.about || songs.external ||
        s->frozen || s->pending || s->pending_loose || midi_object.owned) {
        ui_toast("SONG BUSY: TRY AGAIN", 2000u);
        return 0;
    }
    s->frozen = 1;
    if (gt2_ins_import_begin(&song_stream.instrument, s, lift_offer.instr, midi_file_read,
                             &lift_offer.file, lift_offer.len, 1) != GT2_OK) {
        s->frozen = 0;
        lift_offer.state = SID_OFFER_NONE;
        ui_toast("IMPORT FAILED", 2000u);
        return 0;
    }
    lift_offer.state = SID_OFFER_IMPORTING;
    return 1;
}

static const sid_transport tune_binding = {tb_play,   tb_start,  tb_stop,   tb_pause,
                                           tb_resume, tb_end,    tb_mute,   tb_status,
                                           tb_lift,   tb_import, NULL};

static int tune_midi(void* ctx, uint32_t b) {
    (void)ctx;
    if (sid_tune.state == TUNE_OFF) {
        return 0;
    }
    psid_ctl* c = audio_psid();
    uint32_t st = sid_tune.state == TUNE_ON ? psid_ctl_state(c) : PSID_EMPTY;
    if (b == 0xfau) {
        (void)tune_start(sid_tune.song);
    } else if (b == 0xfcu) {
        if (st == PSID_INIT || st == PSID_PLAYING) {
            (void)psid_ctl_pause(c);
        } else if (st != PSID_PAUSED) {
            tune_stop();
        }
    } else if (b == 0xfbu) {
        if (st == PSID_PAUSED) {
            (void)psid_ctl_resume(c);
        } else if (st == PSID_STOPPED || st == PSID_FAILED) {
            (void)tune_start(sid_tune.song);
        }
    }
    return 1;
}

static void commit_pass(void) {
    static uint16_t master_sent = 0xffffu;
    static uint8_t spread_sent = 0u;
    if (instrument.master != master_sent) {
        master_sent = instrument.master;
        audio_set_master(master_sent);
    }
    if (instrument.spread != spread_sent) {
        spread_sent = instrument.spread;
        audio_set_spread(spread_sent);
    }
    audio_commit();
    uint8_t taken = keys_route_voices_taken(&keys_router);
    if (taken != song_taken && !song_away) {
        song_taken = taken;
        (void)song_ctl_take(audio_song(), taken);
    }
}

static int play(uint32_t now, uint32_t* next_tick, uint32_t* last_frame) {
    uint32_t frame = input_frame();
    int tick_due = (int32_t)(now - *next_tick) >= 0;
    if (frame == *last_frame && !tick_due) {
        return 0;
    }
    *last_frame = frame;
    input_poll(&front, &instrument);
    for (uint32_t n = 0; n < TICK_CATCH_UP && (int32_t)(now - *next_tick) >= 0; n++) {
        *next_tick += TICK_MS;
        panel_master(&front, &instrument, input_master());
        live_tick(&instrument);
        ui_sound_tick(&ui_sounds);
        midi_tick();
        static uint8_t player_seen;
        if (player_seen != keys_router.player) {
            player_seen = keys_router.player;
            song_mode_setup();
        }
        if (route_state->song_req != MIDI_ROUTE_NO_REQUEST) {
            int32_t program = midi_route_take_song(route_state);
            if (sid_tune.state == TUNE_OFF) {
                (void)song_io_program(&songs, (uint32_t)program);
            }
        }
    }
    tune_pass();
    lift_offer_pass();
    if (!song_away) {
        song_pass();
    }
    if (tick_due) {
        keys_route_tick(&keys_router);
        audio_chip_b_use(AUDIO_B_UI, ui_sound_busy(&ui_sounds));
        midi_send();
    }
    if ((int32_t)(now - *next_tick) >= 0) {
        *next_tick = now + TICK_MS;
    }
    if (tick_due || store_fm1_active || songs.state) {
        store_fm1_service(now, &instrument, &keys_router, ui_sound_busy(&ui_sounds));
    }
    if (sync_state->clock_out) {
        midi_sync_pass();
    }
    commit_pass();
    input_leds(&front, &instrument, now);
    return 1;
}

#if defined(SIDOU_TEST_DEATH_MS)
static uint8_t test_death_armed;

static void test_death_arm(void) {
    fm1_delay_ms(30);
    test_death_armed = (input_buttons() >> PANEL_BTN_OCT_DOWN) & 1u;
}

static void test_death(uint32_t now) {
    if (!test_death_armed || now < SIDOU_TEST_DEATH_MS) {
        return;
    }
#if SIDOU_TEST_DEATH_REBOOT
    fm1_reboot();
#else
    __asm__ volatile("cli" ::: "memory");
    for (;;) {
    }
#endif
}
#endif

static void leave_on_purpose(void) {
    bootguard_leave(&boot_guard);
}

static void upgrade(void) {
    fm1_audio_stop();
    audio_halted();
    update_session(leave_on_purpose);
    leave_on_purpose();
    fm1_reboot();
}

static void wait_for_interrupt(void) {
    if (midi_usb_live()) {
        uint32_t t = timer_irqs;
        while (timer_irqs == t) {
        }
        return;
    }
    __asm__ volatile("idle");
}

#if defined(SIDOU_FLASH_SPIKE)
static void spike_service(void) {
    uint32_t now = uptime_ms;
    fm1_wdt_feed();
    midi_service(now);
    bootguard_poll(&boot_guard, now);
    if (update_poll()) {
        upgrade();
    }
}
#endif

static void boot_feed(void) {
    fm1_wdt_feed();
}

static void while_lcd_resets(void) {
    sid_noise_tables_init();
    song_load();
}

static void while_lcd_wakes(void) {
    flash_plain_window();
    store_fm1_boot(boot_feed);
    midi_objects_bind();
}

static void sidou_main(void) {
    lcd_init(while_lcd_resets, while_lcd_wakes);
    audio_init();
    keys_route_init(&keys_router, &instrument, reg_ring_sink, reg_ring_prepare_step,
                    audio_sink_ctx(), audio_sink_ctx_b());
    keys_router.moved = audio_keys;
    keys_router.taking = song_keys_take;
    live_init(&instrument, keys_route_sink, &keys_router, SID_MODEL_6581);
    live_set_preparer(&instrument, keys_route_prepare, &keys_router);
    keys_route_start(&keys_router);
    ui_sound_init(&ui_sounds, &instrument, reg_ring_sink, audio_sink_ctx_b());
    song_boot();
    panel_init(&front);
    input_init();
    panel_master(&front, &instrument, input_master());
    audio_set_master(instrument.master);
    midi_init(&instrument, &keys_router);
    route_state = midi_routed();
    midi_sync_bind(midi_synced(), &song_bound);
    sync_state = midi_synced();
    update_boot();
#if defined(SIDOU_FLASH_SPIKE)
    fm1_timer5_start(isr_timer5, PRIO_TIMER5);
    fm1_irq_enable_all();
    ui_init(&instrument, &ui_sounds, uptime_ms, 0);
    flash_spike_run(spike_service);
#endif
    audio_start();
    fm1_audio_init(abuf, AUDIO_HALF_WORDS, isr_alnk0, PRIO_ALNK0);
    fm1_timer5_start(isr_timer5, PRIO_TIMER5);
    fm1_irq_enable_all();
    audio_irqs_on();
    uint32_t held = input_buttons_at_power_on();
    ui_init(&instrument, &ui_sounds, uptime_ms, (int)((held >> PANEL_BTN_HOME) & 1u));
    ui_set_keys(&keys_router);
    ui_set_panel(&front);
    song_bound = song_binding;
    song_bound.ctx = audio_song();
    song_bound.song = audio_song()->song;
    ui_set_song(&song_bound);
    ui_set_tune(&tune_binding);
    midi_sync_tune(midi_synced(), tune_midi, NULL);
    song_io_hooks io_hooks = {song_capture, song_loaded, read_demo, 0, song_generation,
                              ui_song_force, song_snapshot, song_restore};
    (void)song_capture(0);
    song_io_init(&songs, audio_song(), store_fm1(), ui_settings(), &song_stream.stream, &io_hooks);
    ui_set_song_io(&songs);
    if (ui_settings()->last_song[0]) {
        (void)song_io_load(&songs, ui_settings()->last_song, 1);
    }
#if defined(SIDOU_TEST_DEATH_MS)
    test_death_arm();
#endif
    uint32_t next_tick = uptime_ms + TICK_MS;
    uint32_t last_frame = input_frame();
    for (;;) {
        fm1_wdt_feed();
        uint32_t now = uptime_ms;
        if (midi_drain()) {
            commit_pass();
        }
        if (!play(now, &next_tick, &last_frame)) {
            tune_idle();
        }
        ui_poll(now);
        if (midi_link_active) {
            ui_link(now);
        }
        midi_service(now);
        bootguard_poll(&boot_guard, now);
#if defined(SIDOU_TEST_DEATH_MS)
        test_death(now);
#endif
        if (update_poll()) {
            upgrade();
        }
        wait_for_interrupt();
    }
}

#if !defined(SIDOU_TEST_DEATH_MS)
#define UBOOT_SAMPLES 21u
#define UBOOT_GAP_MS 50u
#define UBOOT_KEY (1u << PANEL_BTN_OCT_DOWN)

static void uboot_gesture(void) {
    for (uint32_t i = 0;; i++) {
        if (input_raw_keys() != UBOOT_KEY) {
            return;
        }
        if (i + 1u == UBOOT_SAMPLES) {
            break;
        }
        fm1_wdt_feed();
        fm1_delay_ms(UBOOT_GAP_MS);
    }
    input_raw_dark();
    fm1_wdt_stop();
    fm1_enter_uboot();
}
#endif

void fm1_cstart(void) {
    uint32_t *d, *s;
    fm1_cpu1_halt();
    fm1_time_init();
    fm1_wdt_arm(WDT_TIMEOUT);
#if !defined(SIDOU_TEST_DEATH_MS)
    uboot_gesture();
#endif
    if (bootguard_boot(&boot_guard)) {
        fm1_wdt_stop();
        fm1_enter_uboot();
    }
    fm1_irq_init();
    for (d = _bss_start; d < _bss_end; d++) {
        *d = 0;
    }
    for (d = _pool_start; d < _pool_end; d++) {
        *d = 0;
    }
    for (s = _data_load, d = _data_start; d < _data_end; s++, d++) {
        *d = *s;
    }
    for (s = _rt_load, d = _rt_start; d < _rt_end; s++, d++) {
        *d = *s;
    }
    for (ctor_fn* c = __init_array_start; c < __init_array_end; c++) {
        (*c)();
    }
    fm1_mailbox_clear();
    sidou_main();
}
