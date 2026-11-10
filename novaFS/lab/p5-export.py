#!/usr/bin/env python3
"""Vérifie les sauvegardes complètes P5 dans les deux sens entre binaires Darwin."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

LAB = Path(__file__).resolve().parent
CPUS = {"aarch64": 0x0100000C, "x86_64": 0x01000007}
NB = 2048
POOL = "13" * 16
OTHER = "24" * 16
OWNER = "35" * 16
CLONE_OWNER = "46" * 16


def binary(arch):
    result = subprocess.run([LAB / "run", "build", "--offline", "--bin", "novafs",
                             "--message-format=json"], env=dict(os.environ, NOVAFS_TARGET=f"{arch}-apple-darwin"),
                            capture_output=True, text=True, check=True)
    [exe] = [Path(m["executable"]) for line in result.stdout.splitlines()
             if (m := json.loads(line)).get("executable") and m["target"]["name"] == "novafs"]
    with exe.open("rb") as file:
        assert file.read(8) == bytes.fromhex("cffaedfe") + CPUS[arch].to_bytes(4, "little")
    return exe


def run(binary, args, source=None, dest=None, expected=0):
    with (source.open("rb") if source else open(os.devnull, "rb")) as stdin:
        if dest:
            with dest.open("xb") as output:
                proc = subprocess.run([binary, *map(str, args)], stdin=stdin, stdout=output, stderr=subprocess.PIPE)
        else:
            proc = subprocess.run([binary, *map(str, args)], stdin=stdin, capture_output=True)
    assert proc.returncode == expected, (args, proc.stderr, proc.stdout)
    return proc


def checks(cli, sector, source, receipt):
    assert json.loads(run(cli, ["check-pool", NB, sector], source).stdout)["statut"] == "coherent"
    assert json.loads(run(cli, ["check", NB, sector, receipt["pool"], receipt["id"], receipt["empreinte"]],
                          source).stdout)["statut"] == "coherent"
    assert json.loads(run(cli, ["check", NB, sector, receipt["pool"], receipt["id"], "ff" * 32],
                          source, expected=1).stdout)["statut"] == "incoherent"
    totals = json.loads(run(cli, ["space-pool", NB, sector], source).stdout)
    view = json.loads(run(cli, ["space", NB, sector, receipt["pool"], receipt["id"]], source).stdout)
    assert totals["occupes_blocs"] == view["occupes_blocs"] and view["logique_blocs"] == 256
    assert 0 < view["exclusifs_blocs"] + view["partages_blocs"] <= totals["occupes_blocs"]


def cycle(directory, writer, reader, sector):
    source = directory / "pool.raw"
    output = directory / "suivant.raw"
    archive = directory / "sauvegarde"
    restored = directory / "restaure.raw"
    data = bytes((i * 29 + 7) % 251 for i in range(4133))
    patch = b"NovaFS-P5"
    expected = data[:4094] + patch + data[4094+len(patch):]
    run(writer, ["pool-format", NB, sector, POOL], dest=source)

    def change(cmd, *args):
        result = run(writer, [cmd, NB, sector, *args], source, output)
        output.replace(source)
        return json.loads(result.stderr) if result.stderr.strip() else {}

    o = change("volume-create", OWNER, "57" * 16, "68" * 16, 256)["id"]
    change("file-create", o, "/profil")
    change("file-write", o, "/profil", 0, data.hex())
    s = change("snapshot", o)["id"]
    c = change("clone", s, CLONE_OWNER)["id"]
    receipt = change("file-write", c, "/profil", 4094, patch.hex())
    change("drop-view", s)
    change("drop-view", o)
    checks(reader, sector, source, receipt)
    assert json.loads(run(writer, ["list-views", NB, sector], source).stdout)["vues"] == [receipt]
    run(writer, ["export", NB, sector, c], source, archive)
    source.unlink()
    info = json.loads(run(reader, ["check-export"], archive).stdout)
    assert json.loads(run(reader, ["check","--export"], archive).stdout) == info
    assert info["statut"] == "coherent" and info["empreinte_source"] == receipt["empreinte"]
    assert info["secteur_source"] == sector
    for dst in (512, 4096):
        restored_receipt = json.loads(run(reader, ["restore", NB, dst, OTHER, OWNER],
                                        archive, restored).stderr)
        assert restored_receipt["pool"] == OTHER
        checks(writer, dst, restored, restored_receipt)
        recovered = run(reader, ["file-read", NB, dst, restored_receipt["id"], "/profil", 0, len(data)],
                        restored).stdout
        assert recovered == expected
        restored.unlink()
    size = archive.stat().st_size
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    truncated = directory / "interrompu"
    truncated.write_bytes(archive.read_bytes()[:-1])
    run(reader, ["check-export"], truncated, expected=1)
    run(reader, ["restore", NB, sector, OTHER, OWNER], truncated, restored, expected=1)
    assert restored.stat().st_size == 0
    return {"octets_export": size, "sha256": digest, "restaurations": 2, "octets_fichier": len(expected)}


def main():
    binaries = {a: binary(a) for a in CPUS}
    records = []
    for arch, writer in binaries.items():
        other, reader = next((a, e) for a, e in binaries.items() if a != arch)
        for sector in (512, 4096):
            with tempfile.TemporaryDirectory() as temp:
                result = cycle(Path(temp), writer, reader, sector)
            records.append(dict(producteur=arch, lecteur=other, secteur=sector, **result))
    for sector in (512, 4096):
        assert len({r["sha256"] for r in records if r["secteur"] == sector}) == 1
    print(json.dumps(records, indent=2))


if __name__ == "__main__":
    main()
