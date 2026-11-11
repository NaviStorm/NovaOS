.PHONY: build kernel run test ram-test storage-test endurance clean
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

storage-test: kernel
	python3 tests/demarrage.py --storage-test

ram-test: kernel
	python3 tests/demarrage.py --ram-test

DURATION ?= 7200

endurance: kernel
	python3 tests/demarrage.py --endurance "$(DURATION)"

clean:
	rm -rf build target

.PHONY: ipc-test ipc-races ipc-perf ipc-endurance
ipc-test ipc-races ipc-perf: kernel
	python3 tests/ipc.py $(patsubst ipc-%,%,$@)

ipc-endurance: kernel
	python3 tests/ipc.py endurance --seconds "$(DURATION)"

.PHONY: block-test block-perf block-run
block-test: kernel
	python3 tests/bloc.py

block-perf: kernel
	python3 tests/bloc.py --perf

SECTOR ?= 512
block-run: kernel
	python3 scripts/bloc_qemu.py --sector "$(SECTOR)" "$(IMAGE)"

.PHONY: elf-test
elf-test: kernel
	python3 tests/elf.py

.PHONY: file-test file-info
file-test: kernel
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	python3 tests/fichiers.py

file-info: kernel
	python3 scripts/bloc_qemu.py --volume --sector "$(SECTOR)" "$(IMAGE)"

.PHONY: lifecycle-test
lifecycle-test: kernel
	python3 tests/cycle_processus.py

.PHONY: namespace-test
namespace-test: kernel
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	python3 tests/namespace.py
