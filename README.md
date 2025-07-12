# kados-ng

The next generation of KaDOS, my hobby operating system written in Rust. This generation mainly targets ARM64, specifically for the Raspberry Pi 4B, but aims to be modular enough to target other architectures eventually.

## Build Prerequisites

You will need the following prerequisites installed:

- [Rustup](https://rustup.rs/)
- Git (use your package manager or download [Git for Windows](https://git-scm.com/downloads/win))
- **(UNIX ONLY)** Common essential build tools like GCC, LD, etc
- **(WINDOWS ONLY)** [Microsoft Visual Studio](https://visualstudio.microsoft.com/vs/community/) (Rustup will install this for you if you want it to)
- [LLVM](https://github.com/llvm/llvm-project) with its binaries in your `PATH` environment variable
- [QEMU](https://www.qemu.org/download/) if you want to run it in an emulator

## Building

`kados-ng` uses a custom builder utility written in Rust to automatically run the shell commands necessary for building, running, flashing, and even chainloading the OS to a real Raspberry Pi.

For a quickstart release-mode build, just run `cargo builder build --release` in the repository root. Omit `--release` if you want debug symbols.

There are many more utilities available via the build tool, run `cargo builder --help` to see them all.

## Running (QEMU Emulator)

`cargo builder run --release`

## Running on a real Raspberry Pi 4B

TODO: document this

## Chainloading over USB UART serial port

TODO: document this

## Developing

This is a solo project, but here's some random development notes if you want to fork it or something:

- Use `python clippy.py` instead of `cargo clippy` as your command for linting, as it will ensure clippy is run with the right target architecture for each crate in the repo. (This is automatic for VS Code users via workspace settings.)
