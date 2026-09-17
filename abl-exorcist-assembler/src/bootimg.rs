//! Android boot image v2 repacking and structural verification.
//!
//! The header layout, section positions and hash digest come from
//! `abootimg-oxide`. This module adds what PocketFed's
//! `pocketfed-aboot-finalize` and `pocketfed-verify-bootimg` did on top of
//! that: which sections a repack may replace, `<S>`/`<E>` command line
//! markers, and the checks a device-ready image has to pass. Device-specific
//! address constants and size caps intentionally live with the caller.

use core::fmt;

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use abootimg_oxide::binrw::{self, BinRead, BinWrite, io::Cursor};
use abootimg_oxide::{HeaderV0, HeaderV0Versioned};
use sha1::Sha1;

/// Size of the Android boot image v2 header in bytes.
pub const HEADER_V2_SIZE: u32 = 1660;
/// Bytes available for the command line: 511 + 1023.
pub const CMDLINE_CAPACITY: usize = CMDLINE_FIELD_SIZE - 1 + EXTRA_CMDLINE_FIELD_SIZE - 1;

const HEADER_VERSION_OFFSET: usize = 40;
const CMDLINE_FIELD_SIZE: usize = 512;
const EXTRA_CMDLINE_FIELD_SIZE: usize = 1024;
const SHA1_SIZE: usize = 20;

// Section positions are computed in usize from u32 sizes; five of them
// summed cannot overflow on a 64-bit host, which is where this runs.
const _: () = assert!(usize::BITS >= 64);

/// A payload's byte range within an image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Section {
    pub offset: usize,
    pub len: usize,
}

/// A parsed Android boot image v2.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootImage {
    pub header: HeaderV0,
    /// `cmdline` field plus `extra_cmdline` field, concatenated and NUL-trimmed.
    pub cmdline: String,
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
    BadPageSize(u32),
    /// The header did not decode; carries the decoder's own explanation.
    Header(String),
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
            Self::BadPageSize(size) => write!(f, "invalid Android boot page size: {size}"),
            Self::Header(message) => write!(f, "Android boot header is invalid: {message}"),
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
            Self::SizeOverflow(description) => write!(f, "{description} overflows u32"),
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
    let header = read_header(image)?;
    let mut parsed = BootImage::new(header)?;
    parsed.cmdline = cmdline_text(&parsed.header.cmdline[..]);

    for (name, section) in parsed.sections() {
        let end = section
            .offset
            .checked_add(section.len)
            .filter(|end| *end <= image.len());
        if end.is_none() {
            return Err(BootImgError::SectionOutOfBounds {
                section: name,
                offset: section.offset,
                len: section.len,
                image_len: image.len(),
            });
        }
    }
    Ok(parsed)
}

/// Rebuild `template` with the given sections replaced.
pub fn repack(template: &[u8], req: &Repack<'_>) -> Result<Vec<u8>, BootImgError> {
    let parsed = parse(template)?;
    parsed.check_complete(template.len())?;

    let recovery_offset = recovery_dtbo_offset(&parsed.header);
    if parsed.recovery_dtbo.len != 0 && recovery_offset != parsed.recovery_dtbo.offset as u64 {
        return Err(BootImgError::RecoveryOffsetMismatch {
            header: recovery_offset,
            computed: parsed.recovery_dtbo.offset as u64,
        });
    }

    let payload = |section: &Section| &template[section.offset..section.offset + section.len];
    let kernel = req.kernel.unwrap_or(payload(&parsed.kernel));
    let ramdisk = req.ramdisk.unwrap_or(payload(&parsed.ramdisk));
    let second = payload(&parsed.second);
    let recovery = payload(&parsed.recovery_dtbo);
    let dtb = payload(&parsed.dtb);

    if kernel.is_empty() {
        return Err(BootImgError::EmptyKernel);
    }
    if ramdisk.is_empty() {
        return Err(BootImgError::EmptyRamdisk);
    }

    let mut header = parsed.header.clone();
    header.kernel_size = u32::try_from(kernel.len())
        .map_err(|_| BootImgError::SizeOverflow("kernel section length"))?;
    header.ramdisk_size = u32::try_from(ramdisk.len())
        .map_err(|_| BootImgError::SizeOverflow("ramdisk section length"))?;

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
        header.cmdline = cmdline_fields(bytes);
    }

    header.hash_digest = hash_digest(kernel, ramdisk, second, recovery, dtb)?;

    let layout = BootImage::new(header)?;
    let mut header = layout.header;
    if !recovery.is_empty() {
        set_recovery_dtbo_offset(&mut header, layout.recovery_dtbo.offset as u64);
    }

    let mut out = Vec::with_capacity(layout.total_len);
    header
        .write(&mut Cursor::new(&mut out))
        .map_err(|err| BootImgError::Header(err.to_string()))?;
    out.resize(layout.kernel.offset, 0);
    out.extend_from_slice(kernel);
    out.resize(layout.ramdisk.offset, 0);
    out.extend_from_slice(ramdisk);
    out.resize(layout.second.offset, 0);
    out.extend_from_slice(second);
    out.resize(layout.recovery_dtbo.offset, 0);
    out.extend_from_slice(recovery);
    out.resize(layout.dtb.offset, 0);
    out.extend_from_slice(dtb);
    out.resize(layout.total_len, 0);
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
    parsed.check_complete(image.len())?;

    let (cmdline, extra_cmdline) = parsed.header.cmdline.split_at(CMDLINE_FIELD_SIZE);
    verify_cmdline_field(cmdline)?;
    verify_cmdline_field(extra_cmdline)?;

    if parsed.header.hash_digest[..SHA1_SIZE] != compute_id(image, &parsed)? {
        return Err(BootImgError::IdMismatch);
    }
    Ok(parsed)
}

/// The SHA-1 the header's `id` field must hold for this image.
pub fn compute_id(image: &[u8], parsed: &BootImage) -> Result<[u8; SHA1_SIZE], BootImgError> {
    let payload = |section: &Section| &image[section.offset..section.offset + section.len];
    let digest = hash_digest(
        payload(&parsed.kernel),
        payload(&parsed.ramdisk),
        payload(&parsed.second),
        payload(&parsed.recovery_dtbo),
        payload(&parsed.dtb),
    )?;
    let mut id = [0u8; SHA1_SIZE];
    id.copy_from_slice(&digest[..SHA1_SIZE]);
    Ok(id)
}

impl BootImage {
    /// Lay out the sections a header describes. The command line is left
    /// empty; [`parse`] fills it.
    fn new(header: HeaderV0) -> Result<Self, BootImgError> {
        let HeaderV0Versioned::V2 {
            recovery_dtbo_size,
            dtb_size,
            ..
        } = header.versioned
        else {
            return Err(BootImgError::UnsupportedHeaderVersion(
                header.header_version(),
            ));
        };
        // The position helpers divide by the page size, so this has to come first.
        if header.page_size < HEADER_V2_SIZE || !header.page_size.is_power_of_two() {
            return Err(BootImgError::BadPageSize(header.page_size));
        }
        let section = |offset: usize, len: u32| Section {
            offset,
            len: len as usize,
        };
        let recovery_dtbo = section(header.recovery_dtbo_position(), recovery_dtbo_size);
        // abootimg-oxide 0.5.2's dtb_position() and boot_image_size() forget
        // the second-stage payload when placing the DTB, so the last two
        // positions are derived here from the (correct) recovery position.
        let page_align = |end: usize| end + header.get_padding_for(end);
        let dtb = section(
            page_align(recovery_dtbo.offset + recovery_dtbo.len),
            dtb_size,
        );
        Ok(Self {
            kernel: section(header.kernel_position(), header.kernel_size),
            ramdisk: section(header.ramdisk_position(), header.ramdisk_size),
            second: section(
                header.second_bootloader_position(),
                header.second_bootloader_size,
            ),
            total_len: page_align(dtb.offset + dtb.len),
            recovery_dtbo,
            dtb,
            cmdline: String::new(),
            header,
        })
    }

    fn sections(&self) -> [(&'static str, &Section); 5] {
        [
            ("kernel", &self.kernel),
            ("ramdisk", &self.ramdisk),
            ("second", &self.second),
            ("recovery_dtbo", &self.recovery_dtbo),
            ("dtb", &self.dtb),
        ]
    }

    /// The checks a bootable image must pass beyond decoding: the mandatory
    /// payloads are present and the file is exactly the layout's length.
    fn check_complete(&self, image_len: usize) -> Result<(), BootImgError> {
        if self.kernel.len == 0 {
            return Err(BootImgError::EmptyKernel);
        }
        if self.ramdisk.len == 0 {
            return Err(BootImgError::EmptyRamdisk);
        }
        if self.dtb.len == 0 {
            return Err(BootImgError::EmptyDtb);
        }
        if image_len < self.total_len {
            return Err(BootImgError::Truncated {
                needed: self.total_len,
                actual: image_len,
            });
        }
        if image_len > self.total_len {
            return Err(BootImgError::TrailingData {
                expected: self.total_len,
                actual: image_len,
            });
        }
        Ok(())
    }
}

fn read_header(image: &[u8]) -> Result<HeaderV0, BootImgError> {
    // Decode failures are reported by the decoder in its own words; the two
    // that callers act on differently, and the version, are told apart here
    // because the decoder accepts v0 and v1 too.
    if let Some(version) = image.get(HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + 4) {
        let version = u32::from_le_bytes(version.try_into().unwrap());
        if version != 2 {
            return Err(BootImgError::UnsupportedHeaderVersion(version));
        }
    }
    // binrw wraps the failing field's error in a backtrace; the cause is
    // what matters here.
    HeaderV0::read(&mut Cursor::new(image)).map_err(|err| match err.root_cause() {
        binrw::Error::BadMagic { .. } => BootImgError::BadMagic,
        binrw::Error::Io(_) => BootImgError::Truncated {
            needed: HEADER_V2_SIZE as usize,
            actual: image.len(),
        },
        other => BootImgError::Header(other.to_string()),
    })
}

trait PagePadding {
    fn get_padding_for(&self, size: usize) -> usize;
}

impl PagePadding for HeaderV0 {
    fn get_padding_for(&self, size: usize) -> usize {
        let page = self.page_size as usize;
        (page - size % page) % page
    }
}

fn recovery_dtbo_offset(header: &HeaderV0) -> u64 {
    match header.versioned {
        HeaderV0Versioned::V1 {
            recovery_dtbo_addr, ..
        }
        | HeaderV0Versioned::V2 {
            recovery_dtbo_addr, ..
        } => recovery_dtbo_addr,
        HeaderV0Versioned::V0 => 0,
    }
}

fn set_recovery_dtbo_offset(header: &mut HeaderV0, offset: u64) {
    if let HeaderV0Versioned::V1 {
        recovery_dtbo_addr, ..
    }
    | HeaderV0Versioned::V2 {
        recovery_dtbo_addr, ..
    } = &mut header.versioned
    {
        *recovery_dtbo_addr = offset;
    }
}

/// The `id` digest over the five payloads, SHA-1 in the first 20 bytes.
fn hash_digest(
    kernel: &[u8],
    ramdisk: &[u8],
    second: &[u8],
    recovery: &[u8],
    dtb: &[u8],
) -> Result<[u8; 32], BootImgError> {
    let (mut kernel, mut ramdisk, mut second, mut recovery, mut dtb) =
        (kernel, ramdisk, second, recovery, dtb);
    HeaderV0::compute_hash_digest::<&[u8], Sha1>(
        Some(&mut kernel),
        Some(&mut ramdisk),
        Some(&mut second),
        Some(&mut recovery),
        Some(&mut dtb),
    )
    .map_err(|_| BootImgError::SizeOverflow("payload length"))
}

fn cmdline_text(fields: &[u8]) -> String {
    let (cmdline, extra_cmdline) = fields.split_at(CMDLINE_FIELD_SIZE);
    let mut text = nul_terminated(cmdline);
    text.push_str(&nul_terminated(extra_cmdline));
    text
}

fn nul_terminated(field: &[u8]) -> String {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// Split a command line over the `cmdline` and `extra_cmdline` fields, each
/// kept NUL-terminated. `content` must fit [`CMDLINE_CAPACITY`].
fn cmdline_fields(content: &[u8]) -> Box<[u8; CMDLINE_FIELD_SIZE + EXTRA_CMDLINE_FIELD_SIZE]> {
    debug_assert!(content.len() <= CMDLINE_CAPACITY);
    let mut fields = Box::new([0u8; CMDLINE_FIELD_SIZE + EXTRA_CMDLINE_FIELD_SIZE]);
    let (head, tail) = content.split_at(content.len().min(CMDLINE_FIELD_SIZE - 1));
    fields[..head.len()].copy_from_slice(head);
    fields[CMDLINE_FIELD_SIZE..CMDLINE_FIELD_SIZE + tail.len()].copy_from_slice(tail);
    fields
}

fn verify_cmdline_field(field: &[u8]) -> Result<(), BootImgError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use abootimg_oxide::OsVersionPatch;
    use sha1::Digest;

    const PAGE_SIZE: u32 = 4096;
    const ID_OFFSET: usize = 576;
    const RECOVERY_DTBO_OFFSET_OFFSET: usize = 1636;

    fn chunk(seed: u8, len: usize) -> Vec<u8> {
        (0..len)
            .map(|index| seed.wrapping_add(index as u8))
            .collect()
    }

    fn align_up(value: usize) -> usize {
        let page = PAGE_SIZE as usize;
        value.div_ceil(page) * page
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
        let kernel_offset = page;
        let ramdisk_offset = align_up(kernel_offset + kernel.len());
        let second_offset = align_up(ramdisk_offset + ramdisk.len());
        let recovery_offset = align_up(second_offset + second.len());
        let dtb_offset = align_up(recovery_offset + recovery.len());
        let total = align_up(dtb_offset + dtb.len());

        let header = HeaderV0 {
            kernel_size: kernel.len() as u32,
            kernel_addr: 0x8000,
            ramdisk_size: ramdisk.len() as u32,
            ramdisk_addr: 0x0100_0000,
            second_bootloader_size: second.len() as u32,
            second_bootloader_addr: 0,
            tags_addr: 0x100,
            page_size: PAGE_SIZE,
            osversionpatch: OsVersionPatch(0),
            board_name: [0; 16],
            cmdline: cmdline_fields(cmdline.as_bytes()),
            hash_digest: hash_digest(kernel, ramdisk, second, recovery, dtb).unwrap(),
            versioned: HeaderV0Versioned::V2 {
                recovery_dtbo_size: recovery.len() as u32,
                recovery_dtbo_addr: if recovery.is_empty() {
                    0
                } else {
                    recovery_offset as u64
                },
                dtb_size: dtb.len() as u32,
                dtb_addr: 0,
            },
        };

        let mut image = Vec::with_capacity(total);
        header.write(&mut Cursor::new(&mut image)).unwrap();
        image.resize(kernel_offset, 0);
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
        assert_eq!(parsed.header.ramdisk_size, new_ramdisk.len() as u32);
        assert_eq!(
            &output[parsed.ramdisk.offset..parsed.ramdisk.offset + parsed.ramdisk.len],
            new_ramdisk.as_slice()
        );
        assert_eq!(
            parsed.second.offset,
            align_up(parsed.ramdisk.offset + parsed.ramdisk.len)
        );
        assert_eq!(
            recovery_dtbo_offset(&parsed.header),
            parsed.recovery_dtbo.offset as u64,
            "recovery DTBO offset follows the relaid payloads"
        );
    }

    #[test]
    fn id_matches_independently_computed_sha1() {
        let template = sample();
        let parsed = parse(&template).unwrap();

        let mut hasher = Sha1::new();
        for (_, section) in parsed.sections() {
            hasher.update(&template[section.offset..section.offset + section.len]);
            hasher.update((section.len as u32).to_le_bytes());
        }
        let mut expected = [0u8; SHA1_SIZE];
        expected.copy_from_slice(&hasher.finalize());

        assert_eq!(expected, compute_id(&template, &parsed).unwrap());
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
        assert!(parsed.cmdline.len() > CMDLINE_FIELD_SIZE - 1);
        assert_eq!(parsed.header.cmdline[CMDLINE_FIELD_SIZE - 1], 0);
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

        assert!(matches!(
            verify(&[]).unwrap_err(),
            BootImgError::Truncated { .. }
        ));

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

        let mut bad_page = template.clone();
        bad_page[36..40].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(verify(&bad_page).unwrap_err(), BootImgError::BadPageSize(0));

        let short = verify(&template[..1000]).unwrap_err();
        assert!(matches!(short, BootImgError::Truncated { .. }), "{short:?}");

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
