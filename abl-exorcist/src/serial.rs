use core::sync::atomic::{AtomicBool, Ordering};

const UART12_BASE: usize = 0x00a9_0000;

const GENI_FORCE_DEFAULT_REG: usize = 0x20;
const GENI_OUTPUT_CTRL: usize = 0x24;
const SE_GENI_CGC_CTRL: usize = 0x28;
const SE_GENI_STATUS: usize = 0x40;
const GENI_FW_REVISION_RO: usize = 0x68;
const SE_GENI_DMA_MODE_EN: usize = 0x258;
const SE_GENI_BYTE_GRAN: usize = 0x254;
const SE_UART_TX_TRANS_CFG: usize = 0x25c;
const SE_GENI_TX_PACKING_CFG0: usize = 0x260;
const SE_GENI_TX_PACKING_CFG1: usize = 0x264;
const SE_UART_TX_WORD_LEN: usize = 0x268;
const SE_UART_TX_STOP_BIT_LEN: usize = 0x26c;
const SE_UART_TX_TRANS_LEN: usize = 0x270;
const SE_UART_RX_TRANS_CFG: usize = 0x280;
const SE_GENI_RX_PACKING_CFG0: usize = 0x284;
const SE_GENI_RX_PACKING_CFG1: usize = 0x288;
const SE_UART_RX_WORD_LEN: usize = 0x28c;
const SE_UART_TX_PARITY_CFG: usize = 0x2a4;
const SE_UART_RX_PARITY_CFG: usize = 0x2a8;
const SE_GENI_M_CMD0: usize = 0x600;
const SE_GENI_M_CMD_CTRL_REG: usize = 0x604;
const SE_GENI_M_IRQ_STATUS: usize = 0x610;
const SE_GENI_M_IRQ_EN: usize = 0x614;
const SE_GENI_M_IRQ_CLEAR: usize = 0x618;
const SE_GENI_S_CMD_CTRL_REG: usize = 0x634;
const SE_GENI_S_IRQ_STATUS: usize = 0x640;
const SE_GENI_S_IRQ_EN: usize = 0x644;
const SE_GENI_S_IRQ_CLEAR: usize = 0x648;
const SE_GENI_TX_FIFON: usize = 0x700;
const SE_GENI_RX_WATERMARK_REG: usize = 0x810;
const SE_GENI_RX_RFR_WATERMARK_REG: usize = 0x814;
const SE_GSI_EVENT_EN: usize = 0xe18;
const SE_IRQ_EN: usize = 0xe1c;
const SE_DMA_GENERAL_CFG: usize = 0xe30;
const SE_DMA_TX_IRQ_CLR: usize = 0xc44;
const SE_DMA_RX_IRQ_CLR: usize = 0xd44;

const GENI_SE_UART: u32 = 2;
const FW_REV_PROTOCOL_MASK: u32 = 0xff << 8;
const FW_REV_PROTOCOL_SHIFT: u32 = 8;
const FORCE_DEFAULT: u32 = 1;
const DEFAULT_IO_OUTPUT_CTRL_MASK: u32 = 0x7f;
const DEFAULT_CGC_EN: u32 = 0x7f;
const DMA_RX_CLK_CGC_ON: u32 = 1 << 0;
const DMA_TX_CLK_CGC_ON: u32 = 1 << 1;
const DMA_AHB_SLV_CLK_CGC_ON: u32 = 1 << 2;
const AHB_SEC_SLV_CLK_CGC_ON: u32 = 1 << 3;
const GENI_DMA_MODE_EN: u32 = 1 << 0;
const GENI_M_IRQ_EN: u32 = 1 << 2;
const GENI_S_IRQ_EN: u32 = 1 << 3;
const DMA_TX_IRQ_EN: u32 = 1 << 1;
const DMA_RX_IRQ_EN: u32 = 1 << 0;
const M_GENI_CMD_ACTIVE: u32 = 1 << 0;
const S_GENI_CMD_ACTIVE: u32 = 1 << 12;
const M_GENI_CMD_CANCEL: u32 = 1 << 2;
const M_GENI_CMD_ABORT: u32 = 1 << 1;
const S_GENI_CMD_ABORT: u32 = 1 << 1;
const M_CMD_DONE_EN: u32 = 1 << 0;
const M_CMD_CANCEL_EN: u32 = 1 << 4;
const M_CMD_ABORT_EN: u32 = 1 << 5;
const S_CMD_ABORT_EN: u32 = 1 << 5;
const M_COMMON_GENI_M_IRQ_EN: u32 =
    (0x7e) | (1 << 22) | (1 << 23) | (1 << 24) | (1 << 25) | (1 << 28) | (1 << 29);
const S_COMMON_GENI_S_IRQ_EN: u32 = (0x3e) | (0x3f << 9) | (1 << 24) | (1 << 25);
const UART_START_TX: u32 = 0x1;
const M_OPCODE_SHIFT: u32 = 27;
const UART_CTS_MASK: u32 = 1 << 1;

const DEF_FIFO_DEPTH_WORDS: u32 = 16;
const BITS_PER_BYTE: u32 = 8;
const POLL_SPINS: usize = 1_000_000;

const PACKING_4X8_CFG0: u32 = 0x0004_380e;
const PACKING_4X8_CFG1: u32 = 0x000c_3e0e;

static ENABLED: AtomicBool = AtomicBool::new(false);

pub fn init() {
    if read_proto() != GENI_SE_UART {
        return;
    }

    if main_active() {
        let _ = poll_tx_done();
        cancel_main();
    }
    if secondary_active() {
        abort_secondary();
    }

    config_packing_4x8();
    init_fifo_mode();

    write32(SE_UART_TX_TRANS_CFG, UART_CTS_MASK);
    write32(SE_UART_TX_PARITY_CFG, 0);
    write32(SE_UART_RX_TRANS_CFG, 0);
    write32(SE_UART_RX_PARITY_CFG, 0);
    write32(SE_UART_TX_WORD_LEN, BITS_PER_BYTE);
    write32(SE_UART_RX_WORD_LEN, BITS_PER_BYTE);
    write32(SE_UART_TX_STOP_BIT_LEN, 0);

    ENABLED.store(true, Ordering::Relaxed);
    write_str("ablx: serial ok\n");
}

pub fn write_str(message: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }

    for byte in message.bytes() {
        if byte == b'\n' && !write_byte(b'\r') {
            return;
        }
        if !write_byte(byte) {
            return;
        }
    }
}

pub fn write_hex(label: &str, value: usize) {
    write_str(label);
    write_str("0x");
    let mut shift = usize::BITS;
    while shift > 0 {
        shift -= 4;
        let digit = ((value >> shift) & 0xf) as u8;
        let byte = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        if !write_byte(byte) {
            return;
        }
    }
    write_str("\n");
}

fn write_byte(byte: u8) -> bool {
    if !ENABLED.load(Ordering::Relaxed) {
        return false;
    }

    if main_active() {
        if !poll_tx_done() {
            ENABLED.store(false, Ordering::Relaxed);
            return false;
        }
        cancel_main();
    }

    write32(SE_GENI_M_IRQ_CLEAR, M_CMD_DONE_EN);
    write32(SE_UART_TX_TRANS_LEN, 1);
    write32(SE_GENI_M_CMD0, UART_START_TX << M_OPCODE_SHIFT);
    write32(SE_GENI_TX_FIFON, u32::from(byte));

    if poll_tx_done() {
        true
    } else {
        ENABLED.store(false, Ordering::Relaxed);
        false
    }
}

fn init_fifo_mode() {
    irq_clear();
    io_init();
    io_set_fifo_mode();
    write32(SE_GENI_RX_WATERMARK_REG, DEF_FIFO_DEPTH_WORDS / 2);
    write32(SE_GENI_RX_RFR_WATERMARK_REG, DEF_FIFO_DEPTH_WORDS - 2);
    set32(SE_GENI_M_IRQ_EN, M_COMMON_GENI_M_IRQ_EN);
    set32(SE_GENI_S_IRQ_EN, S_COMMON_GENI_S_IRQ_EN);

    irq_clear();
    clear32(SE_GENI_DMA_MODE_EN, GENI_DMA_MODE_EN);
}

fn io_init() {
    set32(SE_GENI_CGC_CTRL, DEFAULT_CGC_EN);
    set32(
        SE_DMA_GENERAL_CFG,
        AHB_SEC_SLV_CLK_CGC_ON | DMA_AHB_SLV_CLK_CGC_ON | DMA_TX_CLK_CGC_ON | DMA_RX_CLK_CGC_ON,
    );
    write32(GENI_OUTPUT_CTRL, DEFAULT_IO_OUTPUT_CTRL_MASK);
    write32(GENI_FORCE_DEFAULT_REG, FORCE_DEFAULT);
}

fn io_set_fifo_mode() {
    set32(
        SE_IRQ_EN,
        GENI_M_IRQ_EN | GENI_S_IRQ_EN | DMA_TX_IRQ_EN | DMA_RX_IRQ_EN,
    );
    clear32(SE_GENI_DMA_MODE_EN, GENI_DMA_MODE_EN);
    write32(SE_GSI_EVENT_EN, 0);
}

fn irq_clear() {
    write32(SE_GSI_EVENT_EN, 0);
    write32(SE_GENI_M_IRQ_CLEAR, u32::MAX);
    write32(SE_GENI_S_IRQ_CLEAR, u32::MAX);
    write32(SE_DMA_TX_IRQ_CLR, u32::MAX);
    write32(SE_DMA_RX_IRQ_CLR, u32::MAX);
    write32(SE_IRQ_EN, u32::MAX);
}

fn config_packing_4x8() {
    write32(SE_GENI_TX_PACKING_CFG0, PACKING_4X8_CFG0);
    write32(SE_GENI_TX_PACKING_CFG1, PACKING_4X8_CFG1);
    write32(SE_GENI_RX_PACKING_CFG0, PACKING_4X8_CFG0);
    write32(SE_GENI_RX_PACKING_CFG1, PACKING_4X8_CFG1);
    write32(SE_GENI_BYTE_GRAN, 0);
}

fn read_proto() -> u32 {
    (read32(GENI_FW_REVISION_RO) & FW_REV_PROTOCOL_MASK) >> FW_REV_PROTOCOL_SHIFT
}

fn main_active() -> bool {
    read32(SE_GENI_STATUS) & M_GENI_CMD_ACTIVE != 0
}

fn secondary_active() -> bool {
    read32(SE_GENI_STATUS) & S_GENI_CMD_ACTIVE != 0
}

fn abort_secondary() {
    write32(SE_GENI_S_CMD_CTRL_REG, S_GENI_CMD_ABORT);
    let _ = poll_bit(SE_GENI_S_IRQ_STATUS, S_CMD_ABORT_EN);
    write32(SE_GENI_S_IRQ_CLEAR, S_CMD_ABORT_EN);
    write32(GENI_FORCE_DEFAULT_REG, FORCE_DEFAULT);
}

fn cancel_main() {
    write32(SE_GENI_M_CMD_CTRL_REG, M_GENI_CMD_CANCEL);
    if !poll_bit(SE_GENI_M_IRQ_STATUS, M_CMD_CANCEL_EN) {
        write32(SE_GENI_M_CMD_CTRL_REG, M_GENI_CMD_ABORT);
        let _ = poll_bit(SE_GENI_M_IRQ_STATUS, M_CMD_ABORT_EN);
        write32(SE_GENI_M_IRQ_CLEAR, M_CMD_ABORT_EN);
    }
    write32(SE_GENI_M_IRQ_CLEAR, M_CMD_CANCEL_EN);
}

fn poll_tx_done() -> bool {
    if poll_bit(SE_GENI_M_IRQ_STATUS, M_CMD_DONE_EN) {
        return true;
    }

    write32(SE_GENI_M_CMD_CTRL_REG, M_GENI_CMD_ABORT);
    let _ = poll_bit(SE_GENI_M_IRQ_STATUS, M_CMD_ABORT_EN);
    write32(SE_GENI_M_IRQ_CLEAR, M_CMD_ABORT_EN);
    false
}

fn poll_bit(offset: usize, bit: u32) -> bool {
    let mut spins = 0;
    while spins < POLL_SPINS {
        if read32(offset) & bit != 0 {
            return true;
        }
        core::hint::spin_loop();
        spins += 1;
    }
    false
}

fn set32(offset: usize, bits: u32) {
    write32(offset, read32(offset) | bits);
}

fn clear32(offset: usize, bits: u32) {
    write32(offset, read32(offset) & !bits);
}

fn read32(offset: usize) -> u32 {
    unsafe { ((UART12_BASE + offset) as *const u32).read_volatile() }
}

fn write32(offset: usize, value: u32) {
    unsafe {
        ((UART12_BASE + offset) as *mut u32).write_volatile(value);
    }
}
