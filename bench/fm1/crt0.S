/* SPDX-FileCopyrightText: 2026 Joakim Eriksson
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: BSD-2-Clause
 * Bare-metal pi32v2 entry (after fm1-sid's fm1/crt0.S): set both stacks, call main. */
	.section .text.crt0,"ax",@progbits
	.globl _start
_start:
	goto 1f
	rts
1:	cli
	sp = 0x01C7A000
	ssp = 0x01C7C000
	call main
	goto .
