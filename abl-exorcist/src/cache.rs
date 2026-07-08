const PAGE_TABLE_ENTRIES: usize = 512;

const UART_WINDOW_BASE: usize = 0x00a0_0000;
const UART_WINDOW_L2_INDEX: usize = UART_WINDOW_BASE >> 21;
const DRAM_BASE: usize = 0x8000_0000;
const DRAM_L1_INDEX: usize = DRAM_BASE >> 30;

const PTE_TYPE_BLOCK: u64 = 1 << 0;
const PTE_TYPE_TABLE: u64 = 3 << 0;
const PTE_AF: u64 = 1 << 10;
const PTE_INNER_SHAREABLE: u64 = 3 << 8;
const PTE_ATTR_DEVICE_NGNRNE: u64 = 0 << 2;
const PTE_ATTR_NORMAL: u64 = 1 << 2;
const PTE_PXN: u64 = 1 << 53;
const PTE_UXN: u64 = 1 << 54;

const MAIR_DEVICE_NGNRNE: u64 = 0x00;
const MAIR_NORMAL_WBWA: u64 = 0xff << 8;
const MAIR_EL1_VALUE: usize = (MAIR_DEVICE_NGNRNE | MAIR_NORMAL_WBWA) as usize;

const TCR_EL1_RSVD: usize = 1 << 31;
const TCR_EPD1_DISABLE: usize = 1 << 23;
const TCR_SHARED_INNER: usize = 3 << 12;
const TCR_ORGN_WBWA: usize = 1 << 10;
const TCR_IRGN_WBWA: usize = 1 << 8;
const TCR_T0SZ_32BIT_VA: usize = 64 - 32;
const TCR_EL1_VALUE: usize = TCR_EL1_RSVD
    | TCR_EPD1_DISABLE
    | TCR_SHARED_INNER
    | TCR_ORGN_WBWA
    | TCR_IRGN_WBWA
    | TCR_T0SZ_32BIT_VA;

const SCTLR_M: usize = 1 << 0;
const SCTLR_C: usize = 1 << 2;

#[repr(align(4096))]
#[allow(dead_code)]
struct PageTable([u64; PAGE_TABLE_ENTRIES]);

static mut L1_TABLE: PageTable = PageTable([0; PAGE_TABLE_ENTRIES]);
static mut L2_LOW_TABLE: PageTable = PageTable([0; PAGE_TABLE_ENTRIES]);

pub fn enable_for_sdm670(
    source_start: usize,
    source_end: usize,
    payload: usize,
    image_size: usize,
    fdt: usize,
) -> bool {
    if current_el() != 1 {
        return false;
    }

    invalidate_range(source_start, source_end.saturating_sub(source_start));
    invalidate_range(payload, image_size);
    if let Some((fdt_start, fdt_len)) = raw_fdt_range(fdt) {
        invalidate_range(fdt_start, fdt_len);
    }

    unsafe {
        setup_tables();
    }

    dsb_sy();
    write_mair_el1(MAIR_EL1_VALUE);
    write_tcr_el1(TCR_EL1_VALUE);
    write_ttbr0_el1(l1_table_address());
    dsb_sy();
    invalidate_tlb_el1();

    write_sctlr_el1(read_sctlr_el1() | SCTLR_M | SCTLR_C);
    true
}

pub fn read_sctlr() -> usize {
    read_sctlr_el1()
}

unsafe fn setup_tables() {
    let l1 = unsafe { l1_table() };
    let l2 = unsafe { l2_low_table() };

    unsafe {
        zero_table(l1);
        zero_table(l2);

        l2.add(UART_WINDOW_L2_INDEX).write_volatile(
            UART_WINDOW_BASE as u64
                | PTE_ATTR_DEVICE_NGNRNE
                | PTE_AF
                | PTE_PXN
                | PTE_UXN
                | PTE_TYPE_BLOCK,
        );
        l1.add(0)
            .write_volatile(l2_low_table_address() as u64 | PTE_TYPE_TABLE);
        l1.add(DRAM_L1_INDEX).write_volatile(
            DRAM_BASE as u64 | PTE_ATTR_NORMAL | PTE_INNER_SHAREABLE | PTE_AF | PTE_TYPE_BLOCK,
        );
    }
}

unsafe fn zero_table(table: *mut u64) {
    for index in 0..PAGE_TABLE_ENTRIES {
        unsafe {
            table.add(index).write_volatile(0);
        }
    }
}

fn l1_table_address() -> usize {
    unsafe { l1_table() as usize }
}

fn l2_low_table_address() -> usize {
    unsafe { l2_low_table() as usize }
}

unsafe fn l1_table() -> *mut u64 {
    core::ptr::addr_of_mut!(L1_TABLE).cast::<u64>()
}

unsafe fn l2_low_table() -> *mut u64 {
    core::ptr::addr_of_mut!(L2_LOW_TABLE).cast::<u64>()
}

fn raw_fdt_range(fdt: usize) -> Option<(usize, usize)> {
    if fdt == 0 || read_be_u32_raw(fdt) != 0xd00d_feed {
        return None;
    }

    let totalsize = read_be_u32_raw(fdt + 4) as usize;
    if totalsize < 40 {
        return None;
    }
    Some((fdt, totalsize))
}

fn read_be_u32_raw(address: usize) -> u32 {
    u32::from_be(unsafe { (address as *const u32).read_unaligned() })
}

fn invalidate_range(start: usize, size: usize) {
    if size == 0 {
        return;
    }

    let line_size = dcache_line_size();
    let mut address = align_down(start, line_size);
    let end = align_up(start.saturating_add(size), line_size);

    while address < end {
        unsafe {
            core::arch::asm!("dc ivac, {address}", address = in(reg) address, options(nostack, preserves_flags));
        }
        address += line_size;
    }
    dsb_sy();
}

fn dcache_line_size() -> usize {
    let ctr: usize;
    unsafe {
        core::arch::asm!("mrs {ctr}, ctr_el0", ctr = out(reg) ctr, options(nomem, nostack, preserves_flags));
    }

    4 << ((ctr >> 16) & 0xf)
}

fn align_down(value: usize, alignment: usize) -> usize {
    value & !(alignment - 1)
}

fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

fn current_el() -> usize {
    let value: usize;
    unsafe {
        core::arch::asm!("mrs {value}, CurrentEL", value = out(reg) value, options(nomem, nostack, preserves_flags));
    }
    (value >> 2) & 0x3
}

fn read_sctlr_el1() -> usize {
    let value: usize;
    unsafe {
        core::arch::asm!("mrs {value}, sctlr_el1", value = out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn write_sctlr_el1(value: usize) {
    unsafe {
        core::arch::asm!("msr sctlr_el1, {value}", "isb", value = in(reg) value, options(nostack, preserves_flags));
    }
}

fn write_mair_el1(value: usize) {
    unsafe {
        core::arch::asm!("msr mair_el1, {value}", value = in(reg) value, options(nostack, preserves_flags));
    }
}

fn write_tcr_el1(value: usize) {
    unsafe {
        core::arch::asm!("msr tcr_el1, {value}", value = in(reg) value, options(nostack, preserves_flags));
    }
}

fn write_ttbr0_el1(value: usize) {
    unsafe {
        core::arch::asm!("msr ttbr0_el1, {value}", value = in(reg) value, options(nostack, preserves_flags));
    }
}

fn invalidate_tlb_el1() {
    unsafe {
        core::arch::asm!(
            "dsb sy",
            "tlbi vmalle1",
            "dsb sy",
            "isb",
            options(nostack, preserves_flags)
        );
    }
}

fn dsb_sy() {
    unsafe {
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    }
}
