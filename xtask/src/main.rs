use std::{io::Write, path::PathBuf};

use clap::Parser;
use xshell::{Shell, cmd};

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Run,
    Debug,
    Test,
    TestRunner,
}

#[derive(Parser)]
#[clap(about = "Build the kernel and run it in QEMU")]
struct Args {
    mode: Mode,

    #[clap(long, value_parser)]
    kernel_path: Option<String>,
}

const LINKER_SCRIPT: &str = "linker.ld";
const KERNEL_ELF_NAME: &str = "kernel";
const KERNEL_BIN_NAME: &str = "kernel.bin";
const IMAGE_NAME: &str = "virtio.img";
const MOUNT_DIR: &str = "/mnt/virtio";

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let sh = Shell::new()?;

    if args.mode == Mode::TestRunner {
        let kernel_path = args.kernel_path.unwrap_or_else(|| {
            panic!("Kernel path is required for test runner mode");
        });

        let kernel_path = PathBuf::from(kernel_path);
        let qemu_args = vec![
            "-M",
            "virt",
            "-cpu",
            "cortex-a53",
            "-m",
            "8G",
            "-serial",
            "mon:stdio",
            "-semihosting",
            "-kernel",
            kernel_path.to_str().unwrap(),
        ];
        cmd!(sh, "qemu-system-aarch64").args(qemu_args).run()?;
        return Ok(());
    }

    let build_target = "aarch64-unknown-none";
    let target_dir = PathBuf::from(format!("target/{build_target}"));
    let build_target_dir = target_dir.join("debug");
    let image_path = build_target_dir.join(IMAGE_NAME);
    let arch_dir = PathBuf::from("arch/aarch64");
    let kernel_elf_path = build_target_dir.join(KERNEL_ELF_NAME);
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

    let cargo_output = cmd!(sh, "cargo")
        .args(cargo_args)
        .env("RUSTFLAGS", &rustflags)
        .read_stderr()?;

    cmd!(sh, "llvm-objcopy")
        .arg("-O")
        .arg("binary")
        .arg("--strip-all")
        .arg(&kernel_elf_path)
        .arg(&kernel_bin_path)
        .run()?;

    if !image_path.exists() {
        cmd!(sh, "dd if=/dev/zero of={image_path} bs=1M count=1024").run()?;
        cmd!(sh, "sudo parted {image_path} mklabel msdos").run()?;
        cmd!(
            sh,
            "sudo parted -a none {image_path} mkpart primary fat32 1MiB 100%"
        )
        .run()?;
    }

    cmd!(sh, "sudo losetup -Pf {image_path}").run()?;
    let loop_dev = cmd!(sh, "losetup -j {image_path}").read()?;
    let loop_dev = loop_dev.lines().next().unwrap().split(':').next().unwrap();
    let part = format!("{loop_dev}p1");

    cmd!(sh, "sudo mkfs.vfat {part}").run()?;
    cmd!(sh, "sudo mkdir -p {MOUNT_DIR}").run()?;
    cmd!(sh, "sudo mount {part} {MOUNT_DIR}").run()?;

    let boot_txt = format!(
        r#"
    virtio scan
    scsi scan
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

    cmd!(sh, "sudo cp")
        .arg(format!("{}/boot.scr", target_dir.display()))
        .arg(kernel_bin_path)
        .arg(MOUNT_DIR)
        .run()?;

    cmd!(sh, "sudo umount {MOUNT_DIR}").run()?;
    cmd!(sh, "sudo losetup -d {loop_dev}").run()?;

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
            .args([
                "xtask",
                "test-runner",
                "--kernel-path",
                test_executable.to_str().unwrap(),
            ])
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
