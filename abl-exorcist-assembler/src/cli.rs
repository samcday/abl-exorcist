use std::path::PathBuf;

use clap::Parser;

/// Assemble a kernel payload and write the binary image to stdout.
///
/// The default mode appends a compressed kernel package to a raw,
/// device-appropriate abl-exorcist AArch64 shim. With --ramdisk, the output is
/// an ABLXRD1 container containing the compressed kernel followed by the supplied
/// initramfs bytes, unchanged. Both modes write the binary image to stdout.
///
/// The kernel is converted to a validated raw arm64 Image before assembly.
/// Device selection, shim provenance, Android boot image geometry, partition
/// writes, and flashing belong to higher-level consumers.
#[derive(Parser)]
#[command(version)]
pub struct Cli {
    /// Build an ABLX ramdisk container instead of a shim+kernel payload
    #[arg(long)]
    pub ramdisk: bool,

    /// Raw arm64 Image, Image.gz, Image.zst, or Linux EFI zboot image
    pub kernel: PathBuf,

    /// Raw abl-exorcist arm64 Image, or initramfs bytes with --ramdisk
    #[arg(value_name = "SHIM_OR_INITRD")]
    pub input: PathBuf,
}
