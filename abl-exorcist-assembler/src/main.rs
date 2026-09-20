use std::{
    fs,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Parser;

/// Assemble a kernel payload and write the binary image to stdout.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Build an ABLX ramdisk container instead of a shim+kernel payload
    #[arg(long)]
    ramdisk: bool,

    /// Raw arm64 Image, Image.gz, Image.zst, or Linux EFI zboot image
    kernel: PathBuf,

    /// Raw abl-exorcist arm64 Image, or initramfs bytes with --ramdisk
    #[arg(value_name = "SHIM_OR_INITRD")]
    input: PathBuf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = if cli.ramdisk {
        run_ramdisk(&cli.kernel, &cli.input)
    } else {
        run(&cli.kernel, &cli.input)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(kernel: &Path, shim: &Path) -> Result<(), String> {
    let kernel = fs::read(kernel).map_err(|err| format!("read {}: {err}", kernel.display()))?;
    let shim = fs::read(shim).map_err(|err| format!("read {}: {err}", shim.display()))?;
    let kernel =
        abl_exorcist_assembler::canonicalize_kernel(&kernel).map_err(|err| err.to_string())?;
    let assembled =
        abl_exorcist_assembler::assemble(&kernel, &shim).map_err(|err| err.to_string())?;

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    out.write_all(&assembled)
        .map_err(|err| format!("write assembled image to stdout: {err}"))?;
    out.flush().map_err(|err| format!("flush stdout: {err}"))?;
    Ok(())
}

fn run_ramdisk(kernel: &Path, initrd: &Path) -> Result<(), String> {
    let kernel = fs::read(kernel).map_err(|err| format!("read {}: {err}", kernel.display()))?;
    let initrd = fs::read(initrd).map_err(|err| format!("read {}: {err}", initrd.display()))?;
    let kernel =
        abl_exorcist_assembler::canonicalize_kernel(&kernel).map_err(|err| err.to_string())?;
    let assembled = abl_exorcist_assembler::assemble_ramdisk(&kernel, &initrd)
        .map_err(|err| err.to_string())?;

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    out.write_all(&assembled)
        .map_err(|err| format!("write ramdisk container to stdout: {err}"))?;
    out.flush().map_err(|err| format!("flush stdout: {err}"))?;
    Ok(())
}
