.PHONY: build kernel run test endurance clean
ARCH ?= x86_64
export NOVA_ARCH := $(ARCH)

build: kernel
	sh scripts/iso.sh

# Homebrew peut fournir un rustc distinct de celui qui possède la cible bare-metal.
kernel:
	. scripts/target.sh && RUSTC="$$(rustup which --toolchain stable rustc)" rustup run stable cargo build --release --locked --target "$$rust_target"

run: build
	sh scripts/qemu.sh -serial stdio

test: build
	python3 tests/demarrage.py

DURATION ?= 7200

endurance: kernel
	python3 tests/demarrage.py --endurance "$(DURATION)"

clean:
	rm -rf build target
