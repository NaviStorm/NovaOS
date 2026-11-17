"""Conserver les preuves mesurées et les médias QEMU dans une nouvelle archive vérifiée."""
import argparse
import hashlib
import json
from pathlib import Path
import os
import shutil
import subprocess
import tarfile
import tempfile

QUAL=Path(__file__).resolve().parent
ROOT=QUAL.parent


def digest(path):
    with path.open("rb") as f:return hashlib.file_digest(f,"sha256").hexdigest()


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("rejeu",type=Path)
    p.add_argument("archive",type=Path)
    args=p.parse_args()
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"],prefix="diff-perf-archive-") as td:
        root=Path(td)/"contenu"
        root.mkdir()
        snapshot=Path(td)/"source.tar"
        subprocess.run(["git","archive","--output",snapshot,"HEAD","components","novaOS/user","novaOS/abi",
                        "novaOS/kernel/src/memory/user/elf.rs","livraison-diff-0240",QUAL.name],cwd=ROOT,check=True)
        with tarfile.open(snapshot) as t:t.extractall(root,filter="data")
        (root/"REVISION").write_bytes(subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT))
        machines=root/"machines"
        machines.mkdir()
        for isa,target in [("aarch64","aarch64-unknown-none-softfloat"),("x86_64","x86_64-unknown-none")]:
            for prefix,source in [("ord-",ROOT),("",args.rejeu/("lab-"+isa))]:
                kernel=source/"novaOS/target"/target/"release/novaos"
                for sector in [512,4096]:
                    run=f"{prefix}{isa}-{sector}"
                    original=args.rejeu/run
                    hashes=json.loads((root/QUAL.name/"preuves-invites/qemu"/run/"fichiers-sha256.json").read_text())
                    assert all(digest(original/f)==h for f,h in hashes.items())
                    assert digest(kernel)==json.loads((original/"session.vm.json").read_text())["elf_sha256"]
                    shutil.copytree(original,machines/run)
                shutil.copy2(kernel,machines/("noyau-"+prefix+isa))
        paths=sorted(f for f in root.rglob("*") if f.is_file())
        manifest=root/"MANIFESTE.sha256"
        manifest.write_text("".join(f"{digest(f)}  {f.relative_to(root)}\n" for f in paths))
        args.archive.parent.mkdir(parents=True,exist_ok=True)
        with tarfile.open(args.archive,"x:gz",compresslevel=6) as archive:
            for f in [*paths,manifest]:archive.add(f,arcname=str(f.relative_to(root)),recursive=False)
        check=Path(td)/"controle"
        check.mkdir()
        with tarfile.open(args.archive) as archive:archive.extractall(check,filter="data")
        assert (check/manifest.name).read_bytes()==manifest.read_bytes()
        subprocess.run(["shasum","-a","256","-c",manifest.name],cwd=check,check=True,stdout=subprocess.DEVNULL)
        report=dict(archive_sha256=digest(args.archive),manifeste_sha256=digest(manifest),
                    fichiers_verifies=len(paths),octets=args.archive.stat().st_size)
        with Path(str(args.archive)+".verification.json").open("x") as f:json.dump(report,f,indent=2);f.write("\n")
        print(report)


if __name__=="__main__":main()
