#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
use core::panic::PanicInfo;

#[cfg(target_os = "none")]
core::arch::global_asm!(include_str!("start.S"));

#[cfg(all(target_os = "none", feature = "cache-sdm670-experiment"))]
mod cache;
#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
mod serial;

#[cfg(target_os = "none")]
const ARM64_IMAGE_MIN_SIZE: usize = 64;
#[cfg(target_os = "none")]
const ARM64_IMAGE_SIZE_OFFSET: usize = 16;
#[cfg(target_os = "none")]
const ARM64_IMAGE_MAGIC_OFFSET: usize = 56;
#[cfg(target_os = "none")]
const ARM64_IMAGE_MAGIC: u32 = u32::from_le_bytes(*b"ARM\x64");
#[cfg(target_os = "none")]
const PACKAGE_ALIGN: usize = 0x1000;
#[cfg(target_os = "none")]
const PACKAGE_MAGIC: &[u8; 8] = b"ABLXPKG1";
#[cfg(target_os = "none")]
const PACKAGE_HEADER_LEN: usize = 48;
#[cfg(target_os = "none")]
const PACKAGE_HEADER_SIZE_OFFSET: usize = 8;
#[cfg(target_os = "none")]
const PACKAGE_COMPRESSION_OFFSET: usize = 12;
#[cfg(target_os = "none")]
const PACKAGE_COMPRESSED_SIZE_OFFSET: usize = 16;
#[cfg(target_os = "none")]
const PACKAGE_UNCOMPRESSED_SIZE_OFFSET: usize = 24;
#[cfg(target_os = "none")]
const PACKAGE_IMAGE_SIZE_OFFSET: usize = 32;
#[cfg(target_os = "none")]
const PACKAGE_COMPRESSION_LZ4: u32 = 2;
#[cfg(target_os = "none")]
const RAMDISK_MAGIC: &[u8; 8] = b"ABLXRD1\0";
#[cfg(target_os = "none")]
const RAMDISK_HEADER_LEN: usize = 72;
#[cfg(target_os = "none")]
const RAMDISK_HEADER_SIZE_OFFSET: usize = 8;
#[cfg(target_os = "none")]
const RAMDISK_KERNEL_COMPRESSION_OFFSET: usize = 12;
#[cfg(target_os = "none")]
const RAMDISK_KERNEL_OFFSET_OFFSET: usize = 16;
#[cfg(target_os = "none")]
const RAMDISK_KERNEL_COMPRESSED_SIZE_OFFSET: usize = 24;
#[cfg(target_os = "none")]
const RAMDISK_KERNEL_UNCOMPRESSED_SIZE_OFFSET: usize = 32;
#[cfg(target_os = "none")]
const RAMDISK_KERNEL_IMAGE_SIZE_OFFSET: usize = 40;
#[cfg(target_os = "none")]
const RAMDISK_INITRD_OFFSET_OFFSET: usize = 48;
#[cfg(target_os = "none")]
const RAMDISK_INITRD_SIZE_OFFSET: usize = 56;
#[cfg(any(target_os = "none", test))]
const KERNEL_ALIGN: usize = 0x20_0000;
#[cfg(target_os = "none")]
const KERNEL_MIN_DESTINATION_OFFSET: usize = 0x0600_0000;
#[cfg(any(target_os = "none", test))]
const FDT_MAGIC: u32 = 0xd00d_feed;
#[cfg(any(target_os = "none", test))]
const FDT_HEADER_SIZE: usize = 40;
#[cfg(any(target_os = "none", test))]
const FDT_OFF_DT_STRUCT_OFFSET: usize = 8;
#[cfg(any(target_os = "none", test))]
const FDT_OFF_DT_STRINGS_OFFSET: usize = 12;
#[cfg(any(target_os = "none", test))]
const FDT_OFF_MEM_RSVMAP_OFFSET: usize = 16;
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
struct KernelPackage {
    source_start: usize,
    source_end: usize,
    compressed_source: usize,
    compressed_size: usize,
    uncompressed_size: usize,
    image_size: usize,
    end: usize,
    initrd_range: Option<(usize, usize)>,
}

#[cfg(target_os = "none")]
unsafe extern "C" {
    fn _start() -> !;
}

#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn abl_exorcist_main(fdt: usize) -> ! {
    serial_init();
    trace("ablx: enter\n");
    trace_hex("ablx: fdt ", fdt);
    trace_system_state();

    let Some(package) = read_ramdisk_kernel_package(fdt).or_else(|| {
        trace("ablx: no ramdisk package\n");
        let package_source = package_source()?;
        trace_hex("ablx: appended package ", package_source);
        read_kernel_package(package_source)
    }) else {
        fail("ablx: bad package\n");
    };
    trace_hex("ablx: package ", package.source_start);
    trace_hex("ablx: compressed ", package.compressed_size);
    trace_hex("ablx: image ", package.image_size);

    let Some(payload_entry) = kernel_destination(package.end, package.image_size, fdt) else {
        fail("ablx: no kernel destination\n");
    };
    trace_hex("ablx: kernel ", payload_entry);

    let cache_enabled = enable_cache_experiment(&package, payload_entry, fdt);
    let decompress_start = counter_ticks();
    if !decompress_kernel(&package, payload_entry) {
        fail("ablx: decompress failed\n");
    }
    trace_counter_elapsed(
        "ablx: decompress ticks ",
        "ablx: decompress ms ",
        decompress_start,
        counter_ticks(),
    );
    trace("ablx: decompressed\n");

    rewrite_initrd_range_from_raw_fdt(fdt, &package);
    rewrite_bootargs_from_raw_fdt(fdt);

    trace("ablx: clean+invalidate caches\n");
    let clean_start = counter_ticks();
    clean_invalidate_dcache_range(payload_entry, package.image_size);
    clean_invalidate_raw_fdt(fdt);
    invalidate_icache();
    trace_counter_elapsed(
        "ablx: clean ticks ",
        "ablx: clean ms ",
        clean_start,
        counter_ticks(),
    );
    trace("ablx: jump\n");

    final_handoff(cache_enabled, payload_entry, fdt)
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    trace("ablx: panic\n");
    halt()
}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn serial_init() {
    serial::init();
}

#[cfg(all(target_os = "none", not(feature = "serial-sdm670-uart12")))]
fn serial_init() {}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn trace(message: &str) {
    serial::write_str(message);
}

#[cfg(all(target_os = "none", not(feature = "serial-sdm670-uart12")))]
fn trace(_message: &str) {}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn trace_hex(label: &str, value: usize) {
    serial::write_hex(label, value);
}

#[cfg(all(target_os = "none", not(feature = "serial-sdm670-uart12")))]
fn trace_hex(_label: &str, _value: usize) {}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn trace_system_state() {
    let current_el = read_currentel();
    let sctlr = read_current_sctlr(current_el);

    trace_hex("ablx: currentel ", current_el);
    trace_hex("ablx: sctlr ", sctlr);
    trace_hex("ablx: sctlr.m ", (sctlr >> 0) & 1);
    trace_hex("ablx: sctlr.c ", (sctlr >> 2) & 1);
    trace_hex("ablx: sctlr.i ", (sctlr >> 12) & 1);
    trace_hex("ablx: cntfrq ", read_cntfrq_el0());
    trace_hex("ablx: cntpct ", counter_ticks());
}

#[cfg(all(target_os = "none", not(feature = "serial-sdm670-uart12")))]
fn trace_system_state() {}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn trace_counter_elapsed(ticks_label: &str, ms_label: &str, start: usize, end: usize) {
    let ticks = end.wrapping_sub(start);
    trace_hex(ticks_label, ticks);
    if let Some(ms) = counter_ticks_to_ms(ticks, read_cntfrq_el0()) {
        trace_hex(ms_label, ms);
    } else {
        trace("ablx: elapsed ms unavailable\n");
    }
}

#[cfg(all(target_os = "none", not(feature = "serial-sdm670-uart12")))]
fn trace_counter_elapsed(_ticks_label: &str, _ms_label: &str, _start: usize, _end: usize) {}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn counter_ticks_to_ms(ticks: usize, frequency: usize) -> Option<usize> {
    if frequency == 0 {
        return None;
    }

    let ms = (ticks as u128).checked_mul(1000)? / frequency as u128;
    usize::try_from(ms).ok()
}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn counter_ticks() -> usize {
    read_cntpct_el0()
}

#[cfg(all(target_os = "none", not(feature = "serial-sdm670-uart12")))]
fn counter_ticks() -> usize {
    0
}

#[cfg(all(target_os = "none", feature = "cache-sdm670-experiment"))]
fn enable_cache_experiment(package: &KernelPackage, payload: usize, fdt: usize) -> bool {
    trace("ablx: enable dcache\n");
    let enabled = cache::enable_for_sdm670(
        package.source_start,
        package.source_end,
        payload,
        package.image_size,
        fdt,
    );
    if enabled {
        trace_hex("ablx: sctlr after dcache ", cache::read_sctlr());
        trace_hex("ablx: sctlr.m after ", (cache::read_sctlr() >> 0) & 1);
        trace_hex("ablx: sctlr.c after ", (cache::read_sctlr() >> 2) & 1);
    } else {
        trace("ablx: dcache skipped\n");
    }
    enabled
}

#[cfg(all(target_os = "none", not(feature = "cache-sdm670-experiment")))]
fn enable_cache_experiment(_package: &KernelPackage, _payload: usize, _fdt: usize) -> bool {
    false
}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn read_currentel() -> usize {
    let value: usize;
    unsafe {
        core::arch::asm!("mrs {value}, CurrentEL", value = out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn read_current_sctlr(current_el: usize) -> usize {
    match (current_el >> 2) & 0x3 {
        1 => read_sctlr_el1(),
        2 => read_sctlr_el2(),
        3 => read_sctlr_el3(),
        _ => 0,
    }
}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn read_sctlr_el1() -> usize {
    let value: usize;
    unsafe {
        core::arch::asm!("mrs {value}, sctlr_el1", value = out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn read_sctlr_el2() -> usize {
    let value: usize;
    unsafe {
        core::arch::asm!("mrs {value}, sctlr_el2", value = out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn read_sctlr_el3() -> usize {
    let value: usize;
    unsafe {
        core::arch::asm!("mrs {value}, sctlr_el3", value = out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn read_cntfrq_el0() -> usize {
    let value: usize;
    unsafe {
        core::arch::asm!("mrs {value}, cntfrq_el0", value = out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

#[cfg(all(target_os = "none", feature = "serial-sdm670-uart12"))]
fn read_cntpct_el0() -> usize {
    let value: usize;
    unsafe {
        core::arch::asm!("mrs {value}, cntpct_el0", value = out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

#[cfg(target_os = "none")]
fn fail(message: &str) -> ! {
    trace(message);
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
fn package_source() -> Option<usize> {
    let base = _start as *const () as usize;
    let image_size = usize::try_from(read64(base + ARM64_IMAGE_SIZE_OFFSET)).ok()?;
    let offset = align_up_checked(image_size, PACKAGE_ALIGN)?;
    base.checked_add(offset)
}

#[cfg(target_os = "none")]
fn read_kernel_package(source: usize) -> Option<KernelPackage> {
    if !package_magic_matches(source) {
        trace("ablx: bad package magic\n");
        return None;
    }

    if read32(source + PACKAGE_HEADER_SIZE_OFFSET) as usize != PACKAGE_HEADER_LEN {
        trace("ablx: bad package header\n");
        return None;
    }
    if read32(source + PACKAGE_COMPRESSION_OFFSET) != PACKAGE_COMPRESSION_LZ4 {
        trace("ablx: bad package compression\n");
        return None;
    }

    let compressed_size = usize::try_from(read64(source + PACKAGE_COMPRESSED_SIZE_OFFSET)).ok()?;
    let uncompressed_size =
        usize::try_from(read64(source + PACKAGE_UNCOMPRESSED_SIZE_OFFSET)).ok()?;
    let image_size = usize::try_from(read64(source + PACKAGE_IMAGE_SIZE_OFFSET)).ok()?;
    if compressed_size == 0
        || uncompressed_size < ARM64_IMAGE_MIN_SIZE
        || image_size == 0
        || uncompressed_size > image_size
    {
        trace("ablx: bad package sizes\n");
        return None;
    }

    let compressed_source = source.checked_add(PACKAGE_HEADER_LEN)?;
    let end = compressed_source.checked_add(compressed_size)?;
    Some(KernelPackage {
        source_start: compressed_source,
        source_end: end,
        compressed_source,
        compressed_size,
        uncompressed_size,
        image_size,
        end,
        initrd_range: None,
    })
}

#[cfg(target_os = "none")]
fn read_ramdisk_kernel_package(fdt: usize) -> Option<KernelPackage> {
    let (fdt_start, fdt_end) = raw_fdt_range(fdt)?;
    let fdt_size = fdt_end.checked_sub(fdt_start)?;
    let fdt_slice = unsafe { core::slice::from_raw_parts(fdt_start as *const u8, fdt_size) };
    let Ok(Some((ramdisk_start, ramdisk_end))) = find_initrd_range(fdt_slice) else {
        return None;
    };

    trace_hex("ablx: ramdisk ", ramdisk_start);
    trace_hex("ablx: ramdisk end ", ramdisk_end);
    let ramdisk_size = ramdisk_end.checked_sub(ramdisk_start)?;
    let ramdisk = unsafe { core::slice::from_raw_parts(ramdisk_start as *const u8, ramdisk_size) };
    read_ramdisk_container(ramdisk_start, ramdisk)
}

#[cfg(target_os = "none")]
fn read_ramdisk_container(base: usize, ramdisk: &[u8]) -> Option<KernelPackage> {
    if ramdisk.get(..RAMDISK_MAGIC.len())? != RAMDISK_MAGIC {
        return None;
    }
    if read_le_u32(ramdisk, RAMDISK_HEADER_SIZE_OFFSET)? as usize != RAMDISK_HEADER_LEN {
        trace("ablx: bad ramdisk header\n");
        return None;
    }
    if read_le_u32(ramdisk, RAMDISK_KERNEL_COMPRESSION_OFFSET)? != PACKAGE_COMPRESSION_LZ4 {
        trace("ablx: bad ramdisk compression\n");
        return None;
    }

    let kernel_offset =
        usize::try_from(read_le_u64(ramdisk, RAMDISK_KERNEL_OFFSET_OFFSET)?).ok()?;
    let compressed_size =
        usize::try_from(read_le_u64(ramdisk, RAMDISK_KERNEL_COMPRESSED_SIZE_OFFSET)?).ok()?;
    let uncompressed_size = usize::try_from(read_le_u64(
        ramdisk,
        RAMDISK_KERNEL_UNCOMPRESSED_SIZE_OFFSET,
    )?)
    .ok()?;
    let image_size =
        usize::try_from(read_le_u64(ramdisk, RAMDISK_KERNEL_IMAGE_SIZE_OFFSET)?).ok()?;
    let initrd_offset =
        usize::try_from(read_le_u64(ramdisk, RAMDISK_INITRD_OFFSET_OFFSET)?).ok()?;
    let initrd_size = usize::try_from(read_le_u64(ramdisk, RAMDISK_INITRD_SIZE_OFFSET)?).ok()?;

    let kernel_end = kernel_offset.checked_add(compressed_size)?;
    let initrd_end_offset = initrd_offset.checked_add(initrd_size)?;
    if compressed_size == 0
        || uncompressed_size < ARM64_IMAGE_MIN_SIZE
        || image_size == 0
        || uncompressed_size > image_size
        || initrd_size == 0
        || kernel_offset < RAMDISK_HEADER_LEN
        || kernel_end > ramdisk.len()
        || initrd_offset < RAMDISK_HEADER_LEN
        || initrd_end_offset > ramdisk.len()
    {
        trace("ablx: bad ramdisk sizes\n");
        return None;
    }

    let compressed_source = base.checked_add(kernel_offset)?;
    let source_end = compressed_source.checked_add(compressed_size)?;
    let initrd_start = base.checked_add(initrd_offset)?;
    let initrd_end = initrd_start.checked_add(initrd_size)?;
    Some(KernelPackage {
        source_start: compressed_source,
        source_end,
        compressed_source,
        compressed_size,
        uncompressed_size,
        image_size,
        end: base.checked_add(ramdisk.len())?,
        initrd_range: Some((initrd_start, initrd_end)),
    })
}

#[cfg(target_os = "none")]
fn read_le_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

#[cfg(target_os = "none")]
fn read_le_u64(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset.checked_add(8)?)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

#[cfg(target_os = "none")]
fn package_magic_matches(source: usize) -> bool {
    let magic = unsafe { core::slice::from_raw_parts(source as *const u8, PACKAGE_MAGIC.len()) };
    magic == PACKAGE_MAGIC
}

#[cfg(target_os = "none")]
fn kernel_destination(package_end: usize, image_size: usize, fdt: usize) -> Option<usize> {
    let mut floor = package_end;
    let base = _start as *const () as usize;
    floor = floor.max(base.checked_add(KERNEL_MIN_DESTINATION_OFFSET)?);

    let mut fdt_slice = None;
    if let Some((_, fdt_end)) = raw_fdt_range(fdt) {
        floor = floor.max(fdt_end);

        let fdt_size = fdt_end.checked_sub(fdt)?;
        let slice = unsafe { core::slice::from_raw_parts(fdt as *const u8, fdt_size) };
        if let Ok(Some((_, initrd_end))) = find_initrd_range(slice) {
            floor = floor.max(initrd_end);
        }
        fdt_slice = Some(slice);
    }

    kernel_destination_after_floor(floor, image_size, fdt_slice)
}

#[cfg(any(target_os = "none", test))]
fn kernel_destination_after_floor(
    floor: usize,
    image_size: usize,
    fdt: Option<&[u8]>,
) -> Option<usize> {
    let mut destination = align_up_checked(floor, KERNEL_ALIGN)?;

    loop {
        let end = destination.checked_add(image_size)?;
        let Some(fdt) = fdt else {
            return Some(destination);
        };

        match find_reserved_memory_overlap(fdt, destination, end) {
            Ok(Some((reserved_start, reserved_end))) => {
                trace_reserved_skip(reserved_start, reserved_end);
                destination = align_up_checked(reserved_end, KERNEL_ALIGN)?;
            }
            Ok(None) | Err(_) => return Some(destination),
        }
    }
}

#[cfg(target_os = "none")]
fn trace_reserved_skip(start: usize, end: usize) {
    trace_hex("ablx: skip reserved ", start);
    trace_hex("ablx: reserved end ", end);
}

#[cfg(not(target_os = "none"))]
fn trace_reserved_skip(_start: usize, _end: usize) {}

#[cfg(target_os = "none")]
fn decompress_kernel(package: &KernelPackage, destination: usize) -> bool {
    let input = unsafe {
        core::slice::from_raw_parts(
            package.compressed_source as *const u8,
            package.compressed_size,
        )
    };
    let output = unsafe {
        core::slice::from_raw_parts_mut(destination as *mut u8, package.uncompressed_size)
    };

    let Some(written) = decompress_lz4_block(input, output) else {
        trace("ablx: lz4 failed\n");
        return false;
    };
    if written != package.uncompressed_size {
        trace("ablx: lz4 size mismatch\n");
        return false;
    }

    if read32(destination + ARM64_IMAGE_MAGIC_OFFSET) != ARM64_IMAGE_MAGIC {
        trace("ablx: bad kernel magic\n");
        return false;
    }
    if read64(destination + ARM64_IMAGE_SIZE_OFFSET) as usize != package.image_size {
        trace("ablx: bad kernel image size\n");
        return false;
    }

    let tail_start = destination + package.uncompressed_size;
    let tail_len = package.image_size - package.uncompressed_size;
    unsafe {
        core::ptr::write_bytes(tail_start as *mut u8, 0, tail_len);
    }

    true
}

#[cfg(any(target_os = "none", test))]
fn decompress_lz4_block(input: &[u8], output: &mut [u8]) -> Option<usize> {
    let mut input_pos = 0usize;
    let mut output_pos = 0usize;

    while input_pos < input.len() {
        let token = input[input_pos];
        input_pos += 1;

        let literal_len = read_lz4_length(input, &mut input_pos, usize::from(token >> 4))?;
        let literal_end = input_pos.checked_add(literal_len)?;
        let output_literal_end = output_pos.checked_add(literal_len)?;
        if literal_end > input.len() || output_literal_end > output.len() {
            return None;
        }
        output[output_pos..output_literal_end].copy_from_slice(&input[input_pos..literal_end]);
        input_pos = literal_end;
        output_pos = output_literal_end;

        if input_pos == input.len() {
            break;
        }
        if input_pos.checked_add(2)? > input.len() {
            return None;
        }

        let offset = u16::from_le_bytes([input[input_pos], input[input_pos + 1]]) as usize;
        input_pos += 2;
        if offset == 0 || offset > output_pos {
            return None;
        }

        let match_len =
            read_lz4_length(input, &mut input_pos, usize::from(token & 0x0f))?.checked_add(4)?;
        let match_end = output_pos.checked_add(match_len)?;
        if match_end > output.len() {
            return None;
        }

        copy_lz4_match(output, output_pos, offset, match_len);
        output_pos = match_end;
    }

    Some(output_pos)
}

#[cfg(any(target_os = "none", test))]
fn read_lz4_length(input: &[u8], input_pos: &mut usize, mut len: usize) -> Option<usize> {
    if len != 15 {
        return Some(len);
    }

    loop {
        let extra = *input.get(*input_pos)?;
        *input_pos += 1;
        len = len.checked_add(usize::from(extra))?;
        if extra != 255 {
            return Some(len);
        }
    }
}

#[cfg(any(target_os = "none", test))]
fn copy_lz4_match(output: &mut [u8], output_pos: usize, offset: usize, len: usize) {
    if offset >= len {
        let source_start = output_pos - offset;
        let source_end = source_start + len;
        output.copy_within(source_start..source_end, output_pos);
        return;
    }

    let mut copied = 0usize;
    while copied < len {
        output[output_pos + copied] = output[output_pos - offset + copied];
        copied += 1;
    }
}

#[cfg(target_os = "none")]
fn raw_fdt_range(fdt: usize) -> Option<(usize, usize)> {
    if fdt == 0 || read_be_u32_raw(fdt) != FDT_MAGIC {
        return None;
    }

    let totalsize = read_be_u32_raw(fdt + 4) as usize;
    if totalsize < FDT_HEADER_SIZE {
        return None;
    }
    Some((fdt, fdt.checked_add(totalsize)?))
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
        trace("ablx: bootargs: bad fdt\n");
        return;
    }

    let totalsize = read_be_u32_raw(fdt + 4) as usize;
    if totalsize < FDT_HEADER_SIZE {
        trace("ablx: bootargs: bad fdt size\n");
        return;
    }

    let fdt = unsafe { core::slice::from_raw_parts_mut(fdt as *mut u8, totalsize) };
    match rewrite_bootargs(fdt) {
        Ok(true) => trace("ablx: bootargs rewritten\n"),
        Ok(false) => trace("ablx: bootargs unchanged\n"),
        Err(_) => trace("ablx: bootargs failed\n"),
    }
}

#[cfg(target_os = "none")]
fn rewrite_initrd_range_from_raw_fdt(fdt: usize, package: &KernelPackage) {
    let Some((initrd_start, initrd_end)) = package.initrd_range else {
        return;
    };
    if fdt == 0 || read_be_u32_raw(fdt) != FDT_MAGIC {
        trace("ablx: initrd rewrite: bad fdt\n");
        return;
    }

    let totalsize = read_be_u32_raw(fdt + 4) as usize;
    if totalsize < FDT_HEADER_SIZE {
        trace("ablx: initrd rewrite: bad fdt size\n");
        return;
    }

    let fdt = unsafe { core::slice::from_raw_parts_mut(fdt as *mut u8, totalsize) };
    match rewrite_initrd_range(fdt, initrd_start, initrd_end) {
        Ok(()) => trace("ablx: initrd rewritten\n"),
        Err(_) => trace("ablx: initrd rewrite failed\n"),
    }
}

#[cfg(target_os = "none")]
fn read_be_u32_raw(address: usize) -> u32 {
    u32::from_be(unsafe { (address as *const u32).read_unaligned() })
}

#[cfg(target_os = "none")]
fn clean_invalidate_raw_fdt(fdt: usize) {
    if fdt == 0 || read_be_u32_raw(fdt) != FDT_MAGIC {
        return;
    }

    clean_invalidate_dcache_range(fdt, read_be_u32_raw(fdt + 4) as usize);
}

#[cfg(target_os = "none")]
fn clean_invalidate_dcache_range(start: usize, size: usize) {
    if size == 0 {
        return;
    }

    let line_size = dcache_line_size();
    let mut address = align_down(start, line_size);
    let end = align_up(start.saturating_add(size), line_size);

    while address < end {
        unsafe {
            core::arch::asm!("dc civac, {address}", address = in(reg) address, options(nostack, preserves_flags));
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
fn final_handoff(cache_enabled: bool, entry: usize, fdt: usize) -> ! {
    unsafe {
        core::arch::asm!(
            "cbz x15, 2f",
            "mrs x8, CurrentEL",
            "and x8, x8, #0xc",
            "cmp x8, #0x4",
            "b.ne 2f",
            "dsb sy",
            "mrs x8, sctlr_el1",
            "mov x9, #5",
            "bic x8, x8, x9",
            "msr sctlr_el1, x8",
            "isb",
            "dsb sy",
            "tlbi vmalle1",
            "dsb sy",
            "isb",
            "2:",
            "mov x1, xzr",
            "mov x2, xzr",
            "mov x3, xzr",
            "br x16",
            in("x0") fdt,
            in("x16") entry,
            in("x15") cache_enabled as usize,
            options(noreturn, nostack)
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
#[derive(Clone, Copy)]
struct FdtLayout {
    mem_rsvmap_offset: usize,
    struct_offset: usize,
    struct_end: usize,
    strings_offset: usize,
    strings_end: usize,
}

#[cfg(any(target_os = "none", test))]
fn fdt_layout(fdt: &[u8]) -> Result<FdtLayout, FdtError> {
    if fdt.len() < FDT_HEADER_SIZE || be32_at(fdt, 0)? != FDT_MAGIC {
        return Err(FdtError::BadHeader);
    }

    let mem_rsvmap_offset = be32_at(fdt, FDT_OFF_MEM_RSVMAP_OFFSET)? as usize;
    let struct_offset = be32_at(fdt, FDT_OFF_DT_STRUCT_OFFSET)? as usize;
    let strings_offset = be32_at(fdt, FDT_OFF_DT_STRINGS_OFFSET)? as usize;
    let strings_size = be32_at(fdt, FDT_SIZE_DT_STRINGS_OFFSET)? as usize;
    let struct_size = be32_at(fdt, FDT_SIZE_DT_STRUCT_OFFSET)? as usize;
    let struct_end = checked_end(struct_offset, struct_size, fdt.len())?;
    let strings_end = checked_end(strings_offset, strings_size, fdt.len())?;
    if mem_rsvmap_offset >= fdt.len() {
        return Err(FdtError::BadStructure);
    }

    Ok(FdtLayout {
        mem_rsvmap_offset,
        struct_offset,
        struct_end,
        strings_offset,
        strings_end,
    })
}

#[cfg(any(target_os = "none", test))]
fn find_reserved_memory_overlap(
    fdt: &[u8],
    start: usize,
    end: usize,
) -> Result<Option<(usize, usize)>, FdtError> {
    if end <= start {
        return Ok(None);
    }

    if let Some(range) = find_mem_rsvmap_overlap(fdt, start, end)? {
        return Ok(Some(range));
    }
    find_reserved_memory_node_overlap(fdt, start, end)
}

#[cfg(any(target_os = "none", test))]
fn find_mem_rsvmap_overlap(
    fdt: &[u8],
    start: usize,
    end: usize,
) -> Result<Option<(usize, usize)>, FdtError> {
    let layout = fdt_layout(fdt)?;
    let mut pos = layout.mem_rsvmap_offset;

    loop {
        let entry_end = checked_end(pos, 16, fdt.len())?;
        let address = be64_at(fdt, pos)?;
        let size = be64_at(fdt, pos + 8)?;
        pos = entry_end;

        if address == 0 && size == 0 {
            return Ok(None);
        }
        if size == 0 {
            continue;
        }

        let reserved_start = usize::try_from(address).map_err(|_| FdtError::BadStructure)?;
        let reserved_size = usize::try_from(size).map_err(|_| FdtError::BadStructure)?;
        let reserved_end = reserved_start
            .checked_add(reserved_size)
            .ok_or(FdtError::BadStructure)?;
        if ranges_overlap(start, end, reserved_start, reserved_end) {
            return Ok(Some((reserved_start, reserved_end)));
        }
    }
}

#[cfg(any(target_os = "none", test))]
fn find_reserved_memory_node_overlap(
    fdt: &[u8],
    start: usize,
    end: usize,
) -> Result<Option<(usize, usize)>, FdtError> {
    let layout = fdt_layout(fdt)?;
    let mut pos = layout.struct_offset;
    let mut depth = 0usize;
    let mut root_address_cells = 2usize;
    let mut root_size_cells = 1usize;
    let mut reserved_depth = None;
    let mut reserved_address_cells = root_address_cells;
    let mut reserved_size_cells = root_size_cells;

    while pos < layout.struct_end {
        let token = be32_at(fdt, pos)?;
        pos += 4;

        match token {
            FDT_BEGIN_NODE => {
                let name_start = pos;
                while pos < layout.struct_end && fdt[pos] != 0 {
                    pos += 1;
                }
                if pos == layout.struct_end {
                    return Err(FdtError::BadStructure);
                }
                let name = &fdt[name_start..pos];
                pos = align_up(pos + 1, 4);

                if depth == 1 && name == b"reserved-memory" {
                    reserved_depth = Some(depth + 1);
                    reserved_address_cells = root_address_cells;
                    reserved_size_cells = root_size_cells;
                }
                depth += 1;
            }
            FDT_END_NODE => {
                if depth == 0 {
                    return Err(FdtError::BadStructure);
                }
                if reserved_depth == Some(depth) {
                    reserved_depth = None;
                }
                depth -= 1;
            }
            FDT_PROP => {
                let len = be32_at(fdt, pos)? as usize;
                let name_offset = be32_at(fdt, pos + 4)? as usize;
                pos += 8;
                let value_offset = pos;
                let value_end = checked_end(value_offset, len, layout.struct_end)?;
                pos = align_up(value_end, 4);

                let name = string_at(fdt, layout.strings_offset, layout.strings_end, name_offset)?;
                if depth == 1 {
                    match name {
                        b"#address-cells" => {
                            root_address_cells = read_u32_prop(fdt, value_offset, len)?
                        }
                        b"#size-cells" => root_size_cells = read_u32_prop(fdt, value_offset, len)?,
                        _ => {}
                    }
                }

                if reserved_depth == Some(depth) {
                    match name {
                        b"#address-cells" => {
                            reserved_address_cells = read_u32_prop(fdt, value_offset, len)?
                        }
                        b"#size-cells" => {
                            reserved_size_cells = read_u32_prop(fdt, value_offset, len)?
                        }
                        _ => {}
                    }
                }

                if let Some(reserved_depth) = reserved_depth {
                    if depth == reserved_depth + 1
                        && name == b"reg"
                        && let Some(range) = find_reg_overlap(
                            fdt,
                            value_offset,
                            len,
                            reserved_address_cells,
                            reserved_size_cells,
                            start,
                            end,
                        )?
                    {
                        return Ok(Some(range));
                    }
                }
            }
            FDT_NOP => {}
            FDT_END => break,
            _ => return Err(FdtError::BadStructure),
        }
    }

    Ok(None)
}

#[cfg(any(target_os = "none", test))]
fn find_reg_overlap(
    fdt: &[u8],
    offset: usize,
    len: usize,
    address_cells: usize,
    size_cells: usize,
    start: usize,
    end: usize,
) -> Result<Option<(usize, usize)>, FdtError> {
    let entry_cells = address_cells
        .checked_add(size_cells)
        .ok_or(FdtError::BadStructure)?;
    let entry_len = entry_cells.checked_mul(4).ok_or(FdtError::BadStructure)?;
    if entry_len == 0 || !len.is_multiple_of(entry_len) || size_cells == 0 {
        return Err(FdtError::BadStructure);
    }

    let value_end = checked_end(offset, len, fdt.len())?;
    let mut pos = offset;
    while pos < value_end {
        let reserved_start = read_cells(fdt, pos, address_cells)?;
        pos += address_cells * 4;
        let reserved_size = read_cells(fdt, pos, size_cells)?;
        pos += size_cells * 4;
        if reserved_size == 0 {
            continue;
        }

        let reserved_end = reserved_start
            .checked_add(reserved_size)
            .ok_or(FdtError::BadStructure)?;
        if ranges_overlap(start, end, reserved_start, reserved_end) {
            return Ok(Some((reserved_start, reserved_end)));
        }
    }

    Ok(None)
}

#[cfg(any(target_os = "none", test))]
fn read_u32_prop(fdt: &[u8], offset: usize, len: usize) -> Result<usize, FdtError> {
    if len != 4 {
        return Err(FdtError::BadStructure);
    }
    Ok(be32_at(fdt, offset)? as usize)
}

#[cfg(any(target_os = "none", test))]
fn read_cells(fdt: &[u8], offset: usize, cells: usize) -> Result<usize, FdtError> {
    if cells > (usize::BITS as usize / 32) {
        return Err(FdtError::BadStructure);
    }

    let mut value = 0usize;
    let mut pos = offset;
    for _ in 0..cells {
        value = (value << 32) | be32_at(fdt, pos)? as usize;
        pos += 4;
    }
    Ok(value)
}

#[cfg(any(target_os = "none", test))]
fn ranges_overlap(start: usize, end: usize, other_start: usize, other_end: usize) -> bool {
    start < other_end && other_start < end
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
fn rewrite_initrd_range(fdt: &mut [u8], start: usize, end: usize) -> Result<(), FdtError> {
    if end <= start {
        return Err(FdtError::BadStructure);
    }

    let start_prop = find_chosen_prop(fdt, b"linux,initrd-start")?;
    let end_prop = find_chosen_prop(fdt, b"linux,initrd-end")?;
    write_address_prop(fdt, start_prop, start)?;
    write_address_prop(fdt, end_prop, end)?;
    Ok(())
}

#[cfg(any(target_os = "none", test))]
fn find_bootargs(fdt: &[u8]) -> Result<(usize, usize), FdtError> {
    find_chosen_prop(fdt, b"bootargs")
}

#[cfg(any(target_os = "none", test))]
fn find_initrd_range(fdt: &[u8]) -> Result<Option<(usize, usize)>, FdtError> {
    let start = match find_chosen_prop(fdt, b"linux,initrd-start") {
        Ok(prop) => prop,
        Err(FdtError::NotFound) => return Ok(None),
        Err(err) => return Err(err),
    };
    let end = match find_chosen_prop(fdt, b"linux,initrd-end") {
        Ok(prop) => prop,
        Err(FdtError::NotFound) => return Ok(None),
        Err(err) => return Err(err),
    };

    let start = read_address_prop(fdt, start)?;
    let end = read_address_prop(fdt, end)?;
    if end <= start {
        return Err(FdtError::BadStructure);
    }

    Ok(Some((start, end)))
}

#[cfg(any(target_os = "none", test))]
fn read_address_prop(fdt: &[u8], prop: (usize, usize)) -> Result<usize, FdtError> {
    let (offset, len) = prop;
    match len {
        4 => Ok(be32_at(fdt, offset)? as usize),
        8 => {
            let bytes = fdt.get(offset..offset + 8).ok_or(FdtError::BadStructure)?;
            let value = u64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]);
            usize::try_from(value).map_err(|_| FdtError::BadStructure)
        }
        _ => Err(FdtError::BadStructure),
    }
}

#[cfg(any(target_os = "none", test))]
fn write_address_prop(fdt: &mut [u8], prop: (usize, usize), value: usize) -> Result<(), FdtError> {
    let (offset, len) = prop;
    match len {
        4 => {
            let value = u32::try_from(value).map_err(|_| FdtError::BadStructure)?;
            let bytes = fdt
                .get_mut(offset..offset + 4)
                .ok_or(FdtError::BadStructure)?;
            bytes.copy_from_slice(&value.to_be_bytes());
            Ok(())
        }
        8 => {
            let value = u64::try_from(value).map_err(|_| FdtError::BadStructure)?;
            let bytes = fdt
                .get_mut(offset..offset + 8)
                .ok_or(FdtError::BadStructure)?;
            bytes.copy_from_slice(&value.to_be_bytes());
            Ok(())
        }
        _ => Err(FdtError::BadStructure),
    }
}

#[cfg(any(target_os = "none", test))]
fn find_chosen_prop(fdt: &[u8], property_name: &[u8]) -> Result<(usize, usize), FdtError> {
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
                    && string_at(fdt, strings_offset, strings_end, name_offset)? == property_name
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
fn be64_at(data: &[u8], offset: usize) -> Result<u64, FdtError> {
    let bytes = data.get(offset..offset + 8).ok_or(FdtError::BadStructure)?;
    Ok(u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
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

#[cfg(any(target_os = "none", test))]
fn align_up_checked(value: usize, alignment: usize) -> Option<usize> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return None;
    }

    let mask = alignment - 1;
    Some(value.checked_add(mask)? & !mask)
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

    #[test]
    fn finds_initrd_range_in_fdt() {
        let start = 0x0400_0000u64.to_be_bytes();
        let end = 0x0570_0000u64.to_be_bytes();
        let fdt = test_fdt_props(&[
            (&b"linux,initrd-start"[..], start.as_slice()),
            (&b"linux,initrd-end"[..], end.as_slice()),
        ]);

        assert_eq!(
            find_initrd_range(&fdt),
            Ok(Some((0x0400_0000, 0x0570_0000)))
        );
    }

    #[test]
    fn rewrites_initrd_range_in_fdt() {
        let start = 0x0400_0000u64.to_be_bytes();
        let end = 0x0570_0000u64.to_be_bytes();
        let mut fdt = test_fdt_props(&[
            (&b"linux,initrd-start"[..], start.as_slice()),
            (&b"linux,initrd-end"[..], end.as_slice()),
        ]);

        assert_eq!(
            rewrite_initrd_range(&mut fdt, 0x0450_0000, 0x05c0_0000),
            Ok(())
        );
        assert_eq!(
            find_initrd_range(&fdt),
            Ok(Some((0x0450_0000, 0x05c0_0000)))
        );
    }

    #[test]
    fn rewrites_32_bit_initrd_range_in_fdt() {
        let start = 0x0400_0000u32.to_be_bytes();
        let end = 0x0570_0000u32.to_be_bytes();
        let mut fdt = test_fdt_props(&[
            (&b"linux,initrd-start"[..], start.as_slice()),
            (&b"linux,initrd-end"[..], end.as_slice()),
        ]);

        assert_eq!(
            rewrite_initrd_range(&mut fdt, 0x0450_0000, 0x05c0_0000),
            Ok(())
        );
        assert_eq!(
            find_initrd_range(&fdt),
            Ok(Some((0x0450_0000, 0x05c0_0000)))
        );
    }

    #[test]
    fn missing_initrd_range_is_none() {
        let fdt = test_fdt(b"root=/dev/dm-0\0");

        assert_eq!(find_initrd_range(&fdt), Ok(None));
    }

    #[test]
    fn kernel_destination_skips_reserved_memory_node() {
        let fdt = test_fdt_with_reserved(&[], &[(0x8620_0000, 0x02d0_0000)]);

        assert_eq!(
            kernel_destination_after_floor(0x8608_0000, 0x00a9_0000, Some(&fdt)),
            Some(0x8900_0000)
        );
    }

    #[test]
    fn kernel_destination_skips_memreserve_entry() {
        let fdt = test_fdt_with_memreserve(&[(0x8620_0000, 0x02d0_0000)]);

        assert_eq!(
            kernel_destination_after_floor(0x8608_0000, 0x00a9_0000, Some(&fdt)),
            Some(0x8900_0000)
        );
    }

    #[test]
    fn decompresses_lz4_literals() {
        let mut out = [0; 20];
        let input = [
            0xf0, 5, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j', b'k', b'l', b'm',
            b'n', b'o', b'p', b'q', b'r', b's', b't',
        ];

        assert_eq!(decompress_lz4_block(&input, &mut out), Some(20));
        assert_eq!(&out, b"abcdefghijklmnopqrst");
    }

    #[test]
    fn decompresses_lz4_overlapping_match() {
        let mut out = [0; 6];
        let input = [0x11, b'a', 1, 0];

        assert_eq!(decompress_lz4_block(&input, &mut out), Some(6));
        assert_eq!(&out, b"aaaaaa");
    }

    fn cstr(value: &[u8]) -> &[u8] {
        &value[..nul_terminated_len(value)]
    }

    fn test_fdt(bootargs: &[u8]) -> Vec<u8> {
        test_fdt_props(&[(&b"bootargs"[..], bootargs)])
    }

    fn test_fdt_props(props: &[(&[u8], &[u8])]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut name_offsets = Vec::new();
        for (name, _) in props {
            name_offsets.push(strings.len() as u32);
            strings.extend_from_slice(name);
            strings.push(0);
        }

        let mut structure = Vec::new();
        push_be32(&mut structure, FDT_BEGIN_NODE);
        push_padded_bytes(&mut structure, b"\0");
        push_be32(&mut structure, FDT_BEGIN_NODE);
        push_padded_bytes(&mut structure, b"chosen\0");
        for ((_, value), name_offset) in props.iter().zip(name_offsets.iter()) {
            push_be32(&mut structure, FDT_PROP);
            push_be32(&mut structure, value.len() as u32);
            push_be32(&mut structure, *name_offset);
            push_padded_bytes(&mut structure, value);
        }
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
        fdt.extend_from_slice(&strings);
        fdt
    }

    fn test_fdt_with_reserved(memreserve: &[(u64, u64)], reserved: &[(u64, u64)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let address_cells_offset = add_string(&mut strings, b"#address-cells");
        let size_cells_offset = add_string(&mut strings, b"#size-cells");
        let reg_offset = add_string(&mut strings, b"reg");

        let mut structure = Vec::new();
        push_be32(&mut structure, FDT_BEGIN_NODE);
        push_padded_bytes(&mut structure, b"\0");
        push_u32_prop(&mut structure, address_cells_offset, 2);
        push_u32_prop(&mut structure, size_cells_offset, 2);

        push_be32(&mut structure, FDT_BEGIN_NODE);
        push_padded_bytes(&mut structure, b"reserved-memory\0");
        push_u32_prop(&mut structure, address_cells_offset, 2);
        push_u32_prop(&mut structure, size_cells_offset, 2);

        for (address, size) in reserved {
            let mut reg = Vec::new();
            push_be64(&mut reg, *address);
            push_be64(&mut reg, *size);

            push_be32(&mut structure, FDT_BEGIN_NODE);
            push_padded_bytes(&mut structure, b"reserved\0");
            push_prop(&mut structure, reg_offset, &reg);
            push_be32(&mut structure, FDT_END_NODE);
        }

        push_be32(&mut structure, FDT_END_NODE);
        push_be32(&mut structure, FDT_END_NODE);
        push_be32(&mut structure, FDT_END);

        finish_test_fdt(memreserve, &structure, &strings)
    }

    fn test_fdt_with_memreserve(memreserve: &[(u64, u64)]) -> Vec<u8> {
        let mut structure = Vec::new();
        push_be32(&mut structure, FDT_BEGIN_NODE);
        push_padded_bytes(&mut structure, b"\0");
        push_be32(&mut structure, FDT_END_NODE);
        push_be32(&mut structure, FDT_END);

        finish_test_fdt(memreserve, &structure, &[])
    }

    fn finish_test_fdt(memreserve: &[(u64, u64)], structure: &[u8], strings: &[u8]) -> Vec<u8> {
        let off_mem_rsvmap = FDT_HEADER_SIZE;
        let mem_rsvmap_len = (memreserve.len() + 1) * 16;
        let off_dt_struct = off_mem_rsvmap + mem_rsvmap_len;
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
        for (address, size) in memreserve {
            push_be64(&mut fdt, *address);
            push_be64(&mut fdt, *size);
        }
        push_be64(&mut fdt, 0);
        push_be64(&mut fdt, 0);
        fdt.extend_from_slice(structure);
        fdt.extend_from_slice(strings);
        fdt
    }

    fn add_string(strings: &mut Vec<u8>, value: &[u8]) -> u32 {
        let offset = strings.len() as u32;
        strings.extend_from_slice(value);
        strings.push(0);
        offset
    }

    fn push_u32_prop(out: &mut Vec<u8>, name_offset: u32, value: u32) {
        push_prop(out, name_offset, &value.to_be_bytes());
    }

    fn push_prop(out: &mut Vec<u8>, name_offset: u32, value: &[u8]) {
        push_be32(out, FDT_PROP);
        push_be32(out, value.len() as u32);
        push_be32(out, name_offset);
        push_padded_bytes(out, value);
    }

    fn push_be32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_be_bytes());
    }

    fn push_be64(out: &mut Vec<u8>, value: u64) {
        out.extend_from_slice(&value.to_be_bytes());
    }

    fn push_padded_bytes(out: &mut Vec<u8>, value: &[u8]) {
        out.extend_from_slice(value);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
}
