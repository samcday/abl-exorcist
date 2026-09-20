%bcond check 1

%global crate abl-exorcist-assembler

Name:           %{crate}
Version:        0.0.1
Release:        %autorelease
Summary:        Construct mainline kernel payloads for abl-exorcist

SourceLicense:  GPL-3.0-only
# Draft: regenerate with %%cargo_license_summary against the resolved Fedora crates.
# The additional expressions cover the
# Rust dependencies linked into the host executable.
License:        (0BSD OR MIT OR Apache-2.0) AND GPL-3.0-only AND MIT AND (MIT OR Apache-2.0) AND (MIT OR Zlib OR Apache-2.0)
URL:            https://github.com/samcday/abl-exorcist
Source0:        %{crates_source}

BuildRequires:  cargo-rpm-macros >= 26
%if %{with check}
BuildRequires:  binutils
%endif
BuildRequires:  pkgconfig(libzstd)

%description
abl-exorcist-assembler converts mainline arm64 kernel images and creates the
kernel packages or external initial RAM filesystem containers consumed by the
abl-exorcist AArch64 shim.

The assembler is a host tool. Device selection, Android boot image geometry,
partition writes, and flashing are handled by higher-level consumers.

%prep
%autosetup -n %{crate}-%{version}
# Fedora deliberately discards the upstream lockfile and resolves all Rust
# dependencies from its packaged, offline crate registry.
%cargo_prep

%generate_buildrequires
%cargo_generate_buildrequires

%build
%cargo_build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies

%install
install -Dpm0755 target/rpm/abl-exorcist-assembler \
    %{buildroot}%{_bindir}/abl-exorcist-assembler
install -Dpm0644 abl-exorcist-assembler.1 \
    %{buildroot}%{_mandir}/man1/abl-exorcist-assembler.1

%if %{with check}
%check
%cargo_test

# Fedora patches zstd-sys to use the system shared library.
# LZ4 compression is pure Rust through lz4_flex and has no shared library.
readelf -d target/rpm/abl-exorcist-assembler | grep -F 'libzstd.so.1'
%endif

%files
%license LICENSE
%license LICENSE.dependencies
%doc README.md
%{_bindir}/abl-exorcist-assembler
%{_mandir}/man1/abl-exorcist-assembler.1*

%changelog
%autochangelog
