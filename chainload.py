#!/usr/bin/env python3
import serial, struct, tqdm, sys

CHUNK_SIZE = 4096
BAUD = 921600


def send_kernel(ser: serial.Serial, kernel: bytes) -> bool:
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
        ty = ser.read(4)
        if ty == b"TY:)":
            print("Kernel sent!")
            serial_monitor(ser)
        else:
            print(f"Error receving: {ty}")
    else:
        print("Size error")


def serial_monitor(ser: serial.Serial):
    ser.apply_settings({"timeout": 0.1})
    while True:
        c = ser.read(1024)
        if c:
            sys.stdout.buffer.write(c)
            sys.stdout.buffer.flush()


if __name__ == "__main__":
    kernel_path = sys.argv[1]
    ser = serial.Serial("/dev/ttyUSB0", BAUD, timeout=None)
    kernel = open(kernel_path, "rb").read()
    print(f"Kernel path: {kernel_path}")
    print(f"Kernel size: 0x{len(kernel):x} bytes")
    try:
        while True:
            print("Waiting for ready signal...")
            num_breaks = 0
            while True:
                c = ser.read()
                if c == b"\x03":
                    num_breaks += 1
                    if num_breaks == 3:
                        send_kernel(ser, kernel)
                else:
                    num_breaks = 0
    except KeyboardInterrupt:
        print("\nCancelled")
