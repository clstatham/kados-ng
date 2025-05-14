#!/usr/bin/env python3
import serial, struct, tqdm, sys, argparse
from elftools.elf.elffile import ELFFile

CHUNK_SIZE = 4096


def parse_args():
    parser = argparse.ArgumentParser(description="Chainload a kernel to a Raspberry Pi 4B.")
    parser.add_argument("kernel", help="Path to the kernel binary file.")
    parser.add_argument("--sym", help="Path to the kernel debug symbols file.")
    parser.add_argument("--baud", type=int, default=921600, help="Serial port baud rate.")
    parser.add_argument("--monitor", action="store_true", help="Enable serial monitor mode after sending the kernel.")
    return parser.parse_args()


def send_kernel(ser: serial.Serial, kernel: bytes, args):
    print("Sending kernel size...")
    ser.write(struct.pack("<I", len(kernel)))
    ok = ser.read(2)
    if ok == b"OK":
        print("Sending kernel...")
        it = tqdm.tqdm(range(0, len(kernel), CHUNK_SIZE), unit="bytes", unit_scale=CHUNK_SIZE)
        for i in it:
            ser.write(kernel[i : i + CHUNK_SIZE])
            echo = ser.read(CHUNK_SIZE)
            assert echo == kernel[i : i + CHUNK_SIZE], "error in data stream"
        i += CHUNK_SIZE
        if i < len(kernel):
            ser.write(kernel[i:])
        ser.flush()
        print("Kernel sent!")
        if args.monitor:
            serial_monitor(ser, args)
        else:
            sys.exit(0)
    else:
        print("Size error")


def find_symbol_name(sym_file: ELFFile, addr: int):
    symtab = sym_file.get_section_by_name(".symtab")
    if not symtab:
        symtab = sym_file.get_section_by_name(".dynsym")
    if symtab:
        for sym in symtab.iter_symbols():
            value = sym["st_value"]
            size = sym["st_size"]
            if value <= addr < (value + size):
                return str(sym.name)
    return None


def serial_monitor(ser: serial.Serial, args):
    if args.sym:
        sym_file = ELFFile(open(args.sym, "rb"))
    ser.apply_settings({"timeout": 0.1})
    while True:
        c = ser.read(4096)
        if c:
            for line in c.splitlines():
                if line.startswith(b"[sym]"):
                    if args.sym:
                        addr = int(line[5:])
                        name = find_symbol_name(sym_file, addr)
                        if name:
                            ser.write(bytes(name, encoding="utf-8"))
                        else:
                            ser.write(b"unknown")
                    else:
                        ser.write(b"unknown")
                    ser.write(b"\n")
                elif len(line) > 0:
                    sys.stdout.buffer.write(line)
                    sys.stdout.buffer.write(b"\n")
                    sys.stdout.buffer.flush()


if __name__ == "__main__":
    args = parse_args()
    ser = serial.Serial("/dev/ttyUSB0", args.baud, timeout=None)
    kernel = open(args.kernel, "rb").read()

    print(f"Kernel path: {args.kernel}")
    print(f"Kernel size: 0x{len(kernel):x} ({len(kernel)}) bytes")
    print(f"Baud rate: {args.baud}")
    if args.monitor:
        print("Will monitor after load")
    else:
        print("Will exit after load")

    try:
        while True:
            print("Waiting for ready signal (power cycle your Pi now)...")
            num_breaks = 0
            while True:
                c = ser.read()
                if c == b"\x03":
                    num_breaks += 1
                    if num_breaks == 3:
                        send_kernel(ser, kernel, args)
                else:
                    num_breaks = 0
    except KeyboardInterrupt:
        print("\nCancelled")
        sys.exit(1)
