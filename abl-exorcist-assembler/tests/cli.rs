#![cfg(feature = "std")]

use std::{fs, path::PathBuf, process::Command};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_abl-exorcist-assembler"))
}

#[test]
fn help_and_version_succeed_without_reading_inputs() {
    for args in [vec!["--help"], vec!["-h"], vec!["--ramdisk", "--help"]] {
        let output = command().args(args).output().unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        for text in [
            "Usage:",
            "<KERNEL>",
            "<SHIM_OR_INITRD>",
            "--ramdisk",
            "--version",
        ] {
            assert!(help.contains(text), "missing {text}: {help}");
        }
    }
    for flag in ["--version", "-V"] {
        let output = command().arg(flag).output().unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            concat!("abl-exorcist-assembler ", env!("CARGO_PKG_VERSION"), "\n")
        );
    }
}

#[test]
fn invalid_arguments_fail_without_writing_binary_output() {
    for args in [
        vec![],
        vec!["kernel"],
        vec!["--ramdisk"],
        vec!["--ramdisk", "kernel"],
        vec!["kernel", "shim", "extra"],
        vec!["--ramdisk", "kernel", "initrd", "extra"],
        vec!["--unknown", "kernel", "shim"],
    ] {
        let output = command().args(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("error:"), "{error}");
        assert!(error.contains("Usage:"), "{error}");
    }
}

struct Inputs(PathBuf);

impl Drop for Inputs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn both_modes_preserve_binary_output_and_accept_escaped_paths() {
    let inputs = Inputs(std::env::temp_dir().join(format!(
        "ablx-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    fs::create_dir(&inputs.0).unwrap();
    // Exercise spaces, leading hyphens, and (on Unix) non-UTF-8 file names.
    #[cfg(unix)]
    let kernel_name = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(b"-kernel \xff".to_vec())
    };
    #[cfg(not(unix))]
    let kernel_name = std::ffi::OsString::from("-kernel image");
    let mut kernel = vec![0; 256];
    kernel[16..24].copy_from_slice(&0x2000_u64.to_le_bytes());
    kernel[56..60].copy_from_slice(b"ARM\x64");
    let mut shim = vec![0; 128];
    shim[16..24].copy_from_slice(&0x1000_u64.to_le_bytes());
    shim[56..60].copy_from_slice(b"ARM\x64");
    let initrd = b"initramfs bytes";
    fs::write(inputs.0.join(&kernel_name), &kernel).unwrap();
    fs::write(inputs.0.join("shim image"), &shim).unwrap();
    fs::write(inputs.0.join("initrd image"), initrd).unwrap();

    for ramdisk in [false, true] {
        let mut cmd = command();
        cmd.current_dir(&inputs.0);
        if ramdisk {
            cmd.arg("--ramdisk");
        }
        let output = cmd
            .arg("--")
            .arg(&kernel_name)
            .arg(if ramdisk {
                "initrd image"
            } else {
                "shim image"
            })
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output.stderr);
        assert!(output.stderr.is_empty());
        let expected = if ramdisk {
            abl_exorcist_assembler::assemble_ramdisk(&kernel, initrd)
        } else {
            abl_exorcist_assembler::assemble(&kernel, &shim)
        }
        .unwrap();
        assert_eq!(output.stdout, expected);
    }
}
