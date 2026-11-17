"""Archiver une livraison commitée et vérifier chaque fichier après décompression."""
from laboratoire import *
import tarfile
import sys

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("rejeu",type=Path);p.add_argument("archive",type=Path)
    args=p.parse_args();base=args.rejeu.resolve();archive=args.archive.resolve()
    archive.parent.mkdir(exist_ok=True,parents=True)
    with tempfile.TemporaryDirectory(dir=TMP,prefix="diff-archive-") as td:
        temp=Path(td);root=temp/"dossier";root.mkdir()
        source=temp/"source.tar"
        with source.open("xb") as f:
            subprocess.run(["git","archive","HEAD","components","novaOS/user","novaOS/abi","livraison-diff-0240"],cwd=ROOT,check=True,stdout=f)
        with tarfile.open(source) as tar:tar.extractall(root,filter="data")
        (root/"REVISION").write_text(subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True))
        for arch,target in [("aarch64","aarch64-unknown-none-softfloat"),("x86_64","x86_64-unknown-none")]:
            for prefix,src in [("",base/("lab-"+arch)),("ord-",ROOT)]:
                kernel=src/"novaOS/target"/target/"release/novaos";kind=prefix+arch
                for sector in [512,4096]:
                    run=kind+"-"+str(sector);q=base/run
                    recorded=json.loads((root/"livraison-diff-0240/preuves/qemu"/run/"fichiers-sha256.json").read_text())
                    for name,expected in recorded.items():
                        if digest(q/name)!=expected:raise ValueError(q/name)
                    if json.loads((q/"session.vm.json").read_text())["elf_sha256"]!=digest(kernel):raise ValueError(kernel)
                    shutil.copytree(q,root/"machines"/run)
                shutil.copy2(kernel,root/"machines"/("noyau-"+kind))
        files=sorted(f for f in root.rglob("*") if f.is_file())
        manifest="".join(f"{digest(f)}  {f.relative_to(root)}\n" for f in files)
        (root/"MANIFESTE.sha256").write_text(manifest)
        with tarfile.open(archive,"x:gz",compresslevel=6) as tar:
            for f in [*files,root/"MANIFESTE.sha256"]:tar.add(f,arcname=str(f.relative_to(root)),recursive=False)
        restored=temp/"verification";restored.mkdir()
        with tarfile.open(archive) as tar:tar.extractall(restored,filter="data")
        check=subprocess.run(["shasum","-a","256","-c","MANIFESTE.sha256"],cwd=restored,check=True,capture_output=True)
        assert len(check.stdout.splitlines())==len(files)
        report=archive.with_name(archive.name+".verification.json")
        save(report,dict(sha256=digest(archive),fichiers_verifies=len(files),manifeste_sha256=digest(restored/"MANIFESTE.sha256"),octets=archive.stat().st_size))
        print(report.read_text())
if __name__=="__main__":main()
