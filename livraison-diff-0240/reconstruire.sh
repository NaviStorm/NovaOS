#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(dirname "$here")
: "${NOVAOS_TMPDIR:=$(dirname "$root")/tmp}"
mkdir -p "$NOVAOS_TMPDIR" "$here/bin"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/" CARGO_TARGET_DIR="$NOVAOS_TMPDIR/diff-arc027-target"
snapshot=$(mktemp -d "$TMPDIR/diff-src-XXXXXX")
if [ -f "$root/.git" ] || [ -d "$root/.git" ]; then
 git -C "$root" archive --output="$snapshot/source.tar" HEAD components novaOS/user novaOS/abi
else
 tar cf "$snapshot/source.tar" --exclude target -C "$root" components novaOS/user novaOS/abi
fi
tar xf "$snapshot/source.tar" -C "$snapshot"
# Dans cette copie, Cargo identifie nova-cli par son chemin relatif au workspace.
# Le verrou et le profil de distribution sont ceux du paquet diff.
python3 - "$snapshot/components" <<'PY'
from pathlib import Path
import sys,shutil
base=Path(sys.argv[1])
cli=base/"Cargo.toml"; diff=base/"diff/Cargo.toml"
core,profile=diff.read_text().split("[profile.release]")
diff.write_text(core.replace("[workspace]",""))
text=cli.read_text()
first,rest=text.split("[profile.release]")
others=rest[rest.index("[[bin]]"):]
cli.write_text(first.replace("[workspace]",'[workspace]\nmembers = ["diff"]')+"[profile.release]"+profile+"\n"+others)
shutil.copyfile(base/"diff/Cargo.lock",base/"Cargo.lock")
for directory in [base,base/"diff"]:
    shutil.copyfile(base.parent/"novaOS/user/novafs/service.ld",directory/"service.ld")
PY
export RUSTC=$(rustup which --toolchain stable rustc)
remap="--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}/registry/src=/cargo"
cd "$snapshot/components/diff"
for target in aarch64-apple-darwin aarch64-unknown-none-softfloat x86_64-unknown-none; do
 case "$target" in
 *darwin) export RUSTFLAGS="$remap"; name=diff ;;
 *) export RUSTFLAGS="$remap -Crelocation-model=static -Cno-redzone=yes -Clink-arg=--strip-all -Clink-arg=-zmax-page-size=4096 -Clink-arg=-Tservice.ld"; name="diff-${target%%-*}.elf" ;;
 esac
 rustup run stable cargo build -p nova-diff --offline --locked --release --target "$target"
 cp "$CARGO_TARGET_DIR/$target/release/diff" "$here/bin/$name"
 if [ "$name" = diff ]; then
  # dyld exige LC_UUID. Produire un UUID de contenu, puis resigner le Mach-O.
  python3 - "$here/bin/diff" <<'NOVA_UUID'
import hashlib,struct,sys
from pathlib import Path
p=Path(sys.argv[1]);data=bytearray(p.read_bytes())
magic,=struct.unpack_from("<I",data);assert magic==0xfeedfacf
ncmds,=struct.unpack_from("<I",data,16);pos=32;uuid=None;signature=None
for _ in range(ncmds):
    cmd,size=struct.unpack_from("<II",data,pos)
    if cmd==0x1b:uuid=pos+8
    if cmd==0x1d:signature,=struct.unpack_from("<I",data,pos+8)
    pos+=size
assert uuid is not None and signature is not None
data[uuid:uuid+16]=bytes(16)
value=bytearray(hashlib.sha256(data[:signature]).digest()[:16])
value[6]=(value[6]&15)|0x80;value[8]=(value[8]&63)|0x80
data[uuid:uuid+16]=value
p.write_bytes(data)
NOVA_UUID
  codesign --force --sign - --timestamp=none --identifier org.novaos.diff "$here/bin/diff"
  codesign --verify --strict "$here/bin/diff"
  "$here/bin/diff" --version
 fi
done
file "$here"/bin/*
