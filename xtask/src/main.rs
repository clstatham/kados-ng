use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use xshell::{Shell, cmd};

#[derive(Subcommand, Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Build the kernel for a Raspberry Pi 4b
    Build {
        #[clap(short, long, default_value_t = false)]
        release: bool,
    },
    /// Build the kernel for a Raspberry Pi 4b and emulate it in QEMU
    Run {
        #[clap(short, long, default_value_t = false)]
        release: bool,
    },
    /// Build the kernel for a Raspberry Pi 4b and run it in QEMU with debug options (gdbserver)
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
#[clap(about = "Build the kernel and run it in QEMU")]
pub struct Args {
    /// Mode of operation
    #[command(subcommand)]
    mode: Mode,

    /// Extra args to pass to qemu
    #[clap(long, value_parser)]
    extra_qemu_args: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
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

#[derive(Debug, Default)]
struct ArgVec(Vec<String>);

impl ArgVec {
    pub fn push(&mut self, val: impl AsRef<Path>) {
        self.0.push(val.as_ref().to_string_lossy().into_owned());
    }

    pub fn into_inner(self) -> Vec<String> {
        self.0
    }
}

impl IntoIterator for ArgVec {
    type IntoIter = std::vec::IntoIter<String>;
    type Item = String;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[macro_export]
macro_rules! args {
    ($($val:expr),*) => {{
        let mut args = ArgVec::default();
        $crate::extend!(args <- $($val),*);
        args.into_inner()
    }};
}

#[macro_export]
macro_rules! extend {
    ($args:ident <- $($val:expr),*) => {{
        $(
            $args.push($val);
        )*
    }};
}

pub fn banner() {
    log::info!("{}", "=".repeat(80))
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

    pub fn kernel_img_path(&self) -> PathBuf {
        self.kernel_elf_path().with_extension("img")
    }

    pub fn linker_script_path(&self) -> PathBuf {
        self.arch_dir().join("linker.ld")
    }

    pub fn uboot_dir(&self) -> PathBuf {
        self.target_dir().join("u-boot")
    }

    pub fn limine_dir(&self) -> PathBuf {
        self.target_dir().join("limine")
    }

    pub fn rpi_firmware_dir(&self) -> PathBuf {
        self.target_dir().join("firmware")
    }

    pub fn rustflags(&self) -> String {
        format!(
            "-C link-arg=-T{} -Cforce-frame-pointers=yes -C symbol-mangling-version=v0",
            self.linker_script_path().display(),
        )
    }

    pub fn cargo_args(&self, mode: &str) -> Vec<String> {
        let mut cargo_args = args!(
            mode,
            "--target",
            self.target_json_path(),
            "-p",
            "kernel",
            "-Zbuild-std=core,compiler_builtins,alloc",
            "-Zbuild-std-features=compiler-builtins-mem"
        );

        if self.profile == Profile::Release {
            cargo_args.push("--release".to_string());
        }

        cargo_args
    }

    pub fn full_build_kernel(&self) -> anyhow::Result<()> {
        log::info!("Building kernel with Cargo");

        cmd!(self.sh, "cargo")
            .args(self.cargo_args("build"))
            .env("RUSTFLAGS", self.rustflags())
            .run()?;

        self.create_new_image()?;

        self.build_dependencies()?;

        self.copy_files_to_image()?;

        log::info!("Kernel build complete!");

        Ok(())
    }

    pub fn full_build_kernel_test(&mut self) -> anyhow::Result<()> {
        log::info!("Building kernel tests with Cargo");

        let cargo_output = cmd!(self.sh, "cargo")
            .args(self.cargo_args("test"))
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

        self.create_new_image()?;

        self.build_dependencies()?;

        self.copy_files_to_image()?;

        log::info!("Kernel tests build complete!");

        Ok(())
    }

    pub fn flash(&self, device: &str) -> anyhow::Result<()> {
        self.full_build_kernel()?;

        log::info!("Flashing SD card image to {device} (will sudo)");
        let kernel_img_path = self.kernel_img_path();

        cmd!(
            self.sh,
            "sudo dd if={kernel_img_path} of={device} bs=4M status=progress"
        )
        .run()?;
        cmd!(self.sh, "sync").run()?;

        log::info!("Flash complete!");

        Ok(())
    }

    pub fn run_qemu(&self, debug_adapter: bool) -> anyhow::Result<()> {
        log::info!("Running QEMU");

        let qemu_drive_arg = format!("if=sd,format=raw,file={}", self.kernel_img_path().display());

        let uboot_kernel_arg = format!("{}", self.uboot_dir().join("u-boot.bin").display());
        let uboot_dtb_arg = format!(
            "{}",
            self.uboot_dir()
                .join("arch")
                .join("arm")
                .join("dts")
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
            &uboot_kernel_arg,
            "-dtb",
            &uboot_dtb_arg,
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

    fn create_new_image(&self) -> anyhow::Result<()> {
        log::info!("Creating new SD card image");

        let kernel_img_path = self.kernel_img_path();

        if kernel_img_path.exists() {
            std::fs::remove_file(&kernel_img_path)?;
        }

        cmd!(self.sh, "truncate -s 128M {kernel_img_path}").run()?;
        cmd!(self.sh, "mformat -i {kernel_img_path} ::").run()?;
        cmd!(self.sh, "mmd -i {kernel_img_path} ::/EFI").run()?;
        cmd!(self.sh, "mmd -i {kernel_img_path} ::/EFI/BOOT").run()?;

        Ok(())
    }

    fn build_dependencies(&self) -> anyhow::Result<()> {
        let limine_dir = self.limine_dir();
        let uboot_dir = self.uboot_dir();
        let firmware_dir = self.rpi_firmware_dir();

        log::info!("Building dependencies");

        if !limine_dir.exists() {
            log::info!("Downloading Limine");
            cmd!(
                self.sh,
                "git clone https://github.com/limine-bootloader/limine.git --depth=1 --branch v9.x-binary {limine_dir}"
            )
            .run()?;
        } else {
            let _guard = self.sh.push_dir(&limine_dir);
            cmd!(self.sh, "git fetch").run()?;
        }

        if !uboot_dir.exists() {
            log::info!("Downloading U-Boot");
            cmd!(
                self.sh,
                "git clone https://github.com/u-boot/u-boot.git {uboot_dir}"
            )
            .run()?;
        } else {
            let _guard = self.sh.push_dir(&uboot_dir);
            cmd!(self.sh, "git fetch").run()?;
        }

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

        {
            log::info!("Building U-Boot");
            let _uboot = self.sh.push_dir(&uboot_dir);
            cmd!(self.sh, "make rpi_4_defconfig").run()?;
            let nproc = num_cpus::get().to_string();
            cmd!(self.sh, "make -j{nproc} CROSS_COMPILE=aarch64-none-elf-").run()?;
        }

        Ok(())
    }

    fn copy_files_to_image(&self) -> anyhow::Result<()> {
        log::info!("Copying files to SD card image");

        let kernel_elf_path = self.kernel_elf_path();
        let kernel_img_path = self.kernel_img_path();
        let limine_dir = self.limine_dir();
        let uboot_dir = self.uboot_dir();
        let firmware_dir = self.rpi_firmware_dir();

        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} {uboot_dir}/u-boot.bin ::/u-boot.bin"
        )
        .run()?;

        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} {limine_dir}/BOOTAA64.EFI ::/EFI/BOOT/BOOTAA64.EFI"
        )
        .run()?;
        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} {kernel_elf_path} ::/kados.elf"
        )
        .run()?;
        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} limine.conf ::/limine.conf"
        )
        .run()?;

        cmd!(
            self.sh,
            "mcopy -i {kernel_img_path} config.txt ::/config.txt"
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
            let cx = Context::new(Target::AArch64, release)?;
            cx.full_build_kernel()?;
        }
        Mode::Debug { release } => {
            let cx = Context::new(Target::AArch64, release)?;
            cx.full_build_kernel()?;
            cx.run_qemu(true)?;
        }
        Mode::Run { release } => {
            let cx = Context::new(Target::AArch64, release)?;
            cx.full_build_kernel()?;
            cx.run_qemu(false)?;
        }
        Mode::Flash { device, release } => {
            let cx = Context::new(Target::AArch64, release)?;
            cx.flash(device.as_str())?;
        }
    }

    Ok(())
}
