#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

const ARM64_IMAGE_MIN_SIZE: usize = 64;
const ARM64_IMAGE_SIZE_OFFSET: usize = 16;
const ARM64_IMAGE_MAGIC_OFFSET: usize = 56;
const ARM64_IMAGE_MAGIC: &[u8; 4] = b"ARM\x64";
const PAYLOAD_ALIGN: u64 = 0x20_0000;

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

    fn image(image_size: u64, file_len: usize) -> Vec<u8> {
        let mut image = vec![0; file_len.max(ARM64_IMAGE_MIN_SIZE)];
        image[ARM64_IMAGE_SIZE_OFFSET..ARM64_IMAGE_SIZE_OFFSET + 8]
            .copy_from_slice(&image_size.to_le_bytes());
        image[ARM64_IMAGE_MAGIC_OFFSET..ARM64_IMAGE_MAGIC_OFFSET + ARM64_IMAGE_MAGIC.len()]
            .copy_from_slice(ARM64_IMAGE_MAGIC);
        image
    }
}
