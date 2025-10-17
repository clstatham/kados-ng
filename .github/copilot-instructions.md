# Copilot Instructions for kados-ng

## Project Overview
- **kados-ng** is a modular hobby OS written in Rust, targeting ARM64 (Raspberry Pi 4B) but designed for portability to other architectures.
- Major components are organized under `crates/`:
  - `kernel/`: Core OS logic (task switching, memory, IRQ, logging, etc.)
  - `bootloader/`: Boot logic for hardware initialization
  - `chainloader/`: Chainloading utilities
  - `tools/`: Builder and loader utilities
- Architecture-specific configs and code are under `arch/` and `crates/*/src/arch/`.

## Build & Development Workflow
- **Custom builder**: Use `cargo builder build --release` to build the OS. The builder automates cross-compilation and deployment steps.
- **Linting**: Run `python clippy.py` (not `cargo clippy`) to ensure correct target linting for all crates.
- **Emulation**: Use `cargo builder run --release` to run in QEMU.
- For more builder commands, run `cargo builder --help`.

## Key Patterns & Conventions
- **Rust workspace**: All main logic is split into crates under `crates/`. Each crate has its own `Cargo.toml`.
- **Architecture abstraction**: Platform-specific code is separated into `arch/` folders and config JSONs (e.g., `arch/aarch64/aarch64-kados.json`).
- **No standard library**: Kernel and bootloader code is `#![no_std]` and uses custom panic/logging implementations.
- **Task switching**: See `crates/kernel/src/task/` for context switching and scheduling logic.
- **Memory management**: See `crates/kernel/src/mem/` for custom allocators and paging.
- **Syscalls**: Implemented in `crates/kernel/src/syscall/`.

## External Dependencies & Integration
- Relies on Rust, LLVM, QEMU, and Python for build/lint/test workflows.
- Builder tool (`tools/builder/`) is a Rust crate that wraps build logic and deployment.
- Linting is Python-based for cross-target support.

## Example Workflow
```bash
# Build release kernel
cargo builder build --release

# Run in QEMU
cargo builder run --release

# Lint all crates
python clippy.py
```

## References
- See `README.md` for prerequisites and more details.
- Key source directories: `crates/kernel/`, `crates/bootloader/`, `arch/`, `tools/builder/`.

---

*Update this file as project conventions evolve. Focus on actionable, project-specific guidance for AI agents.*
