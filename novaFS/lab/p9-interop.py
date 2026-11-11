#!/usr/bin/env python3
"""Migration publiée et relue entre les deux ABI Darwin, sur fichiers jetables."""
import contextlib
import fcntl
import hashlib
import json
import os
import runpy
from pathlib import Path
import secrets
import subprocess
import sys
import tempfile
import threading

ROOT = Path(__file__).resolve().parents[1]
BLOCKS = 3000


@contextlib.contextmanager
def feed(payload):
    read, write = os.pipe()
    high = fcntl.fcntl(read, fcntl.F_DUPFD, 10)
    os.close(read)

    def send():
        with os.fdopen(write, "wb") as stream:
            try:
                stream.write(payload)
            except BrokenPipeError:
                pass

    thread = threading.Thread(target=send)
    thread.start()
    try:
        yield high
    finally:
        os.close(high)
        thread.join()


def run(command, auth, aux, source, output, success=True):
    with feed(auth) as fd3, feed(aux) as fd4, source.open("rb") as src, output.open("wb") as out:
        child = ["/bin/sh", "-c", f'exec 3<&{fd3} 4<&{fd4}; exec "$@"', "novafs", *command]
        result = subprocess.run(child, pass_fds=(fd3, fd4), stdin=src, stdout=out,
                                stderr=subprocess.PIPE)
    if success != (result.returncode == 0):
        raise AssertionError((Path(command[0]).name, command[1], result.returncode, result.stderr.decode()))
    return result.stderr


def frame(identity, admin, keys=()):
    return b"NP7KEYS1" + identity + admin + bytes([len(keys)]) + b"".join(
        d + b"\x02\x20\x00" + recovery for d, recovery in keys)


def credentials(phrase, recovery):
    return len(phrase).to_bytes(2, "little") + phrase + recovery


def scenario(directory, writer, reader, p8_writer, p8_reader, sector):
    pool, owner, fs, lv, fs2, lv2 = [secrets.token_bytes(16) for _ in range(6)]
    admin, fresh, old, new, backup = [secrets.token_bytes(32) for _ in range(5)]
    phrase = secrets.token_bytes(29)
    empty, a, b, migrated, out = [directory / s for s in ("vide", "a", "b", "p9", "sortie")]
    empty.touch()
    base = frame(pool, admin)
    data = (bytes(range(211)) * 291)[:59017]

    def action(binary, name, tail, src, dst, keys, aux=b""):
        return run([binary, name, str(BLOCKS), str(sector), *tail, "--cles-fd3"], keys, aux, src, dst)

    action(p8_writer, "pool-format", [pool.hex(), "p7"], empty, a, base)
    first = json.loads(action(p8_writer, "volume-create", [owner.hex(), fs.hex(), lv.hex(), "256", "lz4", "chiffre"],
                              a, b, base, credentials(phrase, old)))
    d = bytes.fromhex(first["domaine"])
    second = json.loads(action(p8_writer, "volume-create", [owner.hex(), fs2.hex(), lv2.hex(), "256", "none", "chiffre"],
                               b, a, frame(pool, admin, [(d, old)]), credentials(phrase, old)))
    domains = [d, bytes.fromhex(second["domaine"])]
    keyed = frame(pool, admin, [(domain, old) for domain in domains])
    action(p8_writer, "file-create", ["1", "/preuve"], a, b, keyed)
    action(p8_writer, "file-write", ["1", "/preuve", "0", "--donnees-fd4"], b, a, keyed, data)
    snap = json.loads(action(p8_writer, "snapshot", ["1"], a, b, keyed))["objet"]
    clone = json.loads(action(p8_writer, "clone", [str(snap), owner.hex()], b, a, keyed))["objet"]
    digest = hashlib.sha256(a.read_bytes()).digest()
    enroll = b"NP9ENRL1" + fresh + bytes([len(domains)]) + b"".join(
        domain + credentials(phrase, new) for domain in domains)
    cmd = [sys.executable, ROOT / "lab/migrate-p9.py", a, migrated, "--secteur", str(sector), "--profil", "p7", "--outil", writer]
    run([*cmd, "--blocs-destination", "128"], keyed, enroll, empty, out, success=False)
    assert not migrated.exists() and hashlib.sha256(a.read_bytes()).digest() == digest
    run(cmd, keyed, enroll, empty, out)
    receipt = json.loads(out.read_bytes())
    assert receipt["sha256"] == hashlib.sha256(migrated.read_bytes()).hexdigest()
    assert digest == hashlib.sha256(a.read_bytes()).digest()
    run(cmd, keyed, enroll, empty, out, success=False)
    alias = directory / "alias"
    os.link(a, alias)
    run([*cmd[:3], alias, *cmd[4:]], keyed, enroll, empty, out, success=False)
    assert digest == hashlib.sha256(a.read_bytes()).digest()
    pool9 = bytes.fromhex(receipt["pool"])
    action(reader, "migration-receipt", [], migrated, out, frame(pool9, fresh))
    signed = json.loads(out.read_bytes())
    assert len(signed["lignees"]) == 2 and signed["vues"][-1]["parent"] == "parent historique inconnu"
    new_keys = [(bytes.fromhex(item["destination"]), new) for item in signed["lignees"]]
    opened = frame(pool9, fresh, new_keys)
    run([p8_reader, "check-pool", str(BLOCKS), str(sector), "--cles-fd3"],
        opened, b"", migrated, out, success=False)
    legacy_write = [p8_reader, "file-write", str(BLOCKS), str(sector), str(clone),
                    "/preuve", "0", "--donnees-fd4", "--cles-fd3"]
    run(legacy_write, opened, b"x", migrated, out, success=False)
    assert out.stat().st_size == 0
    assert receipt["sha256"] == hashlib.sha256(migrated.read_bytes()).hexdigest()
    action(reader, "check-pool", [], migrated, out, opened)
    assert json.loads(out.read_bytes())["format"] == "P9"
    action(reader, "file-times", [str(snap), "/preuve"], migrated, out, opened)
    assert all(t is None for t in json.loads(out.read_bytes()).values())
    action(reader, "file-read", [str(clone), "/preuve", "0", str(len(data))], migrated, out, opened)
    assert out.read_bytes() == data
    action(reader, "file-write", [str(clone), "/preuve", "4095", "--donnees-fd4"], migrated, b, opened, b"ABC")
    action(writer, "file-read", [str(clone), "/preuve", "0", str(len(data))], b, out, opened)
    assert out.read_bytes() == data[:4095] + b"ABC" + data[4098:]
    action(writer, "file-read", [str(snap), "/preuve", "0", str(len(data))], b, out, opened)
    assert out.read_bytes() == data
    action(writer, "file-times", [str(clone), "/preuve"], b, out, opened)
    times = json.loads(out.read_bytes())
    assert times["btime"] is None and times["mtime"] and times["ctime"]
    archives = [directory / name for name in ("snapshot", "clone")]
    for view, path in zip((snap, clone), archives):
        action(reader, "export", [str(view)], b, path, opened, backup)
        assert path.read_bytes()[:8] == b"NV9EXPRT"
        run([writer, "check-export", "--cle-fd4"], b"", backup, path, out)
        assert json.loads(out.read_bytes())["format"] == "P9"
    ra, rb = directory / "restaure-a", directory / "restaure-b"
    restored_id = secrets.token_bytes(16)
    auth = frame(restored_id, fresh)
    action(writer, "pool-format", [restored_id.hex(), "p9"], empty, ra, auth)
    restored_ids = []
    for path, source, target in zip(archives, (ra, rb), (rb, ra)):
        stream = backup + credentials(phrase, new) + path.read_bytes()
        info = json.loads(action(writer, "restore-into", [owner.hex(), "lz4", "--archive-fd4"],
                                 source, target, auth, stream))
        restored_ids.append(info["objet"])
        auth = frame(restored_id, fresh, [(bytes.fromhex(info["domaine"]), new)])
    assert restored_ids[0] != restored_ids[1]
    for restored, expected in zip(restored_ids, (data, data[:4095] + b"ABC" + data[4098:])):
        action(reader, "file-read", [str(restored), "/preuve", "0", str(len(data))], ra, out, auth)
        assert out.read_bytes() == expected
    assert digest == hashlib.sha256(a.read_bytes()).digest() == hashlib.sha256(alias.read_bytes()).digest()
    return {
        "producteur": writer.parents[1].name, "lecteur": reader.parents[1].name, "secteur": sector,
        "lignees": len(domains), "octets_compares": len(data), "restaurations": len(restored_ids),
        "images_source_et_alias_verifiees": True,
    }


if __name__ == "__main__":
    binaries = []
    for isa in ("aarch64", "x86_64"):
        target = isa + "-apple-darwin"
        subprocess.run([ROOT / "lab/run", "build", "--offline", "--bin", "novafs"], check=True,
                       env=dict(os.environ, NOVAFS_TARGET=target))
        binaries.append(ROOT / "target" / target / "release/novafs")
    results = []
    with tempfile.TemporaryDirectory(prefix="novafs-p9-lecteur-historique-") as tmp:
        work = Path(tmp)
        source = runpy.run_path(str(ROOT / "lab/p51-validation.py"))["extraire"](
            "f2742e1f7621eb4b732e81bd472a7156b1f48ecf", work)
        build = runpy.run_path(str(ROOT / "lab/test-pool-interop.py"))["build"]
        old = {arch: build(source, arch, work / "cibles", ["build", "--bin", "novafs"], "novafs")
               for arch in ("aarch64", "x86_64")}
        for writer, reader in (binaries, binaries[::-1]):
            historical_writer = old[writer.parents[1].name.split("-")[0]]
            historical_reader = old[reader.parents[1].name.split("-")[0]]
            for sector in (512, 4096):
                with tempfile.TemporaryDirectory(prefix="novafs-p9-echange-") as case:
                    results.append(scenario(Path(case), writer, reader, historical_writer,
                                            historical_reader, sector))
    print(json.dumps(results, ensure_ascii=False, indent=2))
