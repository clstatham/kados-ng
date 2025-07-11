#!/usr/bin/env python3
import json, subprocess, sys, pathlib

if __name__ == "__main__":
    meta = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            text=True,
        )
    )
    workspace_root = pathlib.Path(meta["workspace_root"])
    members = [pkg for pkg in meta["packages"] if pkg["id"] in meta["workspace_members"]]

    # check if we're on windows, linux, or macOS
    if sys.platform.startswith("win"):
        system_arch = "x86_64-pc-windows-msvc"
    elif sys.platform.startswith("linux"):
        system_arch = "x86_64-unknown-linux-gnu"
    elif sys.platform.startswith("darwin"):
        system_arch = "x86_64-apple-darwin"
    else:
        print(f"Unsupported platform: {sys.platform}")
        sys.exit(1)

    TARGETS = {
        "builder": system_arch,
        "loader": system_arch,
        "kernel": "aarch64-unknown-none",
        "bootloader": "aarch64-unknown-none",
        "chainloader": "aarch64-unknown-none",
    }

    for pkg in members:
        name = pkg["name"]
        cmd = ["cargo", "clippy", "-p", name, "--message-format=json"]
        if name in TARGETS:
            cmd += ["--target", TARGETS[name]]

        with subprocess.Popen(cmd, cwd=workspace_root, text=True, stdout=subprocess.PIPE) as proc:
            for line in proc.stdout:
                try:
                    obj = json.loads(line)
                    obj["workspace_package"] = name
                    print(json.dumps(obj))
                except json.JSONDecodeError:
                    continue
            if proc.wait() != 0:
                sys.exit(proc.returncode)
