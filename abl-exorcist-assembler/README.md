# abl-exorcist-assembler

`abl-exorcist-assembler` is the host-side library and command-line tool for
constructing payloads consumed by the
[`abl-exorcist`](https://github.com/samcday/abl-exorcist) AArch64 shim.

The CLI accepts a raw arm64 `Image`, `Image.gz`, `Image.zst`, or Linux EFI zboot
kernel and canonicalizes it before assembly.

To append a kernel package to a raw device-appropriate shim:

```sh
abl-exorcist-assembler vmlinuz abl-exorcist.bin > prepared-kernel
```

To construct the `ABLXRD1` Android-ramdisk transport used for an external
initrd:

```sh
abl-exorcist-assembler --ramdisk vmlinuz initramfs.img > ablx-ramdisk.img
```

With the default `std` feature, the library exposes kernel normalization
followed by assembly, without filesystem policy:

```rust
let kernel = abl_exorcist_assembler::canonicalize_kernel(vmlinuz)?;
let ramdisk = abl_exorcist_assembler::assemble_ramdisk(&kernel, initrd)?;
```

For `no_std + alloc` consumers, disable default features and pass an already
normalized raw ARM64 `Image` to `assemble(kernel, shim)` or
`assemble_ramdisk(kernel, initrd)`. Both return owned payload bytes. The CLI and
`canonicalize_kernel` require `std`.

Both assembly functions use portable raw-block LZ4. LZ4 HC is no longer used;
payload sizes may increase. Additional compression choices and downstream
size-limit evaluation are follow-up work. The initrd bytes pass through unchanged.

Run `abl-exorcist-assembler --help` for usage or `--version` for its version.
See [the manual page](abl-exorcist-assembler.1) for the command-line contract.

Device selection, shim provenance, Android boot image geometry, partition
writes, and flashing deliberately belong to higher-level consumers.

## License

This project is licensed under the GNU General Public License version 3.0
only. See `LICENSE` for the full license text.
