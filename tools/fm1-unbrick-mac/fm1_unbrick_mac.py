#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
import os
import struct
import sys

import usb.backend.libusb1
import usb.core
import usb.util

import fm1_unbrick as fu

VID, PID = 0x4C4A, 0x8057
LIBUSB = "/opt/homebrew/lib/libusb-1.0.dylib"
TIMEOUT_MS = 5000


def backend():
    return usb.backend.libusb1.get_backend(find_library=lambda _: LIBUSB)


def find_usb():
    return list(usb.core.find(find_all=True, idVendor=VID, idProduct=PID, backend=backend()))


def path_of(dev):
    return "usb:%04x:%04x@%d.%d" % (VID, PID, dev.bus, dev.address)


class SCSIDev:

    def __init__(self, path=None):
        self.is_open = False
        self.path = path
        self.dev = None
        self.tag = 0
        if path is not None:
            self.open(path)

    def __enter__(self):
        return self

    def __exit__(self, *a):
        self.close()

    def open(self, path):
        devs = [d for d in find_usb() if path_of(d) == path]
        if len(devs) != 1:
            raise OSError("no USB device %s" % path)
        d = devs[0]
        if d.is_kernel_driver_active(0):
            d.detach_kernel_driver(0)
        d.set_configuration()
        usb.util.claim_interface(d, 0)
        intf = d.get_active_configuration()[(0, 0)]
        if (intf.bInterfaceClass, intf.bInterfaceSubClass, intf.bInterfaceProtocol) != (8, 6, 0x50):
            raise OSError("not a SCSI bulk-only mass-storage interface")
        eps = {e.bEndpointAddress: e for e in intf}
        self.ep_in = next(a for a in eps if a & 0x80)
        self.ep_out = next(a for a in eps if not a & 0x80)
        self.dev, self.path, self.is_open = d, path, True

    def close(self):
        if self.dev is not None:
            try:
                usb.util.release_interface(self.dev, 0)
            except Exception:
                pass
            usb.util.dispose_resources(self.dev)
            self.dev = None
        self.is_open = False

    def _csw(self):
        try:
            raw = bytes(self.dev.read(self.ep_in, 13, TIMEOUT_MS))
        except usb.core.USBError as e:
            if e.errno != 32:
                raise
            self.dev.clear_halt(self.ep_in)
            raw = bytes(self.dev.read(self.ep_in, 13, TIMEOUT_MS))
        if len(raw) != 13 or raw[:4] != b"USBS":
            raise fu_scsi_exc("bad CSW %s" % raw.hex())
        _, tag, residue, status = struct.unpack("<4sIIB", raw)
        if tag != self.tag:
            raise fu_scsi_exc("CSW tag %d, expected %d" % (tag, self.tag))
        return residue, status

    def execute(self, cdb, data_out, data_in, max_sense_len=32, return_sense_buffer=False):
        if data_out and data_in:
            raise NotImplementedError("Indirect I/O is not supported")
        self.tag = (self.tag + 1) & 0xFFFFFFFF
        n = len(data_out) if data_out else len(data_in) if data_in else 0
        flags = 0x80 if data_in else 0x00
        cbw = struct.pack("<4sIIBBB", b"USBC", self.tag, n, flags, 0, len(cdb)) + bytes(cdb).ljust(16, b"\0")
        self.dev.write(self.ep_out, cbw, TIMEOUT_MS)
        got = b""
        try:
            if data_out:
                self.dev.write(self.ep_out, bytes(data_out), TIMEOUT_MS)
            elif data_in:
                got = bytes(self.dev.read(self.ep_in, n, TIMEOUT_MS))
        except usb.core.USBError as e:
            if e.errno != 32:
                raise
            self.dev.clear_halt(self.ep_in if data_in else self.ep_out)
        residue, status = self._csw()
        if status != 0:
            raise fu_scsi_exc("Transfer failed: CSW status %d, residue %d" % (status, residue))
        if data_in:
            data_in[:len(got)] = bytearray(got)
        return (residue, b"") if return_sense_buffer else residue


_SCSIException = None


def fu_scsi_exc(msg):
    return (_SCSIException or OSError)(msg)


def find_devices():
    return [{"path": path_of(d), "description": "WL82 UBOOT1.00 (USB %d.%d, libusb)" % (d.bus, d.address)}
            for d in find_usb()]


def check_privileges(path):
    if os.geteuid() != 0:
        raise fu.UnbrickError("run with sudo on macOS (detaching the disk driver needs root)")


_load_jlub = fu.load_jlub


def load_jlub(jl_dir):
    global _SCSIException
    import types
    from pathlib import Path
    pkg = types.ModuleType("scsiio")
    pkg.__path__ = [str(Path(jl_dir).resolve() / "scsiio")]
    pkg.SCSIDev = SCSIDev
    sys.modules.setdefault("scsiio", pkg)
    ns = _load_jlub(jl_dir)
    _SCSIException = ns.SCSIException
    ns.SCSIDev = SCSIDev
    return ns


if sys.platform == "darwin":
    fu.platform_supported = lambda: True
    fu.find_devices = find_devices
    fu.check_privileges = check_privileges
    fu.load_jlub = load_jlub

if __name__ == "__main__":
    sys.exit(fu.main())
