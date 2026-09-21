# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.1](https://github.com/samcday/abl-exorcist/releases/tag/v0.0.1) - 2026-09-21

### Added

- *(assembler)* support portable no_std payload assembly
- *(cli)* use clap for assembler arguments

### Fixed

- *(assembler)* reject empty initrds and correct API documentation
- *(assembler)* use existing README until crate docs land

### Other

- *(assembler)* document library and command-line usage
- *(cli)* generate the manual on demand in Cargo output
- *(cli)* generate the manual from clap
- *(assembler)* prepare Cargo metadata for publication
- *(assembler)* include crate-local license
- reset versioning to 0.0.1
- support ramdisk kernel containers
- inner payload decompression
- handle zboot kernel images
- init
