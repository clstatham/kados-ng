use std::{path::PathBuf, process::Command};

use clap::Parser;

#[derive(Parser)]
#[clap(about = "Build the kernel and run it in QEMU")]
struct Args {
    #[clap(short, long, default_value_t = false)]
    run: bool,

    #[clap(short, long, default_value_t = false)]
    debug: bool,
}

fn main() {
    let args = Args::parse();

    let arch = "aarch64";
    let target = "aarch64-kados";

    let arch_dir = PathBuf::from("arch").join(arch);

    let target_file = arch_dir.join(format!("{target}.json"));
    let target_dir = PathBuf::from("target").join(target);

    let linker_script = arch_dir.join("linker.ld");

    let rustflags = format!(
        "-C linker=rust-lld -C link-arg=-T{}",
        linker_script.display()
    );

    let status = Command::new("cargo")
        .args([
            "build",
            "--target",
            target_file.display().to_string().as_str(),
            "-p",
            "kernel",
            "-Zbuild-std=core,compiler_builtins",
            "-Zbuild-std-features=compiler-builtins-mem",
        ])
        .env("RUSTFLAGS", rustflags)
        .status()
        .expect("Failed to execute cargo");

    if !status.success() {
        std::process::exit(1);
    }

    let out_dir = target_dir.join("debug");
    let disk_dir = out_dir.join("disk");
    let kernel_elf_path = out_dir.join("kernel");
    // let kernel_img_path = disk_dir.join("kernel8.img");
    // let boot_cmd_path = out_dir.join("boot.cmd");
    // let boot_scr_path = disk_dir.join("boot.scr");

    // let mkimage_args = [
    //     "-A",
    //     "arm64",
    //     "-O",
    //     "linux",
    //     "-T",
    //     "script",
    //     "-C",
    //     "none",
    //     "-a",
    //     "0x0",
    //     "-e",
    //     "0x0",
    //     "-n",
    //     "kados boot script",
    //     "-d",
    //     boot_cmd_path.to_str().unwrap(),
    //     boot_scr_path.to_str().unwrap(),
    // ];

    // let disk_qemu_arg = format!(
    //     "if=virtio,file=fat:rw:{},format=raw,id=hd0",
    //     disk_dir.display()
    // );
    let mut qemu_args = vec![
        "-M",
        "virt",
        "-cpu",
        "cortex-a53",
        "-m",
        "512M",
        "-serial",
        "stdio",
        "-semihosting",
        "-kernel",
        kernel_elf_path.to_str().unwrap(),
        // "-bios",
        // "arch/aarch64/u-boot/u-boot.bin",
        // "-drive",
        // &disk_qemu_arg,
    ];
    if args.debug {
        qemu_args.push("-s");
        qemu_args.push("-S");
        qemu_args.push("-d");
        qemu_args.push("int");
    }

    // Create the disk directory if it doesn't exist
    std::fs::create_dir_all(&disk_dir).expect("Failed to create disk directory");
    // // Copy the kernel binary to the disk directory and strip the ELF header
    // let status = Command::new("llvm-objcopy")
    //     .args([
    //         "--output-target=binary",
    //         kernel_elf_path.to_str().unwrap(),
    //         kernel_img_path.to_str().unwrap(),
    //     ])
    //     .status()
    //     .expect("Failed to copy kernel binary");
    // if !status.success() {
    //     eprintln!("Failed to copy kernel binary");
    //     std::process::exit(1);
    // }

    // // Create the boot.cmd file
    // std::fs::write(&boot_cmd_path, include_str!("boot.cmd"))
    //     .expect("Failed to write boot.cmd");
    // // Create the boot.scr file
    // let status = Command::new("mkimage")
    //     .args(mkimage_args)
    //     .status()
    //     .expect("Failed to create boot.scr");
    // if !status.success() {
    //     eprintln!("Failed to create boot.scr");
    //     std::process::exit(1);
    // }

    if args.run {
        // Run the kernel using QEMU
        let status = Command::new("qemu-system-aarch64")
            .args(qemu_args)
            .status()
            .expect("Failed to run QEMU: Command failed");
        eprintln!("QEMU exited with {status}");
    }
}
