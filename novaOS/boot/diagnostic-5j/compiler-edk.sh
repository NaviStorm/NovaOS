#!/bin/bash
set -eu
mkdir -p Conf
export WORKSPACE="$PWD" EDK_TOOLS_PATH="$PWD/BaseTools" CONF_PATH="$PWD/Conf"
export PYTHON_COMMAND=/opt/homebrew/bin/python3.12 CLANG_BIN=/opt/homebrew/opt/llvm/bin/
mkdir -p Outils5J
lld="$(rustup run stable rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin/rust-lld"
test -x "$lld"
ln -sfn "$lld" Outils5J/ld.lld
export PATH="$PWD/Outils5J:$PATH"
set +u
set --
source edksetup.sh
set -u
build -a X64 -t CLANGDWARF -p OvmfPkg/OvmfPkgX64.dsc -b RELEASE -n 8 \
  -D SECURE_BOOT_ENABLE=TRUE -D SMM_REQUIRE=TRUE -D FD_SIZE_4MB=TRUE
