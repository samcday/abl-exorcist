//! Android boot image v2 parsing, repacking and structural verification.
//!
//! This is a Rust port of PocketFed's `pocketfed-aboot-finalize` repack logic
//! and the structural half of `pocketfed-verify-bootimg`. Device-specific
//! address constants and size caps intentionally live with the caller.

use core::fmt;

use alloc::string::String;
use alloc::vec::Vec;

use sha1::{Digest, Sha1};

/// Android boot image magic (`ANDROID!`) at offset 0.
pub const BOOT_MAGIC: &[u8; 8] = b"ANDROID!";
/// Size of the Android boot image v2 header in bytes.
pub const HEADER_V2_SIZE: u32 = 1660;
/// Bytes available for the command line: 511 + 1023.
pub const CMDLINE_CAPACITY: usize = 511 + 1023;

const KERNEL_SIZE_OFFSET: usize = 8;
const KERNEL_ADDR_OFFSET: usize = 12;
const RAMDISK_SIZE_OFFSET: usize = 16;
const RAMDISK_ADDR_OFFSET: usize = 20;
const SECOND_SIZE_OFFSET: usize = 24;
const SECOND_ADDR_OFFSET: usize = 28;
const TAGS_ADDR_OFFSET: usize = 32;
const PAGE_SIZE_OFFSET: usize = 36;
const HEADER_VERSION_OFFSET: usize = 40;
const CMDLINE_OFFSET: usize = 64;
const CMDLINE_SIZE: usize = 512;
const ID_OFFSET: usize = 576;
const ID_SIZE: usize = 32;
const SHA1_SIZE: usize = 20;
const EXTRA_CMDLINE_OFFSET: usize = 608;
const EXTRA_CMDLINE_SIZE: usize = 1024;
const RECOVERY_DTBO_SIZE_OFFSET: usize = 1632;
const RECOVERY_DTBO_OFFSET_OFFSET: usize = 1636;
const HEADER_SIZE_OFFSET: usize = 1644;
const DTB_SIZE_OFFSET: usize = 1648;
const DTB_ADDR_OFFSET: usize = 1652;

const CMDLINE_CONTENT_SIZE: usize = CMDLINE_SIZE - 1;

/// A payload's byte range within an image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Section {
    pub offset: usize,
    pub len: usize,
}

/// A parsed Android boot image v2.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootImage {
    pub page_size: u32,
    pub header_version: u32,
    pub kernel_addr: u32,
    pub ramdisk_addr: u32,
    pub second_addr: u32,
    pub tags_addr: u32,
    pub dtb_addr: u64,
    /// `cmdline` field plus `extra_cmdline` field, concatenated and NUL-trimmed.
    pub cmdline: String,
    pub id: [u8; 32],
    pub kernel: Section,
    pub ramdisk: Section,
    pub second: Section,
    pub recovery_dtbo: Section,
    pub dtb: Section,
    /// Page-aligned end of the last payload.
    pub total_len: usize,
}

/// A distinct Android boot image failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BootImgError {
    BadMagic,
    Truncated {
        needed: usize,
        actual: usize,
    },
    UnsupportedHeaderVersion(u32),
    BadHeaderSize(u32),
    BadPageSize(u32),
    TrailingData {
        expected: usize,
        actual: usize,
    },
    EmptyKernel,
    EmptyRamdisk,
    EmptyDtb,
    CmdlineTooLong(usize),
    CmdlineForbiddenCharacter,
    CmdlineNotTerminated,
    CmdlineGarbage,
    IdMismatch,
    RecoveryOffsetMismatch {
        header: u64,
        computed: u64,
    },
    SectionOutOfBounds {
        section: &'static str,
        offset: usize,
        len: usize,
        image_len: usize,
    },
    SizeOverflow(&'static str),
}

impl fmt::Display for BootImgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => f.write_str("Android boot image magic is missing"),
            Self::Truncated { needed, actual } => write!(
                f,
                "image is truncated: needs at least {needed} bytes, has {actual}"
            ),
            Self::UnsupportedHeaderVersion(version) => {
                write!(f, "unsupported Android boot header version: {version}")
            }
            Self::BadHeaderSize(size) => write!(f, "unexpected Android boot header size: {size}"),
            Self::BadPageSize(size) => write!(f, "invalid Android boot page size: {size}"),
            Self::TrailingData { expected, actual } => write!(
                f,
                "trailing data or truncated payloads: image is {actual} bytes, expected {expected}"
            ),
            Self::EmptyKernel => f.write_str("kernel payload is empty"),
            Self::EmptyRamdisk => f.write_str("ramdisk payload is empty"),
            Self::EmptyDtb => f.write_str("DTB payload is empty"),
            Self::CmdlineTooLong(len) => write!(
                f,
                "Android boot image command line is too long: {len} > {CMDLINE_CAPACITY} bytes"
            ),
            Self::CmdlineForbiddenCharacter => {
                f.write_str("command line contains a forbidden NUL, newline or carriage return")
            }
            Self::CmdlineNotTerminated => f.write_str("command line field is not NUL-terminated"),
            Self::CmdlineGarbage => {
                f.write_str("command line field has data after its NUL terminator")
            }
            Self::IdMismatch => f.write_str("Android image ID mismatch"),
            Self::RecoveryOffsetMismatch { header, computed } => write!(
                f,
                "recovery DTBO offset 0x{header:x} does not match payload layout 0x{computed:x}"
            ),
            Self::SectionOutOfBounds {
                section,
                offset,
                len,
                image_len,
            } => write!(
                f,
                "{section} section 0x{offset:x}+0x{len:x} lies outside the {image_len}-byte image"
            ),
            Self::SizeOverflow(description) => write!(f, "{description} overflows usize"),
        }
    }
}

impl std::error::Error for BootImgError {}

/// A request to rebuild an Android boot image with some sections replaced.
///
/// Sections left as `None` are copied verbatim. `second`, `recovery_dtbo` and
/// `dtb` are always copied from the template.
#[derive(Clone, Copy, Debug, Default)]
pub struct Repack<'a> {
    pub kernel: Option<&'a [u8]>,
    pub ramdisk: Option<&'a [u8]>,
    pub cmdline: Option<&'a str>,
    /// When true, the cmdline written is `wrap_cmdline_markers(cmdline)`.
    pub wrap_markers: bool,
}

/// Parse an Android boot image v2.
///
/// Payload bounds are validated against the image length, but trailing data is
/// tolerated (the page-aligned end is reported as [`BootImage::total_len`]).
pub fn parse(image: &[u8]) -> Result<BootImage, BootImgError> {
    if image.len() < BOOT_MAGIC.len() || &image[..BOOT_MAGIC.len()] != BOOT_MAGIC {
        return Err(BootImgError::BadMagic);
    }
    if image.len() < 48 {
        return Err(BootImgError::Truncated {
            needed: 48,
            actual: image.len(),
        });
    }

    let header_version = read_u32(image, HEADER_VERSION_OFFSET)?;
    if header_version != 2 {
        return Err(BootImgError::UnsupportedHeaderVersion(header_version));
    }

    let page_size = read_u32(image, PAGE_SIZE_OFFSET)?;
    if page_size < HEADER_V2_SIZE || !page_size.is_power_of_two() {
        return Err(BootImgError::BadPageSize(page_size));
    }
    let page_size = page_size as usize;
    if image.len() < page_size {
        return Err(BootImgError::Truncated {
            needed: page_size,
            actual: image.len(),
        });
    }

    let header_size = read_u32(image, HEADER_SIZE_OFFSET)?;
    if header_size != HEADER_V2_SIZE {
        return Err(BootImgError::BadHeaderSize(header_size));
    }

    let kernel = Section {
        offset: page_size,
        len: read_u32(image, KERNEL_SIZE_OFFSET)? as usize,
    };
    let ramdisk = Section {
        offset: align_up(
            checked_add(kernel.offset, kernel.len, "ramdisk offset")?,
            page_size,
        )?,
        len: read_u32(image, RAMDISK_SIZE_OFFSET)? as usize,
    };
    let second = Section {
        offset: align_up(
            checked_add(ramdisk.offset, ramdisk.len, "second offset")?,
            page_size,
        )?,
        len: read_u32(image, SECOND_SIZE_OFFSET)? as usize,
    };
    let recovery_dtbo = Section {
        offset: align_up(
            checked_add(second.offset, second.len, "recovery offset")?,
            page_size,
        )?,
        len: read_u32(image, RECOVERY_DTBO_SIZE_OFFSET)? as usize,
    };
    let dtb = Section {
        offset: align_up(
            checked_add(recovery_dtbo.offset, recovery_dtbo.len, "dtb offset")?,
            page_size,
        )?,
        len: read_u32(image, DTB_SIZE_OFFSET)? as usize,
    };
    let total_len = align_up(checked_add(dtb.offset, dtb.len, "image length")?, page_size)?;

    for (name, section) in [
        ("kernel", &kernel),
        ("ramdisk", &ramdisk),
        ("second", &second),
        ("recovery_dtbo", &recovery_dtbo),
        ("dtb", &dtb),
    ] {
        let end = checked_add(section.offset, section.len, "section end")?;
        if end > image.len() {
            return Err(BootImgError::SectionOutOfBounds {
                section: name,
                offset: section.offset,
                len: section.len,
                image_len: image.len(),
            });
        }
    }

    let id: [u8; ID_SIZE] = image[ID_OFFSET..ID_OFFSET + ID_SIZE].try_into().unwrap();
    let cmdline = read_cmdline(image);

    Ok(BootImage {
        page_size: page_size as u32,
        header_version,
        kernel_addr: read_u32(image, KERNEL_ADDR_OFFSET)?,
        ramdisk_addr: read_u32(image, RAMDISK_ADDR_OFFSET)?,
        second_addr: read_u32(image, SECOND_ADDR_OFFSET)?,
        tags_addr: read_u32(image, TAGS_ADDR_OFFSET)?,
        dtb_addr: read_u64(image, DTB_ADDR_OFFSET)?,
        cmdline,
        id,
        kernel,
        ramdisk,
        second,
        recovery_dtbo,
        dtb,
        total_len,
    })
}

/// Rebuild `template` with the given sections replaced.
pub fn repack(template: &[u8], req: &Repack<'_>) -> Result<Vec<u8>, BootImgError> {
    if template.len() < BOOT_MAGIC.len() || &template[..BOOT_MAGIC.len()] != BOOT_MAGIC {
        return Err(BootImgError::BadMagic);
    }
    if template.len() < 48 {
        return Err(BootImgError::Truncated {
            needed: 48,
            actual: template.len(),
        });
    }

    let header_version = read_u32(template, HEADER_VERSION_OFFSET)?;
    if header_version != 2 {
        return Err(BootImgError::UnsupportedHeaderVersion(header_version));
    }

    let page_size = read_u32(template, PAGE_SIZE_OFFSET)?;
    if page_size < HEADER_V2_SIZE || !page_size.is_power_of_two() {
        return Err(BootImgError::BadPageSize(page_size));
    }
    let page_size = page_size as usize;
    if template.len() < page_size {
        return Err(BootImgError::Truncated {
            needed: page_size,
            actual: template.len(),
        });
    }

    let header_size = read_u32(template, HEADER_SIZE_OFFSET)?;
    if header_size != HEADER_V2_SIZE {
        return Err(BootImgError::BadHeaderSize(header_size));
    }

    let kernel_size = read_u32(template, KERNEL_SIZE_OFFSET)?;
    let ramdisk_size = read_u32(template, RAMDISK_SIZE_OFFSET)?;
    let second_size = read_u32(template, SECOND_SIZE_OFFSET)?;
    let recovery_size = read_u32(template, RECOVERY_DTBO_SIZE_OFFSET)?;
    let dtb_size = read_u32(template, DTB_SIZE_OFFSET)?;

    if kernel_size == 0 {
        return Err(BootImgError::EmptyKernel);
    }
    if ramdisk_size == 0 {
        return Err(BootImgError::EmptyRamdisk);
    }
    if dtb_size == 0 {
        return Err(BootImgError::EmptyDtb);
    }

    let kernel_offset = page_size;
    let ramdisk_offset = align_up(
        checked_add(kernel_offset, kernel_size as usize, "ramdisk offset")?,
        page_size,
    )?;
    let second_offset = align_up(
        checked_add(ramdisk_offset, ramdisk_size as usize, "second offset")?,
        page_size,
    )?;
    let recovery_offset = align_up(
        checked_add(second_offset, second_size as usize, "recovery offset")?,
        page_size,
    )?;
    let dtb_offset = align_up(
        checked_add(recovery_offset, recovery_size as usize, "dtb offset")?,
        page_size,
    )?;
    let template_end = align_up(
        checked_add(dtb_offset, dtb_size as usize, "template end")?,
        page_size,
    )?;

    if template.len() < template_end {
        return Err(BootImgError::Truncated {
            needed: template_end,
            actual: template.len(),
        });
    }
    if template.len() > template_end {
        return Err(BootImgError::TrailingData {
            expected: template_end,
            actual: template.len(),
        });
    }

    if recovery_size != 0 {
        let header_recovery = read_u64(template, RECOVERY_DTBO_OFFSET_OFFSET)?;
        if header_recovery != recovery_offset as u64 {
            return Err(BootImgError::RecoveryOffsetMismatch {
                header: header_recovery,
                computed: recovery_offset as u64,
            });
        }
    }

    let section = |offset: usize, len: usize| -> &[u8] { &template[offset..offset + len] };
    let template_kernel = section(kernel_offset, kernel_size as usize);
    let template_ramdisk = section(ramdisk_offset, ramdisk_size as usize);
    let second = section(second_offset, second_size as usize);
    let recovery = section(recovery_offset, recovery_size as usize);
    let dtb = section(dtb_offset, dtb_size as usize);

    let kernel = req.kernel.unwrap_or(template_kernel);
    let ramdisk = req.ramdisk.unwrap_or(template_ramdisk);

    if kernel.is_empty() {
        return Err(BootImgError::EmptyKernel);
    }
    if ramdisk.is_empty() {
        return Err(BootImgError::EmptyRamdisk);
    }

    let new_kernel_size = u32::try_from(kernel.len())
        .map_err(|_| BootImgError::SizeOverflow("kernel section length"))?;
    let new_ramdisk_size = u32::try_from(ramdisk.len())
        .map_err(|_| BootImgError::SizeOverflow("ramdisk section length"))?;

    let new_ramdisk_offset = align_up(
        checked_add(page_size, kernel.len(), "new ramdisk offset")?,
        page_size,
    )?;
    let new_second_offset = align_up(
        checked_add(new_ramdisk_offset, ramdisk.len(), "new second offset")?,
        page_size,
    )?;
    let new_recovery_offset = align_up(
        checked_add(new_second_offset, second.len(), "new recovery offset")?,
        page_size,
    )?;
    let new_dtb_offset = align_up(
        checked_add(new_recovery_offset, recovery.len(), "new dtb offset")?,
        page_size,
    )?;
    let total_len = align_up(
        checked_add(new_dtb_offset, dtb.len(), "new image length")?,
        page_size,
    )?;

    let mut hasher = Sha1::new();
    hasher.update(kernel);
    hasher.update(new_kernel_size.to_le_bytes());
    hasher.update(ramdisk);
    hasher.update(new_ramdisk_size.to_le_bytes());
    hasher.update(second);
    hasher.update(second_size.to_le_bytes());
    hasher.update(recovery);
    hasher.update(recovery_size.to_le_bytes());
    hasher.update(dtb);
    hasher.update(dtb_size.to_le_bytes());
    let digest = hasher.finalize();

    let mut header = template[..page_size].to_vec();
    header[KERNEL_SIZE_OFFSET..KERNEL_SIZE_OFFSET + 4]
        .copy_from_slice(&new_kernel_size.to_le_bytes());
    header[RAMDISK_SIZE_OFFSET..RAMDISK_SIZE_OFFSET + 4]
        .copy_from_slice(&new_ramdisk_size.to_le_bytes());

    if let Some(cmdline) = req.cmdline {
        let cmdline = if req.wrap_markers {
            wrap_cmdline_markers(cmdline)
        } else {
            String::from(cmdline)
        };
        let bytes = cmdline.as_bytes();
        if bytes.iter().any(|byte| matches!(*byte, 0 | b'\n' | b'\r')) {
            return Err(BootImgError::CmdlineForbiddenCharacter);
        }
        if bytes.len() > CMDLINE_CAPACITY {
            return Err(BootImgError::CmdlineTooLong(bytes.len()));
        }
        write_cmdline_fields(&mut header, bytes);
    }

    header[ID_OFFSET..ID_OFFSET + ID_SIZE].fill(0);
    header[ID_OFFSET..ID_OFFSET + SHA1_SIZE].copy_from_slice(&digest);

    if recovery_size != 0 {
        header[RECOVERY_DTBO_OFFSET_OFFSET..RECOVERY_DTBO_OFFSET_OFFSET + 8]
            .copy_from_slice(&(new_recovery_offset as u64).to_le_bytes());
    }

    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(&header);
    out.extend_from_slice(kernel);
    out.resize(new_ramdisk_offset, 0);
    out.extend_from_slice(ramdisk);
    out.resize(new_second_offset, 0);
    out.extend_from_slice(second);
    out.resize(new_recovery_offset, 0);
    out.extend_from_slice(recovery);
    out.resize(new_dtb_offset, 0);
    out.extend_from_slice(dtb);
    out.resize(total_len, 0);
    Ok(out)
}

/// `"<S> {cmdline} <E>"`, idempotent and whitespace-collapsed.
pub fn wrap_cmdline_markers(cmdline: &str) -> String {
    let collapsed = collapse_whitespace(cmdline);
    if collapsed == "<S> <E>" || (collapsed.starts_with("<S> ") && collapsed.ends_with(" <E>")) {
        return collapsed;
    }
    if collapsed.is_empty() {
        return String::from("<S> <E>");
    }

    let mut out = String::with_capacity(collapsed.len() + 8);
    out.push_str("<S> ");
    out.push_str(&collapsed);
    out.push_str(" <E>");
    out
}

/// Structural and integrity verification. Returns the parsed image on success.
pub fn verify(image: &[u8]) -> Result<BootImage, BootImgError> {
    let parsed = parse(image)?;

    if parsed.kernel.len == 0 {
        return Err(BootImgError::EmptyKernel);
    }
    if parsed.ramdisk.len == 0 {
        return Err(BootImgError::EmptyRamdisk);
    }
    if parsed.dtb.len == 0 {
        return Err(BootImgError::EmptyDtb);
    }
    if image.len() < parsed.total_len {
        return Err(BootImgError::Truncated {
            needed: parsed.total_len,
            actual: image.len(),
        });
    }
    if image.len() > parsed.total_len {
        return Err(BootImgError::TrailingData {
            expected: parsed.total_len,
            actual: image.len(),
        });
    }

    verify_cmdline_field(image, CMDLINE_OFFSET, CMDLINE_SIZE)?;
    verify_cmdline_field(image, EXTRA_CMDLINE_OFFSET, EXTRA_CMDLINE_SIZE)?;

    let computed = compute_id(image, &parsed);
    if parsed.id[..SHA1_SIZE] != computed {
        return Err(BootImgError::IdMismatch);
    }

    Ok(parsed)
}

/// The SHA-1 the header's `id` field must hold for this image.
pub fn compute_id(image: &[u8], parsed: &BootImage) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hash_section(&mut hasher, image, &parsed.kernel);
    hasher.update((parsed.kernel.len as u32).to_le_bytes());
    hash_section(&mut hasher, image, &parsed.ramdisk);
    hasher.update((parsed.ramdisk.len as u32).to_le_bytes());
    hash_section(&mut hasher, image, &parsed.second);
    hasher.update((parsed.second.len as u32).to_le_bytes());
    hash_section(&mut hasher, image, &parsed.recovery_dtbo);
    hasher.update((parsed.recovery_dtbo.len as u32).to_le_bytes());
    hash_section(&mut hasher, image, &parsed.dtb);
    hasher.update((parsed.dtb.len as u32).to_le_bytes());

    let digest = hasher.finalize();
    let mut id = [0u8; SHA1_SIZE];
    id.copy_from_slice(&digest);
    id
}

fn hash_section(hasher: &mut Sha1, image: &[u8], section: &Section) {
    hasher.update(&image[section.offset..section.offset + section.len]);
}

fn read_cmdline(image: &[u8]) -> String {
    let mut cmdline = field_content(image, CMDLINE_OFFSET, CMDLINE_SIZE);
    cmdline.push_str(&field_content(
        image,
        EXTRA_CMDLINE_OFFSET,
        EXTRA_CMDLINE_SIZE,
    ));
    cmdline
}

fn field_content(image: &[u8], offset: usize, size: usize) -> String {
    let field = &image[offset..offset + size];
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

fn write_cmdline_fields(header: &mut [u8], cmdline: &[u8]) {
    let split = cmdline.len().min(CMDLINE_CONTENT_SIZE);
    write_field(
        &mut header[CMDLINE_OFFSET..CMDLINE_OFFSET + CMDLINE_SIZE],
        &cmdline[..split],
    );
    write_field(
        &mut header[EXTRA_CMDLINE_OFFSET..EXTRA_CMDLINE_OFFSET + EXTRA_CMDLINE_SIZE],
        &cmdline[split..],
    );
}

fn write_field(field: &mut [u8], content: &[u8]) {
    debug_assert!(content.len() < field.len());
    field.fill(0);
    field[..content.len()].copy_from_slice(content);
}

fn verify_cmdline_field(image: &[u8], offset: usize, size: usize) -> Result<(), BootImgError> {
    let field = &image[offset..offset + size];
    let terminator = field
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(BootImgError::CmdlineNotTerminated)?;
    if field[terminator..].iter().any(|byte| *byte != 0) {
        return Err(BootImgError::CmdlineGarbage);
    }
    Ok(())
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::new();
    for token in input.split_ascii_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(token);
    }
    out
}

fn checked_add(left: usize, right: usize, what: &'static str) -> Result<usize, BootImgError> {
    left.checked_add(right)
        .ok_or(BootImgError::SizeOverflow(what))
}

fn align_up(value: usize, alignment: usize) -> Result<usize, BootImgError> {
    if !alignment.is_power_of_two() {
        return Err(BootImgError::SizeOverflow("alignment"));
    }
    let mask = alignment - 1;
    value
        .checked_add(mask)
        .map(|value| value & !mask)
        .ok_or(BootImgError::SizeOverflow("aligned offset"))
}

fn read_u32(image: &[u8], offset: usize) -> Result<u32, BootImgError> {
    let end = checked_add(offset, 4, "u32 field bounds")?;
    if end > image.len() {
        return Err(BootImgError::Truncated {
            needed: end,
            actual: image.len(),
        });
    }
    Ok(u32::from_le_bytes(image[offset..end].try_into().unwrap()))
}

fn read_u64(image: &[u8], offset: usize) -> Result<u64, BootImgError> {
    let end = checked_add(offset, 8, "u64 field bounds")?;
    if end > image.len() {
        return Err(BootImgError::Truncated {
            needed: end,
            actual: image.len(),
        });
    }
    Ok(u64::from_le_bytes(image[offset..end].try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    const PAGE_SIZE: u32 = 4096;

    fn chunk(seed: u8, len: usize) -> Vec<u8> {
        (0..len)
            .map(|index| seed.wrapping_add(index as u8))
            .collect()
    }

    fn build(
        kernel: &[u8],
        ramdisk: &[u8],
        second: &[u8],
        recovery: &[u8],
        dtb: &[u8],
        cmdline: &str,
    ) -> Vec<u8> {
        let page = PAGE_SIZE as usize;
        let mut header = vec![0u8; page];
        header[..BOOT_MAGIC.len()].copy_from_slice(BOOT_MAGIC);
        header[KERNEL_SIZE_OFFSET..KERNEL_SIZE_OFFSET + 4]
            .copy_from_slice(&(kernel.len() as u32).to_le_bytes());
        header[KERNEL_ADDR_OFFSET..KERNEL_ADDR_OFFSET + 4]
            .copy_from_slice(&0x8000u32.to_le_bytes());
        header[RAMDISK_SIZE_OFFSET..RAMDISK_SIZE_OFFSET + 4]
            .copy_from_slice(&(ramdisk.len() as u32).to_le_bytes());
        header[RAMDISK_ADDR_OFFSET..RAMDISK_ADDR_OFFSET + 4]
            .copy_from_slice(&0x0100_0000u32.to_le_bytes());
        header[SECOND_SIZE_OFFSET..SECOND_SIZE_OFFSET + 4]
            .copy_from_slice(&(second.len() as u32).to_le_bytes());
        header[TAGS_ADDR_OFFSET..TAGS_ADDR_OFFSET + 4].copy_from_slice(&0x100u32.to_le_bytes());
        header[PAGE_SIZE_OFFSET..PAGE_SIZE_OFFSET + 4].copy_from_slice(&PAGE_SIZE.to_le_bytes());
        header[HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + 4]
            .copy_from_slice(&2u32.to_le_bytes());
        header[HEADER_SIZE_OFFSET..HEADER_SIZE_OFFSET + 4]
            .copy_from_slice(&HEADER_V2_SIZE.to_le_bytes());
        header[RECOVERY_DTBO_SIZE_OFFSET..RECOVERY_DTBO_SIZE_OFFSET + 4]
            .copy_from_slice(&(recovery.len() as u32).to_le_bytes());
        header[DTB_SIZE_OFFSET..DTB_SIZE_OFFSET + 4]
            .copy_from_slice(&(dtb.len() as u32).to_le_bytes());

        let cmdline_bytes = cmdline.as_bytes();
        let split = cmdline_bytes.len().min(CMDLINE_CONTENT_SIZE);
        write_field(
            &mut header[CMDLINE_OFFSET..CMDLINE_OFFSET + CMDLINE_SIZE],
            &cmdline_bytes[..split],
        );
        write_field(
            &mut header[EXTRA_CMDLINE_OFFSET..EXTRA_CMDLINE_OFFSET + EXTRA_CMDLINE_SIZE],
            &cmdline_bytes[split..],
        );

        let kernel_offset = page;
        let ramdisk_offset = align_up(kernel_offset + kernel.len(), page).unwrap();
        let second_offset = align_up(ramdisk_offset + ramdisk.len(), page).unwrap();
        let recovery_offset = align_up(second_offset + second.len(), page).unwrap();
        let dtb_offset = align_up(recovery_offset + recovery.len(), page).unwrap();
        let total = align_up(dtb_offset + dtb.len(), page).unwrap();

        if !recovery.is_empty() {
            header[RECOVERY_DTBO_OFFSET_OFFSET..RECOVERY_DTBO_OFFSET_OFFSET + 8]
                .copy_from_slice(&(recovery_offset as u64).to_le_bytes());
        }

        let mut hasher = Sha1::new();
        hasher.update(kernel);
        hasher.update((kernel.len() as u32).to_le_bytes());
        hasher.update(ramdisk);
        hasher.update((ramdisk.len() as u32).to_le_bytes());
        hasher.update(second);
        hasher.update((second.len() as u32).to_le_bytes());
        hasher.update(recovery);
        hasher.update((recovery.len() as u32).to_le_bytes());
        hasher.update(dtb);
        hasher.update((dtb.len() as u32).to_le_bytes());
        let digest = hasher.finalize();
        header[ID_OFFSET..ID_OFFSET + SHA1_SIZE].copy_from_slice(&digest);

        let mut image = Vec::with_capacity(total);
        image.extend_from_slice(&header);
        image.extend_from_slice(kernel);
        image.resize(ramdisk_offset, 0);
        image.extend_from_slice(ramdisk);
        image.resize(second_offset, 0);
        image.extend_from_slice(second);
        image.resize(recovery_offset, 0);
        image.extend_from_slice(recovery);
        image.resize(dtb_offset, 0);
        image.extend_from_slice(dtb);
        image.resize(total, 0);
        image
    }

    fn sample() -> Vec<u8> {
        build(
            &chunk(0x11, 3000),
            &chunk(0x22, 5000),
            &chunk(0x33, 100),
            &chunk(0x44, 700),
            &chunk(0x55, 900),
            "<S> root=LABEL=pfroot rw <E>",
        )
    }

    #[test]
    fn repack_without_replacements_is_byte_identical() {
        let template = sample();

        let output = repack(&template, &Repack::default()).unwrap();

        assert_eq!(output, template);
    }

    #[test]
    fn repack_replaces_ramdisk_and_relays_out_payloads() {
        let template = sample();
        let new_ramdisk = chunk(0x77, 9000);

        let output = repack(
            &template,
            &Repack {
                ramdisk: Some(&new_ramdisk),
                ..Repack::default()
            },
        )
        .unwrap();

        let parsed = verify(&output).unwrap();
        assert_eq!(parsed.ramdisk.len, new_ramdisk.len());
        assert_eq!(
            read_u32(&output, RAMDISK_SIZE_OFFSET).unwrap(),
            new_ramdisk.len() as u32
        );
        assert_eq!(
            &output[parsed.ramdisk.offset..parsed.ramdisk.offset + parsed.ramdisk.len],
            new_ramdisk.as_slice()
        );
        assert_eq!(
            parsed.second.offset,
            align_up(
                parsed.ramdisk.offset + parsed.ramdisk.len,
                PAGE_SIZE as usize
            )
            .unwrap()
        );
    }

    #[test]
    fn id_matches_independently_computed_sha1() {
        let template = sample();
        let parsed = parse(&template).unwrap();

        let mut hasher = Sha1::new();
        hasher.update(&template[parsed.kernel.offset..parsed.kernel.offset + parsed.kernel.len]);
        hasher.update((parsed.kernel.len as u32).to_le_bytes());
        hasher.update(&template[parsed.ramdisk.offset..parsed.ramdisk.offset + parsed.ramdisk.len]);
        hasher.update((parsed.ramdisk.len as u32).to_le_bytes());
        hasher.update(&template[parsed.second.offset..parsed.second.offset + parsed.second.len]);
        hasher.update((parsed.second.len as u32).to_le_bytes());
        hasher.update(
            &template[parsed.recovery_dtbo.offset
                ..parsed.recovery_dtbo.offset + parsed.recovery_dtbo.len],
        );
        hasher.update((parsed.recovery_dtbo.len as u32).to_le_bytes());
        hasher.update(&template[parsed.dtb.offset..parsed.dtb.offset + parsed.dtb.len]);
        hasher.update((parsed.dtb.len as u32).to_le_bytes());
        let mut expected = [0u8; SHA1_SIZE];
        expected.copy_from_slice(&hasher.finalize());

        assert_eq!(expected, compute_id(&template, &parsed));
        assert_eq!(&template[ID_OFFSET..ID_OFFSET + SHA1_SIZE], expected);
    }

    #[test]
    fn long_cmdline_spills_into_extra_field() {
        let cmdline = "x".repeat(600);
        let template = build(
            &chunk(0x11, 3000),
            &chunk(0x22, 5000),
            &[],
            &[],
            &chunk(0x55, 900),
            &cmdline,
        );

        let parsed = verify(&template).unwrap();

        assert_eq!(parsed.cmdline, cmdline);
        assert!(parsed.cmdline.len() > CMDLINE_CONTENT_SIZE);
        assert_eq!(template[CMDLINE_OFFSET + CMDLINE_CONTENT_SIZE], 0);
    }

    #[test]
    fn overlong_cmdline_is_rejected() {
        let template = sample();
        let cmdline = "y".repeat(1600);

        let error = repack(
            &template,
            &Repack {
                cmdline: Some(&cmdline),
                ..Repack::default()
            },
        )
        .unwrap_err();

        assert_eq!(error, BootImgError::CmdlineTooLong(1600));
    }

    #[test]
    fn wrap_markers_is_idempotent_and_collapses_whitespace() {
        assert_eq!(
            wrap_cmdline_markers("foo   bar\nbaz"),
            "<S> foo bar baz <E>"
        );
        assert_eq!(wrap_cmdline_markers("<S> foo  bar <E>"), "<S> foo bar <E>");

        let once = wrap_cmdline_markers("<S> foo bar <E>");
        assert_eq!(wrap_cmdline_markers(&once), once);
    }

    #[test]
    fn verify_rejects_structural_corruption() {
        let template = sample();

        assert_eq!(verify(&[]).unwrap_err(), BootImgError::BadMagic);

        let mut bad_magic = template.clone();
        bad_magic[0] = b'X';
        assert_eq!(verify(&bad_magic).unwrap_err(), BootImgError::BadMagic);

        let mut wrong_version = template.clone();
        wrong_version[HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + 4]
            .copy_from_slice(&3u32.to_le_bytes());
        assert_eq!(
            verify(&wrong_version).unwrap_err(),
            BootImgError::UnsupportedHeaderVersion(3)
        );

        let mut trailing = template.clone();
        trailing.push(0);
        assert_eq!(
            verify(&trailing).unwrap_err(),
            BootImgError::TrailingData {
                expected: template.len(),
                actual: template.len() + 1,
            }
        );

        let mut corrupted_id = template.clone();
        corrupted_id[ID_OFFSET] ^= 0xff;
        assert_eq!(verify(&corrupted_id).unwrap_err(), BootImgError::IdMismatch);
    }

    #[test]
    fn repack_rejects_an_empty_replacement_ramdisk() {
        let template = sample();

        let err = repack(
            &template,
            &Repack {
                ramdisk: Some(&[]),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert_eq!(err, BootImgError::EmptyRamdisk);
    }

    #[test]
    fn repack_rejects_template_with_trailing_data() {
        let mut template = sample();
        template.push(0);

        assert_eq!(
            repack(&template, &Repack::default()).unwrap_err(),
            BootImgError::TrailingData {
                expected: template.len() - 1,
                actual: template.len(),
            }
        );
    }

    #[test]
    fn repack_rejects_recovery_offset_mismatch() {
        let mut template = sample();
        template[RECOVERY_DTBO_OFFSET_OFFSET..RECOVERY_DTBO_OFFSET_OFFSET + 8]
            .copy_from_slice(&0x1234u64.to_le_bytes());

        assert_eq!(
            repack(&template, &Repack::default()).unwrap_err(),
            BootImgError::RecoveryOffsetMismatch {
                header: 0x1234,
                computed: 20480,
            }
        );
    }
}
