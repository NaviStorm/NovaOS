"""Recette différentielle et mesures sur Darwin ARM64."""

import argparse
import fcntl
import hashlib
import json
import math
import os
import platform
import shutil
import signal
import statistics
import subprocess as sp
import time
from pathlib import Path
from flux import measure as verify_streams

HERE = Path(__file__).resolve().parents[1]
NOVA = HERE / "bin/sleep"
GNU = Path(shutil.which("gsleep") or "/opt/homebrew/bin/gsleep")
# Seuils fixés avant campagne, horloge monotone externe, lancement inclus.
SLACK = 0.250
CANCEL = 0.065


def cases(full):
    yield "aucun", [], 0
    for arg in [
        "0",
        "0s",
        "0m",
        "0h",
        "0d",
        "000",
        "+0",
        "  0",
        "0e20",
        "0x0",
        ".0",
        "--",
    ]:
        yield arg, [arg], 0
    for name in ("help", "version"):
        for i in range(1, len(name) + 1):
            flag = "--" + name[:i]
            yield flag, [flag], 0
            if full:
                for suffix, argv in [
                    ("après", ["bad", flag]),
                    ("avant", [flag, "bad"]),
                    ("valeur", [flag + "=0"]),
                    ("littéral", ["--", flag]),
                ]:
                    yield flag + suffix, argv, 0
    for i, args in enumerate(
        [
            ["--h", "--v"],
            ["--v", "--h"],
            ["-x", "--h"],
            ["--h", "-x"],
            ["0", "--", "0"],
            ["--", "--"],
            ["--", "-0"],
            ["0", "--", "--h"],
            ["bad", "bad2", "inf"],
            ["-h"],
            ["-v"],
            ["-1"],
            ["--", "-1"],
            ["--helpful"],
            ["--=x"],
            ["-"],
            [""],
            ["NaN"],
            ["0S"],
            ["1ms"],
            ["0 "],
            ["42d", "42day"],
            ["--", "-1e-999"],
            ["1e999", "invalid"],
            ["0x"],
            ["0xp1"],
            ["1,0.0"],
            ["0é"],
            ["a'b"],
            ["a\nb"],
        ]
    ):
        yield f"syntaxe-{i}", args, 0
    for args, delay in [
        (["0.003"], 0.003),
        ([".002s", "3e-3"], 0.005),
        (["0.0001m"], 0.006),
        ([".000001h"], 0.0036),
        (["1e-7d"], 0.00864),
        (["0x.002p1"], 0.0009765625),
        (["0x0.01d"], float.fromhex("0x0.01d")),
        (["1e-999"], 0),
        (["0x1p-99999"], 0),
    ]:
        yield "durée-" + "+".join(args), args, delay
    for arg in [
        "inf",
        "Infinity",
        "infm",
        "1e999",
        "0x1p10000",
        "9223372036854775808",
        "3600",
    ]:
        yield "arrêt-" + arg, [arg], None


def execute(exe, args, *, loc="C", stop=None, posix=False, fd=None, sig=signal.SIGTERM):
    env = dict(os.environ, LC_ALL=loc)
    env.pop("POSIXLY_CORRECT", None)
    if posix:
        env["POSIXLY_CORRECT"] = "1"
    start = time.monotonic_ns()
    with sp.Popen(
        ["sleep", *args],
        executable=str(exe),
        env=env,
        stdin=sp.PIPE,
        stdout=sp.PIPE if fd is None else fd,
        stderr=sp.PIPE,
    ) as proc:
        try:
            if stop is not None:
                time.sleep(stop)
                assert proc.poll() is None, (exe, args, "sortie anticipée")
                proc.send_signal(sig)
            out, err = proc.communicate(input=b"donnees non consommees\n", timeout=2)
        except BaseException:
            proc.kill()
            proc.wait()
            raise
        return dict(
            rc=proc.returncode,
            stdout=(out or b"").hex(),
            stderr=err.hex(),
            ns=time.monotonic_ns() - start,
        )


def options(full):
    rows = []

    def trial(label, args, delay, locale="C", posix=False, **kw):
        g, n = [
            execute(
                exe,
                args,
                loc=locale,
                stop=CANCEL if delay is None else None,
                posix=posix,
                **kw,
            )
            for exe in (GNU, NOVA)
        ]
        gout, nout = [bytes.fromhex(x["stdout"]) for x in (g, n)]
        gerr = bytes.fromhex(g["stderr"]).replace(b"gsleep:", b"sleep:")
        doc = gout.startswith((b"Usage:", b"sleep (GNU"))
        french = locale in ("fr", "fr-CA", "Fr-Latn-419")
        if doc and gout.startswith(b"Usage:"):
            output = (
                "Attendre la somme" if french else "Wait for the sum"
            ).encode() in nout and all(
                w in nout for w in (b"--help", b"--version", b"NUMBER")
            )
        elif doc:
            output = nout == b"sleep (nova-cli) \xe2\x80\x94 nova-cli 0.20.0\n"
        else:
            output = nout == gout
        error = gerr == bytes.fromhex(n["stderr"])
        # Sur un tube fermé, les deux producteurs doivent être tués par SIGPIPE.
        ok = g["rc"] == n["rc"] and output and error
        if delay is None:
            ok &= n["rc"] == -kw.get("sig", signal.SIGTERM)
        elif delay and n["rc"] == 0:
            ok &= all(delay <= m["ns"] / 1e9 <= delay + SLACK for m in (n, g))
        rows.append(
            dict(
                cas=label,
                argv=[os.fsencode(a).hex() for a in args],
                locale=locale,
                posix=posix,
                demande_s=delay,
                gnu=g,
                nova=n,
                documentation=doc,
                ok=ok,
            )
        )

    for name, args, d in cases(full):
        trial(name, args, d)
    for tag in [
        "en-US",
        "fr",
        "fr-CA",
        "Fr-Latn-419",
        "fr--CA",
        "fr-Abcd",
        "fr_CA",
        "fr_FR.UTF-8",
    ]:
        for flag in ["--help", "--version"]:
            trial("langue-" + tag + flag, [flag], 0, tag)
    for loc in ["C", "fr_FR.UTF-8", "en_US.UTF-8"]:
        for arg in ["0,002", "0.002", "0x0,01p0", "x", "a’b"]:
            trial(
                "locale-" + arg, [arg], 0.002 if arg in ("0,002", "0.002") else 0, loc
            )
    for args in [["0", "--h"], ["--h", "0"], ["0", "--", "0"], ["--", "0"]]:
        trial("POSIX", args, 0, posix=True)
    trial("octet-FF", [b"\xff"], 0)
    trial("option-FF", [b"-\xff"], 0)
    trial("SIGINT", ["inf"], None, sig=signal.SIGINT)
    for flag in ("--help", "--version", "0"):
        with open(os.devnull, "rb") as source:
            trial("sortie-lecture-seule-" + flag, [flag], 0, fd=source)
    rd, wr = os.pipe()
    os.close(rd)
    try:
        trial("tube-sans-lecteur", ["--help"], 0, fd=wr)
        trial("silence-tube-fermé", ["0"], 0, fd=wr)
    finally:
        os.close(wr)
    return rows


def sample(exe, arg):
    start = time.monotonic_ns()
    with sp.Popen(
        ["sleep", arg],
        executable=str(exe),
        stdin=sp.DEVNULL,
        stdout=sp.DEVNULL,
        stderr=sp.DEVNULL,
        env=dict(os.environ, LC_ALL="C"),
    ) as proc:
        _, status, usage = os.wait4(proc.pid, 0)
        elapsed = time.monotonic_ns() - start
        proc.returncode = os.waitstatus_to_exitcode(status)
        assert (
            proc.returncode == 0 and float(arg) <= elapsed / 1e9 <= float(arg) + SLACK
        )
    return dict(
        ns=elapsed,
        retard_ns=elapsed - round(float(arg) * 1e9),
        cpu_ns=round((usage.ru_utime + usage.ru_stime) * 1e9),
        rss_octets=usage.ru_maxrss,
    )


def performance(full):
    rows = []
    table = [
        "Demande | Temps Nova/GNU ms | Retard p95 Nova/GNU ms | CPU Nova/GNU ms | RSS Nova/GNU Kio"
    ]
    for arg in (
        ["0", ".001", ".005", ".020", ".100", ".250"] if full else ["0", ".005", ".100"]
    ):
        pair = [("Nova", NOVA), ("GNU", GNU)]
        data = {key: [] for key, _ in pair}
        for _, exe in pair:
            sample(exe, arg)
        for i in range(15 if full else 5):
            for key, exe in pair if i % 2 == 0 else reversed(pair):
                data[key].append(sample(exe, arg))
        row = [arg]
        for key, scale in [
            ("ns", 1e6),
            ("retard_ns", 1e6),
            ("cpu_ns", 1e6),
            ("rss_octets", 1024),
        ]:
            fields = []
            for label, _ in pair:
                values = sorted(r[key] for r in data[label])
                v = (
                    values[math.ceil(0.95 * len(values)) - 1]
                    if key == "retard_ns"
                    else statistics.median(values)
                )
                fields.append(f"{v / scale:.3f}")
            row.append(" / ".join(fields))
        table.append(" | ".join(row))
        rows.append(dict(duree_s=float(arg), **data))
    print("\n".join(table))
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("action", choices=["options", "mesures"])
    group = p.add_mutually_exclusive_group()
    group.add_argument("--rapide", action="store_true", help="campagne courte (défaut)")
    group.add_argument(
        "--complet", action="store_true", help="tous les croisements ou 15 passages"
    )
    p.add_argument("--out", type=Path, help="fichier JSON de sortie")
    a = p.parse_args()
    assert platform.system() == "Darwin" and platform.machine() == "arm64"
    info = dict(
        systeme=platform.platform(),
        charge_avant=os.getloadavg(),
        oracle=str(GNU.resolve()),
        version=sp.check_output([GNU, "--version"], text=True).splitlines()[0],
        tolerance_s=SLACK,
        annulation_s=CANCEL,
        sha256={
            str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in (NOVA, GNU)
        },
    )
    if a.action == "mesures":
        with (Path(os.environ["NOVAOS_TMPDIR"]) / "arc027-mesures.lock").open(
            "a+"
        ) as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            rows = performance(a.complet)
    else:
        rows = options(a.complet)
        info["flux"] = {
            label: verify_streams(exe) for label, exe in (("GNU", GNU), ("Nova", NOVA))
        }
    info.update(cas=rows, charge_apres=os.getloadavg())
    dest = a.out or Path(os.environ["TMPDIR"]) / ("sleep-" + a.action + ".json")
    dest.write_text(json.dumps(info, ensure_ascii=False, indent=2) + "\n")
    failed = [r["cas"] for r in rows if r.get("ok") is False]
    print(f"{len(rows)} lignes, écarts {failed}, {dest}")
    return int(bool(failed))


if __name__ == "__main__":
    raise SystemExit(main())
