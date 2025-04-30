use std::{io::Write, path::PathBuf};

use clap::Parser;
use xshell::{Shell, cmd};

#[derive(clap::Subcommand, Clone, Debug, PartialEq, Eq)]
enum Mode {
    /// Build the kernel and run it in QEMU
    Run,
    /// Build the kernel and run it in QEMU with debug options (gdbserver)
    Debug,
    /// Build the kernel and run the tests in QEMU
    Test,
    /// Internal mode used by the test runner
    #[clap(hide = true)]
    TestRunner {
        /// Path to the kernel ELF file
        kernel_path: String,
    },
}

#[derive(Parser)]
#[clap(about = "Build the kernel and run it in QEMU")]
struct Args {
    /// Mode of operation
    #[command(subcommand)]
    mode: Mode,
}

impl Args {
    fn kernel_path(&self) -> Option<String> {
        match &self.mode {
            Mode::TestRunner { kernel_path } => Some(kernel_path.clone()),
            _ => None,
        }
    }
}

const LINKER_SCRIPT: &str = "linker.ld";
const KERNEL_ELF_NAME: &str = "kernel";
const KERNEL_BIN_NAME: &str = "kernel.bin";
const IMAGE_NAME: &str = "virtio.img";

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let sh = Shell::new()?;

    let build_target = "aarch64-unknown-none";
    let target_dir = PathBuf::from(format!("target/{build_target}"));
    let build_target_dir = target_dir.join("debug");
    let image_path = build_target_dir.join(IMAGE_NAME);
    let arch_dir = PathBuf::from("arch/aarch64");
    let mut kernel_elf_path = args
        .kernel_path()
        .unwrap_or_else(|| format!("{}/{}", build_target_dir.display(), KERNEL_ELF_NAME));
    let kernel_bin_path = build_target_dir.join(KERNEL_BIN_NAME);
    let rustflags = format!("-C link-arg=-T{}", arch_dir.join(LINKER_SCRIPT).display());

    let mut cargo_args = vec![];

    if args.mode == Mode::Test {
        cargo_args.push("test");
        cargo_args.push("--no-run");
    } else {
        cargo_args.push("build");
    }

    cargo_args.push("--target");
    cargo_args.push(build_target);
    cargo_args.push("-p");
    cargo_args.push("kernel");

    let cargo_output = cmd!(sh, "cargo")
        .args(cargo_args)
        .env("RUSTFLAGS", &rustflags)
        .read_stderr()?;

    if args.mode == Mode::Test {
        let new_kernel_elf = cargo_output
            .split("/deps/")
            .last()
            .unwrap()
            .trim()
            .strip_suffix(')')
            .unwrap()
            .to_string();
        kernel_elf_path = build_target_dir
            .join("deps")
            .join(new_kernel_elf)
            .display()
            .to_string();
    }

    cmd!(sh, "llvm-objcopy")
        .arg("-O")
        .arg("binary")
        .arg("--strip-all")
        .arg(&kernel_elf_path)
        .arg(&kernel_bin_path)
        .run()?;

    if !image_path.exists() {
        cmd!(sh, "truncate -s 64M {image_path}").run()?;
        cmd!(sh, "mformat -i {image_path} -h 64 -t 32 -s 32 -F ::").run()?;
    }

    let boot_txt = format!(
        r#"
    virtio scan
    fatload virtio 0:1 0x40080000 {KERNEL_BIN_NAME}
    go 0x40080000
    "#
    );
    std::fs::File::create(format!("{}/boot.txt", target_dir.display()))?
        .write_all(boot_txt.trim().as_bytes())?;

    cmd!(sh, "mkimage")
        .arg("-d")
        .arg(format!("{}/boot.txt", target_dir.display()))
        .arg("-A")
        .arg("arm64")
        .arg("-O")
        .arg("linux")
        .arg("-T")
        .arg("script")
        .arg("-C")
        .arg("none")
        .arg("-a")
        .arg("0x40080000")
        .arg("-e")
        .arg("0x40080000")
        .arg(format!("{}/boot.scr", target_dir.display()))
        .run()?;

    cmd!(
        sh,
        "mcopy -Do -i {image_path} -s {kernel_bin_path} ::{KERNEL_BIN_NAME}"
    )
    .run()?;
    cmd!(
        sh,
        "mcopy -Do -i {image_path} -s {target_dir}/boot.scr ::boot.scr"
    )
    .run()?;

    if args.mode == Mode::Test {
        let test_executable = cargo_output
            .split("/deps/")
            .last()
            .unwrap()
            .trim()
            .strip_suffix(')')
            .unwrap();

        let test_executable = target_dir.join("debug").join("deps").join(test_executable);

        cmd!(sh, "cargo")
            .args(["xtask", "test-runner", test_executable.to_str().unwrap()])
            .run()?;
        return Ok(());
    }

    let disk_arg = format!("file={},if=virtio,format=raw", image_path.display());

    let mut qemu_args = vec![
        "-M",
        "virt",
        "-cpu",
        "cortex-a53",
        "-m",
        "8G",
        "-serial",
        "mon:stdio",
        "-semihosting",
        "-bios",
        "arch/aarch64/u-boot/u-boot.bin",
        "-drive",
        &disk_arg,
    ];

    if args.mode == Mode::Debug {
        qemu_args.push("-s");
        qemu_args.push("-S");
    }

    cmd!(sh, "qemu-system-aarch64").args(qemu_args).run()?;

    Ok(())
}
