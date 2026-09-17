use std::{
    env,
    ffi::OsString,
    fs,
    io::{self, BufWriter, Write},
    path::Path,
    process::ExitCode,
};

use abl_exorcist_assembler::{AblxMode, bootimg};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let first = args
        .next()
        .ok_or_else(|| usage("missing kernel image path or mode"))?;
    match first.to_str() {
        Some("--ramdisk") => return run_ramdisk(args),
        Some("payload") => return run_payload(args),
        Some("bootimg") => return run_bootimg(args),
        _ => {}
    }

    let kernel = first;
    let shim = args
        .next()
        .ok_or_else(|| usage("missing abl-exorcist image path"))?;
    if args.next().is_some() {
        return Err(usage("too many arguments"));
    }

    let kernel = read_file(&kernel)?;
    let shim = read_file(&shim)?;
    let kernel =
        abl_exorcist_assembler::canonicalize_kernel(&kernel).map_err(|err| err.to_string())?;
    let assembled =
        abl_exorcist_assembler::assemble(&kernel, &shim).map_err(|err| err.to_string())?;

    write_stdout(&assembled, "assembled image")
}

fn run_ramdisk(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let kernel = args
        .next()
        .ok_or_else(|| usage("missing kernel image path"))?;
    let initrd = args
        .next()
        .ok_or_else(|| usage("missing initrd image path"))?;
    if args.next().is_some() {
        return Err(usage("too many arguments"));
    }

    let kernel = read_file(&kernel)?;
    let initrd = read_file(&initrd)?;
    let kernel =
        abl_exorcist_assembler::canonicalize_kernel(&kernel).map_err(|err| err.to_string())?;
    let assembled = abl_exorcist_assembler::assemble_ramdisk(&kernel, &initrd)
        .map_err(|err| err.to_string())?;

    write_stdout(&assembled, "ramdisk container")
}

fn run_payload(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let mut mode = None;
    let mut kernel = None;
    let mut shim = None;
    let mut initrd = None;
    let mut kernel_out = None;
    let mut ramdisk_out = None;

    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--mode") => mode = Some(take(&mut args, "--mode")?),
            Some("--kernel") => kernel = Some(take(&mut args, "--kernel")?),
            Some("--shim") => shim = Some(take(&mut args, "--shim")?),
            Some("--initrd") => initrd = Some(take(&mut args, "--initrd")?),
            Some("--kernel-out") => kernel_out = Some(take(&mut args, "--kernel-out")?),
            Some("--ramdisk-out") => ramdisk_out = Some(take(&mut args, "--ramdisk-out")?),
            Some(other) => {
                return Err(usage(&format!("unexpected payload argument: {other}")));
            }
            None => return Err(usage("payload argument is not valid UTF-8")),
        }
    }

    let mode = required(mode, "--mode")?;
    let mode = match mode.to_str() {
        Some("ramdisk") => AblxMode::Ramdisk,
        Some("kernel-wrap") => AblxMode::KernelWrap,
        _ => return Err(usage("--mode must be ramdisk or kernel-wrap")),
    };
    let kernel = required(kernel, "--kernel")?;
    let shim = required(shim, "--shim")?;
    let kernel_out = required(kernel_out, "--kernel-out")?;
    let ramdisk_out = required(ramdisk_out, "--ramdisk-out")?;

    let kernel = read_file(&kernel)?;
    let shim = read_file(&shim)?;
    let initrd = match &initrd {
        Some(path) => Some(read_file(path)?),
        None => None,
    };

    let payload = abl_exorcist_assembler::assemble_payload(&kernel, initrd.as_deref(), mode, &shim)
        .map_err(|err| err.to_string())?;

    write_file(&kernel_out, &payload.kernel_section)?;
    write_file(&ramdisk_out, &payload.ramdisk_section)?;
    Ok(())
}

fn run_bootimg(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let subcommand = args
        .next()
        .ok_or_else(|| usage("missing bootimg subcommand"))?;
    match subcommand.to_str() {
        Some("repack") => run_bootimg_repack(args),
        Some("verify") => run_bootimg_verify(args),
        Some(other) => Err(usage(&format!("unknown bootimg subcommand: {other}"))),
        None => Err(usage("bootimg subcommand is not valid UTF-8")),
    }
}

fn run_bootimg_repack(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let mut template = None;
    let mut kernel = None;
    let mut ramdisk = None;
    let mut cmdline: Option<String> = None;
    let mut wrap_markers = false;
    let mut output = None;

    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--template") => template = Some(take(&mut args, "--template")?),
            Some("--kernel") => kernel = Some(take(&mut args, "--kernel")?),
            Some("--ramdisk") => ramdisk = Some(take(&mut args, "--ramdisk")?),
            Some("--cmdline") => {
                cmdline = Some(
                    take(&mut args, "--cmdline")?
                        .into_string()
                        .map_err(|_| usage("--cmdline is not valid UTF-8"))?,
                );
            }
            Some("--wrap") => wrap_markers = true,
            Some("--output") => output = Some(take(&mut args, "--output")?),
            Some(other) => {
                return Err(usage(&format!(
                    "unexpected bootimg repack argument: {other}"
                )));
            }
            None => return Err(usage("bootimg repack argument is not valid UTF-8")),
        }
    }

    let template = required(template, "--template")?;
    let output = required(output, "--output")?;
    let template_bytes = read_file(&template)?;
    let kernel_bytes = match &kernel {
        Some(path) => Some(read_file(path)?),
        None => None,
    };
    let ramdisk_bytes = match &ramdisk {
        Some(path) => Some(read_file(path)?),
        None => None,
    };

    let request = bootimg::Repack {
        kernel: kernel_bytes.as_deref(),
        ramdisk: ramdisk_bytes.as_deref(),
        cmdline: cmdline.as_deref(),
        wrap_markers,
    };
    let output_bytes = bootimg::repack(&template_bytes, &request).map_err(|err| err.to_string())?;
    write_file(&output, &output_bytes)
}

fn run_bootimg_verify(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let path = args
        .next()
        .ok_or_else(|| usage("missing boot image path"))?;
    if args.next().is_some() {
        return Err(usage("too many arguments for bootimg verify"));
    }

    let image = read_file(&path)?;
    let parsed = bootimg::verify(&image).map_err(|err| err.to_string())?;

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for line in [
        format!("page_size={}", parsed.page_size),
        format!("header_version={}", parsed.header_version),
        format!("kernel_offset={}", parsed.kernel.offset),
        format!("kernel_size={}", parsed.kernel.len),
        format!("ramdisk_offset={}", parsed.ramdisk.offset),
        format!("ramdisk_size={}", parsed.ramdisk.len),
        format!("second_offset={}", parsed.second.offset),
        format!("second_size={}", parsed.second.len),
        format!("recovery_dtbo_offset={}", parsed.recovery_dtbo.offset),
        format!("recovery_dtbo_size={}", parsed.recovery_dtbo.len),
        format!("dtb_offset={}", parsed.dtb.offset),
        format!("dtb_size={}", parsed.dtb.len),
        format!("total_len={}", parsed.total_len),
        format!("cmdline={}", parsed.cmdline),
        format!("id={}", hex_encode(&parsed.id[..20])),
    ] {
        writeln!(out, "{line}").map_err(|err| format!("write verification output: {err}"))?;
    }
    out.flush().map_err(|err| format!("flush stdout: {err}"))?;
    Ok(())
}

fn take(args: &mut impl Iterator<Item = OsString>, flag: &str) -> Result<OsString, String> {
    args.next()
        .ok_or_else(|| usage(&format!("missing value for {flag}")))
}

fn required(value: Option<OsString>, flag: &str) -> Result<OsString, String> {
    value.ok_or_else(|| usage(&format!("missing {flag}")))
}

fn read_file(path: &OsString) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|err| format!("read {}: {err}", Path::new(path).display()))
}

fn write_file(path: &OsString, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|err| format!("write {}: {err}", Path::new(path).display()))
}

fn write_stdout(bytes: &[u8], description: &str) -> Result<(), String> {
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    out.write_all(bytes)
        .map_err(|err| format!("write {description} to stdout: {err}"))?;
    out.flush().map_err(|err| format!("flush stdout: {err}"))?;
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn usage(error: &str) -> String {
    format!(
        "{error}\nusage: abl-exorcist-assembler /path/to/kernel /path/to/abl-exorcist.bin > /path/to/prepared-abl-exorcist-plus-kernel\n\
       abl-exorcist-assembler --ramdisk /path/to/kernel /path/to/initrd > /path/to/ablx-ramdisk-container\n\
       abl-exorcist-assembler payload --mode ramdisk|kernel-wrap --kernel PATH --shim PATH [--initrd PATH] --kernel-out PATH --ramdisk-out PATH\n\
       abl-exorcist-assembler bootimg repack --template PATH [--kernel PATH] [--ramdisk PATH] [--cmdline STR] [--wrap] --output PATH\n\
       abl-exorcist-assembler bootimg verify PATH"
    )
}
