#!/usr/bin/env python3
"""Échanges P6 sur images neuves, CLI natives et traduites, avec un lecteur/écrivain P5.1 historique."""
import hashlib
import json
from pathlib import Path
import runpy
import tempfile

LAB = Path(__file__).resolve().parent
P5_EXPORT = runpy.run_path(str(LAB / "p5-export.py"))
LEGACY = runpy.run_path(str(LAB / "p51-validation.py"))
BUILD = runpy.run_path(str(LAB / "test-pool-interop.py"))["build"]
ARCHS = P5_EXPORT["CPUS"]
run = P5_EXPORT["run"]
BLOCKS = 2048
POOL, TARGET, OWNER = "13" * 16, "24" * 16, "35" * 16
FS, LV = "57" * 16, "68" * 16
CONTENT = b"".join(bytes([n]) * 4096 for n in range(1, 9))
NAME = "/contenu"
REFERENCE = "3ca74ee"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def create(directory, binary, sector, policy):
    image = directory / "pool.raw"
    out = directory / "copie.raw"
    opts = ["p6"] if policy else []
    run(binary, ["pool-format", BLOCKS, sector, POOL, *opts], dest=image)

    def change(cmd, *args):
        reply = run(binary, [cmd, BLOCKS, sector, *args], image, out)
        out.replace(image)
        return json.loads(reply.stderr) if reply.stderr.strip() else None

    origin = change("volume-create", OWNER, FS, LV, 256, *([policy] if policy else []))["id"]
    change("file-create", origin, NAME)
    change("file-write", origin, NAME, 0, CONTENT.hex())
    snap = change("snapshot", origin)
    clone = change("clone", snap["id"], "46" * 16)
    change("file-write", clone["id"], NAME, 4095, "aa55")
    change("drop-view", origin)
    return image, snap, clone["id"]


def control(binary, sector, path, receipt, *, expected=0):
    h = digest(path)
    args = ["check", BLOCKS, sector, receipt["pool"], receipt["id"], receipt["empreinte"], "--full"]
    for cmd in [args, ["check-pool", BLOCKS, sector, "--full"]]:
        report = json.loads(run(binary, cmd, path, expected=expected).stdout)
        assert report["statut"] == ("coherent" if expected == 0 else "incoherent"), report
        assert digest(path) == h, "vérificateur en écriture"


def read_data(binary, sector, image, view):
    return run(binary, ["file-read", BLOCKS, sector, view, NAME, 0, 65536], image).stdout


def compression(binary, sector, image, view, policy):
    stats = json.loads(run(binary, ["compression-space", BLOCKS, sector, view], image).stdout)
    assert stats["taille_logique_octets"] == len(CONTENT), stats
    if policy == "lz4":
        assert stats["unites_compressees"] > 0 and stats["economie_octets"] > 0, stats
    else:
        assert stats["unites_compressees"] == stats["economie_octets"] == 0
    return stats["economie_octets"]


def altered(source, target):
    raw = bytearray(source.read_bytes())
    positions = [i for i in range(0, len(raw), 4096) if raw[i:i+8] == b"NP6LZ4PK"]
    assert positions
    # Le pack initial reste protégé par le snapshot. Invalider tous les flux
    # évite de choisir un bloc libéré d'une ancienne génération.
    for at in positions:
        raw[at+96] ^= 1
    target.write_bytes(raw)


def p6_cycle(directory, writer, reader, older, sector, policy):
    image, snap, clone = create(directory, writer, sector, policy)
    pool_hash = digest(image)
    expected = CONTENT[:4095] + bytes.fromhex("aa55") + CONTENT[4097:]
    assert read_data(reader, sector, image, clone) == expected
    assert read_data(reader, sector, image, snap["id"]) == CONTENT
    source_savings = compression(reader, sector, image, snap["id"], policy)
    for geometry in (512, 4096):
        control(reader, geometry, image, snap)

    output = directory / "indisponible"
    # Une mutation avec le binaire exact de P5.1 doit sortir en échec avant émission d'image.
    run(older, ["advance", BLOCKS, sector], image, output, expected=1)
    assert output.stat().st_size == 0 and digest(image) == pool_hash
    if policy == "lz4":
        invalid = directory / "corrompu"
        altered(image, invalid)
        control(reader, sector, invalid, snap, expected=1)
    export = directory / "archive.nvx"
    run(writer, ["export", BLOCKS, sector, snap["id"]], image, export)
    image.unlink()
    archive_hash = digest(export)
    archive = export.read_bytes()
    records = int.from_bytes(archive[144:152], "little")
    start = int.from_bytes(archive[10:12], "little")
    logical_hash = hashlib.sha256(archive[start:start + records * 4104]).hexdigest()
    destination = "none" if policy == "lz4" else "lz4"
    restored = directory / "restaure.raw"
    info = json.loads(run(older, ["check-export"], export).stdout)
    assert info["empreinte_source"] == snap["empreinte"] and info["statut"] == "coherent"
    # Le lecteur P5.1 sait aussi restaurer une archive produite avec un pack P6.
    old = json.loads(run(older, ["restore", BLOCKS, sector, TARGET, OWNER], export, restored).stderr)
    assert read_data(older, sector, restored, old["id"]) == CONTENT
    restored.unlink()
    for geometry in (512, 4096):
        restored_info = json.loads(run(reader, ["restore", BLOCKS, geometry, TARGET, OWNER, destination],
                                       export, restored).stderr)
        assert restored_info["compression"] == destination
        control(writer, geometry, restored, restored_info)
        savings = compression(writer, geometry, restored, restored_info["id"], destination)
        assert read_data(writer, geometry, restored, restored_info["id"]) == CONTENT
        assert digest(export) == archive_hash
        restored.unlink()
    return dict(politique_source=policy, politique_destination=destination, sha256_pool=pool_hash,
                sha256_export=archive_hash, sha256_enregistrements_logiques=logical_hash,
                octets_economises_source=source_savings, octets_economises_restauration=savings,
                restaurations_p6=2, restaurations_p51=1, refus_mutation_p51=1,
                controles_sur_image_corrompue=2 if policy == "lz4" else 0)


def migration(directory, old, current, sector):
    source, snap, clone = create(directory, old, sector, None)
    baseline = digest(source)
    dest = directory / "migre.raw"
    target = 512 if sector == 4096 else 4096
    rec = json.loads(run(current, ["migrate-view", BLOCKS, target, TARGET, BLOCKS, sector,
                                    clone, "lz4"], source, dest).stderr)
    assert digest(source) == baseline
    assert read_data(current, sector, source, snap["id"]) == CONTENT
    source.unlink()
    control(current, target, dest, rec)
    assert rec["type"] == "clone" and rec["compression"] == "lz4"
    assert read_data(current, target, dest, rec["id"]) == CONTENT[:4095] + b"\xaa\x55" + CONTENT[4097:]
    return dict(secteur_source=sector, secteur_destination=target, sha256_source=baseline,
                economie_octets=compression(current, target, dest, rec["id"], "lz4"))


def main():
    with tempfile.TemporaryDirectory(prefix="novafs-p6-interop-") as tmp:
        work = Path(tmp)
        old_runner = LEGACY["extraire"](REFERENCE, work)
        old = {a: BUILD(old_runner, a, work / "build", ["build", "--bin", "novafs"], "novafs") for a in ARCHS}
        current = {a: P5_EXPORT["binary"](a) for a in ARCHS}
        results = []
        migrated = []
        for arch in ARCHS:
            other, = (a for a in ARCHS if a != arch)
            for sector in (512, 4096):
                for policy in ("none", "lz4"):
                    with tempfile.TemporaryDirectory(dir=work) as directory:
                        row = p6_cycle(Path(directory), current[arch], current[other], old[other], sector, policy)
                    results.append(dict(producteur=arch, lecteur=other, secteur=sector, **row))
                with tempfile.TemporaryDirectory(dir=work) as directory:
                    row = migration(Path(directory), old[arch], current[other], sector)
                migrated.append(dict(producteur=arch, lecteur=other, **row))
        # Les enregistrements de pages logiques d'une même vue sont identiques
        # pour les deux encodages. Le reçu physique et le secteur restent dans l'en-tête.
        assert len({v["sha256_enregistrements_logiques"] for v in results}) == 1
        for policy in ("none", "lz4"):
            group = [v for v in results if v["politique_source"] == policy]
            assert len({v["sha256_pool"] for v in group}) == 1
            for sector in (512, 4096):
                assert len({v["sha256_export"] for v in group if v["secteur"] == sector}) == 1
        print(json.dumps(dict(reference=REFERENCE, echanges=results, migrations=migrated,
                              executables_p6_sha256={a: digest(b) for a, b in current.items()},
                              executables_p51_sha256={a: digest(b) for a, b in old.items()}),
                         indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
