"""Régression du VFS public pendant l'usage des temporaires privés sur une P9 fictive."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

from cli import HOST, ROOT, Terminal
from preuves_stockage import empreinte, enregistrer, figer_initiale
from session_cli import fixture_p9


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--secteur", type=int, choices=(512, 4096), required=True)
    parser.add_argument("--preuves", type=Path, required=True)
    args = parser.parse_args()
    arch = os.environ["NOVA_ARCH"]
    proofs = args.preuves.resolve()
    proofs.mkdir(parents=True, exist_ok=False)
    fs = ROOT.parent / "novaFS"
    target = "aarch64-apple-darwin"
    subprocess.run([fs / "lab/run", "build", "--offline", "--lib", "--bin", "novafs"],
                   env={**os.environ, "NOVAFS_TARGET": target}, check=True)
    lib = HOST.parent
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as working:
        oracle = Path(working) / "lecteur"
        subprocess.run(["rustup", "run", "stable", "rustc", "--edition=2024", "--target", target,
                        "-L", "dependency=" + str(lib / "deps"), "--extern",
                        "novafs=" + str(lib / "libnovafs.rlib"),
                        Path(__file__).with_name("tmp_prive_oracle.rs"), "-o", oracle], check=True)
        image = proofs / "session.raw"
        fixture_p9(image, args.secteur)
        figer_initiale(image)
        journal = proofs / "session.serial"
        with Terminal(arch, args.secteur, image, "tmp-public", test=True, journal=journal) as terminal:
            terminal.until(b"nsh> ")
            terminal.line("ls -a /", status="sortie 0", has=["documents", "lignes"],
                          absent=[b"NovaOS-tmp-v1", b"NovaOS-cache-v1", b"registre"])
            terminal.line("cat /documents/exemple.txt", status="sortie 0", has=[b"\r\nNovaOS\r\n"])
            terminal.line("touch témoin-cli", status="sortie 0")
            terminal.line("ls -a /", status="sortie 0", has=["témoin-cli"], absent=[b"NovaOS-tmp-v1"])
            terminal.line("rm témoin-cli", status="sortie 0")
            terminal.line("@probe cache-sans-identite", status="sortie 0")
            terminal.line("@probe droits", status="sortie 0")
            terminal.line("@probe revocation", status="sortie 0")
            terminal.exit()
            terminal.until(b"SMP arret motif=normal")
        before = empreinte(image)
        result = subprocess.check_output([oracle, image, str(args.secteur), "propre"], text=True).strip()
        assert empreinte(image) == before, "l'oracle de relecture a écrit"
        enregistrer(proofs, "resultat.json", dict(architecture=arch, secteur=args.secteur,
                    sha256_image=before, sha256_serie=empreinte(journal), oracle=result))
        print(f"{arch}/{args.secteur} CLI publique et nettoyage vérifiés : {result}", flush=True)


if __name__ == "__main__":
    main()
