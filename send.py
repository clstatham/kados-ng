#!/usr/bin/env python3
import serial, struct, sys, zlib

ser = serial.Serial("/dev/ttyUSB0", 115200, timeout=1)
kernel = open("target/aarch64-kados/release/kernel.bin", "rb").read()
print("Waiting for ready signal...")
ser.read(3)
print("Sending kernel...")
ser.write(struct.pack(">I", len(kernel)))  # length
ser.write(kernel)  # image
csum = zlib.crc32(kernel) & 0xFFFF_FFFF
print("Sending checksum...")
ser.write(struct.pack(">I", csum))  # checksum
print("Done!")
