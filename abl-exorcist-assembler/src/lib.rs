#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

#[cfg(feature = "std")]
use std::io::Read;

const ARM64_IMAGE_MIN_SIZE: usize = 64;
const ARM64_IMAGE_SIZE_OFFSET: usize = 16;
const ARM64_IMAGE_MAGIC_OFFSET: usize = 56;
const ARM64_IMAGE_MAGIC: &[u8; 4] = b"ARM\x64";
const PAYLOAD_ALIGN: u64 = 0x20_0000;

#[cfg(feature = "std")]
const GZIP_MAGIC: &[u8; 2] = b"\x1f\x8b";
#[cfg(feature = "std")]
const ZSTD_MAGIC: &[u8; 4] = b"\x28\xb5\x2f\xfd";
#[cfg(feature = "std")]
const LINUX_ZBOOT_MIN_SIZE: usize = 28;
#[cfg(feature = "std")]
const LINUX_ZBOOT_IMAGE_TYPE_OFFSET: usize = 4;
#[cfg(feature = "std")]
const LINUX_ZBOOT_PAYLOAD_OFFSET_OFFSET: usize = 8;
#[cfg(feature = "std")]
const LINUX_ZBOOT_PAYLOAD_SIZE_OFFSET: usize = 12;
#[cfg(feature = "std")]
const LINUX_ZBOOT_COMP_TYPE_OFFSET: usize = 24;
#[cfg(feature = "std")]
const LINUX_ZBOOT_COMP_TYPE_MAX_LEN: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageKind {
    Kernel,
    Shim,
}

impl fmt::Display for ImageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kernel => f.write_str("kernel"),
            Self::Shim => f.write_str("abl-exorcist"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssembleError {
    ImageTooSmall { image: ImageKind, size: usize },
    NotArm64Image { image: ImageKind },
    ZeroImageSize { image: ImageKind },
    ShimTooLarge { len: u64, payload_offset: u64 },
    KernelLargerThanImageSize { len: u64, image_size: u64 },
    InvalidAlignment(u64),
    SizeOverflow(&'static str),
}

impl fmt::Display for AssembleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImageTooSmall { image, size } => write!(
                f,
                "{image} is smaller than an arm64 Image header: {size} < {ARM64_IMAGE_MIN_SIZE}"
            ),
            Self::NotArm64Image { image } => write!(f, "{image} is not a raw arm64 Image"),
            Self::ZeroImageSize { image } => {
                write!(f, "{image} arm64 Image header has zero image_size")
            }
            Self::ShimTooLarge {
                len,
                payload_offset,
            } => write!(
                f,
                "abl-exorcist image length 0x{len:x} exceeds payload offset 0x{payload_offset:x}"
            ),
            Self::KernelLargerThanImageSize { len, image_size } => write!(
                f,
                "kernel file length 0x{len:x} exceeds arm64 image_size 0x{image_size:x}"
            ),
            Self::InvalidAlignment(alignment) => write!(
                f,
                "alignment must be a non-zero power of two: 0x{alignment:x}"
            ),
            Self::SizeOverflow(description) => write!(f, "{description} overflows u64/usize"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for AssembleError {}

#[cfg(feature = "std")]
#[derive(Debug)]
pub enum KernelImageError {
    InvalidImage(AssembleError),
    InvalidGzip(std::io::Error),
    InvalidZstd(std::io::Error),
    ZbootTooSmall {
        size: usize,
    },
    ZbootPayloadOutOfBounds {
        payload_offset: usize,
        payload_size: usize,
        image_size: usize,
    },
    UnsupportedZbootCompression(String),
    SizeOverflow(&'static str),
}

#[cfg(feature = "std")]
impl fmt::Display for KernelImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidImage(err) => write!(f, "{err}"),
            Self::InvalidGzip(err) => write!(f, "decompress gzip kernel image: {err}"),
            Self::InvalidZstd(err) => write!(f, "decompress zstd kernel image: {err}"),
            Self::ZbootTooSmall { size } => {
                write!(
                    f,
                    "Linux EFI zboot image is too small: {size} < {LINUX_ZBOOT_MIN_SIZE}"
                )
            }
            Self::ZbootPayloadOutOfBounds {
                payload_offset,
                payload_size,
                image_size,
            } => write!(
                f,
                "Linux EFI zboot payload extends beyond image: offset 0x{payload_offset:x} + size 0x{payload_size:x} > 0x{image_size:x}"
            ),
            Self::UnsupportedZbootCompression(compression) => write!(
                f,
                "unsupported Linux EFI zboot compression type: {compression}"
            ),
            Self::SizeOverflow(description) => write!(f, "{description} overflows usize"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for KernelImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidImage(err) => Some(err),
            Self::InvalidGzip(err) | Self::InvalidZstd(err) => Some(err),
            _ => None,
        }
    }
}

#[cfg(feature = "std")]
pub fn canonicalize_kernel(kernel: &[u8]) -> Result<Vec<u8>, KernelImageError> {
    if is_arm64_image(kernel) {
        verify_kernel(kernel)?;
        return Ok(kernel.to_vec());
    }

    if is_linux_zboot(kernel) {
        return canonicalize_linux_zboot(kernel);
    }

    if kernel.starts_with(GZIP_MAGIC) {
        return decompress_gzip_kernel(kernel);
    }

    if kernel.starts_with(ZSTD_MAGIC) {
        return decompress_zstd_kernel(kernel);
    }

    Err(KernelImageError::InvalidImage(
        arm64_image_size(kernel, ImageKind::Kernel).unwrap_err(),
    ))
}

pub fn assemble(kernel: &[u8], shim: &[u8]) -> Result<Vec<u8>, AssembleError> {
    let kernel_size = arm64_image_size(kernel, ImageKind::Kernel)?;
    let shim_size = arm64_image_size(shim, ImageKind::Shim)?;
    let payload_offset = payload_source_offset(shim_size)?;
    let shim_file_len =
        u64::try_from(shim.len()).map_err(|_| AssembleError::SizeOverflow("shim file length"))?;

    if shim_file_len > payload_offset {
        return Err(AssembleError::ShimTooLarge {
            len: shim_file_len,
            payload_offset,
        });
    }

    let kernel_file_len = u64::try_from(kernel.len())
        .map_err(|_| AssembleError::SizeOverflow("kernel file length"))?;
    if kernel_file_len > kernel_size {
        return Err(AssembleError::KernelLargerThanImageSize {
            len: kernel_file_len,
            image_size: kernel_size,
        });
    }

    let output_len = payload_offset
        .checked_add(kernel_size)
        .ok_or(AssembleError::SizeOverflow("assembled image length"))?;
    let payload_offset = usize::try_from(payload_offset)
        .map_err(|_| AssembleError::SizeOverflow("payload offset"))?;
    let output_len = usize::try_from(output_len)
        .map_err(|_| AssembleError::SizeOverflow("assembled image length"))?;

    let mut out = Vec::with_capacity(output_len);
    out.extend_from_slice(shim);
    out.resize(payload_offset, 0);
    out.extend_from_slice(kernel);
    out.resize(output_len, 0);
    Ok(out)
}

#[cfg(feature = "std")]
fn canonicalize_linux_zboot(kernel: &[u8]) -> Result<Vec<u8>, KernelImageError> {
    if kernel.len() < LINUX_ZBOOT_MIN_SIZE {
        return Err(KernelImageError::ZbootTooSmall { size: kernel.len() });
    }

    let payload_offset = read_le_u32(kernel, LINUX_ZBOOT_PAYLOAD_OFFSET_OFFSET)?;
    let payload_size = read_le_u32(kernel, LINUX_ZBOOT_PAYLOAD_SIZE_OFFSET)?;
    let payload_end =
        payload_offset
            .checked_add(payload_size)
            .ok_or(KernelImageError::SizeOverflow(
                "Linux EFI zboot payload bounds",
            ))?;
    if payload_end > kernel.len() {
        return Err(KernelImageError::ZbootPayloadOutOfBounds {
            payload_offset,
            payload_size,
            image_size: kernel.len(),
        });
    }

    let payload = &kernel[payload_offset..payload_end];
    let compression = linux_zboot_compression(kernel);
    if compression.starts_with(b"gzip") {
        decompress_gzip_kernel(payload)
    } else if compression.starts_with(b"zstd") {
        decompress_zstd_kernel(payload)
    } else {
        Err(KernelImageError::UnsupportedZbootCompression(
            String::from_utf8_lossy(compression).into_owned(),
        ))
    }
}

#[cfg(feature = "std")]
fn decompress_gzip_kernel(kernel: &[u8]) -> Result<Vec<u8>, KernelImageError> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(kernel)
        .read_to_end(&mut out)
        .map_err(KernelImageError::InvalidGzip)?;
    verify_kernel(&out)?;
    Ok(out)
}

#[cfg(feature = "std")]
fn decompress_zstd_kernel(kernel: &[u8]) -> Result<Vec<u8>, KernelImageError> {
    let out = zstd::stream::decode_all(kernel).map_err(KernelImageError::InvalidZstd)?;
    verify_kernel(&out)?;
    Ok(out)
}

#[cfg(feature = "std")]
fn verify_kernel(kernel: &[u8]) -> Result<(), KernelImageError> {
    arm64_image_size(kernel, ImageKind::Kernel)
        .map(|_| ())
        .map_err(KernelImageError::InvalidImage)
}

#[cfg(feature = "std")]
fn is_arm64_image(image: &[u8]) -> bool {
    image.len() >= ARM64_IMAGE_MIN_SIZE
        && &image[ARM64_IMAGE_MAGIC_OFFSET..ARM64_IMAGE_MAGIC_OFFSET + ARM64_IMAGE_MAGIC.len()]
            == ARM64_IMAGE_MAGIC
}

#[cfg(feature = "std")]
fn is_linux_zboot(image: &[u8]) -> bool {
    image.len() >= LINUX_ZBOOT_MIN_SIZE
        && image.starts_with(b"MZ")
        && &image[LINUX_ZBOOT_IMAGE_TYPE_OFFSET..LINUX_ZBOOT_IMAGE_TYPE_OFFSET + 4] == b"zimg"
}

#[cfg(feature = "std")]
fn linux_zboot_compression(image: &[u8]) -> &[u8] {
    let end = image
        .len()
        .min(LINUX_ZBOOT_COMP_TYPE_OFFSET + LINUX_ZBOOT_COMP_TYPE_MAX_LEN);
    let compression = &image[LINUX_ZBOOT_COMP_TYPE_OFFSET..end];
    let nul = compression
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(compression.len());
    &compression[..nul]
}

#[cfg(feature = "std")]
fn read_le_u32(image: &[u8], offset: usize) -> Result<usize, KernelImageError> {
    let end = offset
        .checked_add(4)
        .ok_or(KernelImageError::SizeOverflow("u32 field bounds"))?;
    if end > image.len() {
        return Err(KernelImageError::ZbootTooSmall { size: image.len() });
    }
    Ok(u32::from_le_bytes(image[offset..end].try_into().unwrap()) as usize)
}

pub fn arm64_image_size(image: &[u8], kind: ImageKind) -> Result<u64, AssembleError> {
    if image.len() < ARM64_IMAGE_MIN_SIZE {
        return Err(AssembleError::ImageTooSmall {
            image: kind,
            size: image.len(),
        });
    }
    if &image[ARM64_IMAGE_MAGIC_OFFSET..ARM64_IMAGE_MAGIC_OFFSET + ARM64_IMAGE_MAGIC.len()]
        != ARM64_IMAGE_MAGIC
    {
        return Err(AssembleError::NotArm64Image { image: kind });
    }

    let image_size = u64::from_le_bytes(
        image[ARM64_IMAGE_SIZE_OFFSET..ARM64_IMAGE_SIZE_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    if image_size == 0 {
        return Err(AssembleError::ZeroImageSize { image: kind });
    }
    Ok(image_size)
}

fn payload_source_offset(shim_size: u64) -> Result<u64, AssembleError> {
    let with_skid = shim_size
        .checked_add(PAYLOAD_ALIGN)
        .ok_or(AssembleError::SizeOverflow("abl-exorcist image size"))?;
    align_up(with_skid, PAYLOAD_ALIGN)
}

fn align_up(value: u64, alignment: u64) -> Result<u64, AssembleError> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return Err(AssembleError::InvalidAlignment(alignment));
    }

    let mask = alignment - 1;
    value
        .checked_add(mask)
        .map(|value| value & !mask)
        .ok_or(AssembleError::SizeOverflow("aligned value"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[cfg(feature = "std")]
    use std::io::Write;

    #[test]
    fn assembles_shim_payload_and_padding() {
        let shim = image(0x1000, 128);
        let kernel = image(0x2000, 256);

        let assembled = assemble(&kernel, &shim).unwrap();

        assert_eq!(&assembled[..shim.len()], shim.as_slice());
        assert_eq!(
            &assembled[PAYLOAD_ALIGN as usize * 2..PAYLOAD_ALIGN as usize * 2 + kernel.len()],
            kernel.as_slice()
        );
        assert_eq!(assembled.len(), PAYLOAD_ALIGN as usize * 2 + 0x2000);
        assert!(
            assembled[shim.len()..PAYLOAD_ALIGN as usize * 2]
                .iter()
                .all(|byte| *byte == 0)
        );
    }

    #[test]
    fn payload_offset_includes_one_alignment_skid() {
        assert_eq!(payload_source_offset(1).unwrap(), PAYLOAD_ALIGN * 2);
        assert_eq!(
            payload_source_offset(PAYLOAD_ALIGN).unwrap(),
            PAYLOAD_ALIGN * 2
        );
        assert_eq!(
            payload_source_offset(PAYLOAD_ALIGN + 1).unwrap(),
            PAYLOAD_ALIGN * 3
        );
    }

    #[test]
    fn reads_arm64_image_size() {
        let image = image(1234, ARM64_IMAGE_MIN_SIZE);

        assert_eq!(arm64_image_size(&image, ImageKind::Kernel), Ok(1234));
    }

    #[cfg(feature = "std")]
    #[test]
    fn canonicalizes_raw_arm64_image() {
        let image = image(0x2000, 256);

        assert_eq!(canonicalize_kernel(&image).unwrap(), image);
    }

    #[cfg(feature = "std")]
    #[test]
    fn canonicalizes_gzip_arm64_image() {
        let image = image(0x2000, 256);
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&image).unwrap();
        let compressed = encoder.finish().unwrap();

        assert_eq!(canonicalize_kernel(&compressed).unwrap(), image);
    }

    #[cfg(feature = "std")]
    #[test]
    fn canonicalizes_zstd_arm64_image() {
        let image = image(0x2000, 256);
        let compressed = zstd::stream::encode_all(image.as_slice(), 0).unwrap();

        assert_eq!(canonicalize_kernel(&compressed).unwrap(), image);
    }

    #[cfg(feature = "std")]
    #[test]
    fn canonicalizes_linux_zboot_gzip_image() {
        let image = image(0x2000, 256);
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&image).unwrap();
        let compressed = encoder.finish().unwrap();
        let zboot = linux_zboot(b"gzip", &compressed);

        assert_eq!(canonicalize_kernel(&zboot).unwrap(), image);
    }

    #[cfg(feature = "std")]
    #[test]
    fn canonicalizes_linux_zboot_zstd_image() {
        let image = image(0x2000, 256);
        let compressed = zstd::stream::encode_all(image.as_slice(), 0).unwrap();
        let zboot = linux_zboot(b"zstd", &compressed);

        assert_eq!(canonicalize_kernel(&zboot).unwrap(), image);
    }

    fn image(image_size: u64, file_len: usize) -> Vec<u8> {
        let mut image = vec![0; file_len.max(ARM64_IMAGE_MIN_SIZE)];
        image[ARM64_IMAGE_SIZE_OFFSET..ARM64_IMAGE_SIZE_OFFSET + 8]
            .copy_from_slice(&image_size.to_le_bytes());
        image[ARM64_IMAGE_MAGIC_OFFSET..ARM64_IMAGE_MAGIC_OFFSET + ARM64_IMAGE_MAGIC.len()]
            .copy_from_slice(ARM64_IMAGE_MAGIC);
        image
    }

    #[cfg(feature = "std")]
    fn linux_zboot(compression: &[u8], payload: &[u8]) -> Vec<u8> {
        let payload_offset = 0x100usize;
        let mut image = vec![0; payload_offset];
        image[..2].copy_from_slice(b"MZ");
        image[LINUX_ZBOOT_IMAGE_TYPE_OFFSET..LINUX_ZBOOT_IMAGE_TYPE_OFFSET + 4]
            .copy_from_slice(b"zimg");
        image[LINUX_ZBOOT_PAYLOAD_OFFSET_OFFSET..LINUX_ZBOOT_PAYLOAD_OFFSET_OFFSET + 4]
            .copy_from_slice(&(payload_offset as u32).to_le_bytes());
        image[LINUX_ZBOOT_PAYLOAD_SIZE_OFFSET..LINUX_ZBOOT_PAYLOAD_SIZE_OFFSET + 4]
            .copy_from_slice(&(payload.len() as u32).to_le_bytes());
        image[LINUX_ZBOOT_COMP_TYPE_OFFSET..LINUX_ZBOOT_COMP_TYPE_OFFSET + compression.len()]
            .copy_from_slice(compression);
        image.extend_from_slice(payload);
        image
    }
}
