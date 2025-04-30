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

const LIMINE_GIT_URL: &str = "https://github.com/limine-bootloader/limine.git";
const LINKER_SCRIPT_NAME: &str = "linker.ld";
const KERNEL_ELF_NAME: &str = "kernel";
const KERNEL_IMG_NAME: &str = "kados.img";

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let sh = Shell::new()?;

    let build_target = "aarch64-unknown-none";
    let target_dir = PathBuf::from(format!("target/{build_target}"));
    let build_target_dir = target_dir.join("debug");
    let arch_dir = PathBuf::from("arch/aarch64");
    let mut kernel_elf_path = args
        .kernel_path()
        .unwrap_or_else(|| format!("{}/{}", build_target_dir.display(), KERNEL_ELF_NAME));
    let kernel_img_path = build_target_dir.join(KERNEL_IMG_NAME);
    let rustflags = format!(
        "-C link-arg=-T{}",
        arch_dir.join(LINKER_SCRIPT_NAME).display()
    );

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
        .ignore_status()
        .output()?;

    let cargo_stdout = String::from_utf8_lossy(&cargo_output.stdout);
    let cargo_stderr = String::from_utf8_lossy(&cargo_output.stderr);

    println!("{cargo_stdout}");

    if !cargo_output.status.success() {
        eprintln!("{cargo_stderr}");
        eprintln!("Cargo command failed: {:?}", cargo_output.status);
        std::process::exit(1);
    }

    if args.mode == Mode::Test {
        let new_kernel_elf = cargo_stderr
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

    let limine_dir = target_dir.join("limine");
    if !limine_dir.exists() {
        cmd!(
            sh,
            "git clone {LIMINE_GIT_URL} --depth=1 --branch v9.x-binary {limine_dir}"
        )
        .run()?;
    }

    if kernel_img_path.exists() {
        std::fs::remove_file(&kernel_img_path)?;
    }

    cmd!(sh, "dd if=/dev/zero of={kernel_img_path} bs=1M count=128").run()?;
    cmd!(sh, "parted {kernel_img_path} -s mklabel gpt").run()?;
    cmd!(sh, "parted {kernel_img_path} -s mkpart ESP fat32 1MiB 100%").run()?;
    cmd!(sh, "parted {kernel_img_path} -s set 1 esp on").run()?;

    cmd!(sh, "mformat -i {kernel_img_path} ::").run()?;
    cmd!(sh, "mmd -i {kernel_img_path} ::/EFI").run()?;
    cmd!(sh, "mmd -i {kernel_img_path} ::/EFI/BOOT").run()?;

    let bootaa64 = limine_dir.join("BOOTAA64.EFI");
    cmd!(
        sh,
        "mcopy -i {kernel_img_path} {bootaa64} ::/EFI/BOOT/BOOTAA64.EFI"
    )
    .run()?;
    cmd!(
        sh,
        "mcopy -i {kernel_img_path} {kernel_elf_path} ::/kados.elf"
    )
    .run()?;
    cmd!(sh, "mcopy -i {kernel_img_path} limine.conf ::/limine.conf").run()?;

    let qemu_drive_arg = format!(
        "if=none,file={},format=raw,id=hd",
        kernel_img_path.display()
    );

    if args.mode == Mode::Test {
        let test_executable = cargo_stderr
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

    let mut qemu_args = vec![
        "-M",
        "virt",
        "-cpu",
        "cortex-a72",
        "-m",
        "4G",
        "-serial",
        "mon:stdio",
        "-semihosting",
        "-bios",
        "/usr/share/edk2/aarch64/QEMU_EFI.fd",
        "-drive",
        &qemu_drive_arg,
        "-device",
        "virtio-blk-device,drive=hd",
    ];

    if args.mode == Mode::Debug {
        qemu_args.push("-s");
        qemu_args.push("-S");
    }

    cmd!(sh, "qemu-system-aarch64").args(qemu_args).run()?;

    Ok(())
}
