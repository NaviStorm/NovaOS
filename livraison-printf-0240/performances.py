"""Comparer les deux exécutables ARM64 avec les mêmes argv et /dev/null."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import shlex
from collections import Counter
import subprocess
import tempfile
import time
from options import GNU, NOVA, TMP, normalize, result, triple


def once(exe, args, dest, expected_status):
    # wait4 reports rusage for this particular child (Darwin ru_maxrss in bytes).
    start = time.perf_counter_ns()
    p = subprocess.Popen(
        [exe, *args], stdout=dest, stderr=subprocess.DEVNULL, env={"LC_ALL": "C"}
    )
    _, status, usage = os.wait4(p.pid, 0)
    p.returncode = os.waitstatus_to_exitcode(status)
    elapsed = time.perf_counter_ns() - start
    if p.returncode != expected_status:
        raise ValueError((exe, p.returncode))
    return dict(
        ns=elapsed,
        rss_octets=usage.ru_maxrss,
        cpu_utilisateur_s=usage.ru_utime,
        cpu_systeme_s=usage.ru_stime,
        statut=p.returncode,
    )


def charge():
    # Les noms de processus seuls suffisent pour relever les campagnes concurrentes.
    try:
        ps = subprocess.check_output(
            ["/bin/ps", "-Ao", "comm="], text=True, stderr=subprocess.DEVNULL
        )
        counts = Counter(Path(line.strip()).name for line in ps.splitlines())
        concurrent = {
            n: count
            for n, count in counts.items()
            if n.startswith("qemu-system-") or n in ("rustc", "cargo", "xorriso")
        }
    except subprocess.SubprocessError:
        concurrent = None
    return dict(moyennes=os.getloadavg(), processus=concurrent)


def run(out, full):
    debut = datetime.now(timezone.utc).isoformat()
    out.mkdir(parents=True, exist_ok=True)
    cases = [("appel court", ["%s", "x"])]
    for n in [1, 4, 12, 24, 30] if full else [30]:
        cases.extend(
            [
                ("texte", ["%s\n", *(["N" * 48] * n)]),
                ("binaire", ["%b", *([r"\0\xffA" * 6] * n)]),
                ("décimal", ["%+.9g\n", *(["98765.125"] * n)]),
            ]
        )
    if full:
        cases.extend([
            ("format vide", [""]),
            ("NUL et TAB", ["%b", *([r"A\0\tB" * 6] * 30)]),
            ("étoiles et positions", ["%3$*1$.*2$f", *(["7", "2", "3.125"] * 10)]),
            ("erreur de format", ["OK%s%n", "X"]),
        ])
    rnd = 25 if full else 7
    rows = []
    before = charge()
    print("Taille ou nombre | Traitement   | Nova printf | GNU printf")
    print("-----------------|--------------|-------------|-----------")
    with open(os.devnull, "wb") as dest:
        for label, args in cases:
            # Check both the wire format and the shell line limits of this base.
            assert (
                len(args) <= 32 and 1 + sum(2 + len(a.encode()) for a in args) <= 2560
            )
            assert len(shlex.join(["printf", *args]).encode()) < 2048
            a = result(NOVA, args, "C")
            b = result(GNU, args, "C")
            assert a == (b[0], b[1], normalize(b[2]))
            if label == "erreur de format":
                assert a[0] == 1 and a[1] == b"OKX" and a[2]
            else:
                assert a[0] == 0 and not a[2]
            samples = {"Nova": [], "GNU": []}
            exes = {"Nova": NOVA, "GNU": GNU}
            for name, exe in exes.items():
                for _ in range(3):
                    once(exe, args, dest, a[0])
            for i in range(rnd):
                for name in ["GNU", "Nova"] if i % 2 else ["Nova", "GNU"]:
                    samples[name].append(once(exes[name], args, dest, a[0]))
            summary = {}
            for name, s in samples.items():
                med = statistics.median(x["ns"] for x in s)
                summary[name] = dict(
                    mediane_ns=med,
                    min_ns=min(x["ns"] for x in s),
                    max_ns=max(x["ns"] for x in s),
                    mad_ns=statistics.median(abs(x["ns"] - med) for x in s),
                    octets_par_s=len(a[1]) * 1e9 / med,
                    pic_rss_octets=max(x["rss_octets"] for x in s),
                )
            n = len(args) - 1
            print(
                f"{n:12} arg. | {label:12} | {summary['Nova']['mediane_ns'] / 1e6:8.3f} ms | {summary['GNU']['mediane_ns'] / 1e6:7.3f} ms"
            )
            rows.append(
                dict(
                    traitement=label,
                    args=args,
                    sortie_sha256=hashlib.sha256(a[1]).hexdigest(),
                    octets=len(a[1]),
                    captures={"Nova": triple(a), "GNU": triple(b)},
                    rapport=summary["Nova"]["mediane_ns"] / summary["GNU"]["mediane_ns"],
                    seuil_respecte=10 * summary["Nova"]["mediane_ns"] <= 11 * summary["GNU"]["mediane_ns"],
                    brut=samples,
                    resume=summary,
                )
            )
    (out / "performances.json").write_text(
        json.dumps(
            dict(
                debut_utc=debut,
                fin_utc=datetime.now(timezone.utc).isoformat(),
                executables={
                    name: {
                        "chemin": str(exe),
                        "sha256": hashlib.sha256(exe.read_bytes()).hexdigest(),
                        "version": result(exe, ["--version"], "C")[1].splitlines()[0].decode(),
                    }
                    for name, exe in exes.items()
                },
                machine=platform.platform(),
                architecture=platform.machine(),
                profil="complet" if full else "court",
                charge_avant=before,
                charge_apres=charge(),
                destination="/dev/null",
                echauffements=3,
                passages=rnd,
                conditions="Mesure par processus sur le Mac partagé ; lancement inclus.",
                cas=rows,
            ),
            indent=2,
            ensure_ascii=False,
        )
        + "\n"
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--complet", action="store_true")
    p.add_argument("--sortie", type=Path)
    a = p.parse_args()
    TMP.mkdir(parents=True, exist_ok=True)
    run(a.sortie or Path(tempfile.mkdtemp(prefix="perf-", dir=TMP)), a.complet)
