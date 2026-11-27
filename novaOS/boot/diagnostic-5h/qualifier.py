"""Qualification 5H du diagnostic x86_64 avec les observateurs indépendants de 5F."""

import argparse
import re
import subprocess
import sys
from pathlib import Path

from virt.firmware.varstore.edk2 import Edk2VarStore

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5f"))
from qualifier import digest, exiger, observer_cas, qualification, sauver

ACTIVE = "NovaOS: Secure Boot actif - EFI admis par la politique UEFI"
OFF = "NovaOS: Secure Boot inactif - chaine de demarrage non authentifiee"


def scenario(root, topo, media, store, status, supports, fw):
    name = store + "-" + media
    result = observer_cas(
        root,
        name,
        topo,
        supports / (media + ".img"),
        supports / (store + ".fd"),
        fw,
        statut=status,
        denied=status in (9, None),
    )
    log = (root / name / "firmware.log").read_text(errors="replace")
    expected = OFF if store == "inactif" else ACTIVE
    if status is None:
        exiger("NovaOS:" not in log, "Exécution après révocation")
    else:
        exiger(
            expected in log and (ACTIVE if expected == OFF else OFF) not in log,
            "État affiché",
        )
    if status == 16:
        exiger("NovaOS: catalogue verifie - candidats=1" in log, "Sélection interne")
        exiger(
            "NovaOS: DMA non etabli" in log and not result["rapport"]["noeuds"],
            "Refus DMA",
        )
        units = re.findall(
            r"NovaOS: DRHD segment=(\d+) base=(0x[0-9a-f]+) include_all=(true|false)",
            log,
        )
        hardware = result["vtd"]["entree"]
        exiger(
            len(units) == len(hardware) == result["rapport"]["drhd"], "Couverture DRHD"
        )
        for (_, base, _), unit in zip(units, hardware, strict=True):
            exiger(
                int(base, 16) == int(unit["fenetre"].split("-")[0], 16)
                and not unit["tes"],
                "Base/TES",
            )
        result["drhd_decrits"] = units
    else:
        exiger("NovaOS: catalogue verifie" not in log, "Publication après refus")
    result["confiance_affichee"] = expected if status is not None else None
    result["repli_interne_disqualifiant"] = status == 9
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("supports", type=Path)
    p.add_argument("firmware", type=Path)
    p.add_argument("sortie", type=Path)
    p.add_argument("--worktrees", type=Path, required=True)
    a = p.parse_args()
    supports, fw, out = map(Path.resolve, (a.supports, a.firmware, a.sortie))
    stores = [
        Edk2VarStore(supports / (s + ".fd")).get_varlist()
        for s in ("nominal", "inactif")
    ]
    exiger(set(stores[0]) == set(stores[1]), "Variables différentes")
    diff = {k for k in stores[0] if stores[0][k].data != stores[1][k].data}
    exiger(diff == {"SecureBootEnable"}, "Seul le choix du propriétaire doit varier")
    out.mkdir(parents=True, exist_ok=False)
    result = {
        "qemu": subprocess.check_output(
            ["qemu-system-x86_64", "--version"], text=True
        ).splitlines()[0],
        "firmware": digest(fw),
        "support": digest(supports / "diagnostic.img"),
        "garde": {},
        "cas": {},
    }
    try:
        with qualification(a.worktrees.resolve(), result["garde"]):
            for topo in ("direct", "pont"):
                root = out / topo
                root.mkdir()
                cases = result["cas"][topo] = {}
                matrix = [
                    (store, image, status)
                    for store in ("nominal", "inactif")
                    for image, status in (
                        ("diagnostic", 16),
                        ("signature-interne", 6),
                        ("paquet", 18),
                    )
                ]
                matrix += [
                    ("nominal", "configuration", 9),
                    ("revoque", "diagnostic", None),
                    ("inactif", "sans-signature", 16),
                ]
                for store, image, status in matrix:
                    cases[store + "-" + image] = scenario(
                        root, topo, image, store, status, supports, fw
                    )
                    sauver(out / "bilan.json", result)
    finally:
        sauver(out / "bilan.json", result)


if __name__ == "__main__":
    main()
