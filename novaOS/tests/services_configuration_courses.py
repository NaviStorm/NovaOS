"""Deux opérations CAS en vol puis révocation après admission, sur le canal de maintenance du banc."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import zlib

from services import ROOT, guest, fixture_p9


def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def verify_slots(records):
    """Vérification du format public du banc, indépendante du décodeur du gestionnaire."""
    at = 0
    nodes = []
    volumes = []
    for generation in (2, 1):
        oid, size = struct.unpack_from("<QQ", records, at)
        at += 16
        body = records[at:at + size]
        at += size
        assert size == 4160 and body[:8] == b"NSVCCFG1"
        nodes.append(oid)
        volumes.append(body[8:40])
        assert struct.unpack_from("<Q", body, 40)[0] == generation
        length, crc = struct.unpack_from("<II", body, 48)
        assert crc == zlib.crc32(body[:52]) ^ zlib.crc32(body[64:])
        assert not any(body[56:64]) and not any(body[64 + length:])
        data = body[64:64 + length]
        assert data[:8] == b"NVSVCC01"
        count, = struct.unpack_from("<Q", data, 8)
        assert count == 4
        pos = 16
        for expected in (1, 2, 9, 10):
            version, service, _, _ = struct.unpack_from("<4Q", data, pos)
            pos += 32
            assert version == 1 and service == expected
            for _ in range(3):
                n, = struct.unpack_from("<Q", data, pos)
                pos += 8 * (1 + n)
            auto, = struct.unpack_from("<Q", data, pos)
            assert auto == int(service != 9 or generation == 2)
            pos += 9 * 8
        assert pos == len(data)
    assert nodes[0] != nodes[1] and volumes[0] == volumes[1]
    assert records[at + 8:] == b"NovaOS\n"


def trial(arch, cpus, sector, out, kernel, oracle):
    out.mkdir(parents=True, exist_ok=False)
    image = out / "volume.raw"
    fixture_p9(image, sector)
    initial = digest(image)
    iso = out / "race.iso"
    env = {**os.environ, "NOVA_ARCH": arch, "NOVA_KERNEL_ELF": str(kernel)}
    subprocess.run(["sh", ROOT / "scripts/iso.sh", "services-config-race", iso],
                   env=env, stdout=subprocess.DEVNULL, check=True)
    serial = out / "race.serial"
    raw = guest(arch, sector, cpus, image, iso, "services-config-race", serial, False)
    records = subprocess.check_output([oracle, "inspect", image, str(sector)])
    verify_slots(records)
    accounting = {}
    for phase in ("bloc_repos", "bloc_collecte"):
        line = next(line for line in raw.splitlines() if f"RAM phase={phase} ".encode() in line)
        accounting[phase] = {k.decode(): int(v) for k, v in re.findall(rb"(\w+)=(\d+)", line)}
    for field in ("processus", "ipc", "utilisateur", "upile", "kpile", "buddy", "meta_actifs"):
        assert accounting["bloc_repos"][field] == accounting["bloc_collecte"][field], field
    result = dict(arch=arch, vcpus=cpus, secteur=sector, generation=2,
                  service9_desactive_en_generation1=True, service9_actif_en_generation2=True,
                  noyau=digest(kernel), iso=digest(iso), serie=digest(serial),
                  image_avant=initial, image_apres=digest(image), oracle=digest(oracle),
                  verification_racines=hashlib.sha256(records).hexdigest(),
                  memoire=accounting)
    (out / "resultats.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=["aarch64", "x86_64"])
    parser.add_argument("vcpus", type=int, choices=[1, 2, 4])
    parser.add_argument("secteur", type=int, choices=[512, 4096])
    parser.add_argument("preuves", type=Path)
    parser.add_argument("noyau", type=Path)
    parser.add_argument("oracle", type=Path)
    args = parser.parse_args()
    trial(args.arch, args.vcpus, args.secteur, args.preuves.resolve(),
          args.noyau.resolve(strict=True), args.oracle.resolve(strict=True))
