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
    /// Flash the built image to an SD card for the Raspberry Pi
    Flash {
        /// Device to flash to (e.g. /dev/sdb)
        device: String,
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

    /// Target to build for
    #[clap(long)]
    target: Target,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Target {
    #[clap(name = "aarch64")]
    AArch64,
}

impl Target {
    pub fn target_dir(&self) -> &'static str {
        match self {
            Self::AArch64 => "aarch64-kados",
        }
    }

    pub fn arch_dir(&self) -> PathBuf {
        match self {
            Self::AArch64 => PathBuf::from("arch").join("aarch64"),
        }
    }
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

#[macro_export]
macro_rules! args {
    ($($val:expr),*) => {{
        let mut args = Vec::<String>::new();
        $crate::extend!(args <- $($val),*);
        args
    }};
}

#[macro_export]
macro_rules! extend {
    ($args:ident <- $($val:expr),*) => {{
        $(
            $args.push(AsRef::<str>::as_ref($val).to_string());
        )*
    }};
}

pub struct Context {
    sh: Shell,
    target: Target,
    profile: Profile,
    build_root: PathBuf,
    kernel_elf_path: PathBuf,
}

impl Context {
    pub fn new(target: Target, release: bool) -> anyhow::Result<Self> {
        let mut this = Self {
            sh: Shell::new()?,
            target,
            profile: if release {
                Profile::Release
            } else {
                Profile::Debug
            },
            build_root: env!("CARGO_MANIFEST_DIR").parse::<PathBuf>()?.join(".."), // ./xtask/.. = ./
            kernel_elf_path: PathBuf::new(),
        };

        this.kernel_elf_path = this.target_dir().join("kernel"); // default kernel elf

        Ok(this)
    }

    pub fn target_dir(&self) -> PathBuf {
        self.build_root
            .join("target")
            .join(self.target.target_dir())
            .join(self.profile.to_string())
    }

    pub fn arch_dir(&self) -> PathBuf {
        self.target.arch_dir()
    }

    pub fn target_json_path(&self) -> PathBuf {
        self.arch_dir()
            .join(self.target.target_dir())
            .with_extension("json")
    }

    pub fn kernel_elf_path(&self) -> PathBuf {
        self.kernel_elf_path.clone()
    }

    pub fn kernel_bin_path(&self) -> PathBuf {
        self.kernel_elf_path().with_extension("bin")
    }

    pub fn kernel_img_path(&self) -> PathBuf {
        self.kernel_elf_path().with_extension("img")
    }

    pub fn linker_script_path(&self, module: &str) -> PathBuf {
        self.build_root
            .join("crates")
            .join(module)
            .join("src")
            .join(self.target.arch_dir())
            .join("linker.ld")
    }

    // pub fn uboot_dir(&self) -> PathBuf {
    //     self.build_root.join("target").join("u-boot")
    // }

    // pub fn limine_dir(&self) -> PathBuf {
    //     self.build_root.join("target").join("limine")
    // }

    pub fn rpi_firmware_dir(&self) -> PathBuf {
        self.build_root.join("target").join("firmware")
    }

    // pub fn iso_root_dir(&self) -> PathBuf {
    //     self.target_dir().join("iso_root")
    // }

    // pub fn iso_path(&self) -> PathBuf {
    //     self.target_dir().join("kernel.iso")
    // }

    pub fn rustflags(&self, module: &str) -> String {
        format!(
            "-C link-arg=-T{} -Cforce-frame-pointers=yes -C symbol-mangling-version=v0",
            self.linker_script_path(module).display(),
        )
    }

    pub fn cargo_args(&self, mode: &str, module: &str) -> Vec<String> {
        let mut cargo_args = args!(
            mode,
            "--target",
            &self.target_json_path().to_string_lossy(),
            "-p",
            module,
            "-Zbuild-std=core,compiler_builtins,alloc",
            "-Zbuild-std-features=compiler-builtins-mem"
        );

        if self.profile == Profile::Release {
            extend!(cargo_args <- "--release");
        }

        cargo_args
    }

    pub fn full_build_kernel(&self) -> anyhow::Result<()> {
        log::info!("Building kernel with Cargo");

        cmd!(self.sh, "cargo")
            .args(self.cargo_args("build", "kernel"))
            .env("RUSTFLAGS", self.rustflags("kernel"))
            .run()?;

        match self.target {
            Target::AArch64 => {
                let kernel_elf_path = self.kernel_elf_path();
                let kernel_bin_path = self.kernel_bin_path();
                cmd!(
                    self.sh,
                    "llvm-objcopy -O binary {kernel_elf_path} {kernel_bin_path}"
                )
                .run()?;

                self.create_new_image_rpi()?;

                self.build_dependencies_rpi()?;

                self.copy_files_to_image_rpi()?;
            }
        }

        log::info!("Kernel build complete!");

        Ok(())
    }

    pub fn full_build_kernel_test(&mut self) -> anyhow::Result<()> {
        log::info!("Building kernel tests with Cargo");

        let cargo_output = cmd!(self.sh, "cargo")
            .args(self.cargo_args("test", "kernel"))
            .arg("--no-run")
            .ignore_status()
            .output()?;

        let cargo_stdout = String::from_utf8_lossy(&cargo_output.stdout);
        let cargo_stderr = String::from_utf8_lossy(&cargo_output.stderr);

        println!("{cargo_stdout}");

        if !cargo_output.status.success() {
            eprintln!("{cargo_stderr}");
            log::error!("Cargo command failed: {:?}", cargo_output.status);
            anyhow::bail!("Cargo command failed");
        }

        // override self.kernel_elf_path with the test elf

        let new_kernel_elf = cargo_stderr
            .split("/deps/")
            .last()
            .unwrap()
            .trim()
            .strip_suffix(')')
            .unwrap()
            .to_string();
        self.kernel_elf_path = self.target_dir().join("deps").join(new_kernel_elf);

        // proceed with build as normal

        match self.target {
            Target::AArch64 => {
                self.create_new_image_rpi()?;

                self.build_dependencies_rpi()?;

                self.copy_files_to_image_rpi()?;
            }
        }

        log::info!("Kernel tests build complete!");

        Ok(())
    }

    pub fn run_qemu(&self, debug_adapter: bool) -> anyhow::Result<()> {
        match self.target {
            Target::AArch64 => self.run_qemu_rpi(debug_adapter),
        }
    }

    pub fn flash_rpi(&self, device: &str) -> anyhow::Result<()> {
        self.full_build_kernel()?;

        log::info!("Copying to SD card device {device} (will sudo)");
        let kernel_elf_path = self.kernel_elf_path();
        let kernel_bin_path = self.kernel_bin_path();
        let firmware_dir = self.rpi_firmware_dir();

        cmd!(self.sh, "sudo umount {device}")
            .ignore_status()
            .run()?;

        cmd!(self.sh, "sudo mkdir -p /mnt/rpi-sd").run()?;
        cmd!(self.sh, "sudo mount {device} /mnt/rpi-sd").run()?;
        cmd!(self.sh, "sudo rm -rf /mnt/rpi-sd/*").run()?;
        cmd!(self.sh, "sudo mkdir -p /mnt/rpi-sd/overlays").run()?;

        cmd!(
            self.sh,
            "llvm-objcopy -O binary {kernel_elf_path} {kernel_bin_path}"
        )
        .run()?;

        cmd!(self.sh, "sudo cp {kernel_bin_path} /mnt/rpi-sd/kernel8.img").run()?;
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

        cmd!(self.sh, "sudo umount {device}").run()?;

        log::info!("Copy complete!");

        Ok(())
    }

    pub fn run_qemu_rpi(&self, debug_adapter: bool) -> anyhow::Result<()> {
        log::info!("Running QEMU");

        let qemu_drive_arg = format!("file={},if=sd,format=raw", self.kernel_img_path().display());

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
            "-drive",
            &qemu_drive_arg,
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

    fn create_new_image_rpi(&self) -> anyhow::Result<()> {
        log::info!("Creating new SD card image");

        let kernel_img_path = self.kernel_img_path();

        if kernel_img_path.exists() {
            std::fs::remove_file(&kernel_img_path)?;
        }

        cmd!(self.sh, "truncate -s 128M {kernel_img_path}").run()?;
        cmd!(self.sh, "mformat -i {kernel_img_path} ::").run()?;
        cmd!(self.sh, "mmd -i {kernel_img_path} ::/overlays").run()?;

        Ok(())
    }

    fn build_dependencies_rpi(&self) -> anyhow::Result<()> {
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

    fn copy_files_to_image_rpi(&self) -> anyhow::Result<()> {
        log::info!("Copying files to SD card");

        let kernel_elf_path = self.kernel_elf_path();
        let kernel_bin_path = self.kernel_bin_path();
        let kernel_img_path = self.kernel_img_path();
        let firmware_dir = self.rpi_firmware_dir();

        cmd!(
            self.sh,
            "llvm-objcopy -O binary {kernel_elf_path} {kernel_bin_path}"
        )
        .run()?;

        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} {kernel_bin_path} ::/kernel8.img"
        )
        .run()?;

        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} config.txt ::/config.txt"
        )
        .run()?;

        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} {firmware_dir}/boot/bootcode.bin ::/bootcode.bin"
        )
        .run()?;
        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} {firmware_dir}/boot/start4.elf ::/start4.elf"
        )
        .run()?;
        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} {firmware_dir}/boot/fixup4.dat ::/fixup4.dat"
        )
        .run()?;
        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} {firmware_dir}/boot/bcm2711-rpi-4-b.dtb ::/bcm2711-rpi-4-b.dtb"
        )
        .run()?;

        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} {firmware_dir}/boot/overlays/disable-bt.dtbo ::/overlays/disable-bt.dtbo"
        )
        .run()?;

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
            let cx = Context::new(args.target, release)?;
            cx.full_build_kernel()?;
        }
        Mode::Debug { release } => {
            let cx = Context::new(args.target, release)?;
            cx.full_build_kernel()?;
            cx.run_qemu(true)?;
        }
        Mode::Run { release } => {
            let cx = Context::new(args.target, release)?;
            cx.full_build_kernel()?;
            cx.run_qemu(false)?;
        }
        Mode::Flash { device, release } => {
            let cx = Context::new(args.target, release)?;
            cx.flash_rpi(device.as_str())?;
        }
    }

    Ok(())
}
