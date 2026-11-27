"""Contre-lecture 5H, hors invité, sur les seules preuves publiées."""

import json
import re
import struct
import tarfile
from hashlib import sha256
from pathlib import Path

PROOFS = Path(__file__).resolve().parents[2] / "docs/validation/diagnostic-uefi-5h"
CASES = {
    "nominal-diagnostic": 16,
    "inactif-diagnostic": 16,
    "inactif-sans-signature": 16,
    "nominal-signature-interne": 6,
    "inactif-signature-interne": 6,
    "nominal-paquet": 18,
    "inactif-paquet": 18,
    "nominal-configuration": 9,
    "revoque-diagnostic": None,
}


def require(ok, name):
    if not ok:
        raise ValueError(name)


def member(archive, name):
    item = archive.getmember(name)
    require(item.isfile() and item.size <= 16 * 1024 * 1024, name)
    return archive.extractfile(item).read()


def check():
    for line in (PROOFS / "SHA256SUMS").read_text().splitlines():
        h, name = line.split()
        require(sha256((PROOFS / name).read_bytes()).hexdigest() == h, name)
    data = json.loads((PROOFS / "bilan.json").read_bytes())
    require(
        set(data["cas"]) == {"direct", "pont"} and "liberation" in data["garde"],
        "profils",
    )
    with (
        tarfile.open(PROOFS / "supports.tar.gz") as inputs,
        tarfile.open(PROOFS / "observations.tar.gz") as evidence,
    ):
        pe = member(inputs, "diagnostic.efi")
        header = struct.unpack_from("<I", pe, 60)[0]
        require(pe[header : header + 4] == b"PE\0\0", "PE")
        entry = struct.unpack_from("<I", pe, header + 24 + 16)[0]
        size = struct.unpack_from("<I", pe, header + 24 + 56)[0]
        for topo, cases in data["cas"].items():
            require(set(cases) == set(CASES), topo)
            for name, r in cases.items():
                prefix = f"{topo}/{name}/"
                mode, img = name.split("-", 1)
                require(
                    sha256(member(inputs, img + ".img")).hexdigest() == r["image"], name
                )
                require(
                    sha256(member(inputs, mode + ".fd")).hexdigest() == r["magasin"],
                    mode,
                )
                stats = [
                    next(
                        x["stats"]
                        for x in json.loads(line)
                        if x.get("device") == "interne"
                    )
                    for line in member(
                        evidence, prefix + "compteurs-qmp.jsonl"
                    ).splitlines()
                ]
                log = member(evidence, prefix + "firmware.log").decode(errors="replace")
                require(
                    len(stats) == (2 if CASES[name] is None else 3)
                    and r["disque_intact"],
                    name,
                )
                for sample in stats:
                    require(
                        all(
                            sample[k] == 0
                            for k in ("wr_operations", "wr_bytes", "flush_operations")
                        ),
                        name,
                    )
                require(
                    stats[0]["rd_operations"] == 0
                    and stats[-1]["rd_operations"]
                    == (11 if CASES[name] in (None, 9) else 0),
                    name,
                )
                if CASES[name] is None:
                    require(
                        "Access Denied" in log
                        and "No bootable option" in log
                        and "NovaOS:" not in log,
                        name,
                    )
                    continue
                report = r["rapport"]
                raw = member(evidence, prefix + "rapport-ram.bin")
                addr, rip, stop, base, status, _, _, units, nodes, _, live, _ = (
                    struct.unpack_from("<12Q", raw, 16)
                )
                require(
                    raw.startswith(b"NOVA5E-RAM-FINAL")
                    and status == CASES[name]
                    and nodes == live == 0,
                    name,
                )
                require(
                    base + entry == rip and base <= addr <= base + size - len(raw), name
                )
                require(
                    r["entree"] == r["fin"]
                    and stats[1]["rd_operations"] == stats[2]["rd_operations"]
                    and stats[1]["rd_bytes"] == stats[2]["rd_bytes"],
                    name,
                )
                require(
                    r["halt"]
                    and report["statut"] == status
                    and stop <= r["rip_final"] < stop + 8,
                    name,
                )
                require(
                    r["confiance_affichee"] in log
                    and ("chaine de demarrage non authentifiee" in log)
                    == (mode == "inactif"),
                    name,
                )
                pci = json.loads(member(evidence, prefix + "pci-entree.json"))[
                    "controleurs"
                ]
                require(
                    {v["classe"] for v in pci.values()} == {0x0200, 0x0106}
                    and all(v["bme"] for v in pci.values()),
                    name,
                )
                vtd = json.loads(member(evidence, prefix + "vtd-entree.json"))
                require(
                    len(vtd) == int(topo == "pont") and all(not x["tes"] for x in vtd),
                    name,
                )
                if status == 16:
                    require(
                        "catalogue verifie - candidats=1" in log
                        and "DMA non etabli" in log
                        and units == len(vtd),
                        name,
                    )
                if status == 9:
                    require(
                        "Access Denied" in log and r["repli_interne_disqualifiant"],
                        name,
                    )
                if name == "nominal-diagnostic":
                    rsp = int(
                        re.search(
                            rb"RSP=([0-9a-fA-F]+)",
                            member(evidence, prefix + "cpu-final.txt"),
                        )[1],
                        16,
                    )
                    print(
                        f"{topo}: image [{base:#x},{base + size:#x}[, rapport {addr:#x}, RSP final {rsp:#x}"
                    )
    print("18 cellules, 16 intervalles EFI, 2 replis internes disqualifiants.")


if __name__ == "__main__":
    check()
