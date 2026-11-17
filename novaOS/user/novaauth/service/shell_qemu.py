"""Exerce la session du vrai nsh sur une copie P9 jetable et une autorité séparée."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import select
import shutil
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "scripts"))
from bloc_qemu import command


def digest(path):
    with path.open("rb") as src:
        return hashlib.file_digest(src, "sha256").hexdigest()


def prepare_vm(arch, sector, iso, genesis, output, prefix):
    output.mkdir(parents=True, exist_ok=False)
    disks = []
    for filename in ("identites.p9", "autorite.raw"):
        destination = output / filename
        shutil.copyfile(genesis / (prefix + filename), destination)
        disks.append(destination)
    disk, authority = disks
    if (disk.stat().st_size, authority.stat().st_size) != (512 * 4096, 12 * 4096):
        raise ValueError("géométrie de qualification inconnue")
    desc = output / "disque.vmdk"
    desc.write_text(
        '# Disk DescriptorFile\nversion=1\nCID=abcd0033\nparentCID=ffffffff\n'
        'createType="twoGbMaxExtentFlat"\n' +
        ''.join(f'RW {f.stat().st_size // 512} FLAT "{f.name}" 0\n' for f in disks))
    argv = command(arch, sector, desc, controls=["-display","none","-monitor","none","-serial","stdio"])
    i = argv.index("-drive") + 1
    argv[i] = argv[i].replace("format=raw","format=vmdk")
    environment = {**os.environ, "NOVA_ISO":str(iso.resolve()), "NOVA_ARCH":arch,
                   "NOVA_ARM_CPU":"max", "NOVA_VCPUS":"2"}
    return argv, environment, disk, authority


def run(arch, sector, iso, genesis, output, package=None, denial=False):
    argv, environment, disk, authority = prepare_vm(arch, sector, iso, genesis, output, "neuf-")
    results = dict(architecture=arch, secteur=sector, iso_sha256=digest(iso),
                   autorite_initiale=digest(authority), p9_initial=digest(disk))
    raw = bytearray()
    sent = 0
    stream = package.read_bytes() if package else b""
    cursor = 0
    consumed = 0
    denied = False
    published = False
    start = time.monotonic()
    with (output/"invite.serial").open("xb") as trace, subprocess.Popen(
        argv,env=environment,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0
    ) as vm:
        try:
            while time.monotonic() - start < (240 if package else 90) and vm.poll() is None:
                if not select.select([vm.stdout], [], [], 1)[0]:
                    continue
                chunk = os.read(vm.stdout.fileno(), 32768)
                if not chunk:
                    break
                trace.write(chunk)
                trace.flush()
                raw.extend(chunk)
                if not sent and b"NXLOGIN route shell=" in raw and b"nsh>" in raw:
                    vm.stdin.write(b"true && echo NX_LIVE_$?__FIN\n")
                    vm.stdin.flush()
                    sent = 1
                if sent == 1 and denial and b"NX_LIVE_0__FIN" not in raw and b"saisie refus" in raw:
                    denied = True
                    break
                if sent == 1 and re.search(rb'NX_LIVE_0__FIN\r?\n',raw):
                    vm.stdin.write(f"install --hex {len(stream)}; echo NX_INSTALL_$?__FIN\n".encode() if package else b"exit\n")
                    vm.stdin.flush()
                    sent = 2
                if sent == 2 and package:
                    # Attendre le contrôle de flux de l'invité, sans écrire sur le disque hôte monté.
                    pos = raw.find(b"NXPK donnees", cursor)
                    if pos >= 0:
                        cursor = pos + len(b"NXPK donnees")
                        part = stream[consumed:consumed+96]
                        if not part:
                            raise ValueError("demande de données au-delà du paquet")
                        vm.stdin.write(part.hex().encode("ascii"))
                        vm.stdin.flush()
                        consumed += len(part)
                    result = re.search(rb'NX_INSTALL_(\d+)__FIN\r?\n', raw)
                    if result:
                        published = result[1] == b"0" and consumed == len(stream)
                        vm.stdin.write(b"exit\n")
                        vm.stdin.flush()
                        sent = 3
                if b"absents=0x0 muets=0x0" in raw or b"Panique du noyau" in raw:
                    break
        finally:
            if vm.poll() is None:
                vm.terminate()
            vm.wait(10)
            tail = vm.stdout.read()
            trace.write(tail)
            raw.extend(tail)
    roles = re.search(rb'NXLOGIN route shell=(\d+) verificateur=(\d+) signataire=(\d+)', raw)
    results.update(
        pid=[int(x) for x in roles.groups()] if roles else [],
        commande_authentifiee=(sent >= 2), paquet_publie=published, authentification_refusee=denied,
        arret_ordonne=(b"BLOC session terminee ok=true" in raw and b"absents=0x0 muets=0x0" in raw),
        duree_hote_s=round(time.monotonic()-start,3), trace_sha256=digest(output/"invite.serial"))
    results["succes"] = (len(set(results["pid"])) == 3 and results["commande_authentifiee"] and results["arret_ordonne"])
    if package:
        results.update(paquet_sha256=hashlib.sha256(stream).hexdigest(), octets_transmis=consumed)
        results["succes"] = results["succes"] and published
    if denial:
        results["succes"] = bool(roles and denied and not results["commande_authentifiee"])
    (output/"resultat.json").write_text(json.dumps(results,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps(results, ensure_ascii=False),flush=True)
    return results["succes"]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=("aarch64","x86_64"))
    parser.add_argument("secteur", type=int, choices=(512,4096))
    for arg in ("iso","genese","sortie"):
        parser.add_argument(arg,type=Path)
    parser.add_argument("--paquet",type=Path)
    parser.add_argument("--refus-auth",action="store_true")
    args = parser.parse_args()
    sys.exit(0 if run(args.arch,args.secteur,args.iso,args.genese,args.sortie,args.paquet,args.refus_auth) else 1)
