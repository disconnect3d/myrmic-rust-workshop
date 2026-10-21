#!/usr/bin/env python3
"""Recover an ESP USB Serial/JTAG connection after flashing through Parallels.

Keep `myrmic flash --monitor` running, then run this in another terminal:
    sudo python3 scripts/reset-usb.py

This resets the host USB connection; it does not erase or flash firmware.
"""

import fcntl
import os
from pathlib import Path


def main():
    devices = []
    for device in Path('/sys/bus/usb/devices').iterdir():
        try:
            vendor = (device / 'idVendor').read_text().strip()
            product = (device / 'idProduct').read_text().strip()
        except FileNotFoundError:
            continue
        if (vendor, product) == ('303a', '1001'):
            devices.append(device)

    if len(devices) != 1:
        raise SystemExit(
            f'Expected one Espressif USB Serial/JTAG device, found {len(devices)}. '
            'Connect only the target board to this VM.'
        )

    device = devices[0]
    bus = int((device / 'busnum').read_text())
    number = int((device / 'devnum').read_text())
    port = f'/dev/bus/usb/{bus:03d}/{number:03d}'
    try:
        fd = os.open(port, os.O_WRONLY)
    except PermissionError:
        raise SystemExit('Run this command with sudo to reset the USB connection.')
    try:
        fcntl.ioctl(fd, 0x5514, 0)  # USBDEVFS_RESET, from linux/usbdevice_fs.h
    finally:
        os.close(fd)
    print(f'Reset {port}; output should resume in the running monitor.')


if __name__ == '__main__':
    main()
