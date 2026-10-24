// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU boot and main loop. Init order after fm1-sid's firmware/main.c (fm1-emu) and
 * Felucca's src/main.c: watchdog first, fatal vectors, memory, then the LCD, the live
 * instrument, the key matrix and MIDI in (TRS UART, USB), audio (ALNK0 IRQ) and the 10 kHz
 * tick (TIMER5 IRQ, which also polls MIDI), interrupts on, the screen's first frame (sent by
 * the main loop), and a main loop that plays MIDI and the panel into the instrument, redraws
 * the screen, answers the update protocol and feeds the watchdog. The boot-loop guard comes first of all (fm1_cstart).
 *
 * This file includes the HAL's interrupt, audio and timer headers; input.c and lcd.c include
 * the input and LCD ones (platform/fm1/VENDORED.md). */
#include <stdint.h>
#include <string.h>

/* The HAL's second-core hold (fm1_cpu1_halt), which Felucca's fm1_cstart runs first at every
 * boot and before every reset. SIDOU never starts the second core, but one left running (by the
 * ROM, another firmware, a reset that did not hold it) would run whatever its PC points at while
 * SIDOU clears and fills RAM. The HAL builds the hold only with FM1_CPU1_USED; fm1_irq.h's
 * second-core fault paths, which need Felucca's fm1_cpu1.S, stay out (FM1_CPU1_USED 0 for it). */
#define FM1_CPU1_USED 1
#include "fm1_sys.h" /* before fm1_irq.h: FM1_RESET_PREP, here the hold */
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

/* The IRQ entry wrappers (hal/fm1_isr.S) and the C bodies they call. */
void isr_alnk0(void);
void isr_timer5(void);
void fm1_alnk0_irq(void);
void fm1_timer5_irq(void);
void fm1_cstart(void);

/* Priorities: TIMER5 above ALNK0, so the tick keeps its pace while a half buffer renders. */
#define PRIO_ALNK0 3u
#define PRIO_TIMER5 4u
/* fm1_wdt_arm: 0x0A is 1 s, each step doubles it. */
#define WDT_TIMEOUT 0x0Du
#define TICK_MS (1000u / LIVE_TICK_HZ)
/* live_tick catches up at most this many ticks in one pass (reg_ring.h's worst pass). A pass
 * runs at most one slice of the spare chip's preparation however many ticks it catches up
 * (reg_ring_pass_end, from audio_commit at the pass's end): two would be 2 x 40k on top of a
 * view's 68k, past the 120k poll allowance. */
#define TICK_CATCH_UP 2u

static int32_t abuf[2u * AUDIO_HALF_WORDS] __attribute__((aligned(4)));
/* Survives resets (.noinit): boots that die early in a row end in UBOOT (app/bootguard.h). */
static bootguard boot_guard __attribute__((section(".noinit")));
static volatile uint32_t uptime_ms;
static live instrument;
/* Which chip the keys play, and the two chips' setups (app/keys_route.h). */
static keys_route keys_router;
/* The UI sounds on chip B's spare voice, off by default (app/ui_sound.h). */
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

/* The audio interrupt renders: a TIMER5 tick nested in it leaves USB's poll for later
 * (fm1_timer5_irq). */
static volatile uint8_t in_audio;
/* TIMER5 interrupts so far: the main loop's wait for the next one while a host is there. */
static volatile uint32_t timer_irqs;

void fm1_alnk0_irq(void) {
    in_audio = 1;
    uint8_t p = fm1_audio_pending();
    fm1_audio_ack_aux(p);
    if (p & FM1_AUDIO_HALF) {
        audio_fill(&abuf[fm1_audio_free_half() * AUDIO_HALF_WORDS]);
        fm1_audio_ack_half();
    }
    audio_block_end = fm1_ticks(); /* a write window may start now (store_fm1.c) */
    in_audio = 0;
}

/* Every 5th tick (2 kHz) each, on different ticks, as Felucca does: USB on tick 0, the TRS
 * UART on tick 2. The UART's poll runs nested in the audio ISR too (it touches nothing the
 * render does). USB's never does, as in Felucca's fm1_timer5_irq: a poll due while a block
 * renders is owed, and runs at the first tick after the render (within a render's length,
 * about 0.2 ms idle and at most a block's 2.9 ms). */
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
        usb_audio_isr(); /* USB audio's packet, one a USB frame (usb_audio.h) */
    }
    /* USB audio out (usb_audio.h): a block rendered since the last tick is in the free half
     * buffer until the DMA moves on, which raises the half's pending flag. */
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

/* fm1_fault_c (fm1_irq.h) has already recorded the crash in fm1_crash (.noinit), where the
 * next boot can find it. Stop the DMA so the buffer does not loop, then reset. */
static void fm1_fault(const fm1_crash_t* c) {
    (void)c;
    fm1_audio_stop();
    fm1_reboot();
}

/* --- The song (M2c Task 6): SIDOU's first song, played by the song views' transport
 * (app/song_view.h, bound below). --- */

/* 1 while a .sid tune has the song's memory (M3 Task 9, "A .sid tune" below): song control's
 * state, the song's model and its work space are the tune's, so nothing here reads them (the
 * transport's members answer as a stopped song's, the song views have none, song_io refuses
 * everything, the MIDI objects are busy) until the song is made again and read back. */
static uint8_t song_away;

/* The first song (app/demo_song.h) from flash into the song's region: the audio interrupt never
 * reads flash. */
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

/* The song's chip A (song_chip_hooks, app/keys_route.h's contract). Called a poll at a time
 * from PLAY on: once the keys have left chip A (or KEYS is A), the song's setup (its CHPA, or
 * the keys' with AUTHENTIC envelopes, decision record 0005) is what chip A should have
 * (keys_route_song_wants, the one writer of song_want: keys_route's tick brings chip A
 * there, preparing it in the background, as it does the other chip for the keys); the song
 * starts once chip A has it. M2c Task 8's save writes song_want back into the CHPA. */
static uint8_t song_setup_on; /* chip A's setup for the song is asked for */
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
            song_want = kr->ctl[0].has; /* chip A as the song finds it (the keys' setup) */
            if (c) {
                setup_decode(&song_want, c);
            } else {
                song_want.gate_reset = 0u; /* AUTHENTIC */
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

/* Once a stop has reached the interrupt: chip A back to the keys (keys_route takes it back to
 * their setup, and the keys back to it). */
static uint8_t song_mine; /* keys_route_set_song(1) is the transport's */

static void song_chip_back(void* ctx) {
    keys_route_set_song(ctx, 0);
    song_mine = 0;
}

/* In the LCD's reset wait, before anything else: the song's region and the first song in it. */
static void song_load(void) {
    gt2_song* s = audio_song_boot();
    gt2_caps caps = {SONG_MULTISPEED_MAX};
    gt2_read_info info;
    /* The song is SIDOU's own: it always reads (host tests check it does, with these caps). */
    gt2_sng_read(s, read_demo, 0, &caps, &info);
}

/* A row's character locks (M2c Task 7): chip A's controls move to them with chip_ctl's glide,
 * as a MIDI CC moves them (decision record 0006: 0.32 s end to end, so coarse by nature),
 * and back to the song's own setup after (what chip A wanted when the first lock came). */
static chip_setup song_char_base;
static uint8_t song_base_valid;
static int16_t song_char_on[4] = {-1, -1, -1, -1};
static uint32_t song_warned; /* warnings shown since PLAY */

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

static const midi_sync* sync_state; /* midi_synced(), for play()'s look at CLOCK OUT */
static int song_chip_edit(void* ctx, const chip_setup* setup, uint32_t fields) {
    keys_route* kr = ctx;
    if (song_away) {
        /* A tune's chip A: the edit straight to what it follows (no song base, no locks). */
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
    /* Ownership changes can temporarily set song_want to the keys' setup. Only explicit
     * song edits and the song's own start change its authoritative, unlocked base. */
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
    /* Shrink before growing; check every intermediate trailer padding transition before
     * changing a byte, so the sequence cannot fail after a partial capture. */
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
    if (songs.restoring) {
        /* Runtime was untouched or restored already; reapplying KEYS releases held notes. */
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
        base.gate_reset = 0; /* plain songs default to AUTHENTIC */
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
        return SYSEX_BUSY; /* keep the current import's rollback file immutable */
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
            sc->state = SONG_IDLE;
            sc->shapes_used = 0;
            sc->player.shape = NULL;
            sc->bg_on = sc->bg_dirty = sc->widened = sc->ff_on = sc->deriving = 0;
            sc->want_handover = sc->spare_wait = sc->hold_next = 0;
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
    case SYSEX_SAVED: text = "SAVED"; break;
    case SYSEX_BUSY: text = "STORE BUSY: RESEND"; break;
    case SYSEX_FULL: text = "STORE FULL"; break;
    case SYSEX_NOT_FOUND: text = "MIDI: NOT FOUND"; break;
    case SYSEX_CHECKSUM: text = "MIDI: CHECKSUM"; break;
    case SYSEX_TIMEOUT: text = "MIDI: TIMEOUT"; break;
    case SYSEX_ABORTED: text = "MIDI: ABORTED"; break;
    case SYSEX_IO: text = "MIDI: FILE ERROR"; break;
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

static int16_t song_error;    /* the refusal last shown */

/* A refusal or a warning as a toast (the SONG view's hint until it exists, M2c Task 8). */
static void song_tell(song_ctl* sc) {
    if (sc->error != song_error) {
        song_error = sc->error;
        if (song_error == SONG_ERR_MULTISPEED) {
            /* as the reader says it (gt2_sng.c) */
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

/* fm1-emu's tests only (emu-gate tests/song.rs, "edits while playing"): edit requests a test
 * writes into RAM, which the main loop makes with the song model's primitives and sends as
 * M2d's editor will. op: 0 none, 1 a row write (a: pattern, b: row, cell), 2 a row inserted
 * and 3 a row deleted (a rebuild: a swap); the main loop clears op once the edit is sent and
 * records the song's block count then (the edit applies at the next block). Nothing on the
 * device writes it. */
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
            return; /* in order: this one again next pass */
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

/* RAM-only requests for emulator integration tests; no device input writes this hook. */
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

/* --- The song views' player (app/song_transport.h) over song_ctl --- */

/* An audition (jams with the song stopped, song_ctl_audition) ends this long after its last
 * jam is let go, and chip A goes back to the keys. */
#define AUDITION_HOLD_MS 2000u

static song_status song_seen; /* the row events' positions, kept between frames */
static uint8_t jam_wait[3];   /* a jam note waiting for the audition: on, voice, note */
static uint8_t jam_instr;
static uint8_t jam_wait_off;  /* and its release */
static uint8_t jam_held;      /* a jam note sounds and is not let go */
static uint32_t jam_last_ms;  /* the last jam's time */

static int st_play_from(void* ctx, uint32_t sub, uint32_t pos) {
    song_ctl* sc = ctx;
    if (song_away) {
        return 1; /* a tune has the song's memory: nothing to play (Task 11 routes PLAY) */
    }
    if (midi_object.owned && midi_object.target == 1u) {
        return 1;
    }
    if (song_io_busy(&songs) && !song_io_saving(&songs)) {
        return 1; /* transport requests during a load do not play a half-read model */
    }
    if (!song_ctl_play(sc, sub, pos)) {
        return 0;
    }
    sc->chance_seed = uptime_ms; /* each PLAY rolls its chances afresh */
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
    return song_ctl_stop(ctx); /* 0 while it starts: asked again until it plays */
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

/* While the song plays, straight to the player; stopped, through the audition, which may
 * take a few ticks to start (chip A taken for the song): the note waits for it. */
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

/* The song views' listener of the event ring (song_events below): each voice's place. */
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

static void st_status(void* ctx, song_status* out) {
    song_ctl* sc = ctx;
    if (song_away) {
        memset(out, 0, sizeof *out); /* a song that has not played */
        for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
            out->order_pos[v] = out->row[v] = SONG_T_NOWHERE;
        }
        return;
    }
    *out = song_seen; /* the rows song_events has heard, up to this pass */
    out->applied = sc->applied + sc->idle_edits;
    out->state = (uint8_t)song_ctl_shown_state(sc);
    out->subtune = sc->play_sub;
    out->again = (uint8_t)song_ctl_again(sc);
    /* REC's clock: the last call, and the samples since it over the call period (read as
     * st_clock does: the interrupt moves them) */
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
    }
    /* the play's options; before one, the song's (or the defaults), read again each second */
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
    /* the player's tempo: calls a row, its packed form less one (funktempo below 2) */
    const gt2_player* p = &sc->player;
    uint8_t t = p->shape ? p->chn[0].tempo : 0u;
    if (!p->shape) {
        out->speed[0] = out->speed[1] = 6u; /* GoatTracker's default until a play */
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

/* M2e's MIDI clock (app/midi_sync.h): the held start, the align, the period, the seeks and
 * the calls' samples, through the same transport as the song views'. */
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
        memset(out, 0, sizeof *out); /* stopped, no tempo: MIDI's clock out sends nothing */
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
            out->speed[0] = out->speed[1] = 0u; /* no tempo before a play */
        } else if (t < 2u) {
            out->speed[0] = p->funktempo[0];
            out->speed[1] = p->funktempo[1];
        } else {
            out->speed[0] = out->speed[1] = (uint8_t)(t + 1u);
        }
        out->calls = sc->calls;
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

/* The primitives make the authoritative prospective size reservation. Only an edit
 * prepared before a save freeze may still drain through the transport. */
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
};
static song_transport song_bound;

/* Once a pass: the audition's jam that waited and its end, the transport's poll (a slice of
 * its analysis), and chip A with the song or the keys. */

/* The voices the keys take from the song under KEYS A (song_ctl_take, read by the interrupt at
 * each block's start): taken at once, before the voice's first register goes to the ring
 * (keys_route.taking), so the song never writes over a key's first note (the M2 review's
 * 2.1); given back in commit_pass, after the ring is committed with the key's gate-off, so
 * the song's registers come back after the keys' last write. */
static uint8_t song_taken;

static void song_keys_take(void* ctx, uint32_t voices) {
    (void)ctx;
    if (song_away) {
        return; /* a tune's voices are never the keys' (KEYS A acts as AUTO) */
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

/* The event ring's one reader (core/player/song_ctl.h song_ctl_events), once a pass: every
 * event goes, in order, to each of the song's listeners, the song views' places
 * (song_seen_rows) and SCENE's music hooks (ui_song), so no reader takes one from another. A
 * pass drains at most a ring of them. */
#define SONG_EVENTS_AT_ONCE 16u
static uint8_t song_heard_state; /* the transport's state the listeners last heard */

/* Called only when there are events or the state changed (song_pass): most passes have
 * neither, and the test costs them a few instructions. */
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
        keys_route_set_song(&keys_router, 1); /* the keys leave chip A */
        song_mine = 1;
    }
    song_test_edits(sc);
    if (song_io_test_hook.op) {
        song_io_test();
    }
    uint32_t slices = sc->slices;
    audio_song_poll();
    if (sc->ev_head != sc->ev_tail || sc->state != song_heard_state) {
        song_events(sc); /* song_ctl_state's, read in place */
    }
    if (songs.state || songs.about_play || songs.message[0]) {
        song_file_pass(sc, slices);
    }
    st = song_ctl_state(sc);
    engaged = st == SONG_LOADING || st == SONG_PLAYING || sc->auditioning || sc->handback;
    if (song_mine && !engaged) {
        keys_route_set_song(&keys_router, 0); /* stopped, or a refused play: the keys come back */
        song_mine = 0;
    }
    song_tell(sc);
    st = song_ctl_shown_state(sc);
    front.playing = st == SONG_PLAYING || st == SONG_LOADING;
}


/* --- A .sid tune (M3 Task 9; docs/plans/2026-10-19-m3-sid-files.md, "Memory" and "The keys,
 * chip B and the rest of the firmware while a tune plays"): the tune over the song's memory,
 * its 6502 a frame ahead in the main loop (psid_ctl, a slice a pass at most), its writes placed
 * at their cycles by the audio interrupt (audio.c). It starts once the song has stopped and gone
 * idle and the interrupt has left it (AUDIO_NONE), on chips made fresh for it with its setup and
 * clock, the keys on chip B (a 2SID tune: silent); it ends for good once its stop has played out,
 * the interrupt has left it, the song's memory is the song's again and the song is read back
 * from the store (song_io_away). Until Task 11's browser and NOW PLAYING, only the emulator
 * tests' hook below starts one. --- */

enum {
    TUNE_OFF,        /* the song has its memory */
    TUNE_SONG_STOP,  /* asked for: the song stops and goes idle, its views and files let go */
    TUNE_TO_NONE,    /* the interrupt leaves the song */
    TUNE_TO_PSID,    /* the tune's state made; the interrupt takes the tune */
    TUNE_PREP,       /* the data from the store, a part a pass; the keys leave the tune's chips,
                      * which are made fresh with its setup */
    TUNE_ON,         /* psid_ctl has it: playing, paused, stopped or failed */
    TUNE_END,        /* ending for good: its stop plays out */
    TUNE_BACK_NONE,  /* the interrupt leaves the tune */
    TUNE_BACK_SONG,  /* the song's state made again; the interrupt takes the song */
    TUNE_BACK_READ   /* the song read back from the store; then its views have it again */
};
/* The header's first bytes, which psid_parse reads (to $7D at most: a v2+ header and a data
 * load address); the file's length bounds the rest. */
#define TUNE_HEAD_BYTES 0x80u
/* The data a pass brings in from the store (in reads of TUNE_PART bytes). */
#define TUNE_PART 256u
#define TUNE_PASS_BYTES 2048u

static struct {
    uint8_t state;
    uint8_t chips;   /* the tune's: 1, or 2 for a 2SID tune */
    uint8_t fresh;   /* TUNE_PREP: the chips' resets asked for */
    uint8_t toasted; /* KEYS OFF: 2SID TUNE shown since power-on */
    uint8_t clocked; /* the chips have the tune's sample step (theirs again at the end) */
    uint16_t song;   /* the song asked for, 0 the header's start song */
    int16_t told;    /* the failure last shown */
    store_file file;
    uint32_t offset; /* TUNE_PREP: the file's bytes brought in */
    uint32_t pal_q16; /* the chips' sample step at the PAL clock */
    /* each chip's sid before its fresh reset; from the start on, the one its shadow follows */
    sid* was[2];
    chip_setup want[2]; /* the tune's chip A and chip B setups */
} sid_tune;

/* A chip's sample step at `clock` (sid.c sid_init_begin's, at the device's rate). */
static uint32_t tune_step_q16(uint32_t clock) {
    return ((clock / AUDIO_SAMPLE_RATE) << 16) |
           (((clock % AUDIO_SAMPLE_RATE) << 16) / AUDIO_SAMPLE_RATE);
}

/* A chip's setup for the header's 2-bit model code (SID MODEL at AUTO, Task 11's setting): a
 * named model plays reSIDfp's default profile of it at character 0, "unknown" and "both" the
 * keys' setup; a tune's envelopes are always the chip's own (AUTHENTIC). */
static void tune_setup(chip_setup* out, uint32_t model) {
    chip_setup_of(&instrument, out);
    if (model == 1u || model == 2u) {
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

/* The header of the store's file into h: 1, or 0 with a toast. */
static int tune_header(psid_header* h) {
    uint8_t head[TUNE_HEAD_BYTES];
    uint32_t n = sid_tune.file.length < TUNE_HEAD_BYTES ? sid_tune.file.length : TUNE_HEAD_BYTES;
    if (store_file_read(store_fm1(), &sid_tune.file, 0, head, n) != (int)n) {
        ui_toast("SID: READ ERROR", 2000u);
        return 0;
    }
    int r = psid_parse(head, sid_tune.file.length, h);
    if (r != PSID_OK) {
        ui_toast(r == PSID_E_RSID ? "RSID: NOT YET" : r == PSID_E_MUS ? "MUS: NOT SUPPORTED" :
                 r == PSID_E_TOO_LONG ? "TOO LONG" : "BROKEN .SID", 2000u);
        return 0;
    }
    return 1;
}

/* Asks for the store's file `name` to play, song `song` (0 the header's start song): 1 if it
 * goes (the song stops first), 0 with a toast if it cannot. */
static int tune_play(const char* name, uint32_t song) {
    psid_header h;
    if (sid_tune.state != TUNE_OFF) {
        return 0;
    }
    if (store_file_open(store_fm1(), name, &sid_tune.file) != STORE_OK ||
        sid_tune.file.type != STORE_FILE_SID) {
        ui_toast("SID NOT FOUND", 2000u);
        return 0;
    }
    if (!tune_header(&h)) {
        return 0;
    }
    if (keys_router.player) {
        ui_toast("MIDI MODE HAS CHIP A", 2000u); /* MULTITIMBRAL, WHOLE-CHIP: chip A is theirs */
        return 0;
    }
    sid_tune.song = (uint16_t)song;
    sid_tune.chips = (uint8_t)(h.sid2 ? 2u : 1u);
    sid_tune.told = 0;
    sid_tune.state = TUNE_SONG_STOP;
    return 1;
}

static void tune_stop(void) {
    if (sid_tune.state == TUNE_ON) {
        (void)psid_ctl_stop(audio_psid());
    }
}

/* Ends the tune for good: the song back. Before the song let go of its memory, nothing more. */
static void tune_end(void) {
    if (sid_tune.state == TUNE_SONG_STOP) {
        sid_tune.state = TUNE_OFF;
    } else if (sid_tune.state != TUNE_OFF && sid_tune.state < TUNE_END) {
        sid_tune.state = TUNE_END;
    }
}

/* A failure psid_ctl stopped the tune with, as a toast, once. */
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

/* TUNE_PREP's pass: a part of the data, the chips' fresh resets once the keys have left them,
 * and the start once both are done. */
static void tune_prep(psid_ctl* c) {
    const psid_header* h = audio_psid_header();
    uint32_t end = h->data_off + h->data_len;
    for (uint32_t k = 0; k < TUNE_PASS_BYTES && sid_tune.offset < end; k += TUNE_PART) {
        uint8_t part[TUNE_PART];
        uint32_t n = end - sid_tune.offset < TUNE_PART ? end - sid_tune.offset : TUNE_PART;
        if (store_file_read(store_fm1(), &sid_tune.file, sid_tune.offset, part, n) != (int)n) {
            ui_toast("SID: READ ERROR", 2000u);
            tune_end();
            return;
        }
        psid_ctl_load_part(c, sid_tune.offset, part, n);
        sid_tune.offset += n;
    }
    if (!sid_tune.fresh) {
        if (!keys_route_ready_for_song(&keys_router)) {
            return; /* the keys still play chip A */
        }
        for (uint32_t k = 0; k < sid_tune.chips; k++) {
            sid_tune.was[k] = audio_chip(k);
            keys_route_fresh(&keys_router, k);
        }
        sid_tune.fresh = 1;
    }
    if (sid_tune.offset < end) {
        return;
    }
    for (uint32_t k = 0; k < sid_tune.chips; k++) {
        if (audio_chip(k) == sid_tune.was[k] || !chip_ctl_has(&keys_router.ctl[k], &sid_tune.want[k])) {
            return; /* its reset has not reached the interrupt yet */
        }
    }
    /* The chips the tune starts on, fresh, at its clock (the interrupt renders neither now). */
    uint32_t q16 = tune_step_q16(h->clock == 2u ? PSID_CLOCK_NTSC : PSID_CLOCK_PAL);
    sid_tune.pal_q16 = audio_chip(0)->cycles_per_sample_q16;
    for (uint32_t k = 0; k < sid_tune.chips; k++) {
        sid_tune.was[k] = audio_chip(k);
        sid_tune.was[k]->cycles_per_sample_q16 = q16;
    }
    sid_tune.clocked = 1;
    if (!psid_ctl_play(c, sid_tune.song)) {
        ui_toast("SID: NO SUCH SONG", 2000u);
    }
    sid_tune.state = TUNE_ON;
}

/* Once a pass, where the song's pass runs: the tune's steps (above), and while it plays its
 * slice, its shadows following a chip swapped under it, its failures told. */
static void tune_pass(void) {
    psid_ctl* c = audio_psid();
    switch (sid_tune.state) {
    case TUNE_OFF:
        return;
    case TUNE_SONG_STOP: {
        song_ctl* sc = audio_song();
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
            return; /* the song's own pass brings it there */
        }
        if (!song_io_away(&songs, 1)) {
            (void)ui_tune(0); /* a load or a save in hand: again next pass */
            return;
        }
        song_away = 1;
        front.playing = 0;
        audio_player(AUDIO_NONE);
        sid_tune.state = TUNE_TO_NONE;
        return;
    }
    case TUNE_TO_NONE: {
        if (audio_player_now() != AUDIO_NONE) {
            return;
        }
        /* The song's state is free: the tune's is made over it. */
        c = audio_psid_init();
        psid_header* h = audio_psid_header();
        if (!tune_header(h)) {
            sid_tune.state = TUNE_END; /* the file changed under it: the song back */
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
        (void)psid_ctl_load(c, h, NULL); /* the 64 KiB as at power-on; the data follows */
        sid_tune.offset = 0;
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
            audio_chip_b_use(AUDIO_B_TUNE, 1); /* the interrupt renders it with chip A */
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
                    /* A model or profile change swapped the chip (its registers came with it,
                     * audio.c psid_swapped): its reads follow the new one from here. */
                    sid_tune.was[k] = s;
                    sid_shadow_init(&c->w->shadow[k], s, psid_env_now(&c->w->env));
                }
            }
        }
        (void)audio_psid_poll();
        tune_tell(c);
        st = psid_ctl_state(c);
        front.playing = st == PSID_INIT || st == PSID_PLAYING;
        return;
    }
    case TUNE_END:
        (void)psid_ctl_stop(c);
        (void)psid_ctl_poll(c); /* sees the stop played out; runs nothing once stopped */
        if (c->stop_wait) {
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
        (void)audio_song_remake(); /* the song's region and transport, the song wiped */
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
        (void)song_io_away(&songs, 0); /* the song read back, by song_io's own passes */
        sid_tune.state = TUNE_BACK_READ;
        return;
    case TUNE_BACK_READ:
        /* The song views get their transport back once the song is whole again. */
        if (song_io_busy(&songs)) {
            return;
        }
        (void)ui_tune(0);
        sid_tune.state = TUNE_OFF;
        return;
    default:
        return;
    }
}

/* A wake that ran no pass (play runs at the key scan's frames and the ticks, about 1.1 kHz, three
 * passes a block): a slice of the tune's 6502 all the same, so it keeps ahead of the audio at
 * the interrupts' pace (a slice is at most PSID_SLICE_CYCLES, about a third of a block's). No
 * chip preparation or store work runs in such a wake, so the slice is the wake's only one. */
static void tune_idle(void) {
    if (sid_tune.state == TUNE_ON) {
        (void)psid_ctl_poll(audio_psid());
    }
}

/* fm1-emu's tests only (emu-gate tests/psid.rs, psid_budget.rs, ram_audio.rs), TEMPORARY until
 * Task 11's browser and NOW PLAYING play tunes: requests a test writes into RAM. op 1 plays the
 * store's .sid file `name`, song `song` (0 the header's start song); 2 stops it (it stays
 * loaded); 3 ends it for good (the song back from the store). result: 1 accepted, 0 refused; the
 * main loop clears op once it has taken it. Nothing on the device writes it. */
typedef struct {
    volatile uint32_t op;
    uint32_t song;
    char name[STORE_NAME_MAX + 1u];
    int32_t result;
} psid_test_req;
psid_test_req psid_test_hook;

static void psid_test(void) {
    psid_test_req* r = &psid_test_hook;
    switch (r->op) {
    case 1: r->result = tune_play(r->name, r->song); break;
    case 2: tune_stop(); r->result = 1; break;
    case 3: tune_end(); r->result = 1; break;
    default: break;
    }
    r->op = 0;
}

/* The end of a pass: MASTER (the pot or MIDI's volume) to the audio path as its output gain,
 * the pass's register writes made visible to the interrupt, then the voices the keys let go
 * in them given back to the song (song_taken). */
static void commit_pass(void) {
    static uint16_t master_sent = 0xffffu; /* (a pass at idle does this compare and the take's) */
    static uint8_t spread_sent = 0u;          /* mono at boot, as the interrupt starts */
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
        song_taken = taken; /* the voices let go (the gate-offs are in), or held at a start */
        (void)song_ctl_take(audio_song(), taken);
    }
}

/* One main-loop pass: the panel into the instrument, the 100 Hz work, then the queued
 * register writes go to the audio interrupt in one batch. The panel only changes when the
 * key scan finishes a frame (~1.1 ms), so a wake-up from the 10 kHz tick in between does
 * nothing here. */
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
        midi_tick(); /* MIDI's mode: its bends, filter and ARP steps */
        static uint8_t player_seen;
        if (player_seen != keys_router.player) {
            player_seen = keys_router.player;
            song_mode_setup();
        }
        if (route_state->song_req != MIDI_ROUTE_NO_REQUEST) {
            int32_t program = midi_route_take_song(route_state);
            (void)song_io_program(&songs, (uint32_t)program);
        }
    }
    if (psid_test_hook.op) {
        psid_test();
    }
    tune_pass();
    if (!song_away) {
        song_pass();
    }
    if (tick_due) {
        /* Once a pass, after its live ticks: the keys' chip has had its turn, then the other
         * chip follows the keys' setup (chip B's first reset at boot among it) and the keys
         * move. */
        keys_route_tick(&keys_router);
        audio_chip_b_use(AUDIO_B_UI, ui_sound_busy(&ui_sounds)); /* chip B in use for a sound */
        midi_send(); /* once a pass at most, so at most once a tick */
    }
    if ((int32_t)(now - *next_tick) >= 0) {
        *next_tick = now + TICK_MS; /* too far behind: skip, the LFO only slows down */
    }
    /* The store (store_fm1.h): at every key-scan frame (about 1.1 kHz) while it has work in
     * hand, else once a tick; its share of the pass within STORE_POLL_WORST, none in a pass
     * that ran a chip, character-table or song slice. Here, not after every wake: an idle
     * store costs the main loop nothing between frames. */
    if (tick_due || store_fm1_active || songs.state) {
        store_fm1_service(now, &instrument, &keys_router, ui_sound_busy(&ui_sounds));
    }
    if (sync_state->clock_out) {
        midi_sync_pass(); /* clock out's times, about once a millisecond */
    }
    commit_pass();
    input_leds(&front, &instrument, now);
    return 1;
}

#if defined(SIDOU_TEST_DEATH_MS)
/* Hardware-checklist builds only (docs/hardware-test.md; CMake SIDOU_TEST_DEATH): with OCT-
 * held at power-on, die SIDOU_TEST_DEATH_MS after boot as an early crash would, before the
 * boot is stable, so the boot guard sends the third such boot to UBOOT. Without the key the
 * build runs as the normal one, so the normal installer always reaches it. A hang with
 * interrupts off lets the watchdog reset the chip; SIDOU_TEST_DEATH_REBOOT resets it through
 * P33 as the fault handler does. */
static uint8_t test_death_armed;

/* Right after interrupts are on: the key scan needs a few frames to debounce (as Felucca reads
 * its power-on combinations). */
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

/* Before an intentional reset: not a failed boot. */
static void leave_on_purpose(void) {
    bootguard_leave(&boot_guard);
}

/* The host asked to upgrade (update.h): the audio stops for good, step 1 runs, and on success
 * the chip restarts into the update loader. If nothing was committed (refused, failed), the
 * firmware restarts the same way it booted. */
static void upgrade(void) {
    fm1_audio_stop();
    audio_halted();
    update_session(leave_on_purpose);
    leave_on_purpose();
    fm1_reboot();
}

/* The end of a main-loop pass: until the next interrupt. With no USB host, or one asleep or
 * gone, the core sleeps in `idle` (it saves power, and fm1-emu skips ahead). While a host has
 * the device configured, it spins until the next TIMER5 tick instead, as Felucca's main loop,
 * ChoralRoot's and the update loader's never idle: on the device, a host saw no IN packet
 * from SIDOU's `idle` build (docs/hardware-test.md, section 8), while the controller's DMA
 * reads each IN packet from RAM when the host's token comes, likely while the core sleeps.
 * The spin wakes the loop at the tick as `idle` does, so a pass's work is the same either way;
 * fm1-emu, which has no host, always takes the `idle`. */
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
/* The flash spike's build only (flash_spike.h): what its waits run, the main loop's services
 * but play and the screen, so the normal installer reaches it at any time but mid-measurement. */
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

/* During the panel's wake-up wait (lcd_init's 120 ms, whatever this takes): the plain XIP
 * window for the store (update.h; its cache invalidation takes milliseconds), then the store's
 * mount, its recovery and the pool's fill (store_fm1.h; a few milliseconds, but a blank
 * region's first format, up to 22 erases), before the audio starts. */
/* The boot's watchdog feed, for store_fm1_boot: a function of its own, so the main loop's
 * fm1_wdt_feed stays inlined (taking the inline's address made the compiler call it there). */
static void boot_feed(void) {
    fm1_wdt_feed();
}

/* During the panel's reset wait (lcd_init's 150 ms): the audio path's noise tables, built in
 * POOL (core/sid/sid.c; about 0.2 ms, before anything plays), then the song's region. In the
 * wait, so the boot's timeline stays as it was. */
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
    /* Interrupts are still off: lcd_init and input_init write port A, which carries PA2. The
     * noise tables are built and the song's region loads in the panel's reset wait
     * (while_lcd_resets), the store in its wake-up wait (while_lcd_wakes). */
    lcd_init(while_lcd_resets, while_lcd_wakes);
    audio_init();
    /* The keys play chip A, as in M1; chip B is made with the keys' setup from the first
     * ticks, prepared in the background (its first reset at boot only: app/reg_ring.h). */
    keys_route_init(&keys_router, &instrument, reg_ring_sink, reg_ring_prepare_step,
                    audio_sink_ctx(), audio_sink_ctx_b());
    keys_router.moved = audio_keys;
    keys_router.taking = song_keys_take;
    live_init(&instrument, keys_route_sink, &keys_router, SID_MODEL_6581);
    /* From here on a profile or model change prepares the spare chip a slice a tick. */
    live_set_preparer(&instrument, keys_route_prepare, &keys_router);
    keys_route_start(&keys_router);
    ui_sound_init(&ui_sounds, &instrument, reg_ring_sink, audio_sink_ctx_b());
    song_boot();
    panel_init(&front);
    input_init();
    panel_master(&front, &instrument, input_master());
    audio_set_master(instrument.master); /* the interrupt's first block has it */
    midi_init(&instrument, &keys_router);
    route_state = midi_routed();
    midi_sync_bind(midi_synced(), &song_bound); /* MIDI's clock plays the song */
    sync_state = midi_synced();
    update_boot();
#if defined(SIDOU_FLASH_SPIKE)
    /* Not SIDOU: the flash spike, with the audio never started (flash_spike.h). The views are
     * set up only for the update protocol's progress frame (ui_update). */
    fm1_timer5_start(isr_timer5, PRIO_TIMER5);
    fm1_irq_enable_all();
    ui_init(&instrument, &ui_sounds, uptime_ms, 0);
    flash_spike_run(spike_service);
#endif
    audio_start();
    /* Sound first: the audio interrupt, the tick (the key scan, MIDI, the clock) and the
     * interrupts start before the screen's first frame, which the main loop then sends without
     * waiting on it (ui_init). */
    fm1_audio_init(abuf, AUDIO_HALF_WORDS, isr_alnk0, PRIO_ALNK0);
    fm1_timer5_start(isr_timer5, PRIO_TIMER5);
    fm1_irq_enable_all();
    audio_irqs_on();
    /* HOME held at power-on plays the full boot (app/boot.h): its level once the key scan
     * has debounced it, about 3 ms after the tick started; the audio already runs. */
    uint32_t held = input_buttons_at_power_on();
    /* uptime_ms counts every timer tick since TIMER4 started (fm1_timer5_irq). */
    ui_init(&instrument, &ui_sounds, uptime_ms, (int)((held >> PANEL_BTN_HOME) & 1u));
    ui_set_keys(&keys_router);
    ui_set_panel(&front);
    song_bound = song_binding;
    song_bound.ctx = audio_song();
    song_bound.song = audio_song()->song;
    ui_set_song(&song_bound);
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
/* The power-on UBOOT gesture, SIDOU's own way back that needs nothing of it but this: OCT-
 * alone held while powering on, and still alone a second later, enters the mask ROM's USB
 * boot (UBOOT), where MvaveFM1Unbricker or jl-uboot-tool install whatever the update path no
 * longer can (docs/reference/controls.md, "At power-on"). It runs first of all, before
 * memory, the boot guard, interrupts, the LCD, audio, the store, settings and USB. Before it
 * only crt0 (the stack), fm1_time_init (TIMER4's three registers) and fm1_wdt_arm (P33
 * writes, the same as the boot guard's path relies on), none of which waits on anything that
 * can fail; it reads the matrix with input_raw_keys, GPIO only, no variable. UBOOT_SAMPLES
 * whole-matrix scans UBOOT_GAP_MS apart (~1 s) must each see OCT- and nothing else: any other
 * button or note key, or OCT- open in any one of them, and the boot goes on as usual. The
 * first scan costs a normal boot about 0.3 ms. The confirmation: the backlight goes dark
 * (PA2 high), then the ROM's black screen. The checklist's dying builds leave it out: there
 * OCT- held at power-on is theirs (docs/hardware-test.md, section 10). */
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
    /* The watchdog would reset the chip out of UBOOT while a host installs. */
    fm1_wdt_stop();
    fm1_enter_uboot();
}
#endif

void fm1_cstart(void) {
    uint32_t *d, *s;
    fm1_cpu1_halt(); /* first thing, as Felucca's fm1_cstart (registers only) */
    fm1_time_init();
    fm1_wdt_arm(WDT_TIMEOUT);
#if !defined(SIDOU_TEST_DEATH_MS)
    uboot_gesture();
#endif
    /* First, before anything that may crash: the third boot in a row that dies before it is
     * stable goes to UBOOT, which still accepts updates (not the 7D soft key: a direct entry). */
    if (bootguard_boot(&boot_guard)) {
        /* The watchdog armed above would reset the chip out of UBOOT while a host installs. */
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
