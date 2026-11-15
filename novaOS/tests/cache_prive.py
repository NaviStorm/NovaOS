"""Qualification du cache sur P9. Une invocation possède sa cellule et ses images jetables."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import select
import shutil
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from bloc_qemu import command
from session_cli import fixture_p9

TARGETS = {"x86_64": "x86_64-unknown-none", "aarch64": "aarch64-unknown-none-softfloat"}
MARKERS = {
    "cache-test": b"CACHE-P4 fonctionnel OK",
    "cache-pression": b"CACHE-P4 pression OK",
    "cache-propriete-invalide": b"CACHE-P4 proprietaire invalide refuse",
    "cache-reprise": b"CACHE-P4 reprise OK",
    "cache-actualise": b"CACHE-P4 actualisation OK",
    "cache-restaure": b"CACHE-P4 restauration refusee",
    "cache-reinstalle": b"CACHE-P4 reinstallation refusee",
    "cache-coupure": b"CACHE-P4 purge-intention-durable",
    "cache-reprise-purge": b"CACHE-P4 reprise-purge OK",
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def vm(arch, ncpu, sector, disk, iso, profile, log, cut):
    argv = command(arch, sector, disk, controls=["-monitor", "none", "-display", "none", "-serial", "stdio"])
    env = dict(os.environ, NOVA_ARCH=arch, NOVA_VCPUS=str(ncpu), NOVA_ISO=str(iso))
    output = bytearray()
    mask = hex((1 << ncpu) - 1)
    halt = f"attendus={mask} en_ligne={mask} ack={mask} absents=0x0 muets=0x0".encode()
    end = time.monotonic() + 240
    with log.open("xb") as stream, subprocess.Popen(argv, env=env, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0) as proc:
        try:
            while time.monotonic() < end:
                if not select.select([proc.stdout], [], [], 0.5)[0]:
                    continue
                part = os.read(proc.stdout.fileno(), 65536)
                if not part:
                    break
                stream.write(part)
                output.extend(part)
                if cut and MARKERS[profile] in output:
                    proc.kill()
                    assert proc.wait(timeout=10) == -signal.SIGKILL
                    return bytes(output)
                if re.search(rb"SMP arret motif=[^\n]+\n", output):
                    if (not cut and MARKERS[profile] in output and b"BLOC session terminee ok=true" in output
                            and b"SMP arret motif=normal" in output and halt in output):
                        return bytes(output)
                    break
        finally:
            if proc.poll() is None:
                proc.kill()
            proc.wait(timeout=10)
    raise RuntimeError(f"{log} : {bytes(output[-1900:]).decode(errors='replace')}")


def memory(serial):
    phases = {}
    for phase in (b"bloc_repos", b"bloc_collecte"):
        line = next(line for line in serial.splitlines() if b"RAM phase=" + phase + b" " in line)
        phases[phase] = {k.decode(): int(v) for k, v in re.findall(rb"(\w+)=(\d+)", line)}
    start, end = phases.values()
    for field in ("processus", "ipc", "utilisateur", "upile", "kpile", "heap", "buddy", "meta_actifs"):
        assert end[field] == start[field], (field, start, end)
    assert start["libres"] - end["libres"] == sum(end[k] - start[k] for k in ("dma", "tables"))
    counters = re.search(rb"cadres_init=(\d+) cadres_fin=(\d+) allocations=(\d+) pic_cadres=(\d+) buddy_init=(\d+) buddy_fin=(\d+)", serial)
    assert counters and counters[5] == counters[6]
    broker = re.search(rb"CACHE memoire courtier pic=(\d+) final=(\d+) octets", serial)
    assert broker and int(broker[2]) == 0, "allocations du courtier après drainage"
    return dict(repos=start, collecte=end, allocations=int(counters[3]), pic_cadres=int(counters[4]),
                courtier_pic=int(broker[1]), courtier_final=int(broker[2]))


def inventory(raw):
    """Utiliser les OID, les noms et les octets relus par le moteur hôte."""
    rows = {}
    for line in raw.decode("ascii").splitlines():
        kind, oid, name, *rest = line.split()
        path = bytes.fromhex(name).decode()
        assert path not in rows
        value = None
        if kind == "F":
            value = bytes.fromhex(rest[1]) if len(rest) == 2 else b""
            assert len(value) == int(rest[0])
        else:
            assert kind == "D" and not rest
        rows[path] = (int(oid), value)
    return rows


def witness(rows, key):
    owners = [(path, oid) for path, (oid, data) in rows.items()
              if re.fullmatch(r"/a[0-9a-f]{64}", path) and data is None
              and rows.get(path + "/temoin", (None, None))[1] == bytes([key])]
    assert len(owners) == 1, (key, owners)
    return owners[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arch", choices=TARGETS, required=True)
    parser.add_argument("--vcpus", type=int, choices=(1, 2, 4), required=True)
    parser.add_argument("--secteur", type=int, choices=(512, 4096), required=True)
    parser.add_argument("--preuves", type=Path, required=True)
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--pression", action="store_true", help="Pression physique sur un volume neuf")
    parser.add_argument("--capacites", action="store_true", help="Qualification ciblée du masque de l’acteur 11")
    parser.add_argument("--alterer", type=Path, help="Outil de qualification pour altérer le marqueur sur une copie")
    args = parser.parse_args()
    if sum(map(bool, (args.capacites, args.pression, args.alterer))) > 1:
        parser.error("les qualifications ciblées exigent des volumes distincts")
    out = args.preuves.resolve()
    out.mkdir(parents=True, exist_ok=False)
    disk = out / "volume.raw"
    fixture_p9(disk, args.secteur)
    template = Path(os.environ.get("NOVA_KERNEL_ELF", ROOT/"target"/TARGETS[args.arch]/"release/novaos"))
    kernel = out / "novaos.elf"
    shutil.copyfile(template, kernel)
    env = dict(os.environ, NOVA_KERNEL_ELF=str(kernel), NOVA_ARCH=args.arch)
    qemu = subprocess.check_output([f"qemu-system-{args.arch}", "--version"], text=True).splitlines()[0]
    rows = []
    def read(image, mode):
        return subprocess.check_output([args.oracle.resolve(), image, str(args.secteur), mode])
    public = read(disk, "catalogue")

    def run(profile, image, *, cut=False):
        iso = out / (profile + ".iso")
        if not iso.exists():
            subprocess.run(["sh", ROOT/"scripts/iso.sh", profile, iso], env=env, check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)
        label = f"{len(rows):02d}-{profile}"
        serial = vm(args.arch, args.vcpus, args.secteur, image, iso, profile, out/(label+".serial"), cut)
        if profile == "cache-test":
            assert b"CAPS-P3 START legacy et masque valides doublon, hors plafond DENIED, version et reserves BAD_ARGUMENT" in serial
            assert b"CAPS-P3 acteur 11 tmp accessible cache refuse avant restauration ancien jeton revoque" in serial
            assert b"CACHE-P4 pret VFS lecture cache ecriture lecture tmp retrait OK" in serial
        raw = image.read_bytes()
        snapshot = out / (label+".raw.gz")
        with gzip.open(snapshot, "wb") as f:
            f.write(raw)
        with gzip.open(snapshot, "rb") as f:
            assert f.read() == raw
        after_public = read(image, "catalogue")
        if profile == "cache-pression":
            before = inventory(public)
            after = inventory(after_public)
            _, filler = after.pop("/charge-cache-qualification")
            assert before == after
            assert filler and len(filler) % 4096 == 0 and filler.count(0x75) == len(filler)
        else:
            assert public == after_public, "catalogue public modifié"
        catalog = read(image, "catalogue-cache")
        (out/(label+".catalogue")).write_bytes(catalog)
        if not cut:
            read(image, "propre")
        assert image.read_bytes() == raw, "écriture de l'oracle"
        rows.append(dict(profil=profile, coupure_qemu=cut, noyau=sha(kernel.read_bytes()),
                         iso=sha(iso.read_bytes()), serie=sha(serial), disque=sha(raw),
                         public_initial=sha(public), public=sha(after_public), cache=sha(catalog), qemu=qemu, architecture=args.arch,
                         vcpus=args.vcpus, secteur=args.secteur, memoire=None if cut else memory(serial)))
        (out/"resultats.json").write_text(json.dumps(rows, indent=2, ensure_ascii=False)+"\n")
        print(args.arch, args.vcpus, args.secteur, label, flush=True)
        return inventory(catalog)

    if args.pression:
        state = run("cache-pression", disk)
        assert not any(value == bytes([42]) for _, value in state.values())
        assert witness(state, 73) != witness(state, 91)
        return
    normal = run("cache-test", disk)
    owner, gen = witness(normal, 42)
    other = [witness(normal, v) for v in (73, 91)]
    assert len({owner, *(p for p, _ in other)}) == 3
    if args.capacites:
        return
    if args.alterer:
        damaged = out / "provenance-invalide.raw"
        shutil.copyfile(disk, damaged)
        subprocess.run([args.alterer.resolve(), damaged, str(args.secteur)], check=True)
        before = inventory(read(damaged, "catalogue-cache"))
        marker = f"/r{gen:016x}"
        assert before.keys() == normal.keys()
        assert [p for p in normal if normal[p] != before[p]] == [marker]
        original, changed = normal[marker][1], before[marker][1]
        assert len(original) == len(changed) == 104 and sum(a != b for a,b in zip(original, changed)) == 1
        assert run("cache-propriete-invalide", damaged) == before
    assert run("cache-reprise", disk) == normal
    assert run("cache-actualise", disk) == normal

    restored = out/"restauration.raw"
    shutil.copyfile(disk, restored)
    assert run("cache-restaure", restored) == normal
    assert run("cache-reinstalle", restored) == normal

    pending = run("cache-coupure", disk, cut=True)
    tomb = f"/d{gen:016x}"
    assert pending[tomb] == normal[f"/r{gen:016x}"] and f"/r{gen:016x}" not in pending
    assert pending[owner][0] == gen and pending[owner+"/temoin"] == normal[owner+"/temoin"]
    # Une restauration non attestée d'une image AVEC tombe doit également rester en quarantaine.
    shutil.copyfile(disk, restored)
    assert run("cache-restaure", restored) == pending
    resumed = run("cache-reprise-purge", disk)
    assert resumed[owner][0] > gen and tomb not in resumed and f"/r{gen:016x}" not in resumed
    assert owner + "/temoin" not in resumed
    for path, oid in other:
        assert resumed[path][0] == oid
        assert {k:v for k,v in resumed.items() if k.startswith(path+"/")} == {
            k:v for k,v in normal.items() if k.startswith(path+"/")}


if __name__ == "__main__":
    main()
