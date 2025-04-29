use std::{path::PathBuf, process::Command};

use clap::Parser;

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

fn main() {
    let args = Args::parse();

    let arch = "aarch64";
    let target = format!("{arch}-kados");

    let base_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("..");

    let arch_dir = base_dir.join(PathBuf::from("arch")).join(arch);

    let target_file = arch_dir.join(format!("{target}.json"));
    let target_file_str = target_file.display().to_string();
    let target_dir = PathBuf::from("target").join(target);

    let linker_script = arch_dir.join("linker.ld");

    let out_dir = target_dir.join("debug");
    let kernel_elf_path = args
        .kernel_path
        .map(PathBuf::from)
        .unwrap_or_else(|| out_dir.join("kernel"));

    let rustflags = format!(
        "-C linker=rust-lld -C link-arg=-T{}",
        linker_script.display()
    );

    let mut cargo_args = vec![];

    if args.mode == Mode::Test {
        cargo_args.push("test");
        cargo_args.push("--no-run");
    } else {
        cargo_args.push("build");
    }

    cargo_args.extend([
        "--target",
        target_file_str.as_str(),
        "-p",
        "kernel",
        "-Zbuild-std=core,compiler_builtins",
        "-Zbuild-std-features=compiler-builtins-mem",
    ]);

    let output = Command::new("cargo")
        .args(cargo_args)
        .env("RUSTFLAGS", &rustflags)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to execute cargo")
        .wait_with_output()
        .unwrap();

    if !output.status.success() {
        eprintln!("Cargo build failed");
        std::process::exit(1);
    }

    if args.mode == Mode::Test {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let test_executable = stderr
            .split("/deps/")
            .last()
            .unwrap()
            .trim()
            .strip_suffix(')')
            .unwrap();

        let test_executable = target_dir.join("debug").join("deps").join(test_executable);
        // kernel_elf_path = test_executable;

        let status = Command::new("cargo")
            .args([
                "xtask",
                "test-runner",
                "--kernel-path",
                test_executable.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        if !status.success() {
            eprintln!("Test runner failed");
            std::process::exit(1);
        }
        return;
    }

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

    match args.mode {
        Mode::Run => {
            // No additional arguments needed for run mode
            run_qemu(&qemu_args);
        }
        Mode::Debug => {
            qemu_args.push("-s");
            qemu_args.push("-S");
            // Add debug-specific arguments if needed
            run_qemu(&qemu_args);
        }
        Mode::TestRunner => {
            // Add test-specific arguments if needed
            run_qemu(&qemu_args);
        }
        Mode::Test => {}
    }
}

fn run_qemu(args: &[&str]) {
    let status = Command::new("qemu-system-aarch64")
        .args(args)
        .status()
        .expect("Failed to run QEMU");
    if !status.success() {
        eprintln!("QEMU exited with {status}");
    }
}
