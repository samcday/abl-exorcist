#[cfg(feature = "manpage")]
#[path = "src/cli.rs"]
mod cli;

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/cli.rs");

    #[cfg(feature = "manpage")]
    {
        use clap::CommandFactory;

        let out_dir = std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR");
        clap_mangen::Man::new(cli::Cli::command())
            .manual("User Commands")
            .generate_to(out_dir)?;
    }

    Ok(())
}
