"""Ajoute un parcours de test au cp de la copie privée. Le lanceur et son catalogue restent identiques."""
import os
from pathlib import Path
import shutil
import subprocess
import sys

root=Path(__file__).resolve().parents[2]
arch=sys.argv[1]
target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[arch]
lab=Path(os.environ["TMPDIR"])/("labo-"+arch)
lab.mkdir(exist_ok=True)
for name in ("novaOS","components","novaFS"):
    shutil.copytree(root/name,lab/name,dirs_exist_ok=True,ignore=shutil.ignore_patterns("target","build","__pycache__"))
src=lab/"components/coreutils"
shutil.copyfile(Path(__file__).with_name("temoin.rs"),src/"temoin.rs")
p=src/"cp.rs"
s=p.read_text().replace("mod common;","mod common;\nmod temoin;")
s=s.replace("    common::run(",'''    let args=nova_cli::process::arguments().unwrap_or_default();
    if let [source, dest] = args.as_slice() && matches!(*source,"entree"|"vide") && matches!(*dest,"copie"|"zero"|"occupe"|"mesure1"|"mesure2") {
        let res=nova_cli::fileapi::Client::scoped().and_then(|c| temoin::run(c,dest));
        nova_cli::runtime::exit(res.err().unwrap_or(0));
    }
    common::run(''')
p.write_text(s)
env=dict(os.environ,CARGO_TARGET_DIR=str(lab/"target"))
subprocess.run(["make","-C",lab/"novaOS","ARCH="+arch,"kernel"],env=env,check=True)
print(lab/"target"/target/"release/novaos")
