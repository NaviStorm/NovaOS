"""Comparaison appariée du coût global EFI. Trois paires par topologie, sans sonde QMP pendant l'intervalle."""

import argparse
import json
from pathlib import Path
import sys

NOVA = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(NOVA / "boot/diagnostic-5e"), str(NOVA / "tests/ethernet_v1")]
from banc import guest, digest
from port_qemu import qualification


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for k in ("supports_h", "supports_i", "fw", "cal_i", "out", "worktrees"):
        p.add_argument(k, type=Path)
    a = p.parse_args()
    h, i, fw, cal, out, wt = [
        p.resolve()
        for p in (a.supports_h, a.supports_i, a.fw, a.cal_i, a.out, a.worktrees)
    ]
    out.mkdir(parents=True, exist_ok=False)
    base = json.loads(
        (NOVA / "docs/validation/diagnostic-uefi-5h/bilan.json").read_text()
    )
    new = json.loads((cal / "bilan.json").read_text())
    if digest(fw) != base["firmware"] or digest(fw) != new["firmware"]:
        raise ValueError("Firmware différent")
    result = {"garde": {}, "paires": []}
    with qualification(wt, result["garde"]):
        for topo in ("direct", "pont"):
            for repetition in range(3):
                for profile in ("5h", "5i") if repetition % 2 == 0 else ("5i", "5h"):
                    ref = (
                        base["cas"][topo]["nominal-diagnostic"]
                        if profile == "5h"
                        else new["cas"][topo + "-nominal-diagnostic"]
                    )
                    source = h if profile == "5h" else i
                    media = source / "diagnostic.img"
                    if digest(media) != ref["image"]:
                        raise ValueError("Image différente de la calibration")
                    run = guest(
                        out / f"{topo}-{repetition}-{profile}",
                        media,
                        source / "nominal.fd",
                        fw,
                        topo,
                        expected=ref["rapport"],
                        measure_duration=True,
                    )
                    r = run["rapport"]
                    if (
                        r["statut"] != 16
                        or r["vivants"]
                        or not run["halt"]
                        or not run["disque_intact"]
                        or run["entree"] != run["fin"]
                    ):
                        raise ValueError("Issue ou disque différent")
                    result["paires"].append(
                        {
                            "profil": profile,
                            "topologie": topo,
                            "paire": repetition,
                            "duree_ns": run["duree_invite_ns"],
                            "arena": r["arena"],
                            "image": ref["image"],
                        }
                    )
    (out / "bilan.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
