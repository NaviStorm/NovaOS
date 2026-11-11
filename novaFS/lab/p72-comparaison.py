#!/usr/bin/env python3
"""Diagnostic préalable et comparaison appariée sur deux cibles Darwin."""
import argparse
from collections import defaultdict
import gzip
import hashlib
import json
from itertools import product
from math import ceil
import os
from pathlib import Path
import platform
import re
import runpy
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
BASE = "a4ba7173a6dc212e1a59d868e4cfa4488690b4ec"
P7 = runpy.run_path(str(ROOT / "lab/p7-validation.py"))
ARCHES = P7["ARCHES"]
PROFILES = ("clair", "chiffre", "lz4", "snapshot", "clone")


def probes(root):
    """Sondes ajoutées aux copies de compilation. Les appels de production restent identiques."""
    fields = {}
    def sample(name):
        fields[name] = None
        return f'#[cfg(feature="latency-probes")] let _p{name} = crate::latency_probes::enter(crate::latency_probes::Phase::{name});\n'
    def insert(file, old, new):
        path = root / "src" / file
        s = path.read_text()
        assert s.count(old) == 1, (path, old)
        path.write_text(s.replace(old, new))
    def function(file, name, phase):
        s = (root / "src" / file).read_text()
        signatures = re.findall(r'\bfn '+name+r'\b[^{]*\{', s)
        assert len(signatures) == 1, (name, signatures)
        header, = signatures
        insert(file, header, header+"\n"+sample(phase))
    def region(file, start, end, phase):
        insert(file, start, sample(phase)+start)
        insert(file, end, f'#[cfg(feature="latency-probes")] drop(_p{phase});\n'+end)

    for file, fn, phase in (
        ("secure_pool.rs","read_header","Entetes"), ("secure_pool.rs","load","Banque"),
        ("secure_pool.rs","signed_views","SignaturesVues"), ("secure_pool.rs","save","PreparationBanque"),
        ("secure_commit.rs","decode","Attestation"), ("secure_commit.rs","publish","Publication"),
        ("secure_unit.rs","fingerprint","EmpreintePaire"), ("crypto.rs","verify_mac","VerificationMac"),
        ("shared_graph.rs","open","Graphe"), ("shared_graph.rs","allocate","ReservationPhysique"),
        ("engine.rs","reserve","ReservationFS"), ("shared.rs","validate_admission","Admission"),
        ("shared.rs","mount_verified_volume","MontageVue"), ("shared.rs","stage_change","PreparationFS"),
        ("cache.rs","invalidate","Invalidation")):
        function(file,fn,phase)
    # Limiter les deux portées au calcul cryptographique.
    insert("secure_unit.rs", "let (nonce, tag) = keys.key(ctx)?.seal(", sample("Scellement")+"let (nonce, tag) = keys.key(ctx)?.seal(")
    insert("secure_unit.rs", "header[AAD..NONCE_END].copy_from_slice(&nonce);",
           '#[cfg(feature="latency-probes")] drop(_pScellement);\nheader[AAD..NONCE_END].copy_from_slice(&nonce);')
    insert("secure_unit.rs","keys.key(ctx)?.open(",sample("OuvertureAEAD")+"keys.key(ctx)?.open(")
    region("secure_commit.rs", "for copies in records.chunks_exact(2)", "verify(d, previous)?;", "SelectionGenerations")
    start = "let gate = self.disk.mutation_lock().ok_or(Error::ReadOnly)?;"
    insert("secure_pool.rs", start, sample("Acquisition")+start)
    end = "// Un drain peut stabiliser"
    insert("secure_pool.rs",end,'#[cfg(feature="latency-probes")] drop(_pAcquisition);\n'+sample("SectionExclusive")+end)
    insert("secure_pool.rs","let mut validation = Validation::new(\n            self.log.head(),\n            pool,",sample("Prevalidation")+"let mut validation = Validation::new(\n            self.log.head(),\n            pool,")
    end = "let old = crate::shared::views_at_head("
    insert("secure_pool.rs",end,'#[cfg(feature="latency-probes")] drop(_pPrevalidation);\n'+end)
    path = root / "lab/latency_probes.rs"
    s = path.read_text()
    s = s.replace("    Decompression,\n}", "    Decompression,\n"+",\n".join(fields)+",\n}")
    s = s.replace("[&str; 12]", f"[&str; {12+len(fields)}]")
    s = s.replace('    "decompression_lz4",','    "decompression_lz4",\n'+
                  ",\n".join(json.dumps(f) for f in fields)+",")
    path.write_text(s)


def build(dest, version):
    with tempfile.TemporaryDirectory(prefix="novafs-p72-") as tmp:
        tmp = Path(tmp)
        src = tmp/"novaFS"
        if version == "p71":
            tar = tmp/"base.tar"
            with tar.open("wb") as f:
                subprocess.run(["git","archive",BASE,"novaFS"], cwd=ROOT.parent, stdout=f, check=True)
            subprocess.run(["tar","-xf",tar,"-C",tmp],check=True)
        else:
            shutil.copytree(ROOT,src,ignore=shutil.ignore_patterns("target","docs","__pycache__"))
        shutil.copyfile(ROOT/"examples/p72-lab.rs", src/"examples/p72-lab.rs")
        probes(src)
        for arch in ARCHES:
            for detail in (False,True):
                env = dict(os.environ, NOVAFS_TARGET=arch+"-apple-darwin", CARGO_TARGET_DIR=str(dest/"build"))
                cmd = [src/"lab/run","build","--offline","--example","p72-lab","--example","p7-lab","--message-format=json"]
                if detail:
                    cmd += ["--features","latency-probes"]
                logs = subprocess.check_output(cmd, env=env, text=True)
                for line in logs.splitlines():
                    msg = json.loads(line)
                    if exe := msg.get("executable"):
                        with open(exe,"rb") as f:
                            assert f.read(8) == bytes.fromhex("cffaedfe")+P7["CPUS"][arch].to_bytes(4,"little")
                        shutil.copy2(exe,dest/f"{version}-{arch}-{detail}-{msg['target']['name']}")


def measure_series(dest, fixture, context, count):
    arch, version, detailed = context["arch"], context["version"], context["sondes"]
    binary = dest / f"{version}-{arch}-{detailed}-p72-lab"
    events, process = P7["run_measured"](
        binary, "mesurer", context["secteur"], context["profil"],
        0x1357 + context["repetition"], fixture, count,
    )
    if context["profil"] != "clair":
        process.update(events.pop(0))
    assert len(events) == count
    for event in events:
        assert event["lectures"][:3] == [2, 8, 18]
        assert event["pile_reservee_octets"] + event["tas_octets"] <= 7 * 2**20
        if detailed:
            event["phases"] = {
                name: [values["mur"]["n"], values["mur"]["somme_ns"], values["cpu_thread"]["somme_ns"]]
                for name, values in event["phases"].items()
            }
            assert event["phases"]["Graphe"][0] == (2 if version == "p71" else 1)
    return events, dict(**context, **process)


def file_hash(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def bench(dest, diagnostic):
    groups = defaultdict(list)
    processes = []
    label = "diagnostic" if diagnostic else "comparaison"
    profiles = ("clone",) if diagnostic else PROFILES
    repetitions = range(1 if diagnostic else 2)
    passes = (True,) if diagnostic else (False, True)
    with gzip.open(dest / (label + ".jsonl.gz"), "wt") as out:
        for rep, sector, profile in product(repetitions, (512, 4096), profiles):
            fixture = dest / f"{profile}-{sector}-{rep}.img"
            if not fixture.exists():
                with tempfile.TemporaryDirectory(dir=dest) as tmp:
                    image = Path(tmp) / "source"
                    P7["run"](dest / "p71-aarch64-False-p72-lab", "creer", sector, profile, 0x1357 + rep, image, 0)
                    image.rename(fixture)
            digest = file_hash(fixture)
            versions = ("p71",) if diagnostic else ("p71", "p72")
            if rep % 2:
                versions = versions[::-1]
            for arch, detailed, version in product(ARCHES, passes, versions):
                context = dict(arch=arch, profil=profile, secteur=sector, repetition=rep,
                               version=version, sondes=detailed, image_sha256=digest)
                count = 32 if diagnostic or not detailed else 4
                events, process = measure_series(dest, fixture, context, count)
                processes.append(process)
                groups[version, arch, profile, detailed].extend(events)
                for event in events:
                    out.write(json.dumps(dict(**context, **event), separators=(",", ":")) + "\n")
            assert file_hash(fixture) == digest
            print(label, rep, sector, profile, flush=True)

    rows = []
    for (version, arch, profile, detailed), events in groups.items():
        row = P7["summarize"](events)
        n = len(events)
        ordered = sorted(events, key=lambda event: event["mur_ns"])
        tail = ordered[ceil(n * .99) - 1]
        row.update(moyenne_ms=sum(e["mur_ns"] for e in events) / n / 1e6,
                   lectures_moy=sum(sum(e["lectures"]) for e in events) / n,
                   allocations_moy=sum(e["allocations"] for e in events) / n,
                   appel_p99=tail, appel_max=ordered[-1])
        if detailed:
            row["phases_moy"] = {
                name: [sum(e["phases"][name][i] for e in events) / n for i in range(3)]
                for name in tail["phases"]
            }
        rows.append(dict(version=version, arch=arch, profil=profile, sondes=detailed, **row))
    info = dict(
        reference=BASE, macos=platform.mac_ver()[0],
        rust=subprocess.check_output(["rustup", "run", "stable", "rustc", "--version"], text=True).strip(),
        binaires={exe.name:file_hash(exe) for exe in dest.glob("*-p72-lab")},
        processus=processes, distributions=rows,
    )
    if not diagnostic:
        info["horloges"] = {a: P7["run"](dest / f"p72-{a}-False-p7-lab", "horloge")[0] for a in ARCHES}
    with gzip.open(dest / (label + ".json.gz"), "wt") as f:
        json.dump(info, f, ensure_ascii=False)


def check_exchanges(dest):
    historic = runpy.run_path(str(ROOT / "lab/p71-comparaison.py"))
    exchanges = []
    # Le format et les exports doivent rester lisibles dans les deux sens.
    p7 = historic["OLD"]
    with tempfile.TemporaryDirectory(prefix="novafs-echange-p72-") as tmp:
        tmp = Path(tmp)
        archive = tmp / "sources.tar"
        with archive.open("wb") as f:
            subprocess.run(["git", "archive", p7, "novaFS"], cwd=ROOT.parent, stdout=f, check=True)
        subprocess.run(["tar", "-xf", archive, "-C", tmp], check=True)
        for arch in ARCHES:
            historic["construire"](tmp/"novaFS", arch, False, dest/f"p7-{arch}-False-p7-lab")
    for version, name in (("p7", "0.9.9"), ("p71", "0.9.10")):
        bins = {(label, arch, False):dest/f"{v}-{arch}-False-p7-lab"
                for label,v in (("p7",version),("p71","p72")) for arch in ARCHES}
        cases = historic["echanges"](bins, dest)
        for case in cases:
            for key in ("producteur", "lecteur"):
                case[key] = {"p7":name, "p71":"0.9.11"}[case[key]]
        exchanges.extend(cases)
    P7["write"](dest/"echanges.json", exchanges)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("etape",choices=("reference","diagnostic","candidat","comparer","echanges"))
    p.add_argument("sortie",type=Path)
    args=p.parse_args()
    dest=args.sortie.resolve()
    dest.mkdir(exist_ok=True,parents=True)
    if args.etape in ("reference","candidat"):
        build(dest,"p71" if args.etape=="reference" else "p72")
    elif args.etape=="echanges":
        check_exchanges(dest)
    else:
        bench(dest,args.etape=="diagnostic")


if __name__=="__main__":
    main()
