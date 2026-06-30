#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
use core::panic::PanicInfo;

#[cfg(target_os = "none")]
core::arch::global_asm!(include_str!("start.S"));

#[cfg(target_os = "none")]
const ARM64_IMAGE_SIZE_OFFSET: usize = 16;
#[cfg(target_os = "none")]
const ARM64_IMAGE_MAGIC_OFFSET: usize = 56;
#[cfg(target_os = "none")]
const ARM64_IMAGE_MAGIC: u32 = u32::from_le_bytes(*b"ARM\x64");
#[cfg(target_os = "none")]
const PAYLOAD_ALIGN: usize = 0x20_0000;
#[cfg(any(target_os = "none", test))]
const FDT_MAGIC: u32 = 0xd00d_feed;
#[cfg(any(target_os = "none", test))]
const FDT_HEADER_SIZE: usize = 40;
#[cfg(any(target_os = "none", test))]
const FDT_OFF_DT_STRUCT_OFFSET: usize = 8;
#[cfg(any(target_os = "none", test))]
const FDT_OFF_DT_STRINGS_OFFSET: usize = 12;
#[cfg(any(target_os = "none", test))]
const FDT_SIZE_DT_STRINGS_OFFSET: usize = 32;
#[cfg(any(target_os = "none", test))]
const FDT_SIZE_DT_STRUCT_OFFSET: usize = 36;
#[cfg(any(target_os = "none", test))]
const FDT_BEGIN_NODE: u32 = 1;
#[cfg(any(target_os = "none", test))]
const FDT_END_NODE: u32 = 2;
#[cfg(any(target_os = "none", test))]
const FDT_PROP: u32 = 3;
#[cfg(any(target_os = "none", test))]
const FDT_NOP: u32 = 4;
#[cfg(any(target_os = "none", test))]
const FDT_END: u32 = 9;
#[cfg(any(target_os = "none", test))]
const CMDLINE_START: &[u8] = b"<S>";
#[cfg(any(target_os = "none", test))]
const CMDLINE_END: &[u8] = b"<E>";
#[cfg(any(target_os = "none", test))]
const ANDROIDBOOT_PREFIX: &[u8] = b"androidboot.";

#[cfg(target_os = "none")]
unsafe extern "C" {
    fn _start() -> !;
}

#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn abl_exorcist_main(fdt: usize) -> ! {
    let payload_source = payload_source();
    if read32(payload_source + ARM64_IMAGE_MAGIC_OFFSET) != ARM64_IMAGE_MAGIC {
        halt();
    }

    let payload_size = read64(payload_source + ARM64_IMAGE_SIZE_OFFSET) as usize;
    if payload_size == 0 {
        halt();
    }

    rewrite_bootargs_from_raw_fdt(fdt);

    let payload_entry = align_down(payload_source, PAYLOAD_ALIGN);
    move_payload(payload_source, payload_entry, payload_size);
    clean_dcache_range(payload_entry, payload_size);
    clean_raw_fdt(fdt);
    invalidate_icache();

    jump_to_payload(payload_entry, fdt)
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    halt()
}

#[cfg(target_os = "none")]
fn halt() -> ! {
    loop {
        unsafe {
            core::arch::asm!("wfe", options(nomem, nostack, preserves_flags));
        }
    }
}

#[cfg(target_os = "none")]
fn payload_source() -> usize {
    let base = _start as *const () as usize;
    let image_size = read64(base + ARM64_IMAGE_SIZE_OFFSET) as usize;
    align_up(image_size + PAYLOAD_ALIGN, PAYLOAD_ALIGN) + base
}

#[cfg(target_os = "none")]
fn move_payload(source: usize, destination: usize, size: usize) {
    if source == destination || size == 0 {
        return;
    }

    unsafe {
        core::ptr::copy(source as *const u8, destination as *mut u8, size);
    }
}

#[cfg(target_os = "none")]
fn read32(address: usize) -> u32 {
    unsafe { (address as *const u32).read_volatile() }
}

#[cfg(target_os = "none")]
fn read64(address: usize) -> u64 {
    unsafe { (address as *const u64).read_volatile() }
}

#[cfg(target_os = "none")]
fn rewrite_bootargs_from_raw_fdt(fdt: usize) {
    if fdt == 0 || read_be_u32_raw(fdt) != FDT_MAGIC {
        return;
    }

    let totalsize = read_be_u32_raw(fdt + 4) as usize;
    if totalsize < FDT_HEADER_SIZE {
        return;
    }

    let fdt = unsafe { core::slice::from_raw_parts_mut(fdt as *mut u8, totalsize) };
    let _ = rewrite_bootargs(fdt);
}

#[cfg(target_os = "none")]
fn read_be_u32_raw(address: usize) -> u32 {
    u32::from_be(unsafe { (address as *const u32).read_unaligned() })
}

#[cfg(target_os = "none")]
fn clean_raw_fdt(fdt: usize) {
    if fdt == 0 || read_be_u32_raw(fdt) != FDT_MAGIC {
        return;
    }

    clean_dcache_range(fdt, read_be_u32_raw(fdt + 4) as usize);
}

#[cfg(target_os = "none")]
fn clean_dcache_range(start: usize, size: usize) {
    if size == 0 {
        return;
    }

    let line_size = dcache_line_size();
    let mut address = align_down(start, line_size);
    let end = align_up(start.saturating_add(size), line_size);

    while address < end {
        unsafe {
            core::arch::asm!("dc cvac, {address}", address = in(reg) address, options(nostack, preserves_flags));
        }
        address += line_size;
    }

    unsafe {
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    }
}

#[cfg(target_os = "none")]
fn dcache_line_size() -> usize {
    let ctr: usize;
    unsafe {
        core::arch::asm!("mrs {ctr}, ctr_el0", ctr = out(reg) ctr, options(nomem, nostack, preserves_flags));
    }

    4 << ((ctr >> 16) & 0xf)
}

#[cfg(target_os = "none")]
fn invalidate_icache() {
    unsafe {
        core::arch::asm!(
            "ic iallu",
            "dsb sy",
            "isb",
            options(nostack, preserves_flags)
        );
    }
}

#[cfg(target_os = "none")]
fn jump_to_payload(entry: usize, fdt: usize) -> ! {
    unsafe {
        core::arch::asm!(
            "dsb sy",
            "isb",
            "mov x1, xzr",
            "mov x2, xzr",
            "mov x3, xzr",
            "br x16",
            in("x0") fdt,
            in("x16") entry,
            options(noreturn)
        );
    }
}

#[cfg(any(target_os = "none", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FdtError {
    BadHeader,
    BadStructure,
    NotFound,
}

#[cfg(any(target_os = "none", test))]
fn rewrite_bootargs(fdt: &mut [u8]) -> Result<bool, FdtError> {
    let (offset, len) = find_bootargs(fdt)?;
    let changed = filter_cmdline_in_place(&mut fdt[offset..offset + len]);
    if changed {
        clean_string_tail(&mut fdt[offset..offset + len]);
    }
    Ok(changed)
}

#[cfg(any(target_os = "none", test))]
fn find_bootargs(fdt: &[u8]) -> Result<(usize, usize), FdtError> {
    if fdt.len() < FDT_HEADER_SIZE || be32_at(fdt, 0)? != FDT_MAGIC {
        return Err(FdtError::BadHeader);
    }

    let struct_offset = be32_at(fdt, FDT_OFF_DT_STRUCT_OFFSET)? as usize;
    let strings_offset = be32_at(fdt, FDT_OFF_DT_STRINGS_OFFSET)? as usize;
    let strings_size = be32_at(fdt, FDT_SIZE_DT_STRINGS_OFFSET)? as usize;
    let struct_size = be32_at(fdt, FDT_SIZE_DT_STRUCT_OFFSET)? as usize;
    let struct_end = checked_end(struct_offset, struct_size, fdt.len())?;
    let strings_end = checked_end(strings_offset, strings_size, fdt.len())?;

    let mut pos = struct_offset;
    let mut depth = 0usize;
    let mut chosen_depth = None;

    while pos < struct_end {
        let token = be32_at(fdt, pos)?;
        pos += 4;

        match token {
            FDT_BEGIN_NODE => {
                let name_start = pos;
                while pos < struct_end && fdt[pos] != 0 {
                    pos += 1;
                }
                if pos == struct_end {
                    return Err(FdtError::BadStructure);
                }
                let name = &fdt[name_start..pos];
                pos = align_up(pos + 1, 4);
                if depth == 1 && name == b"chosen" {
                    chosen_depth = Some(depth + 1);
                }
                depth += 1;
            }
            FDT_END_NODE => {
                if depth == 0 {
                    return Err(FdtError::BadStructure);
                }
                if chosen_depth == Some(depth) {
                    chosen_depth = None;
                }
                depth -= 1;
            }
            FDT_PROP => {
                let len = be32_at(fdt, pos)? as usize;
                let name_offset = be32_at(fdt, pos + 4)? as usize;
                pos += 8;
                let value_offset = pos;
                let value_end = checked_end(value_offset, len, struct_end)?;
                pos = align_up(value_end, 4);

                if chosen_depth == Some(depth)
                    && string_at(fdt, strings_offset, strings_end, name_offset)? == b"bootargs"
                {
                    return Ok((value_offset, len));
                }
            }
            FDT_NOP => {}
            FDT_END => break,
            _ => return Err(FdtError::BadStructure),
        }
    }

    Err(FdtError::NotFound)
}

#[cfg(any(target_os = "none", test))]
fn string_at(
    fdt: &[u8],
    strings_offset: usize,
    strings_end: usize,
    name_offset: usize,
) -> Result<&[u8], FdtError> {
    let start = strings_offset
        .checked_add(name_offset)
        .ok_or(FdtError::BadStructure)?;
    if start >= strings_end {
        return Err(FdtError::BadStructure);
    }

    let mut end = start;
    while end < strings_end && fdt[end] != 0 {
        end += 1;
    }
    if end == strings_end {
        return Err(FdtError::BadStructure);
    }

    Ok(&fdt[start..end])
}

#[cfg(any(target_os = "none", test))]
fn filter_cmdline_in_place(value: &mut [u8]) -> bool {
    let text_len = nul_terminated_len(value);
    let Some((start_marker, end_marker)) = find_cmdline_markers(&value[..text_len]) else {
        return false;
    };

    let mut out = 0;
    append_filtered_androidboot(value, &mut out, 0, start_marker.start);

    let real_start = skip_ascii_space(value, start_marker.next, end_marker.start);
    let real_end = trim_ascii_space_end(value, end_marker.start);
    append_range(value, &mut out, real_start, real_end);

    append_filtered_androidboot(value, &mut out, end_marker.next, text_len);

    if out < value.len() {
        value[out] = 0;
        out += 1;
    }
    while out < value.len() {
        value[out] = 0;
        out += 1;
    }

    true
}

#[cfg(any(target_os = "none", test))]
fn clean_string_tail(value: &mut [u8]) {
    let text_len = nul_terminated_len(value);
    let mut pos = text_len.saturating_add(1);
    while pos < value.len() {
        value[pos] = 0;
        pos += 1;
    }
}

#[cfg(any(target_os = "none", test))]
fn find_cmdline_markers(value: &[u8]) -> Option<(Token, Token)> {
    let mut pos = 0;
    while let Some(token) = next_token(value, value.len(), pos) {
        if &value[token.start..token.end] == CMDLINE_START {
            let mut end_pos = token.next;
            while let Some(end) = next_token(value, value.len(), end_pos) {
                if &value[end.start..end.end] == CMDLINE_END {
                    return Some((token, end));
                }
                end_pos = end.next;
            }
            return None;
        }
        pos = token.next;
    }
    None
}

#[cfg(any(target_os = "none", test))]
fn append_filtered_androidboot(value: &mut [u8], out: &mut usize, start: usize, end: usize) {
    let mut pos = start;
    while let Some(token) = next_token(value, end, pos) {
        if value[token.start..token.end].starts_with(ANDROIDBOOT_PREFIX) {
            append_range(value, out, token.start, token.end);
        }
        pos = token.next;
    }
}

#[cfg(any(target_os = "none", test))]
fn append_range(value: &mut [u8], out: &mut usize, start: usize, end: usize) {
    if start >= end {
        return;
    }

    if *out != 0 {
        value[*out] = b' ';
        *out += 1;
    }

    let mut read = start;
    while read < end {
        value[*out] = value[read];
        *out += 1;
        read += 1;
    }
}

#[cfg(any(target_os = "none", test))]
#[derive(Clone, Copy)]
struct Token {
    start: usize,
    end: usize,
    next: usize,
}

#[cfg(any(target_os = "none", test))]
fn next_token(value: &[u8], len: usize, mut pos: usize) -> Option<Token> {
    while pos < len && is_ascii_space(value[pos]) {
        pos += 1;
    }
    if pos >= len {
        return None;
    }

    let start = pos;
    let mut in_quote = false;
    while pos < len {
        match value[pos] {
            b'"' => in_quote = !in_quote,
            byte if is_ascii_space(byte) && !in_quote => break,
            _ => {}
        }
        pos += 1;
    }

    Some(Token {
        start,
        end: pos,
        next: pos,
    })
}

#[cfg(any(target_os = "none", test))]
fn trim_ascii_space_end(value: &[u8], mut end: usize) -> usize {
    while end > 0 && is_ascii_space(value[end - 1]) {
        end -= 1;
    }
    end
}

#[cfg(any(target_os = "none", test))]
fn skip_ascii_space(value: &[u8], mut start: usize, end: usize) -> usize {
    while start < end && is_ascii_space(value[start]) {
        start += 1;
    }
    start
}

#[cfg(any(target_os = "none", test))]
fn nul_terminated_len(value: &[u8]) -> usize {
    let mut len = 0;
    while len < value.len() && value[len] != 0 {
        len += 1;
    }
    len
}

#[cfg(any(target_os = "none", test))]
fn be32_at(data: &[u8], offset: usize) -> Result<u32, FdtError> {
    let bytes = data.get(offset..offset + 4).ok_or(FdtError::BadStructure)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(any(target_os = "none", test))]
fn checked_end(start: usize, len: usize, max: usize) -> Result<usize, FdtError> {
    let end = start.checked_add(len).ok_or(FdtError::BadStructure)?;
    if end > max {
        return Err(FdtError::BadStructure);
    }
    Ok(end)
}

#[cfg(any(target_os = "none", test))]
fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

#[cfg(target_os = "none")]
fn align_down(value: usize, alignment: usize) -> usize {
    value & !(alignment - 1)
}

#[cfg(any(target_os = "none", test))]
fn is_ascii_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

#[cfg(not(target_os = "none"))]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_markers_leave_cmdline_unchanged() {
        let mut cmdline = b"foo root=/dev/dm-0 androidboot.serialno=123\0xxxx".to_vec();
        let original = cmdline.clone();

        assert!(!filter_cmdline_in_place(&mut cmdline));
        assert_eq!(cmdline, original);
    }

    #[test]
    fn half_marked_cmdline_is_unchanged() {
        let mut cmdline = b"androidboot.serialno=123 <S> foo root=/dev/dm-0\0xxxx".to_vec();
        let original = cmdline.clone();

        assert!(!filter_cmdline_in_place(&mut cmdline));
        assert_eq!(cmdline, original);
    }

    #[test]
    fn filters_prefix_and_suffix_around_marked_cmdline() {
        let mut cmdline = b"root=/dev/dm-0 androidboot.serialno=123 <S> foo bar dm=\"keep me\" <E> skip_initramfs androidboot.mode=normal dm=\"drop me\"\0xxxx".to_vec();

        assert!(filter_cmdline_in_place(&mut cmdline));
        assert_eq!(
            cstr(&cmdline),
            b"androidboot.serialno=123 foo bar dm=\"keep me\" androidboot.mode=normal"
        );
        assert!(
            cmdline[cstr(&cmdline).len() + 1..]
                .iter()
                .all(|byte| *byte == 0)
        );
    }

    #[test]
    fn filters_appended_cmdline() {
        let mut cmdline =
            b"<S> foo root=UUID=123 <E> root=/dev/dm-0 androidboot.bootdevice=1d84000.ufshc\0"
                .to_vec();

        assert!(filter_cmdline_in_place(&mut cmdline));
        assert_eq!(
            cstr(&cmdline),
            b"foo root=UUID=123 androidboot.bootdevice=1d84000.ufshc"
        );
    }

    #[test]
    fn filters_prepended_cmdline() {
        let mut cmdline =
            b"root=/dev/dm-0 androidboot.serialno=123 <S> foo root=UUID=123 <E>\0".to_vec();

        assert!(filter_cmdline_in_place(&mut cmdline));
        assert_eq!(
            cstr(&cmdline),
            b"androidboot.serialno=123 foo root=UUID=123"
        );
    }

    #[test]
    fn markers_inside_quotes_are_ignored() {
        let mut cmdline = b"foo=\"<S>\" bar=\"<E>\" root=/dev/dm-0\0".to_vec();
        let original = cmdline.clone();

        assert!(!filter_cmdline_in_place(&mut cmdline));
        assert_eq!(cmdline, original);
    }

    #[test]
    fn rewrites_chosen_bootargs_in_fdt() {
        let mut fdt = test_fdt(b"root=/dev/dm-0 <S> foo <E> androidboot.serialno=123\0");

        assert_eq!(rewrite_bootargs(&mut fdt), Ok(true));

        let (offset, len) = find_bootargs(&fdt).unwrap();
        assert_eq!(
            cstr(&fdt[offset..offset + len]),
            b"foo androidboot.serialno=123"
        );
    }

    fn cstr(value: &[u8]) -> &[u8] {
        &value[..nul_terminated_len(value)]
    }

    fn test_fdt(bootargs: &[u8]) -> Vec<u8> {
        let strings = b"bootargs\0";
        let mut structure = Vec::new();
        push_be32(&mut structure, FDT_BEGIN_NODE);
        push_padded_bytes(&mut structure, b"\0");
        push_be32(&mut structure, FDT_BEGIN_NODE);
        push_padded_bytes(&mut structure, b"chosen\0");
        push_be32(&mut structure, FDT_PROP);
        push_be32(&mut structure, bootargs.len() as u32);
        push_be32(&mut structure, 0);
        push_padded_bytes(&mut structure, bootargs);
        push_be32(&mut structure, FDT_END_NODE);
        push_be32(&mut structure, FDT_END_NODE);
        push_be32(&mut structure, FDT_END);

        let off_mem_rsvmap = FDT_HEADER_SIZE;
        let off_dt_struct = off_mem_rsvmap + 16;
        let off_dt_strings = off_dt_struct + structure.len();
        let totalsize = off_dt_strings + strings.len();

        let mut fdt = Vec::new();
        push_be32(&mut fdt, FDT_MAGIC);
        push_be32(&mut fdt, totalsize as u32);
        push_be32(&mut fdt, off_dt_struct as u32);
        push_be32(&mut fdt, off_dt_strings as u32);
        push_be32(&mut fdt, off_mem_rsvmap as u32);
        push_be32(&mut fdt, 17);
        push_be32(&mut fdt, 16);
        push_be32(&mut fdt, 0);
        push_be32(&mut fdt, strings.len() as u32);
        push_be32(&mut fdt, structure.len() as u32);
        fdt.extend_from_slice(&[0; 16]);
        fdt.extend_from_slice(&structure);
        fdt.extend_from_slice(strings);
        fdt
    }

    fn push_be32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_be_bytes());
    }

    fn push_padded_bytes(out: &mut Vec<u8>, value: &[u8]) {
        out.extend_from_slice(value);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
}
