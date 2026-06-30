use std::{
    env, fs,
    io::{self, BufWriter, Write},
    path::Path,
    process::ExitCode,
};

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
    let kernel = args
        .next()
        .ok_or_else(|| usage("missing kernel image path"))?;
    let shim = args
        .next()
        .ok_or_else(|| usage("missing abl-exorcist image path"))?;
    if args.next().is_some() {
        return Err(usage("too many arguments"));
    }

    let kernel =
        fs::read(&kernel).map_err(|err| format!("read {}: {err}", Path::new(&kernel).display()))?;
    let shim =
        fs::read(&shim).map_err(|err| format!("read {}: {err}", Path::new(&shim).display()))?;
    let assembled =
        abl_exorcist_assembler::assemble(&kernel, &shim).map_err(|err| err.to_string())?;

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    out.write_all(&assembled)
        .map_err(|err| format!("write assembled image to stdout: {err}"))?;
    out.flush().map_err(|err| format!("flush stdout: {err}"))?;
    Ok(())
}

fn usage(error: &str) -> String {
    format!(
        "{error}\nusage: abl-exorcist-assembler /path/to/kernel /path/to/abl-exorcist.bin > /path/to/prepared-abl-exorcist-plus-kernel"
    )
}
