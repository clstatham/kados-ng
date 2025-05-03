use std::path::PathBuf;

use clap::{Parser, Subcommand};
use xshell::{Shell, cmd};

#[derive(Subcommand, Clone, Debug, PartialEq, Eq)]
enum Mode {
    /// Build the kernel for a Raspberry Pi 4b
    Build,
    /// Build the kernel for a Raspberry Pi 4b and emulate it in QEMU
    Run,
    /// Build the kernel for a Raspberry Pi 4b and run it in QEMU with debug options (gdbserver)
    Debug,
    /// Build the kernel for a Raspberry Pi 4b and run tests in QEMU
    Test,
    /// Flash the built image to an SD card for the Raspberry Pi
    Flash {
        /// Device to flash to (e.g. /dev/sdb)
        device: String,
    },
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

    /// Extra args to pass to qemu
    #[clap(long, value_parser)]
    extra_qemu_args: Option<String>,
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

    let build_target = "aarch64-kados";
    let target_dir = PathBuf::from(format!("target/{build_target}"));
    let build_target_dir = target_dir.join("debug");
    let arch_dir = PathBuf::from("arch/aarch64");
    let mut kernel_elf_path = args
        .kernel_path()
        .unwrap_or_else(|| format!("{}/{}", build_target_dir.display(), KERNEL_ELF_NAME));
    let kernel_img_path = build_target_dir.join(KERNEL_IMG_NAME);
    let target_file_path = arch_dir.join(format!("{build_target}.json"));

    let rustflags = format!(
        "-C link-arg=-T{} -Cforce-frame-pointers=yes -C symbol-mangling-version=v0",
        arch_dir.join(LINKER_SCRIPT_NAME).display(),
    );

    if let Mode::Flash { device } = args.mode {
        cmd!(
            sh,
            "sudo dd if=target/aarch64-kados/debug/kados.img of={device} bs=4M status=progress"
        )
        .run()?;
        cmd!(sh, "sync").run()?;
        return Ok(());
    }

    let mut cargo_args = vec![];

    if args.mode == Mode::Test {
        cargo_args.push("test");
        cargo_args.push("--no-run");
    } else {
        cargo_args.push("build");
    }

    cargo_args.push("--target");
    cargo_args.push(target_file_path.to_str().unwrap());
    cargo_args.push("-p");
    cargo_args.push("kernel");
    cargo_args.push("-Zbuild-std=core,compiler_builtins,alloc");
    cargo_args.push("-Zbuild-std-features=compiler-builtins-mem");

    if args.mode == Mode::Test {
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
    } else {
        cmd!(sh, "cargo")
            .args(cargo_args)
            .env("RUSTFLAGS", &rustflags)
            .run()?;
    }

    if kernel_img_path.exists() {
        std::fs::remove_file(&kernel_img_path)?;
    }

    cmd!(sh, "truncate -s 128M {kernel_img_path}").run()?;
    cmd!(sh, "mformat -i {kernel_img_path} ::").run()?;
    cmd!(sh, "mmd -i {kernel_img_path} ::/EFI").run()?;
    cmd!(sh, "mmd -i {kernel_img_path} ::/EFI/BOOT").run()?;

    let limine_dir = target_dir.join("limine");
    if !limine_dir.exists() {
        cmd!(
            sh,
            "git clone {LIMINE_GIT_URL} --depth=1 --branch v9.x-binary {limine_dir}"
        )
        .run()?;
    }

    if !std::fs::exists("u-boot")? {
        cmd!(sh, "git clone https://github.com/u-boot/u-boot.git").run()?;
    }

    if !std::fs::exists("firmware")? {
        cmd!(
            sh,
            "git clone --depth=1 https://github.com/raspberrypi/firmware.git"
        )
        .run()?;
    }

    {
        let _uboot = sh.push_dir("u-boot");
        cmd!(sh, "make rpi_4_defconfig").run()?;
        let nproc = num_cpus::get().to_string();
        cmd!(sh, "make -j{nproc} CROSS_COMPILE=aarch64-none-elf-").run()?;
        cmd!(sh, "mcopy -i ../{kernel_img_path} u-boot.bin ::/u-boot.bin").run()?;
    }

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

    cmd!(sh, "mcopy -i {kernel_img_path} config.txt ::/config.txt").run()?;

    cmd!(
        sh,
        "mcopy -i {kernel_img_path} firmware/boot/start4.elf ::/start4.elf"
    )
    .run()?;
    cmd!(
        sh,
        "mcopy -i {kernel_img_path} firmware/boot/fixup4.dat ::/fixup4.dat"
    )
    .run()?;
    cmd!(
        sh,
        "mcopy -i {kernel_img_path} firmware/boot/bcm2711-rpi-4-b.dtb ::/bcm2711-rpi-4-b.dtb"
    )
    .run()?;

    if args.mode == Mode::Test {
        cmd!(sh, "cargo")
            .args(["xtask", "test-runner", kernel_elf_path.as_str()])
            .run()?;
        return Ok(());
    }

    if !matches!(args.mode, Mode::Build) {
        let qemu_drive_arg_rpi = format!("if=sd,format=raw,file={}", kernel_img_path.display());

        let mut qemu_args = vec![];

        qemu_args.extend([
            "-M",
            "raspi4b",
            "-cpu",
            "cortex-a72",
            "-drive",
            &qemu_drive_arg_rpi,
            "-kernel",
            "u-boot/u-boot.bin",
            "-dtb",
            "u-boot/arch/arm/dts/bcm2711-rpi-4-b.dtb",
            "-D",
            "target/log.txt",
            "-d",
            "int,guest_errors",
            "-m",
            "2G",
            "-serial",
            "mon:stdio",
            "-semihosting",
        ]);

        if matches!(args.mode, Mode::Debug) {
            qemu_args.push("-s");
            qemu_args.push("-S");
        }

        if let Some(extra_args) = args.extra_qemu_args.as_ref() {
            for arg in extra_args.split_whitespace() {
                qemu_args.push(arg);
            }
        }

        cmd!(sh, "qemu-system-aarch64").args(qemu_args).run()?;
    }

    Ok(())
}
