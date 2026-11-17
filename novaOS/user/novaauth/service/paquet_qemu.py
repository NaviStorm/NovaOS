"""Qualification du provisionneur sur un vrai P9 ayant déjà reçu cut dans un invité."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import select
import struct
import subprocess
import time

from shell_qemu import digest, prepare_vm


class Serial:
    def __init__(self, vm, trace):
        self.vm = vm
        self.trace = trace
        self.data = bytearray()
        self.offset = 0

    def expect(self, pattern, seconds=55):
        deadline = time.monotonic() + seconds
        while True:
            match = re.search(pattern, self.data[self.offset:])
            if match:
                self.offset += match.end()
                return match
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([self.vm.stdout], [], [], remaining)[0]:
                raise TimeoutError(pattern)
            block = os.read(self.vm.stdout.fileno(), 32768)
            if not block:
                raise EOFError(pattern)
            self.data.extend(block)
            self.trace.write(block)
            self.trace.flush()

    def send(self, data):
        self.vm.stdin.write(data)
        self.vm.stdin.flush()

    def submit(self, line, payload=None):
        self.send(line.encode() + b"\n")
        if payload:
            for offset in range(0, len(payload), 96):
                self.expect(b"NXPK donnees")
                self.send(payload[offset:offset+96].hex().encode())
    def command(self, line, payload=None):
        start = self.offset
        self.submit(line, payload)
        self.expect(rb"nsh> ")
        self.send(b"echo NXR_$?__FIN\n")
        result = self.expect(rb"NXR_(\d+)__FIN\r?\n")
        self.expect(rb"nsh> ")
        return int(result[1]), bytes(self.data[start:self.offset])


def payload_digest(blob):
    length, = struct.unpack_from("<I", blob)
    return hashlib.sha256(blob[4+length+64:]).hexdigest()


def run(a):
    argv, env, disk, authority = prepare_vm(a.arch, a.sector, a.iso, a.source, a.output, "")
    packet = a.package.read_bytes()
    evidence = dict(mode=a.mode, architecture=a.arch, secteur=a.sector, iso=digest(a.iso),
                    p9_initial=digest(disk), autorite_initiale=digest(authority),
                    paquet=digest(a.package), commandes=[], succes=False)
    def record(cmd, expected, payload=None):
        rc, output = serial.command(cmd, payload)
        row = dict(commande=cmd, statut=rc, attendu=expected)
        evidence["commandes"].append(row)
        if rc != expected:
            raise ValueError(f"{cmd}: statut {rc}, attendu {expected}")
        return output

    def inspect(name, checksum):
        output = record("install --inspect " + name, 0)
        match = re.search(rb'NXPK objet '+name.encode()+rb' ([0-9a-f]{32}) ([0-9a-f]{64})', output)
        if not match or match[2].decode() != checksum:
            raise ValueError("empreinte P9 différente: " + name)
        evidence[name] = dict(installation=match[1].decode(),sha256=checksum)

    begun = time.monotonic()
    serial_path = a.output / "invite.serial"
    with serial_path.open("xb") as trace, subprocess.Popen(
        argv, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, bufsize=0
    ) as vm:
        serial = Serial(vm, trace)
        try:
            serial.expect(rb"nsh> ")
            inspect("cut",payload_digest(a.cut.read_bytes()))
            if a.mode in ("avant", "apres"):
                record("install --inspect temoin",126)
                start = serial.offset
                serial.submit(f"install --hex {len(packet)}",packet)
                serial.expect(b"NXPK interruption " + a.mode.encode() + b" decision")
                match = re.search(rb"NXPK requete ([0-9a-f]{32})",serial.data[start:])
                if not match:
                    raise ValueError("identifiant absent avant la panne")
                evidence["requete"] = match[1].decode()
                evidence["interruption_observee"] = True
                evidence["succes"] = True
                # Arrêt de l'émulateur après la borne de décision, avant toute
                # réponse au client. La récupération démarre avec ces mêmes octets.
                vm.kill()
            elif a.mode == "relecture-avant":
                record("install --inspect temoin",126)
                record("install --receipt " + a.receipt,1)
            elif a.mode == "relecture":
                inspect("temoin",payload_digest(packet))
                if a.receipt:
                    record("install --receipt "+a.receipt, 0)
            else:
                record("install --inspect temoin",126)
                record("temoin",127)
                if a.mode == "signature":
                    n = struct.unpack_from("<I", packet)[0]
                    packet = bytearray(packet)
                    packet[4+n] ^= 1
                elif a.mode == "empreinte":
                    packet = bytearray(packet)
                    packet[-1] ^= 1
                expected = {"installation":0,"signature":5,"empreinte":5,"architecture":3}[a.mode]
                output = record("install --hex "+str(len(packet)), expected, packet)
                match = re.search(rb'NXPK requete ([0-9a-f]{32})', output)
                if not match:
                    raise ValueError("identifiant de transaction absent")
                evidence["requete"] = match[1].decode()
                receipt = record("install --receipt",0 if expected==0 else 1)
                gen = re.search(rb"NXPK generation (\d+)",receipt)
                if not gen or (int(gen[1]) > 0) != (expected==0):
                    raise ValueError("décision de publication incohérente")
                evidence["generation_acquittee"] = int(gen[1])
                if expected==0:
                    inspect("temoin",payload_digest(packet))
                else:
                    record("install --inspect temoin",126)
                inspect("cut",payload_digest(a.cut.read_bytes()))
            if a.mode not in ("avant","apres"):
                serial.send(b"exit\n")
                serial.expect(rb"absents=0x0 muets=0x0")
                routes = re.findall(rb'NXLOGIN route shell=(\d+) verificateur=(\d+) signataire=(\d+)',serial.data)
                if len(routes)!=1 or len(set(routes[0]))!=3 or b"BLOC session terminee ok=true" not in serial.data:
                    raise ValueError("changement de route ou arrêt incomplet")
                evidence["pid"] = [int(pid) for pid in routes[0]]
            if digest(a.iso)!=evidence["iso"]:
                raise ValueError("image système changée")
            evidence["succes"] = True
        except (ValueError, TimeoutError, EOFError, BrokenPipeError) as exc:
            evidence["erreur"] = str(exc)
        finally:
            if vm.poll() is None:
                vm.terminate()
            vm.wait(10)
            trace.write(vm.stdout.read())
    evidence.update(trace=digest(serial_path), p9_final=digest(disk),
                    autorite_finale=digest(authority), duree_hote_s=round(time.monotonic()-begun,3))
    (a.output/"resultat.json").write_text(json.dumps(evidence,ensure_ascii=False,indent=2)+"\n")
    print(a.arch,a.sector,a.mode,evidence["succes"],evidence.get("erreur",""),flush=True)
    return evidence["succes"]


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=("x86_64","aarch64"))
    p.add_argument("sector",type=int,choices=(512,4096))
    for name in ("iso","source","output","cut","package"):
        p.add_argument(name,type=Path)
    p.add_argument("mode",choices=("installation","relecture","signature","empreinte","architecture","avant","apres","relecture-avant"))
    p.add_argument("--receipt")
    raise SystemExit(0 if run(p.parse_args()) else 1)
