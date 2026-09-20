![abl-exorcist](assets/hero.png)

# abl-exorcist

Drive the evil spirits from your Android bootloader.

`abl-exorcist` is a tiny AArch64 shim that sits between ABL and a mainline
kernel. It ensures that:

 * The expectations of ABL are met.
 * The mainline kernel is shielded from the weirdness of ABL.

```
rustup target add aarch64-unknown-none

# Build the shim ELF
cargo build --release --target aarch64-unknown-none -p abl-exorcist
llvm-objcopy -O binary target/aarch64-unknown-none/release/abl-exorcist abl-exorcist.bin

# Prepare a shim+kernel payload using the assembler. The kernel input may be a
# raw arm64 Image, Image.gz, Image.zst, or a Linux EFI zboot vmlinuz.efi:
cargo run -p abl-exorcist-assembler -- /path/to/kernel/Image abl-exorcist.bin > /tmp/blessed
```

To keep a slow ABL implementation from inflating the full kernel, put only the
gzip-compressed shim in the Android kernel section and carry the real kernel in
an ABLX ramdisk container:

```sh
cargo run -p abl-exorcist-assembler -- \
    --ramdisk /path/to/kernel/Image /path/to/initramfs.img \
    > /tmp/ablx-ramdisk.img

gzip -n -9 -c abl-exorcist.bin > /tmp/abl-exorcist.bin.gz
mkbootimg \
    --kernel /tmp/abl-exorcist.bin.gz \
    --ramdisk /tmp/ablx-ramdisk.img \
    ...
```

The assembler canonicalizes the kernel to a raw arm64 Image and stores it as a
raw LZ4 block. The original initramfs is appended unchanged. At boot, the shim
decompresses the kernel and rewrites the FDT initrd range so Linux sees only the
original initramfs.

The assembler has a [crate README](abl-exorcist-assembler/README.md) and
[manual page](abl-exorcist-assembler/abl-exorcist-assembler.1).

## Portable assembly library

`assemble(kernel, shim)` and `assemble_ramdisk(kernel, initrd)` are available
with `no_std + alloc`. Both take an already-normalized raw ARM64 `Image` and
return owned payload bytes:

```toml
abl-exorcist-assembler = { version = "0.0.1", default-features = false }
```

This is the intended dependency declaration for the first crates.io release;
until it is published, use a path or pinned Git dependency.

The default `std` feature additionally enables `canonicalize_kernel` for gzip,
zstd and Linux EFI zboot inputs, and the host CLI. File I/O and kernel
normalization are not required by either assembly function.

Assembly uses portable raw-block LZ4 in every feature configuration. It no
longer uses LZ4 HC, so compressed bytes and payload sizes can differ and the
result may be larger. The `ABLXPKG1` and `ABLXRD1` formats and shim decoder are
unchanged. Downstream size-limit evaluation and additional compression choices
are follow-up work; this API currently selects LZ4 only.

### Validation

```sh
cargo test --workspace --locked
cargo test -p abl-exorcist-assembler --no-default-features --locked
cargo test -p abl-exorcist --locked
rustup target add wasm32-unknown-unknown aarch64-unknown-none
cargo build --manifest-path tests/no-std-consumer/Cargo.toml --target wasm32-unknown-unknown --locked
cargo build --manifest-path tests/no-std-consumer/Cargo.toml --target aarch64-unknown-none --locked
```

The shim tests decode both assembler outputs with the device's own LZ4 decoder.
The separate `no_std` consumer calls both APIs without workspace feature
unification, on wasm32 and on a target that has no standard library.

## Hardware features

All hardware-specific behavior is opt-in with Cargo features:

- `serial-sdm670-uart12` traces on GENI UART12 at `0x00a90000`.
- `serial-sdm845-uart9` traces on GENI UART9 at `0x00a84000`.
- `cache-sdm670-experiment` and `cache-sdm845-experiment` are opt-in aliases
  for the same guarded identity-mapped D-cache experiment. Both SoCs currently
  use the same device and first-GiB DRAM mappings.
- `bdaddr-google-sdm670` copies Google's bootloader-provided Bluetooth address
  from `/chosen/cdt/cdb2/bt_addr` into the standard `local-bd-address`
  property of the `qcom,wcn3990-bt` controller. The target DT must provide a
  six-byte placeholder so the fixup never needs to resize the FDT.

Select at most one serial feature. The cache experiment requires an EL1,
cache-off handoff and refuses to activate unless every live memory range fits
the identity mappings it installs.

## But what does it *do*?

For now, two main things:

### Command-line filtering

ABL passes down lots of useful kernel commandline in `/chosen/bootargs`. But
also a lot of junk. In particular, it passes things that confuse a mainline
kernel or the initrd the distro generated (things like `ro` and `root=`). The
junk is filtered out and `androidboot.*` args are waved through.

To ensure that the stuff that was intended to be passed along isn't mutilated,
you **must** enclose the real cmdline with `<S>` and `<E>` tokens, e.g:

```
mkbootimg --kernel /tmp/blessed ... --cmdline '<S> real shit here <E>'
```

The command line will *only* be filtered and rewritten if these markers are
present.

### Kernel text_offset masquerading

[The arm64 kernel header has a `text_offset` field][arm64-kernel-header].
Early Pixels (3/3a/3XL) expect that this field has a specific value. Modern
mainline kernels no longer need this field and leave it at `0`.

## License

This project is licensed under the GNU General Public License version 3.0 only.
See [LICENSE](LICENSE) for the full license text.

[arm64-kernel-header]: https://docs.kernel.org/arch/arm64/booting.html#call-the-kernel-image
