"""Relire les médias et les quatre exécutions du moteur CUT candidat."""
import gzip
import hashlib
import json
from pathlib import Path
import re
import tarfile

ROOT = Path(__file__).resolve().parent
PREVOL = ROOT.parent / "creneau"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify():
    manifest = json.loads((ROOT / "manifeste.json").read_text())
    preflight = json.loads((PREVOL / "prevol-qemu.json").read_text())
    media = ROOT / "medias.tar.gz"
    with media.open("rb") as stream:
        assert hashlib.file_digest(stream, "sha256").hexdigest() == manifest["archive_sha256"]
    with tarfile.open(media) as archive:
        assert set(archive.getnames()) == set(manifest["membres"])
        for name, meta in manifest["membres"].items():
            data = archive.extractfile(name).read()
            assert len(data) == meta["octets"] and sha(data) == meta["sha256"], name
        for arch in ("aarch64", "x86_64"):
            for sector in (512, 4096):
                tag = f"{arch}-{sector}"
                path = ROOT / tag
                result = json.loads((path / "resultats.json").read_text())
                historical = ROOT.parent / "qemu" / f"{tag}-sdk" / "resultats.json"
                ids = result["cas_reussis"]
                assert ids == json.loads(historical.read_text())["cas_reussis"]
                assert len(set(ids)) == len(ids) == preflight["vecteurs"] == 1954
                assert result["arch"] == arch and result["secteur"] == sector
                def member(name):
                    return archive.extractfile(f"{tag}/{name}").read()
                kernel = member("demarre.elf")
                for name, key in (("sonde", "elf_sonde_sha256"), ("cut", "elf_cut_sha256")):
                    elf = member(f"{name}.elf")
                    assert sha(elf) == result[key] == preflight["binaires"][arch][name]["sha256"]
                    assert (elf in kernel) == (name == "sonde")
                assert sha(kernel) == result["noyau_sha256"]
                assert sha(member("sonde.iso")) == result["iso_sha256"]
                assert sha(member("vecteurs.bin")) == result["oracle_sha256"] == preflight["sha256_vecteurs"]
                assert sha(member("session.raw")) == result["volume_avant_sha256"] == result["volume_apres_sha256"]
                assert any("readonly=on" in arg and "session.raw" in arg for arg in result["commande"])
                assert any(f"logical_block_size={sector},physical_block_size={sector}" in arg for arg in result["commande"])
                serial = gzip.decompress((path / "sonde.serial.gz").read_bytes())
                pid = re.search(rb"DEBUG pid=(\d+) CUT MATRICE 1954 OK", serial).group(1)
                level = b"EL0" if arch == "aarch64" else b"Ring3"
                assert re.search(rb"PROC pid=" + pid + rb" niveau=" + level + rb" pc=0x[0-9a-f]+ sortie=0", serial)
                assert b"SDK exemple admis sortie=0 restitution=OK" in serial
                expected = {(i, j) for i in range(3) for j in range(4)}
                observed = re.findall(rb"DEBUG pid=" + pid + rb" CUT FIN (\d+) (\d+)", serial)
                assert len(observed) == 12 and {(int(i), int(j)) for i, j in observed} == expected
                samples = result["mesures"]
                assert len(samples) == 12 and {(x["test"], x["passage"]) for x in samples} == expected
                sizes = (285214720, 16780800, 16779776)
                assert all(x["octets"] == sizes[x["test"]] and x["secondes"] > 0 for x in samples)
                assert sizes[0] > result["ram_octets_qmp"] == 256 * 2**20
                assert result["champs_longs_reussis"] == 3
                print(f"{tag} : 1 954 vecteurs, 3 champs longs, 3 traitements × 4 passages, sortie 0")


if __name__ == "__main__":
    verify()
