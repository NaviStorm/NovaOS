"""Qualification hôte et compilation nue. Lancer depuis le worktree avec CARGO_TARGET_DIR privé."""
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
os.chdir(ROOT)
BUILD = Path(os.environ["CARGO_TARGET_DIR"]).resolve()
if not BUILD.is_relative_to(Path(os.environ["NOVAOS_TMPDIR"]).resolve()):
    sys.exit("Choisir un CARGO_TARGET_DIR privé sous NOVAOS_TMPDIR")
def tool(name):
    return subprocess.check_output(["rustup","which","--toolchain","stable",name],text=True).strip()
env=dict(os.environ,RUSTC=tool("rustc"),RUSTDOC=tool("rustdoc"),CARGO_PROFILE_RELEASE_STRIP="none")
cargo=["rustup","run","stable","cargo"]
def run(args, *, extra=None, output=False):
    result=subprocess.run(args,env=env | (extra or {}),check=True,text=True,
                          stdout=subprocess.PIPE if output else None)
    return result.stdout
def build(command,isa,*args,extra=None,output=False):
    return run([*cargo,command,"--locked","--offline","--release","--target",isa,*args],extra=extra,output=output)
size=shutil.which("llvm-size") or "/opt/homebrew/opt/llvm/bin/llvm-size"
print("Hôte",platform.platform(),run([env["RUSTC"],"-V"],output=True).strip(),flush=True)
for isa in ("aarch64-apple-darwin","x86_64-apple-darwin"):
    print("Invocations Darwin",isa,flush=True)
    print(build("test",isa,"--quiet",output=True))
    build("run",isa,"--example","selection")
    for series in range(3):
        result=build("test",isa,"--lib","mesures::catalogue","--","--ignored","--nocapture","--test-threads=1",output=True)
        print("Série",series+1,isa,flush=True)
        print(result,flush=True)
    run([size,str(BUILD/isa/"release/examples/selection")])
for isa in ("aarch64-unknown-none-softfloat","x86_64-unknown-none"):
    print("Objet nu",isa,flush=True)
    flags='-C relocation-model=static --cfg sha2_backend="soft" --cfg sha2_force_soft --cfg chacha20_backend="soft" --cfg poly1305_backend="soft" --cfg curve25519_dalek_backend="serial"'
    if isa=="x86_64-unknown-none": flags+=" -C no-redzone=yes"
    path=BUILD/isa/"release/catalogue-4b-mesure.o"
    build("rustc",isa,"--lib","--",f"--emit=obj={path}",extra={"RUSTFLAGS":flags})
    run([size,str(path)])
