"""Premier pipe ARC-027, depuis nsh sur une image P9 jetable conservée."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import shutil
import tempfile

from cli import HOST, Terminal
from journalisation import arret

FILES = {"court": b"A\nB\nC", "vide": b"", "binaire": bytes(range(256)) * 64 + b"\xff\nfin"}


def host(image, *args, output=subprocess.PIPE):
    with image.open("rb") as stream:
        return subprocess.run([HOST, *args], stdin=stream, stdout=output,
                              stderr=subprocess.PIPE, check=True).stdout


def fixture(out, sector, files=FILES):
    source, image = out / "genese.raw", out / "session.raw"
    script = []
    for name, data in files.items():
        script.append("create /" + name)
        for off in range(0, len(data), 512):
            script.append(f"write /{name} {off} {data[off:off+512].hex()}")
    with source.open("xb") as dest:
        result = subprocess.run([HOST, "script", "512", str(sector)], input=("\n".join(script)+"\n").encode(),
                                stdout=dest, stderr=subprocess.PIPE, check=True)
    fs, lv = re.search(rb"FS=(\w+) LV=(\w+)", result.stderr).groups()
    with image.open("xb") as dest:
        host(source, "migrate-content-p9", "512", str(sector), fs, lv, output=dest)
    return image, fs, lv


def fingerprint(data):
    h = 0xcbf29ce484222325
    for b in data:
        h = ((h ^ b) * 0x100000001b3) & ((1 << 64) - 1)
    return f"{h:016x}"


def qualify(arch, out, sector, cpus):
    out.mkdir(parents=True, exist_ok=False)
    os.environ["NOVA_VCPUS"] = str(cpus)
    os.environ["NOVA_RAM"] = "256"
    image, fs, lv = fixture(out, sector)
    results = []
    def command(vm, source, rc, *, gated=False):
        vm.vm.stdin.write(f"{source}; echo PIPE_RC_$?__fin\n".encode())
        prefix = b""
        if gated:
            prefix = vm.until(b"PIPE attente producteur")
            vm.vm.stdin.write(b"+")
        serial = (prefix + vm.until(f"PIPE_RC_{rc}__fin".encode())).decode(errors="replace")
        vm.until(b"nsh> ")
        results.append(dict(source=source, rc=rc, serial=serial))
        (out/"resultats.json").write_text(json.dumps(results, indent=2, ensure_ascii=False))
        return serial

    def drained_resources(log):
        row = re.search(r"^RAM phase=bloc_collecte .*", log.read_text(), re.M)
        assert row, log
        return {key: int(value) for key, value in re.findall(r"\b(ipc|utilisateur|processus|dma)=(\d+)\b", row.group())}

    control = out / "temoin-sans-pipe.serial"
    with Terminal(arch, sector, image, "pipe", test=True, journal=control) as vm:
        vm.until(b"nsh> ")
        assert "2" in command(vm, "wc -l court", 0).split()
        arret(vm, cpus)
    baseline = drained_resources(control)
    assert set(baseline) == {"ipc", "utilisateur", "processus", "dma"}
    assert all(baseline[field] == 0 for field in ("ipc", "utilisateur", "processus"))

    for probes in (False, True):
        log = out/("qualification.serial" if probes else "ordinaire.serial")
        with Terminal(arch, sector, image, "pipe", test=probes, journal=log) as vm:
            vm.until(b"nsh> ")
            for name, data in FILES.items():
                serial = command(vm, f"cat {name} | wc -l", 0)
                assert str(data.count(b"\n")) in serial.splitlines(), serial
                if probes:
                    roles = re.findall(r"PIPE role=(cat|wc) pid=(\d+) isole=true", serial)
                    assert [r for r,_ in roles]==["cat","wc"] and roles[0][1]!=roles[1][1], serial
                    assert sorted(re.findall(r"CLI collecte pid=(\d+)", serial)) == sorted(pid for _, pid in roles)
                    assert "[pipe cat=0]" in serial and "[pipe wc=0]" in serial
                    assert "PIPE retrait noeuds=0 attentes=0 file=0" in serial
                    assert f"lus={len(data)} fnv={fingerprint(data)}" in serial
                    assert re.search(r"pic=[0-8] blocages=", serial)
            missing = command(vm, "cat fichier-absent | wc -l", 0)
            assert "0" in missing.splitlines() and "introuvable" in missing
            assert "2" in command(vm, "nsh -c 'cat court|wc -l'", 0).splitlines()
            help_text = command(vm, "cat --help | wc -l", 0)
            assert "CLI spawn" not in help_text and "PIPE role=" not in help_text
            assert "PIPE role=" not in command(vm, "cat court | wc -c", 3)
            assert "a|b" in command(vm, "echo 'a|b'", 0).splitlines()
            assert "SAUTE" in command(vm, "false && cat court | wc -l || echo SAUTE", 0).splitlines()
            command(vm, "nsh -c 'touch interdit; echo a | cat'", 3)
            command(vm, "test -e interdit", 1)
            command(vm, "false", 1)
            command(vm, "false --help", 0)
            command(vm, "exit --help", 0)
            assert "SESSION" in command(vm, "echo SESSION", 0).splitlines()
            if probes:
                assert "[pipe cat=20]" in missing and "[pipe wc=0]" in missing
                slow = command(vm, "@probe pipe-pression binaire", 0, gated=True)
                assert re.search(r"pic=8 blocages=[1-9]", slow)
                assert f"lus={len(FILES['binaire'])} fnv={fingerprint(FILES['binaire'])}" in slow
                early = command(vm, "@probe pipe-fermeture binaire", 3, gated=True)
                assert "[pipe wc=3]" in early and "[pipe cat=11]" in early
                assert "PIPE retrait noeuds=0 attentes=0 file=0" in early
                admission = command(vm, "@probe pipe-admission binaire", 5, gated=True)
                assert re.search(r"\[pipe cat=(5|10)\]", admission) and "PIPE role=wc" not in admission
                assert "PIPE retrait noeuds=0 attentes=0 file=0" in admission
                # Une révocation retire un droit déjà remis. Le premier appel
                # après retrait peut constater DENIED, une attente retirée REVOKED.
                vm.vm.stdin.write(b"@probe pipe-revocation binaire; echo REVOQUE_$?__fin\n")
                prefix = vm.until(b"PIPE attente producteur")
                vm.vm.stdin.write(b"+")
                serial = (prefix + vm.until(b"nsh> ")).decode(errors="replace")
                assert re.search(r"REVOQUE_(5|10)__fin", serial), serial
                assert re.search(r"\[pipe cat=(5|10)\]", serial) and re.search(r"\[pipe wc=(5|10)\]", serial)
                assert "PIPE retrait noeuds=0 attentes=0 file=0" in serial
                results.append(dict(source="revocation",serial=serial))
                command(vm, "cat court | wc -l", 0)
            arret(vm, cpus)
        if probes:
            after = drained_resources(log)
            assert after == baseline, (after, baseline)
            results.append(dict(ressources_sans_pipe=baseline, ressources_apres_pipes=after))
    records = host(image,"inspect","512",str(sector))
    current = max(re.findall(r"slot=(\d+) génération=(\d+)".encode(),records), key=lambda x:int(x[1]))
    host(image,"verify","512",str(sector),fs,lv)
    for name,data in FILES.items():
        restored=host(image,"salvage","512",str(sector),fs,lv,current[0],"/"+name)
        assert restored==data
        results.append(dict(fichier=name, sha256=hashlib.sha256(restored).hexdigest()))
    (out/"resultats.json").write_text(json.dumps(results, indent=2, ensure_ascii=False))
    print(f"{arch} P9/{sector} {cpus} vCPU : {len(results)} observations")


def compare_gnu(out):
    records = json.loads((out / "resultats.json").read_text())
    cat, wc = [shutil.which("g" + name) or name for name in ("cat", "wc")]
    versions = [subprocess.check_output([tool, "--version"], text=True).splitlines()[0] for tool in (cat, wc)]
    assert all("GNU coreutils" in version for version in versions), versions
    report = {}
    for name, data in FILES.items():
        with tempfile.TemporaryFile(dir=os.environ["NOVAOS_TMPDIR"]) as source:
            source.write(data)
            source.seek(0)
            with subprocess.Popen([cat], stdin=source, stdout=subprocess.PIPE) as producer:
                consumer = subprocess.run([wc, "-l"], stdin=producer.stdout, capture_output=True)
                producer.stdout.close()
                codes = [producer.wait(), consumer.returncode]
        count = consumer.stdout.strip().decode("ascii")
        guest = [row for row in records if row.get("source") == f"cat {name} | wc -l"]
        assert codes == [0, 0] and len(guest) >= 2
        assert all(row["rc"] == codes[1] and count in row["serial"].splitlines() for row in guest)
        assert any(all(f"[pipe {cmd}={rc}]" in row["serial"] for cmd, rc in zip(("cat", "wc"), codes)) for row in guest)
        assert any(row.get("fichier") == name and row["sha256"] == hashlib.sha256(data).hexdigest() for row in records)
        report[name] = dict(octets=len(data), lignes=int(count), statuts=codes)
    print(json.dumps(dict(versions=versions, fichiers=report), ensure_ascii=False, indent=2))


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch",choices=("aarch64","x86_64"))
    parser.add_argument("out",type=Path)
    parser.add_argument("--sector",type=int,choices=(512,4096),default=512)
    parser.add_argument("--cpus",type=int,default=2)
    parser.add_argument("--oracle-gnu", action="store_true", help="comparer les résultats archivés aux deux utilitaires GNU")
    args=parser.parse_args()
    if args.oracle_gnu:
        compare_gnu(args.out.resolve())
    else:
        qualify(args.arch,args.out.resolve(),args.sector,args.cpus)
