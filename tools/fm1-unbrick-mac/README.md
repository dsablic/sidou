<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# fm1-unbrick-mac: recover an FM-1 from macOS

[MvaveFM1Unbricker](https://github.com/Quixotic7/MvaveFM1Unbricker) writes a firmware package
(`.fwsc`) onto an FM-1 that sits in the chip's ROM boot mode (the USB disk `WL82 UBOOT1.00`,
USB 4C4A:8057). It does so through
[jl-uboot-tool](https://github.com/kagaimiq/jl-uboot-tool), which talks SCSI to that disk
through the OS's SCSI pass-through: Linux's `SG_IO` or Windows' SPTI. macOS has no such
pass-through for a USB disk, so the unbricker refuses to run on a Mac.

`fm1_unbrick_mac.py` adds the missing back end. It speaks the USB Mass Storage Bulk-Only
Transport (a command block, the data, a status block) to the device directly through libusb
(pyusb), after detaching macOS's disk driver, which needs root. It exposes the same
`SCSIDev.execute(cdb, data_out, data_in)` as jl-uboot-tool's `scsiio/linuxport.py`, and
patches only the unbricker's platform hooks (device scan, `SCSIDev`, privilege check)
before calling its unmodified `main()`. All of the unbricker's checks still apply: the
device must answer as WL82 UBOOT1.00 with chip key 0x980F and flash ID 0x856014, the whole
flash is read twice and compared before anything is written, only 0x4000..0x92FFF is
written, and every byte is read back and compared.

It recovered a real FM-1 on 2026-10-17 ([the report](../../docs/hardware-results/2026-10-17-first-flash.md)).
A Linux VM (UTM) was tried first and did not work: through UTM's USB 3.0 controller the
unbricker's first SCSI command failed and the ROM dropped out of boot mode; through its USB
2.0 controller the full-speed device never reached the guest.

## Setup (once)

```sh
brew install libusb
git clone https://github.com/Quixotic7/MvaveFM1Unbricker.git
cd MvaveFM1Unbricker
git clone https://github.com/kagaimiq/jl-uboot-tool.git && git -C jl-uboot-tool checkout adb3f18
python3 -m venv ../venv
../venv/bin/pip install pyusb pyyaml -r jl-uboot-tool/requirements.txt
cp /path/to/sidou/tools/fm1-unbrick-mac/fm1_unbrick_mac.py .
```

Tested with MvaveFM1Unbricker 1.2.0 (707470a), jl-uboot-tool adb3f18, libusb from Homebrew
(`/opt/homebrew/lib/libusb-1.0.dylib`; edit `LIBUSB` in the script for another path),
Python 3.9 and macOS on Apple silicon.

## Use

Put the FM-1 into ROM boot mode first, for example with a Flipper Zero and
[flipper-fm1-recovery](https://github.com/merthsoft/flipper-fm1-recovery) or with the
[FM-1 Transporter](https://github.com/kurogedelic/FM-1-transporter), and plug it into the Mac.
`ioreg -p IOUSB -l | grep "USB Product Name"` then shows `WL80UBOOT1.00`. The unit stays in
that mode while it is powered.

```sh
sudo ../venv/bin/python fm1_unbrick_mac.py info                  # reads only: chip key, flash ID
sudo ../venv/bin/python fm1_unbrick_mac.py restore FM-1.fwsc     # backup x2, write, verify
```

`restore` asks for a typed confirmation before it writes. If a write or verify fails, do not
power the FM-1 off: run `restore` again. A power cycle after a successful restore starts the
firmware written. Everything a run reads (both full-flash backups, the log) is kept in
`backups/fm1-recovery-<date>/`.
