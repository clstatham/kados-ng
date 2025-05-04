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
    #[clap(name = "x86_64")]
    X86_64,
}

impl Target {
    pub fn target_dir(&self) -> &'static str {
        match self {
            Self::AArch64 => "aarch64-kados",
            Self::X86_64 => "x86_64-kados",
        }
    }

    pub fn arch_dir(&self) -> PathBuf {
        match self {
            Self::AArch64 => PathBuf::from("arch").join("aarch64"),
            Self::X86_64 => PathBuf::from("arch").join("x86_64"),
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

    pub fn iso_root_dir(&self) -> PathBuf {
        self.target_dir().join("iso_root")
    }

    pub fn iso_path(&self) -> PathBuf {
        self.target_dir().join("kernel.iso")
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
            &self.target_json_path().to_string_lossy(),
            "-p",
            "kernel",
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
            .args(self.cargo_args("build"))
            .env("RUSTFLAGS", self.rustflags())
            .run()?;

        match self.target {
            Target::AArch64 => {
                self.create_new_image_rpi()?;

                self.build_dependencies_rpi()?;

                self.copy_files_to_image_rpi()?;
            }
            Target::X86_64 => {
                self.build_dependencies_pc()?;
                self.create_iso_pc()?;
            }
        }

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

        match self.target {
            Target::AArch64 => {
                self.create_new_image_rpi()?;

                self.build_dependencies_rpi()?;

                self.copy_files_to_image_rpi()?;
            }
            Target::X86_64 => {
                self.build_dependencies_pc()?;
                self.create_iso_pc()?;
            }
        }

        log::info!("Kernel tests build complete!");

        Ok(())
    }

    pub fn run_qemu(&self, debug_adapter: bool) -> anyhow::Result<()> {
        match self.target {
            Target::AArch64 => self.run_qemu_rpi(debug_adapter),
            Target::X86_64 => self.run_qemu_pc(debug_adapter),
        }
    }

    pub fn flash_rpi(&self, device: &str) -> anyhow::Result<()> {
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

    pub fn run_qemu_rpi(&self, debug_adapter: bool) -> anyhow::Result<()> {
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

    fn create_new_image_rpi(&self) -> anyhow::Result<()> {
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

    fn build_dependencies_rpi(&self) -> anyhow::Result<()> {
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

    fn copy_files_to_image_rpi(&self) -> anyhow::Result<()> {
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

    fn build_dependencies_pc(&self) -> anyhow::Result<()> {
        let limine_dir = self.limine_dir();

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

        {
            let _dir = self.sh.push_dir(self.limine_dir());
            cmd!(self.sh, "make").run()?;
        }

        Ok(())
    }

    fn create_iso_pc(&self) -> anyhow::Result<()> {
        log::info!("Creating ISO image");

        std::fs::create_dir_all(self.iso_root_dir())?;

        cmd!(self.sh, "cp")
            .arg(self.kernel_elf_path())
            .arg("limine.conf")
            .arg(self.limine_dir().join("limine-bios.sys"))
            .arg(self.limine_dir().join("limine-bios-cd.bin"))
            .arg(self.limine_dir().join("limine-uefi-cd.bin"))
            .arg(self.iso_root_dir())
            .run()?;

        cmd!(self.sh, "mv")
            .arg(
                self.iso_root_dir()
                    .join(self.kernel_elf_path.file_name().unwrap()),
            )
            .arg(self.iso_root_dir().join("kados.elf"))
            .run()?;

        cmd!(self.sh, "xorriso")
            .arg("-as")
            .arg("mkisofs")
            .arg("-b")
            .arg("limine-bios-cd.bin")
            .arg("-no-emul-boot")
            .arg("-boot-load-size")
            .arg("4")
            .arg("-boot-info-table")
            .arg("--efi-boot")
            .arg("limine-uefi-cd.bin")
            .arg("-efi-boot-part")
            .arg("--efi-boot-image")
            .arg("--protective-msdos-label")
            .arg(self.iso_root_dir())
            .arg("-o")
            .arg(self.iso_path())
            .run()?;

        let limine_deploy = self.limine_dir().join("limine");
        cmd!(self.sh, "{limine_deploy}")
            .arg("bios-install")
            .arg(self.iso_path())
            .run()?;

        Ok(())
    }

    fn run_qemu_pc(&self, debug_adapter: bool) -> anyhow::Result<()> {
        log::info!("Running QEMU");

        let qemu_cdrom_arg = format!("{}", self.iso_path().display());
        let mut qemu_args = vec![
            "-M",
            "q35",
            "-cpu",
            "EPYC",
            "-D",
            "target/log.txt",
            "-d",
            "int,guest_errors",
            "-no-reboot",
            "-no-shutdown",
            "-m",
            "4G",
            "-serial",
            "stdio",
            "-cdrom",
            &qemu_cdrom_arg,
        ];

        if debug_adapter {
            qemu_args.push("-s");
            qemu_args.push("-S");
        }

        cmd!(self.sh, "qemu-system-x86_64").args(qemu_args).run()?;

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
