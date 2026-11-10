#!/usr/bin/env python3
"""Mesure la CLI P2 sur des images ordinaires jetables sous macOS."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import tempfile

LAB = Path(__file__).resolve().parent
BLOCK = 4096
BLOCKS = 2**21
POOL_BLOCKS = 4352
FS_ID = "0b" * 16
LV_ID = "1d" * 16
ARCHES = {"aarch64": 0x100000C, "x86_64": 0x1000007}


def cargo(env, action, *args):
    subprocess.run([LAB / "run", action, "--offline", *args], env=env, stdout=sys.stderr, check=True)


def clone(src, dest):
    # clonefile sur APFS préserve les trous du fichier. cmp relira tous les octets.
    subprocess.run(["/bin/cp", "-c", src, dest], check=True)


def make_images(tmp, bench):
    healthy = tmp / "sain.raw"
    with healthy.open("x+b") as image:
        image.truncate(BLOCK * BLOCKS)
        subprocess.run([bench, "4096", "prepare-controle"], stdin=image,
                       stdout=subprocess.DEVNULL, check=True)
    if healthy.stat().st_blocks * 512 >= healthy.stat().st_size:
        raise RuntimeError("le fichier de 8 Gio n’est pas creux sur ce support")
    damaged = tmp / "corrompu.raw"
    clone(healthy, damaged)
    # Seule la dernière écriture du marqueur contient cette page uniforme.
    # La borne de recherche couvre 16 Mio de cette fixture déterministe.
    with damaged.open("r+b", buffering=0) as file:
        target = [i for i in range(4096) if file.read(BLOCK) == bytes([0x5c]) * BLOCK]
        if len(target) != 1:
            raise RuntimeError("le bloc privé de la dernière génération doit être unique")
        file.seek(target[0] * BLOCK)
        file.write(b"\x5d")
    return healthy, damaged, target[0]


def stack_reservation(binary, cpu):
    # En-tête mach_header_64 et commande LC_MAIN d’un exécutable mono-cible.
    with binary.open("rb") as stream:
        header = stream.read(32)
        if header[:8] != bytes.fromhex("cffaedfe") + struct.pack("<I", cpu):
            raise RuntimeError(f"type Mach-O inattendu pour {binary}")
        commands = struct.unpack_from("<I", header, 16)[0]
        for _ in range(commands):
            kind, n = struct.unpack("<II", stream.read(8))
            if not 8 <= n <= 65536:
                raise ValueError("commande Mach-O invalide")
            command = stream.read(n - 8)
            if kind == 0x80000028:  # LC_MAIN
                return struct.unpack("<QQ", command)[1]
    raise RuntimeError("LC_MAIN manquant")


def measure(binary, image, command, case, block, instrumented):
    expected = int(case == "lv_corrompu")
    with image.open("rb") as stream:
        result = subprocess.run(["/usr/bin/time", "-l", binary, *command],
                                stdin=stream, capture_output=True, text=True)
    if result.returncode != expected:
        raise RuntimeError(f"code {result.returncode}, attendu {expected} : {result.stderr}")
    report = json.loads(result.stdout)
    if report["statut"] != ("incoherent" if expected else "coherent"):
        raise RuntimeError(report)
    if expected:
        faults = [slot["anomalie"] for slot in report["slots"] if not slot["vue_verifiee"]]
        if len(faults) != 1 or faults[0]["bloc"] != block:
            raise RuntimeError("le diagnostic ne localise pas l'unique bloc altéré")
    if case == "pool_16_lv":
        if len(report["volumes"]) != 16 or any(
                v["contenu"]["etat"] != "verifie" for v in report["volumes"]):
            raise RuntimeError("les seize LV prêts doivent tous être vérifiés")
    match = re.search(r"mesure pic_tas_octets=(\d+) pile_reservee_octets=(\d+) total_octets=(\d+)", result.stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", result.stderr)
    footprint = re.search(r"(\d+)\s+peak memory footprint", result.stderr)
    if bool(match) != instrumented or not rss or not footprint:
        raise RuntimeError("mesures Darwin incomplètes : " + result.stderr)
    record = {"rss_pic": int(rss[1]), "empreinte_pic": int(footprint[1])}
    if instrumented:
        heap, stack, total = map(int, match.groups())
        if heap + stack != total:
            raise RuntimeError("somme tas/pile divergente")
        record.update(cas=case, secteur=int(command[2]), code=expected, tas_pic=heap,
                      pile_reservee=stack, tas_plus_pile=total)
    return record, report


def restitution(bench, sector):
    output = subprocess.check_output([bench, str(sector), "restitution-controle"], text=True)
    rows = []
    for case, cycles, before, after, peak in re.findall(
            r"controle=(\w+) cycles=(\d+) vivant_avant=(\d+) vivant_apres=(\d+) pic_phase=(\d+)", output):
        if before != after or int(cycles) != 64:
            raise RuntimeError("restitution incomplète")
        rows.append({"cas": case, "secteur": sector, "cycles": int(cycles), "avant": int(before),
                     "apres": int(after), "pic_phase": int(peak)})
    if len(rows) != 3:
        raise RuntimeError("trois contrôles de restitution requis")
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pool", type=Path, help="fixture P0 externe de 16 LV, lue en RO")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("macOS requis")
    results = {"unite": "octet", "architectures": {}}
    with tempfile.TemporaryDirectory(prefix="novafs-controle-") as temp:
        tmp = Path(temp)
        target_dir = tmp / "cargo"
        binaries = {}
        for arch, cpu in ARCHES.items():
            target = arch + "-apple-darwin"
            env = {**os.environ, "NOVAFS_TARGET": target, "CARGO_TARGET_DIR": str(target_dir)}
            dest = tmp / arch
            dest.mkdir()
            out = target_dir / target / "release"
            cargo(env, "build", "--bin", "novafs")
            standard = dest / "novafs-standard"
            shutil.copy2(out / "novafs", standard)
            cargo(env, "build", "--bin", "novafs", "--bin", "novafs-bench", "--features", "check-metrics")
            binary = dest / "novafs"
            shutil.copy2(out / "novafs", binary)
            shutil.copy2(out / "novafs-bench", dest / "novafs-bench")
            for measured in (binary, standard):
                if stack_reservation(measured, cpu) != 2 * 1024 * 1024:
                    raise RuntimeError("réservation de pile Mach-O inattendue")
            binaries[arch] = binary
        healthy, damaged, bad_block = make_images(tmp, binaries["aarch64"].with_name("novafs-bench"))
        pool = args.pool
        if pool is None:
            pool = tmp / "pool.raw"
            env = {**os.environ, "NOVAFS_TARGET": "aarch64-apple-darwin",
                   "CARGO_TARGET_DIR": str(target_dir), "NOVAFS_POOL_FIXTURE": str(pool)}
            cargo(env, "test", "--test", "volumes", "p2_pool_image_de_mesure", "--", "--ignored", "--exact")
        if not pool.is_file() or pool.stat().st_size != POOL_BLOCKS * BLOCK:
            raise ValueError("une image ordinaire du pool de 16 LV est requise")
        images = {"lv_sain": healthy, "lv_corrompu": damaged, "pool_16_lv": pool}
        for name, image in images.items():
            clone(image, tmp / (name + ".avant"))
        results.update(lv_octets=healthy.stat().st_size, lv_alloues_hote=healthy.stat().st_blocks * 512,
                       bloc_corrompu=bad_block, pool_octets=pool.stat().st_size)
        for arch, binary in binaries.items():
            records, cycles = [], []
            for sector in (512, 4096):
                for case, image in images.items():
                    command = (["check-pool", str(POOL_BLOCKS), str(sector)] if case == "pool_16_lv"
                               else ["check", str(BLOCKS), str(sector), FS_ID, LV_ID])
                    row, diag = measure(binary, image, command, case, bad_block, True)
                    standard = binary.with_name("novafs-standard")
                    reference, other = measure(standard, image, command, case, bad_block, False)
                    if diag != other:
                        raise RuntimeError("le diagnostic dépend de l’instrumentation")
                    row["sans_instrumentation"] = reference
                    records.append(row)
                cycles.extend(restitution(binary.with_name("novafs-bench"), sector))
            results["architectures"][arch] = {"sha256_cli": hashlib.sha256(binary.read_bytes()).hexdigest(),
                                               "sha256_standard": hashlib.sha256(standard.read_bytes()).hexdigest(),
                                               "pile_lc_main": stack_reservation(standard, ARCHES[arch]),
                                               "cli": records, "restitution": cycles}
        for name, image in images.items():
            subprocess.run(["/usr/bin/cmp", "-s", image, tmp / (name + ".avant")], check=True)
        results["comparaison_integrale_apres"] = list(images)
    print(json.dumps(results, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
