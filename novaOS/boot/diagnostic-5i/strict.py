"""Qualifier l'entrée terminale 5I, de la première instruction à HLT, collecte externe incluse."""

import argparse
import json
from pathlib import Path
import subprocess

from suspendu import observe as inventory
from sonde import state
from temoin import fixtures, require, sauver
from banc import digest, guest
from port_qemu import qualification
from virt.firmware.varstore.edk2 import Edk2VarStore


from point_entree import entry_witness


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for opt in ("supports", "firmware", "sortie", "worktrees", "lecteur"):
        p.add_argument(opt, type=Path)
    a = p.parse_args()
    supports, fw, out, wt, reader = [
        x.resolve() for x in (a.supports, a.firmware, a.sortie, a.worktrees, a.lecteur)
    ]
    out.mkdir(parents=True, exist_ok=False)
    result = {
        "cas": {},
        "garde": {},
        "qemu": subprocess.check_output(
            ["qemu-system-x86_64", "--version"], text=True
        ).splitlines()[0],
        "firmware": digest(fw),
        "lecteur": digest(reader),
    }
    stores = {
        s: Edk2VarStore(supports / (s + ".fd")).get_varlist()
        for s in ("nominal", "inactif")
    }
    require(
        set(stores["nominal"]) == set(stores["inactif"])
        and {
            k
            for k, v in stores["nominal"].items()
            if v.data != stores["inactif"][k].data
        }
        == {"SecureBootEnable"}
        and stores["nominal"]["SecureBootEnable"].data == b"\x01"
        and stores["inactif"]["SecureBootEnable"].data == b"\x00",
        "Modes OVMF",
    )
    # Le même jeu de vecteurs ACPI que la contre-épreuve.

    fixtures(out)
    matrix = [
        (topo, store, img, status, None)
        for topo in ("direct", "pont")
        for store in ("nominal", "inactif")
        for img, status in (("diagnostic", 16),)
    ]
    matrix.extend(
        ("direct", store, img, status, None)
        for store in ("nominal", "inactif")
        for img, status in (("signature-interne", 6), ("paquet", 18))
    )
    matrix.extend(
        ("direct", "nominal", "diagnostic", 16, name)
        for name in ("deux", "ambigu", "invalide")
    )
    with qualification(wt, result["garde"]):
        for topo, store, img, code, table in matrix:
            name = table or "-".join((topo, store, img))
            kw = dict(
                media=supports / (img + ".img"),
                variables=supports / (store + ".fd"),
                firmware=fw,
                topology=topo,
                table=out / (table + ".bin") if table else None,
            )
            first = guest(out / (name + "-cal"), **kw)
            require(
                first["rapport"] and first["rapport"]["statut"] == code,
                "Calibration " + name,
            )

            def sample(q, phase):
                sauver(Path("modele-" + phase + ".json"), state(q))
                if phase != "entree":
                    return
                sauver(
                    Path("point-entree.json"),
                    entry_witness(q, kw["media"], first["rapport"]["entree"]),
                )
                if table != "invalide":
                    inventory(q, phase, reader)
                    return
                try:
                    inventory(q, phase, reader)
                except ValueError as err:
                    require(str(err) == "Longueur DMAR", "Refus différent")
                    Path("refus.txt").write_text(str(err) + "\n")
                else:
                    raise ValueError("DMAR invalide acceptée")

            dest = out / name
            run = guest(
                dest,
                **kw,
                expected=first["rapport"],
                observe=sample,
                observe_reset=True,
                interval_events=(
                    "memory_region_ops_write",
                    "memory_region_subpage_write",
                ),
            )
            clean = (
                not run["evenements_diagnostic"]
                and run["octets_avant"]
                == run["octets_avant_sortie"]
                == run["octets_trace"]
            )
            require(
                run["disque_intact"]
                and run["entree"] == run["fin"]
                and not any(run["fin"].values()),
                "E/S SATA",
            )
            states = [
                (dest / f"modele-{s}.json").read_bytes()
                for s in ("entree", "sortie", "fin")
            ]
            require(states[0] == states[1] == states[2], "Registres modifiés")
            r = run["rapport"]
            require(
                r["statut"] == code
                and not r["noeuds"]
                and r["vivants"] == 0
                and run["halt"],
                "Rapport terminal",
            )
            journal = (dest / "journal-5i.txt").read_text()
            require(
                ("catalogue verifie" in journal) == (code == 16), "Vérification interne"
            )
            filepath = dest / "inventaire.json"
            if table == "invalide":
                require(not filepath.exists(), "Inventaire partiel")
            else:
                inv = json.loads(filepath.read_text())
                n = len(inv["modele"]["pci"])
                require(
                    len(inv["portees"]) == n == (7 if topo == "pont" else 6),
                    "Périmètre de la photographie",
                )
                rows = inv["portees"]
                sata = next(
                    key
                    for key, v in inv["modele"]["pci"].items()
                    if v["classe_revision"] >> 16 == 0x0106
                )
                nic = next(
                    key
                    for key, v in inv["modele"]["pci"].items()
                    if v["classe_revision"] >> 16 == 0x0200
                )
                if table == "ambigu":
                    require(rows[sata]["drhd"] == "unknown", "Ambiguïté non signalée")
                elif table == "deux":
                    require(
                        rows[sata]["rmrr"] == "1"
                        and rows[nic]["rmrr"] == "0"
                        and rows[nic]["drhd"] != rows[sata]["drhd"]
                        and not inv["drhd_corroborees"],
                        "Double DRHD",
                    )
                elif topo == "pont":
                    require(
                        len(inv["drhd_corroborees"]) == 1
                        and rows[sata]["drhd"]
                        == rows[nic]["drhd"]
                        == format(inv["drhd_corroborees"][0], "x")
                        and rows[nic]["taille"] == 2,
                        "Unité commune",
                    )
            result["cas"][name] = {
                "image": run["image"],
                "magasin": run["magasin"],
                "mode": store,
                "rapport": r,
                "zero_ecriture": clean,
                "support": img,
                "topologie": topo,
                "io": run["fin"],
                "trace_octets": run["octets_trace"],
                "coupe_entree": run["octets_avant"],
                "coupe_scellement": run["octets_avant_sortie"],
                "disque_intact": True,
                "statut_inventaire": "refuse" if table == "invalide" else "observe",
            }
            sauver(out / "bilan.json", result)
        name = "refus-dbx"
        dest = out / name
        denied = guest(
            dest, supports / "diagnostic.img", supports / "revoque.fd", fw, "direct"
        )
        log = (dest / "firmware.log").read_text(errors="replace")
        require(
            denied["rapport"] is None
            and "No bootable option" in log
            and denied["disque_intact"]
            and denied["fin"]["rd_operations"] > 0
            and not any(n for k, n in denied["fin"].items() if not k.startswith("rd_")),
            "Chaîne révoquée ou témoin de repli",
        )
        result["refus_dbx"] = {
            "rapport": None,
            "io_firmware": denied["fin"],
            "disque_intact": denied["disque_intact"],
        }
    result["serie_sans_ecriture"] = all(
        row["zero_ecriture"] for row in result["cas"].values()
    )
    sauver(out / "bilan.json", result)
    if not result["serie_sans_ecriture"]:
        raise SystemExit(
            "REFUS : une transaction a été observée entre le point d'entrée et HLT."
        )


if __name__ == "__main__":
    main()
