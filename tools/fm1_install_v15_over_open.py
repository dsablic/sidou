#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Felucca's fm1_install.py with one added rule, M-VAVE's own: the stock V15 package (FM-1_015)
may be written by FM1 Open's update loader (identity ota-FM1C_...).

FM1 Open (identity FM-1_8000013, port "FM1 Community") is not a Felucca-family firmware. After
step 1 its loader answers ota-FM1C_..., and the installer's model check refuses it for a
package of another model, though M-VAVE's own updater (fm1_open sdk/WebUpdate.js) accepts any
ota- loader for a stock package. Every other check is the installer's own, and this wrapper
does nothing for a package that is not FM-1_015.

    tools/fm1_install_v15_over_open.py FELUCCA_TOOLS_DIR FM-1.fwsc --port "FM1 Community" [--yes]

FELUCCA_TOOLS_DIR is the folder with Felucca 1.6.1's fm1_install.py. The other arguments are
the installer's. Run it only when you mean to talk to the unit: it opens MIDI ports.
"""

import sys
from pathlib import Path
from typing import Callable, Optional

LOADER_PREFIX = "ota-FM1C_"
STOCK = "FM-1_015"


def patched_model_of(orig: Callable[[str], str], product: Optional[str]) -> Callable[[str], str]:
    def model_of(text: str) -> str:
        if product == STOCK and text.startswith(LOADER_PREFIX):
            return orig(STOCK)
        return orig(text)

    return model_of


def package_product(installer, args) -> Optional[str]:
    for a in args:
        p = Path(a)
        if a.lower().endswith(".fwsc") and p.is_file():
            return installer.product_of(p.read_bytes())
    return None


def main(argv) -> int:
    if len(argv) < 3:
        print(__doc__)
        return 2
    sys.path.insert(0, str(Path(argv[1]).resolve()))
    import fm1_install as installer

    installer.model_of = patched_model_of(installer.model_of, package_product(installer, argv[2:]))
    return installer.main(argv[2:])


if __name__ == "__main__":
    sys.exit(main(sys.argv))
