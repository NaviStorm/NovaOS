#!/bin/sh
set -eu
root=$(CDPATH= cd "$(dirname "$0")/../.." && pwd)
tmp=${NOVAOS_TMPDIR:-$(dirname "$root")/tmp}
g=${GNU_UNAME_SOURCE:-$tmp/sort-arc027-gnu}
u=${UUTILS_SOURCE:-$tmp/sort-arc027-uutils}
cd "$root"
git rev-parse HEAD
git -C "$g" remote get-url origin
git -C "$g" describe --tags --always
git -C "$g" rev-parse HEAD HEAD:gnulib
git -C "$u" remote get-url origin
git -C "$u" rev-parse HEAD
for f in src/uname.c src/uname-uname.c src/uname.h tests/misc/uname-labeled.sh doc/coreutils.texi COPYING; do shasum -a 256 "$g/$f"; done
for f in src/uu/uname/src/uname.rs src/uu/uname/Cargo.toml Cargo.toml LICENSE; do shasum -a 256 "$u/$f"; done
oracle=${GNU_UNAME:-/opt/homebrew/bin/guname}
realpath "$oracle"
LC_ALL=C "$oracle" --version
LC_ALL=C "$oracle" --help
shasum -a 256 "$oracle" livraison-uname-0240/bin/*
file livraison-uname-0240/bin/*
rustup run stable rustc --version --verbose
rustup run stable cargo --version
python3 --version
qemu-system-aarch64 --version
qemu-system-x86_64 --version
sw_vers
