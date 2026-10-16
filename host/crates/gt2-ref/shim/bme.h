/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
/* The gt2-ref harness's stand-in for GoatTracker's BME library header (bme/bme.h, which
 * needs SDL). GoatTracker's song, table and relocation code includes "bme.h"; the harness
 * puts this directory first on the include path. It declares what that code calls and
 * gives the editor's key codes distinct values: the editor's key handling compiles but
 * never runs. Written for SIDOU; no BME code is copied. */
#ifndef GT2REF_BME_H
#define GT2REF_BME_H

#include <stdio.h>

/* bme_end.h: one byte, or a little-endian word, to or from a file. */
unsigned fread8(FILE* file);
void fwrite8(FILE* file, unsigned data);
void fwritele16(FILE* file, unsigned data);

/* bme_io.h: the linked datafile, here the player sources (gt2ref_shim.c). */
int io_openlinkeddatafile(unsigned char* datafile);
int io_open(char* name);
int io_lseek(int handle, int bytes, int whence);
int io_read8(int handle);
void io_close(int handle);

/* bme_win.h */
extern int win_quitted;

/* bme_main.h's key codes, which the editor's code names. */
#define KEY_0 1
#define KEY_1 2
#define KEY_2 3
#define KEY_3 4
#define KEY_4 5
#define KEY_5 6
#define KEY_6 7
#define KEY_7 8
#define KEY_8 9
#define KEY_9 10
#define KEY_A 11
#define KEY_APOST2 12
#define KEY_B 13
#define KEY_BACKSPACE 14
#define KEY_C 15
#define KEY_COLON 16
#define KEY_COMMA 17
#define KEY_D 18
#define KEY_DEL 19
#define KEY_DOWN 20
#define KEY_E 21
#define KEY_END 22
#define KEY_ENTER 23
#define KEY_ESC 24
#define KEY_F 25
#define KEY_G 26
#define KEY_H 27
#define KEY_HOME 28
#define KEY_I 29
#define KEY_INS 30
#define KEY_J 31
#define KEY_K 32
#define KEY_L 33
#define KEY_LEFT 34
#define KEY_M 35
#define KEY_N 36
#define KEY_O 37
#define KEY_P 38
#define KEY_PGDN 39
#define KEY_PGUP 40
#define KEY_Q 41
#define KEY_R 42
#define KEY_RIGHT 43
#define KEY_S 44
#define KEY_SPACE 45
#define KEY_T 46
#define KEY_U 47
#define KEY_UP 48
#define KEY_V 49
#define KEY_W 50
#define KEY_X 51
#define KEY_Y 52
#define KEY_Z 53

#endif
