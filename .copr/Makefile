VERSION := $(shell sed -n 's/^version = "\([^"]*\)"/\1/p' abl-exorcist-assembler/Cargo.toml | head -n1)
SPEC_VERSION := $(shell sed -n 's/^Version:[[:space:]]*//p' packaging/abl-exorcist-assembler.spec | head -n1)
CRATE := abl-exorcist-assembler
SOURCE_ARCHIVE := $(CRATE)-$(VERSION).crate

outdir ?= $(CURDIR)
spec ?= packaging/abl-exorcist-assembler.spec

.PHONY: srpm
srpm:
	@test -n "$(VERSION)"
	@test "$(VERSION)" = "$(SPEC_VERSION)"
	@cmp -s LICENSE abl-exorcist-assembler/LICENSE
	@mkdir -p "$(outdir)"
	@command -v cargo >/dev/null 2>&1 || dnf -y install cargo
	@command -v rpmbuild >/dev/null 2>&1 || dnf -y install rpm-build
	@cargo package --locked --no-verify --allow-dirty --package "$(CRATE)"
	@cp "target/package/$(SOURCE_ARCHIVE)" "$(outdir)/$(SOURCE_ARCHIVE)"
	@rpmbuild -bs "$(spec)" \
		--define "_sourcedir $(abspath $(outdir))" \
		--define "_srcrpmdir $(abspath $(outdir))"
