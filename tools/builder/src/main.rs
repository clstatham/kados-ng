use std::{fmt::Display, path::PathBuf};

use clap::{Parser, Subcommand};
use xshell::{Shell, cmd};

#[derive(Subcommand, Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Build the kernel
    Build {
        #[clap(short, long, default_value_t = false)]
        release: bool,
    },
    /// Build the kernel and emulate it in QEMU
    Run {
        #[clap(short, long, default_value_t = false)]
        release: bool,
    },
    /// Build the kernel and run it in QEMU with debug options (gdbserver)
    Debug {
        #[clap(short, long, default_value_t = false)]
        release: bool,
    },
    /// Copy the kernel to an SD card for the Raspberry Pi
    Flash {
        /// Device to flash to (e.g. /dev/sdb)
        device: String,
        #[clap(short, long, default_value_t = false)]
        release: bool,
    },
    /// Build and copy the chainloader to an SD card for the Raspberry Pi
    FlashChainloader {
        /// Device to flash to (e.g. /dev/sdb)
        device: String,
    },
    /// Send the kernel over USB UART to the Raspberry Pi
    Load {
        #[clap(short, long, default_value_t = false)]
        release: bool,
    },
}

#[derive(Parser)]
#[clap(about = "kados-ng build tool")]
pub struct Args {
    /// Mode of operation
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Debug,
    Release,
}

impl Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Debug => write!(f, "debug"),
            Self::Release => write!(f, "release"),
        }
    }
}

pub struct Context {
    sh: Shell,
    profile: Profile,
    build_root: PathBuf,
}

impl Context {
    pub fn new(release: bool) -> anyhow::Result<Self> {
        Ok(Self {
            sh: Shell::new()?,
            profile: if release {
                Profile::Release
            } else {
                Profile::Debug
            },
            build_root: env!("CARGO_MANIFEST_DIR")
                .parse::<PathBuf>()?
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf(),
        })
    }

    pub fn target_dir(&self) -> PathBuf {
        self.build_root
            .join("target")
            .join("aarch64-kados")
            .join(self.profile.to_string())
    }

    pub fn arch_dir(&self) -> PathBuf {
        self.build_root.join("arch").join("aarch64")
    }

    pub fn target_json_path(&self) -> PathBuf {
        self.arch_dir().join("aarch64-kados.json")
    }

    pub fn bootloader_elf_path(&self) -> PathBuf {
        self.target_dir().join("libbootloader.a")
    }

    pub fn kernel_elf_path(&self) -> PathBuf {
        self.target_dir().join("kernel")
    }

    pub fn kernel_bin_path(&self) -> PathBuf {
        self.kernel_elf_path().with_extension("bin")
    }

    pub fn kernel_sym_path(&self) -> PathBuf {
        self.kernel_elf_path().with_extension("sym")
    }

    pub fn chainloader_elf_path(&self) -> PathBuf {
        self.target_dir().join("chainloader")
    }

    pub fn chainloader_bin_path(&self) -> PathBuf {
        self.chainloader_elf_path().with_extension("bin")
    }

    pub fn linker_script_path(&self, module: &str) -> PathBuf {
        self.build_root
            .join("crates")
            .join(module)
            .join("src")
            .join("arch")
            .join("aarch64")
            .join("linker.ld")
    }

    pub fn rpi_firmware_dir(&self) -> PathBuf {
        self.build_root.join("target").join("firmware")
    }

    pub fn rustflags(&self, module: &str) -> String {
        let mut flags = "-Cforce-frame-pointers=yes -C symbol-mangling-version=v0".to_string();
        if module == "bootloader" {
            flags.push_str(&format!(
                " -Clink-arg=-r -Clink-arg=-T{}",
                self.linker_script_path(module).display(),
            ));
        } else if module == "kernel" {
            flags.push_str(&format!(
                " -Clink-arg=-T{} -Lnative={} -Clink-arg=-lbootloader",
                self.linker_script_path(module).display(),
                self.target_dir().display(),
            ));
        } else {
            flags.push_str(&format!(
                " -Clink-arg=-T{}",
                self.linker_script_path(module).display()
            ));
        }
        log::debug!("RUSTFLAGS={}", &flags);
        flags
    }

    pub fn cargo_args(&self, mode: &str, module: &str) -> Vec<String> {
        let mut cargo_args = vec![
            mode.to_string(),
            "--target".to_string(),
            self.target_json_path().to_string_lossy().into_owned(),
            "-p".to_string(),
            module.to_string(),
            "-Zbuild-std=core,compiler_builtins,alloc".to_string(),
            "-Zbuild-std-features=compiler-builtins-mem".to_string(),
        ];

        if self.profile == Profile::Release {
            cargo_args.push("--release".to_string());
        }

        cargo_args
    }

    pub fn build_bootloader(&self) -> anyhow::Result<()> {
        log::info!("Building bootloader with Cargo");

        cmd!(self.sh, "cargo")
            .args(self.cargo_args("build", "bootloader"))
            .env("RUSTFLAGS", self.rustflags("bootloader"))
            .run()?;

        log::info!("Bootloader build complete!");

        Ok(())
    }

    pub fn full_build_kernel(&self) -> anyhow::Result<()> {
        self.build_bootloader()?;

        log::info!("Building kernel with Cargo");

        cmd!(self.sh, "cargo")
            .args(self.cargo_args("build", "kernel"))
            .env("RUSTFLAGS", self.rustflags("kernel"))
            .run()?;

        let kernel_elf_path = self.kernel_elf_path();
        let kernel_bin_path = self.kernel_bin_path();
        let kernel_sym_path = self.kernel_sym_path();

        cmd!(
            self.sh,
            "llvm-objcopy --only-keep-debug {kernel_elf_path} {kernel_sym_path}"
        )
        .run()?;

        cmd!(
            self.sh,
            "llvm-objcopy -O binary --strip-all {kernel_elf_path} {kernel_bin_path}"
        )
        .run()?;

        log::info!("Kernel build complete!");

        Ok(())
    }

    pub fn build_chainloader_rpi(&self) -> anyhow::Result<()> {
        log::info!("Building chainloader with Cargo");

        cmd!(self.sh, "cargo")
            .args(self.cargo_args("build", "chainloader"))
            .env("RUSTFLAGS", self.rustflags("chainloader"))
            .run()?;

        let chainloader_elf_path = self.chainloader_elf_path();
        let chainloader_bin_path = self.chainloader_bin_path();
        cmd!(
            self.sh,
            "llvm-objcopy -O binary {chainloader_elf_path} {chainloader_bin_path}"
        )
        .run()?;

        log::info!("Chainloader build complete!");

        Ok(())
    }

    pub fn flash_chainloader_rpi(&self, device: &str) -> anyhow::Result<()> {
        log::info!("Copying chainloader to SD card device {device} (will sudo)");
        let chainloader_bin_path = self.chainloader_bin_path();

        cmd!(self.sh, "sudo umount {device}")
            .ignore_status()
            .run()?;

        self.copy_common(device)?;

        cmd!(
            self.sh,
            "sudo cp {chainloader_bin_path} /mnt/rpi-sd/kernel8.img"
        )
        .run()?;

        cmd!(self.sh, "sudo umount {device}").run()?;

        log::info!("Copy complete!");

        Ok(())
    }

    pub fn flash_kernel_rpi(&self, device: &str) -> anyhow::Result<()> {
        log::info!("Copying kernel to SD card device {device} (will sudo)");
        let kernel_bin_path = self.kernel_bin_path();

        cmd!(self.sh, "sudo umount {device}")
            .ignore_status()
            .run()?;

        cmd!(self.sh, "sudo cp {kernel_bin_path} /mnt/rpi-sd/kernel8.img").run()?;

        self.copy_common(device)?;

        cmd!(self.sh, "sudo umount {device}").run()?;

        log::info!("Copy complete!");

        Ok(())
    }

    fn copy_common(&self, device: &str) -> anyhow::Result<()> {
        let firmware_dir = self.rpi_firmware_dir();

        cmd!(self.sh, "sudo mkdir -p /mnt/rpi-sd").run()?;
        cmd!(self.sh, "sudo mount {device} /mnt/rpi-sd").run()?;
        cmd!(self.sh, "sudo rm -rf /mnt/rpi-sd/*").run()?;
        cmd!(self.sh, "sudo mkdir -p /mnt/rpi-sd/overlays").run()?;

        cmd!(self.sh, "sudo cp config.txt /mnt/rpi-sd/config.txt").run()?;
        cmd!(
            self.sh,
            "sudo cp {firmware_dir}/boot/start4.elf /mnt/rpi-sd/start4.elf"
        )
        .run()?;
        cmd!(
            self.sh,
            "sudo cp {firmware_dir}/boot/bootcode.bin /mnt/rpi-sd/bootcode.bin"
        )
        .run()?;
        cmd!(
            self.sh,
            "sudo cp {firmware_dir}/boot/fixup4.dat /mnt/rpi-sd/fixup4.dat"
        )
        .run()?;
        cmd!(
            self.sh,
            "sudo cp {firmware_dir}/boot/bcm2711-rpi-4-b.dtb /mnt/rpi-sd/bcm2711-rpi-4-b.dtb"
        )
        .run()?;
        cmd!(
            self.sh,
            "sudo cp {firmware_dir}/boot/overlays/disable-bt.dtbo /mnt/rpi-sd/overlays/disable-bt.dtbo"
        )
        .run()?;

        Ok(())
    }

    pub fn run_qemu_rpi(&self, debug_adapter: bool) -> anyhow::Result<()> {
        log::info!("Running QEMU");

        let kernel_arg = format!("{}", self.kernel_bin_path().display());
        let dtb_arg = format!(
            "{}",
            self.rpi_firmware_dir()
                .join("boot")
                .join("bcm2711-rpi-4-b.dtb")
                .display()
        );

        let mut qemu_args = vec![];

        qemu_args.extend([
            "-M",
            "raspi4b",
            "-cpu",
            "cortex-a72",
            "-kernel",
            &kernel_arg,
            "-dtb",
            &dtb_arg,
            "-D",
            "target/log.txt",
            "-d",
            "int,guest_errors",
            "-m",
            "2G",
            "-serial",
            "stdio",
            "-semihosting",
        ]);

        if debug_adapter {
            qemu_args.push("-s");
            qemu_args.push("-S");
        }

        cmd!(self.sh, "qemu-system-aarch64").args(qemu_args).run()?;

        Ok(())
    }

    pub fn build_dependencies_rpi(&self) -> anyhow::Result<()> {
        let firmware_dir = self.rpi_firmware_dir();

        log::info!("Building dependencies");

        if !firmware_dir.exists() {
            log::info!("Downloading RPi Firmware");
            cmd!(
                self.sh,
                "git clone --depth=1 https://github.com/raspberrypi/firmware.git {firmware_dir}"
            )
            .run()?;
        } else {
            let _guard = self.sh.push_dir(&firmware_dir);
            cmd!(self.sh, "git fetch").run()?;
        }

        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    let args = Args::parse();

    match args.mode {
        Mode::Build { release } => {
            let cx = Context::new(release)?;
            cx.full_build_kernel()?;
        }
        Mode::Debug { release } => {
            let cx = Context::new(release)?;
            cx.full_build_kernel()?;
            cx.build_dependencies_rpi()?;
            cx.run_qemu_rpi(true)?;
        }
        Mode::Run { release } => {
            let cx = Context::new(release)?;
            cx.full_build_kernel()?;
            cx.build_dependencies_rpi()?;
            cx.run_qemu_rpi(false)?;
        }
        Mode::Flash { device, release } => {
            let cx = Context::new(release)?;
            cx.full_build_kernel()?;
            cx.build_dependencies_rpi()?;
            cx.flash_kernel_rpi(device.as_str())?;
        }
        Mode::FlashChainloader { device } => {
            let cx = Context::new(true)?;
            cx.build_chainloader_rpi()?;
            cx.flash_chainloader_rpi(device.as_str())?;
        }
        Mode::Load { release } => {
            let cx = Context::new(release)?;
            cx.full_build_kernel()?;
            let kernel_bin_path = cx.kernel_bin_path();
            let kernel_sym_path = cx.kernel_sym_path();
            cmd!(
                cx.sh,
                "cargo loader client {kernel_bin_path} --symbol-path {kernel_sym_path}"
            )
            .run()?;
        }
    }

    Ok(())
}
