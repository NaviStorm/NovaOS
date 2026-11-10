#!/usr/bin/env python3
"""Sondes de diagnostic dans une copie temporaire du code source NovaFS sur Darwin."""
import argparse
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile

LAB = Path(__file__).resolve().parent
REPO = LAB.parent.parent
# Les portées sont inclusives : en particulier pin/release sont récursives.
PHASES = {
    "src/fs/data.rs": {"read": "FsRead", "write": "FsWrite"},
    "src/codec.rs": {"check_meta": "FsIdentity", "data_hash": "FsDataHash"},
    "src/engine.rs": {"prepare": "FsPrepare", "publish": "FsPublish", "reserve": "FsReserve"},
    "src/shared_graph.rs": {
        "read_indexed_page": "PoolTranslate", "pointers": "PoolReadMap", "checked_page": "PoolCheckPage",
        "allocate": "PoolAllocate", "pin": "PoolPin", "release": "PoolRelease",
        "patch_root": "PoolPatch", "commit": "PoolCommit", "catalog_page": "PoolCatalog",
        "check_transition": "PoolLink", "store_data": "PoolStore", "checksum": "PoolHash",
    },
    "src/shared.rs": {
        "mount_cached": "MountedLookup", "stage_change": "PrepareContent",
        "publish_content": "PublishContent", "commit_prepared": "CheckAndCommit",
        "validate_admission": "Quota",
    },
}
OPTIONAL = {
    "src/engine.rs": {"cache_published_data": "CachePublished"},
}
ZERO = "PoolZero"


def copy_source(dest, ref):
    if ref == "travail":
        shutil.copytree(LAB.parent, dest / "novaFS",
                        ignore=shutil.ignore_patterns("target", "__pycache__", "docs"))
    else:
        archive = subprocess.check_output(["git", "-C", REPO, "archive", ref, "novaFS"])
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(dest, filter="data")
    return dest / "novaFS"


def instrument(root, shared):
    for file, phases in PHASES.items():
        path = root / file
        if not shared and not path.exists():
            continue
        source = path.read_text()
        optional = {name: phase for name, phase in OPTIONAL.get(file, {}).items()
                    if f"fn {name}(" in source}
        for name, phase in (phases | optional).items():
            if name == "read" and "fn read_with_cursor(" in source:
                name = "read_with_cursor"
            if name == "mount_cached" and "fn mount_with_cursor<" in source:
                name = "mount_with_cursor"
            matches = list(re.finditer(rf"\bfn {name}\b", source))
            if len(matches) != 1:
                raise RuntimeError(f"point de mesure ambigu {file} {name}: {len(matches)}")
            start = source.index("{", matches[0].end()) + 1
            probe = f"\n let _profil = crate::latency_probes::enter(crate::latency_probes::Phase::{phase});\n"
            source = source[:start] + probe + source[start:]
        if file == "src/shared.rs" and "fs.prepare_write_data(" in source:
            # P5.1 transporte les références validées hors du plan. La portée
            # équivalente à Engine::write reste préparation interne + commit.
            marker = "let (plan, refs) = fs.prepare_write_data("
            if source.count(marker) != 1:
                raise RuntimeError("portée de l'écriture P5.1 introuvable")
            source = source.replace(marker, "let _profil = crate::latency_probes::enter("
                                    "crate::latency_probes::Phase::FsWrite);\n            " + marker)
        for expression in ("*data == [0; 4096]", "*page == [0; 4096]"):
            if file in ("src/shared.rs", "src/shared_graph.rs"):
                # Portée limitée à la comparaison intégrale, sans chronométrer
                # la branche ni le traitement du bloc qui suit.
                source = source.replace(expression, "{ let _zero = crate::latency_probes::enter("
                                        f"crate::latency_probes::Phase::{ZERO}); " + expression + " }")
        path.write_text(source)
    variants = [phase for groups in (PHASES, OPTIONAL) for group in groups.values()
                for phase in group.values()] + [ZERO]
    path = root / "lab/latency_probes.rs"
    code = path.read_text()
    code = code.replace("    Empty,", "\n".join("    " + n + "," for n in variants) + "\n    Empty,")
    code = code.replace('    "sonde_vide",', "\n".join('    "' + n + '",' for n in variants) + '\n    "sonde_vide",')
    code, changes = re.subn(r"const NAMES: \[&str; (\d+)\]",
                            lambda match: f"const NAMES: [&str; {int(match[1]) + len(variants)}]", code)
    if changes != 1:
        raise ValueError("dimension des sondes introuvable")
    path.write_text(code)
    shutil.copyfile(LAB / "p51profil.rs", root / "lab/p51profil.rs")
    manifest = root / "Cargo.toml"
    manifest.write_text(manifest.read_text().replace("[features]", "[features]\np5-profil = []"))
    with manifest.open("a") as f:
        f.write('\n[[example]]\nname="p51profil"\npath="lab/p51profil.rs"\nrequired-features=["latency-probes"]\n')


def build(root, work, arch, shared):
    target = f"{arch}-apple-darwin"
    env = dict(os.environ, NOVAFS_TARGET=target, CARGO_TARGET_DIR=str(work / "target"))
    features = "latency-probes,p5-profil" if shared else "latency-probes"
    cargo = [root / "lab/run", "build", "--example", "p51profil", "--features", features, "--offline"]
    subprocess.run(cargo, check=True, env=env)
    exe = work / "target" / target / "release/examples/p51profil"
    with exe.open("rb") as f:
        cpu = 0x0100000c if arch == "aarch64" else 0x01000007
        assert f.read(8) == bytes.fromhex("cffaedfe") + cpu.to_bytes(4, "little")
    return exe


def verify(lines, shared):
    records = [json.loads(s) for s in lines.splitlines()]
    assert len(records) == (28 if shared else 4)
    for row in records[1:]:
        writing = row["operation"].startswith("ecriture")
        if row["flushs"] != (2 * row["operations"] if writing else 0):
            raise ValueError(f"nombre de synchronisations incohérent pour {row['operation']}")
        if not writing and any(row["ecritures"].values()):
            raise ValueError(f"écriture inattendue pendant {row['operation']}")


def measure(exe, dest, shared):
    lines = subprocess.check_output([exe], text=True)
    verify(lines, shared)
    dest.write_text(lines)
    print(dest, flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ref", default="ae508ce", help="commit Git ou travail")
    parser.add_argument("--sortie", type=Path, required=True)
    parser.add_argument("--arch", nargs="+", choices=["aarch64", "x86_64"], default=["aarch64", "x86_64"])
    args = parser.parse_args()
    args.sortie.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="novafs-p51-profil-") as name:
        root = copy_source(Path(name), args.ref)
        shared = (root / "src/shared.rs").is_file()
        instrument(root, shared)
        for arch in args.arch:
            exe = build(root, Path(name), arch, shared)
            measure(exe, args.sortie / f"{args.ref}-{arch}.jsonl", shared)


if __name__ == "__main__":
    main()
