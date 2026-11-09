.PHONY: build kernel run test clean

build: kernel
	sh scripts/iso.sh

# Homebrew peut fournir un rustc distinct de celui qui possède la cible bare-metal.
kernel:
	RUSTC="$$(rustup which --toolchain stable rustc)" rustup run stable cargo build --release --locked

run: build
	sh scripts/qemu.sh -serial stdio

test: build
	python3 tests/demarrage.py

clean:
	rm -rf build target
