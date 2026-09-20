# Fedora and COPR packaging

This is a packaging draft, not a validated Fedora or COPR build. It is intended
to build against Fedora's packaged Rust crates and system `libzstd`, without a
Cargo vendor archive. LZ4 compression uses the portable Rust `lz4_flex` crate;
there is no `liblz4` dependency.

Before marking this ready:

- Check `lz4_flex`, `flate2`, `zstd`, and their transitive dependencies in each
  intended build root; no Fedora/EPEL version range is confirmed yet.
- Run the SRPM and RPM builds in COPR and resolve any macro/tooling requirements.
- Regenerate the spec's dependency license expression from the resolved packages.
- Verify installation, the manual page, CLI smoke checks, and system zstd linkage.

Configure a COPR SCM package with:

- clone URL: `https://github.com/samcday/abl-exorcist.git`
- committish: a release tag or pinned commit
- spec: `packaging/abl-exorcist-assembler.spec`
- SRPM method: `make_srpm`

COPR invokes `.copr/Makefile` to create the standalone, publishable Cargo crate
and then the source RPM. The spec requests an offline binary build from that archive. The SRPM step
uses `cargo package --no-verify` only to produce source; it is not a substitute
for the verified Cargo packaging gate before publication.

For a release, keep the version in the spec and
`abl-exorcist-assembler/Cargo.toml` identical, commit the updated `Cargo.lock`,
and tag that commit. Once the standalone crate is published, its source crate,
binary, and `-devel` subpackages can be generated with standard `rust2rpm`;
the upstream COPR spec here only ships the host executable.
