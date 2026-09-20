#![no_std]

extern crate alloc;

use abl_exorcist_assembler::{AssembleError, assemble, assemble_ramdisk};
use alloc::vec::Vec;

pub fn kernel_payload(kernel: &[u8], shim: &[u8]) -> Result<Vec<u8>, AssembleError> {
    assemble(kernel, shim)
}

pub fn ramdisk_payload(kernel: &[u8], initrd: &[u8]) -> Result<Vec<u8>, AssembleError> {
    assemble_ramdisk(kernel, initrd)
}
