"""Relecture hors invité des artefacts 5E et des observations indépendantes."""

import gzip
from hashlib import sha256
import json
from pathlib import Path
import re
import struct
import tarfile

HERE = Path(__file__).resolve().parent
PROOFS = HERE.parents[1] / "docs/validation/diagnostic-uefi-5e"


def require(condition, label):
    if not condition:
        raise ValueError(label)


def checks(manifest, load):
    for line in manifest.splitlines():
        digest, name = line.split(maxsplit=1)
        require(sha256(load(name)).hexdigest() == digest, name)


def archive(path):
    with tarfile.open(path, "r:gz") as source:
        items = {}
        for item in source:
            require(
                item.isfile()
                and item.size <= 16 * 1024 * 1024
                and item.name not in items,
                item.name,
            )
            items[item.name] = source.extractfile(item).read()
    checks(items["SHA256SUMS"].decode(), items.__getitem__)
    return items


def main():
    checks((PROOFS / "SHA256SUMS").read_text(), lambda n: (PROOFS / n).read_bytes())
    lab = archive(PROOFS / "laboratoire.tar.gz")
    raw = archive(PROOFS / "preuves.tar.gz")
    runs = json.loads((PROOFS / "bilan.json").read_bytes())
    canonical = gzip.decompress((PROOFS / "diagnostic-x86_64.img.gz").read_bytes())
    image = sha256(canonical).hexdigest()
    require(
        len(canonical) == 8388608
        and runs["firmware"] == sha256(lab["ovmf/code.fd"]).hexdigest(),
        "firmware et media",
    )
    require(
        runs["cas"]["direct"]["image"] == runs["cas"]["pont"]["image"] == image,
        "image commune",
    )
    disks = json.loads(raw["integrite-disques.json"])
    require(
        len(disks) == 22 and set(disks.values()) == {image},
        "intégrité des 22 disques fictifs",
    )
    require(len(runs["cas"]) == 14, "matrice")
    for name, case in runs["cas"].items():
        observed = case["rapport"]
        folder = name if case["entree"] is not None else name + "-initial"
        path = "qemu/" + folder + "/"
        samples = []
        for line in raw[path + "compteurs-qmp.jsonl"].splitlines():
            stats = next(
                r["stats"] for r in json.loads(line) if r.get("device") == "interne"
            )
            samples.append(
                {
                    k: v
                    for k, v in stats.items()
                    if k.endswith(("_operations", "_bytes"))
                }
            )
        require(
            samples[0] == case["initial"] and samples[-1] == case["fin"], name + " QMP"
        )
        require(case["disque_intact"] and case["valide"], name)
        if case["entree"] is not None:
            require(
                len(samples) == 3 and samples[1] == samples[2] == case["entree"],
                name + " intervalle",
            )
            trace = raw[path + "transactions.log"]
            start, end = case["octets_avant"], case["octets_avant_sortie"]
            require(
                0 < start == end <= len(trace)
                and trace[start:].decode().splitlines()
                == case["evenements_diagnostic"],
                name + " trace",
            )
            for event in case["evenements_diagnostic"]:
                m = re.fullmatch(
                    r"pci_cfg_write (qemu-xhci \S+) @0x4 <- (0x[0-9a-f]+)", event
                )
                require(
                    m and case["commandes_pci_entree"].get(m[1]) == int(m[2], 16),
                    name + " commande USB",
                )
        if observed:
            data = raw[path + "rapport-ram.bin"]
            require(
                len(data) == 112 + 64 * 72 and data[:16] == b"NOVA5E-RAM-FINAL",
                name + " signature RAM",
            )
            v = struct.unpack_from("<12Q", data, 16)
            require(
                v[4] == observed["statut"]
                and v[8] == len(observed["noeuds"])
                and v[10] == 0,
                name + " bilan RAM",
            )
            require(
                case["halt"]
                and observed["arret"] <= case["rip_final"] < observed["arret"] + 8,
                name + " arrêt",
            )
            for i, node in enumerate(observed["noeuds"]):
                require(
                    tuple(data[112 + i * 72 : 116 + i * 72])
                    == (node["type"], node["compatible"], node["plan"], 0),
                    name + " décision",
                )
        if case["attendu"] is None:
            require(
                b"Access Denied" in raw[path + "firmware.log"]
                and case["fin"]["rd_operations"] > 0,
                name + " refus et lectures",
            )
    require(
        runs["repli_observe"] and runs["porte_physique"] == "bloquee",
        "contre-exemple de repli",
    )
    for arch in ("aarch64", "x86_64"):
        count = sum(
            len(re.findall(rb"^test .* \.\.\. ok$", data, re.M))
            for name, data in raw.items()
            if name.startswith("hotes/") and name.endswith(arch + "-apple-darwin.log")
        )
        require(count == 35, arch)
    print(
        "OK — même support, 14 scénarios, 22 disques fictifs, 35+35 tests hôtes ; repli firmware confirmé"
    )


if __name__ == "__main__":
    main()
