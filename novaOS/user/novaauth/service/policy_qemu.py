"""Trois démarrages successifs et restauration du seul P9 sur des copies neuves."""
import argparse
import hashlib
import shutil
import subprocess
from pathlib import Path
from qemu import qualify

def sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()

def run(a):
    out = a.sortie.resolve()
    out.mkdir(parents=True, exist_ok=False)
    genesis = out / "genese"
    genesis.mkdir()
    for name in ("identites.p9", "autorite.raw"):
        shutil.copyfile(a.genese / ("neuf-" + name), genesis / name)
    digest = sha(a.app)
    with (out / "provisionnement.txt").open("w") as log:
        subprocess.run([a.oracle, "init", genesis, str(a.secteur), a.alice.read_text().strip(),
                        a.bob.read_text().strip(), digest], stdout=log, check=True)
    for f in list(genesis.iterdir()):
        f.rename(genesis / ("neuf-" + f.name))
    previous = None
    for mode in ("attribution", "retrait", "reprise", "recul"):
        disk = out / mode
        qualify(a.arch, a.secteur, a.iso, genesis, disk, previous,
                "recul" if mode == "recul" else "ouvert", mode)
        if mode != "recul":
            files = [disk / "identites.p9", disk / "autorite.raw"]
            before = [sha(f) for f in files]
            with (disk / "oracle.txt").open("w") as log:
                subprocess.run([a.oracle, "read", disk, str(a.secteur), digest,
                                "member" if mode == "attribution" else "removed"],
                               check=True, stdout=log)
            assert before == [sha(f) for f in files]
            previous = disk
        else:
            assert sha(disk / "autorite.raw") == sha(previous / "autorite.raw")
            with (disk / "oracle.txt").open("w") as log:
                subprocess.run([a.oracle, "read", disk, str(a.secteur), digest, "recul"], check=True, stdout=log)

    for mode in ("avant", "apres", "mise-a-jour", "reinstallation"):
        fixture = out / ("genese-" + mode)
        fixture.mkdir()
        for name in ("identites.p9", "autorite.raw"):
            shutil.copyfile(genesis / ("neuf-" + name), fixture / name)
        approved_hash = sha(a.app.with_name("policy-update.elf")) if mode == "mise-a-jour" else digest
        with (fixture / "preparation.txt").open("w") as log:
            subprocess.run([a.oracle, "fixture", fixture, str(a.secteur), mode, approved_hash], check=True, stdout=log)
        for name in ("identites.p9", "autorite.raw"):
            (fixture / name).rename(fixture / ("neuf-" + name))
        disk = out / mode
        qualify(a.arch, a.secteur, a.iso, fixture, disk, policy=mode)
        if mode in ("avant", "apres"):
            with (disk / "oracle.txt").open("w") as log:
                subprocess.run([a.oracle, "read", disk, str(a.secteur), digest, mode], check=True, stdout=log)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=("aarch64","x86_64"))
    parser.add_argument("secteur", type=int, choices=(512,4096))
    for name in ("iso", "app", "oracle", "genese", "alice", "bob", "sortie"):
        parser.add_argument(name, type=Path)
    run(parser.parse_args())
