.PHONY: build kernel run test ram-test storage-test endurance clean
ARCH ?= x86_64
export NOVA_ARCH := $(ARCH)

build: kernel
	sh scripts/iso.sh

# Homebrew peut fournir un rustc distinct de celui qui possède la cible bare-metal.
.PHONY: abi-generate abi-check
abi-generate:
	python3 scripts/generate_abi.py

abi-check:
	python3 scripts/generate_abi.py --check

kernel: abi-check
	. scripts/target.sh && RUSTC="$$(rustup which --toolchain stable rustc)" rustup run stable cargo build --release --locked --target "$$rust_target"

ifneq ($(strip $(IMAGE)),)
run: cli
else
run: build
	sh scripts/qemu.sh -serial stdio
endif

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

.PHONY: cli cli-image cli-test
ifneq ($(strip $(IMAGE)),)
cli: kernel
	python3 scripts/session_cli.py run --sector "$(SECTOR)" "$(IMAGE)"
else
cli: kernel
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	mkdir -p build
	python3 scripts/session_cli.py demo --sector "$(SECTOR)" "build/shell-$(ARCH)-$(SECTOR).raw"
endif

cli-image:
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	python3 scripts/session_cli.py image --sector "$(SECTOR)" "$(IMAGE)"

cli-test: kernel
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	python3 tests/cli.py

.PHONY: cli-p9-test
cli-p9-test: kernel
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	python3 tests/cli_p9.py

.PHONY: nsh-test
nsh-test: kernel
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	python3 tests/nsh.py

.PHONY: nsh-scripts-test
nsh-scripts-test: kernel
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	python3 tests/nsh_scripts.py

.PHONY: vi-test
vi-test: kernel
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	python3 tests/vi.py

.PHONY: vi-memoire-test vi-volumes-test
vi-memoire-test:
	mkdir -p build
	rustup run stable rustc --edition 2024 -O --test --target "$(ARCH)-apple-darwin" tests/vi_memoire.rs -o "build/vi-memoire-$(ARCH)"
	"build/vi-memoire-$(ARCH)" --nocapture --test-threads=1

.PHONY: mount-progress-test
mount-progress-test:
	mkdir -p build
	rustup run stable rustc --edition 2024 --test --target "$(ARCH)-apple-darwin" tests/progression_montage.rs -o "build/progression-montage-$(ARCH)"
	"build/progression-montage-$(ARCH)"

vi-volumes-test: kernel
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	python3 tests/vi_volumes.py

.PHONY: vi-ecran-test
vi-ecran-test: kernel
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	python3 tests/vi_ecran.py

.PHONY: nsh-terminal-test
nsh-terminal-test: kernel
	NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
	python3 tests/nsh_terminal.py

.PHONY: threads-test
threads-test:
	python3 -B tests/threads.py

.PHONY: smp-test
smp-test: kernel
	python3 -B tests/smp.py

.PHONY: materiel-modeles-test materiel-test
materiel-modeles-test:
	python3 -B tests/materiel_modeles.py

materiel-test: kernel
	python3 -B tests/materiel.py

.PHONY: materiel-pilote-test
materiel-pilote-test:
	. scripts/target.sh && export CARGO_TARGET_DIR="$(CURDIR)/build/materiel-phase2" && \
	  RUSTC="$$(rustup which --toolchain stable rustc)" rustup run stable cargo build --release --locked --target "$$rust_target" --features materiel-lab && \
	  NOVA_KERNEL_ELF="$$CARGO_TARGET_DIR/$$rust_target/release/novaos" python3 -B tests/materiel_pilote.py

.PHONY: vtd-edu-test
vtd-edu-test:
	@test "$(ARCH)" = x86_64
	. scripts/target.sh && export CARGO_TARGET_DIR="$(CURDIR)/build/vtd-phase3/cargo" && \
	  RUSTC="$$(rustup which --toolchain stable rustc)" rustup run stable cargo build --release --locked --target "$$rust_target" --features vtd-lab && \
	  export NOVA_KERNEL_ELF="$$CARGO_TARGET_DIR/$$rust_target/release/novaos" && \
	  python3 -B tests/vtd_edu.py --elf "$$NOVA_KERNEL_ELF" && \
	  python3 -B tests/vtd_dmar.py --elf "$$NOVA_KERNEL_ELF" --preuves "$$(mktemp -d build/vtd-phase3/admission.XXXXXX)/resultats"

.PHONY: instances-test
instances-test: kernel
	python3 -B tests/instances.py

.PHONY: instances-reboot-test
instances-reboot-test: kernel
	python3 -B tests/instances_reboot.py

.PHONY: instances-modele-test
instances-modele-test:
	mkdir -p build
	rustup run stable rustc --edition 2024 --test --target "$(ARCH)-apple-darwin" tests/instances_modele.rs -o "build/instances-modele-$(ARCH)"
	"build/instances-modele-$(ARCH)"
