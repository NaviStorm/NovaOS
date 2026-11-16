"""Archive des rejeux QEMU. Les médias identiques partagent leurs octets dans le tar."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile


def digest(stream):
    return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("worktree", type=Path)
    ap.add_argument("elf_historiques", type=Path)
    ap.add_argument("archive", type=Path)
    args = ap.parse_args()
    root = args.worktree.resolve()
    proof = root/"novaOS/build/arc027-awk-temoins"
    sources = {str(p.relative_to(proof)):p for p in proof.rglob("*") if p.is_file()}
    for arch in ("aarch64","x86_64"):
        for size in (512,4096):
            for role in ("head","wc"):
                prefix = f"{role}-{arch}-{size}"
                log = (proof/(prefix+".txt")).read_text()
                assert ("fichiers relus" if role=="head" else "observations") in log.splitlines()[-1]
                assert prefix+"/resultats.json" in sources
        for program in ("sdk-example","threads-test"):
            ref = root/"novaOS/build/abi-registre"/arch/program
            result = json.loads((ref/"resultat.json").read_text())
            kernel, = ref.glob("*-unknown-*/release/novaos")
            elf = args.elf_historiques/(arch+"-"+program)/"original.elf"
            serial = root/"novaOS/build"/result["journal"]
            prefix = "abi/"+arch+"-"+program+"/"
            for name,path,key in (("original.elf",elf,"elf_sha256"),("noyau.elf",kernel,"noyau_sha256"),
                                  ("uart.serial",serial,"journal_sha256")):
                with path.open("rb") as f: assert digest(f) == result[key]
                sources[prefix+name] = path
            sources[prefix+"resultat.json"] = ref/"resultat.json"
    hashes = {}
    members = {}
    with tarfile.open(args.archive,"x:xz",preset=8) as archive:
        for name,p in sorted(sources.items()):
            with p.open("rb") as stream:
                sha = digest(stream)
                members[name] = sha
                info = archive.gettarinfo(str(p),arcname=name)
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                if sha in hashes:
                    info.type,info.linkname,info.size = tarfile.LNKTYPE,hashes[sha],0
                    archive.addfile(info)
                else:
                    hashes[sha] = name
                    stream.seek(0)
                    archive.addfile(info,stream)
        manifest = "".join(f"{sha}  {name}\n" for name,sha in sorted(members.items())).encode()
        info = tarfile.TarInfo("SHA256SUMS")
        info.size = len(manifest)
        archive.addfile(info, io.BytesIO(manifest))
    # Validate each stored payload and its hardlink target in a sequential pass.
    verified = {}
    with tarfile.open(args.archive,"r|xz") as archive:
        for item in archive:
            if item.name == "SHA256SUMS":
                assert archive.extractfile(item).read() == manifest
                continue
            sha = verified[item.linkname] if item.islnk() else digest(archive.extractfile(item))
            assert sha == members[item.name],item.name
            verified[item.name] = sha
    assert verified == members
    print(len(verified), "membres vérifiés,", len(hashes), "contenus distincts")


if __name__ == "__main__":
    main()
