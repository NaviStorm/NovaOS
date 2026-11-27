#!/bin/sh
# Adaptateur de laboratoire compilé dans un répertoire neuf. SMI négociées par OVMF.
set -eu
src=$(cd "$1" && pwd)
mkdir "$2"
out=$(cd "$2" && pwd)
arc="$src/amont/qemu-11.1.3.tar.xz"
test "$(shasum -a 256 "$arc" | cut -d' ' -f1)" = 5a8289c7940dc77d3873c81a6c29ba3afe1446fc9d36e77fa1658cdbecb84b22
tar -xf "$arc" -C "$out"
cd "$out/qemu-11.1.3"
test "$(shasum -a 256 "$src/compilation/11.1.3-1/fw_cfg-pio.diff" | cut -d' ' -f1)" = ef953de95eaaad688f89bd49acf71c7bac7b75b594b5af5b8408dffbbd7cd3a9
patch -p1 < "$src/compilation/11.1.3-1/fw_cfg-pio.diff"
python3 - <<'PY'
from pathlib import Path
p = Path("hw/nvram/fw_cfg.c")
s = p.read_text()
old = "    /* nothing, write support removed in QEMU v2.4+ */"
assert s.count(old) == 1
p.write_text(s.replace(old, '''    /* Private cold-boot fixture. Only the chipset's eight negotiation bytes
     * may be submitted over PIO. The chipset checks the complete request. */
    uint16_t key = s->cur_entry;
    if (!s->files || key < FW_CFG_FILE_FIRST || fw_cfg_dma_enabled(s)) {
        return;
    }
    unsigned index = key - FW_CFG_FILE_FIRST;
    if (index >= be32_to_cpu(s->files->count) ||
        strcmp(s->files->f[index].name, "etc/smi/requested-features")) {
        return;
    }
    FWCfgEntry *entry = &s->entries[0][key];
    if (!entry->allow_write || entry->len != 8 || s->cur_offset >= 8 ||
        !entry->data || entry->write_cb) {
        return;
    }
    s->nova_smi_request[s->cur_offset++] = value;
    if (s->cur_offset == 8) {
        memcpy(entry->data, s->nova_smi_request, 8);
    }'''))
p = Path("include/hw/nvram/fw_cfg.h")
s = p.read_text()
assert s.count("    uint32_t cur_offset;") == 1
p.write_text(s.replace("    uint32_t cur_offset;", "    uint8_t nova_smi_request[8];\n    uint32_t cur_offset;"))
PY
mkdir "$out/build"
cd "$out/build"
"$out/qemu-11.1.3/configure" --target-list=x86_64-softmmu \
  --python=/opt/homebrew/opt/python@3.11/bin/python3.11 \
  --disable-docs --disable-werror --disable-tools --disable-guest-agent \
  --disable-download --disable-debug-info --disable-gtk --disable-sdl \
  --disable-cocoa --disable-vnc --enable-fdt=system --disable-hvf \
  --extra-cflags=-I/opt/homebrew/opt/dtc/include \
  --extra-ldflags=-L/opt/homebrew/opt/dtc/lib --enable-tcg --enable-trace-backends=log
ninja -j8 qemu-system-x86_64
