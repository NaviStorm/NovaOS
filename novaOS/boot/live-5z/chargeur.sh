#!/bin/sh
# Construction privée à partir de l'archive de publication amont v10.8.5.
set -eu
archive=$1
src=$2
correctif=$3
printf '%s  %s\n' 84e1e2789a0be980f791627da0e9b189b9ae95cf629acd1994992ba3dc275c5d "$archive" | shasum -a 256 -c -
mkdir "$src"
tar -xf "$archive" -C "$src" --strip-components=1
cd "$src"
patch -p1 < "$correctif"
if ! command -v ld.lld >/dev/null 2>&1 && test -z "${LD_FOR_TARGET:-}"; then
    rustc=$(rustup which --toolchain stable rustc)
    host=$("$rustc" -vV | sed -n 's/^host: //p')
    LD_FOR_TARGET=$("$rustc" --print sysroot)/lib/rustlib/$host/bin/gcc-ld/ld.lld
    export LD_FOR_TARGET
fi
./configure --enable-uefi-x86-64
make -j4
