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

# Prepare a shim+kernel payload using the assembler:
cargo run -p abl-exorcist-assembler -- /path/to/kernel/Image abl-exorcist.bin > /tmp/blessed
```

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
