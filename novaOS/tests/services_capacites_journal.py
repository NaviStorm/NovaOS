"""Première restriction du journal sur un catalogue historique personnalisé à quatre entrées."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import zlib

from cli import Terminal
from journalisation import archive, arret, demarrage, rows, sortie
from services_capacites import changer, droits, etat, memoire, sha, verifie
from services_configuration import commande
from session_cli import fixture_p9


def declarations(payload):
    assert payload[:8] == b"NVSVCC01"
    count, = struct.unpack_from("<Q", payload, 8)
    assert count in (4, 5)
    at = 16
    result = {}
    for expected in (1, 2, 9, 10, 16)[:count]:
        start = at
        version, ident, _, mask = struct.unpack_from("<4Q", payload, at)
        assert version == 1 and ident == expected
        at += 32
        for _ in range(3):
            size, = struct.unpack_from("<Q", payload, at)
            at += (size + 1) * 8
        policy = struct.unpack_from("<9Q", payload, at)
        at += 72
        if ident in (9, 10):
            assert policy[0] == 1 and policy[1] == 1 and policy[5] == 1, (ident, policy)
        result[ident] = (mask, payload[start:at])
    assert at == len(payload)
    return result


def read_slots(tool, image, sector, output):
    data = subprocess.check_output([tool, "inspect", image, str(sector)])
    output.write_bytes(data)
    at, slots = 0, {}
    for index in (0, 1):
        oid, length = struct.unpack_from("<QQ", data, at)
        assert oid and length == 4160
        at += 16
        body = data[at:at + length]
        at += length
        assert body[:8] == b"NSVCCFG1" and not any(body[56:64])
        generation, size, crc = struct.unpack_from("<QII", body, 40)
        assert generation % 2 == index and 0 < size <= 4096
        assert crc == zlib.crc32(body[:52]) ^ zlib.crc32(body[64:])
        assert not any(body[64 + size:])
        slots[generation] = declarations(body[64:64 + size])
    assert data[at + 8:] == b"NovaOS\n"
    return slots


def identities(vm):
    result = {}
    for service, name, answer in [(9, "storage-info", b"NVFS"), (10, "cache", b"CACH")]:
        text = commande(vm, f"@services status {service}")
        match = re.search(r"essai=(\d+) pid=(\d+) instance=([0-9a-f]{16}(?:/[0-9a-f]{16}){3})", text)
        assert match and int(match[1]) and int(match[2]), text
        assert match[3].split("/")[2:] != ["0" * 16] * 2
        value = int.from_bytes(answer, "little")
        assert f"service={name} id={service} valeur={value}" in sortie(vm, f"service info {service}")
        assert f"service={name} id={service} restart=failure retries=1" in sortie(vm, f"service policy {service}")
        result[service] = dict(essai=int(match[1]), pid=int(match[2]), instance=match[3], info=value)
    return result


def trial(arch, vcpus, sector, out, tool):
    out.mkdir(parents=True, exist_ok=False)
    os.environ["NOVA_VCPUS"] = str(vcpus)
    original, image = out / "base.raw", out / "journal.raw"
    fixture_p9(original, sector)
    origin = sha(original)
    subprocess.run([tool, "legacy-custom", original, image, str(sector), "2"], check=True)
    initial = read_slots(tool, image, sector, out / "slots-avant.bin")
    assert list(sorted(initial)) == [1, 2]
    assert initial[1] == initial[2] and list(initial[2]) == [1, 2, 9, 10]
    before = initial[2]
    serial = out / "cycle.serial"
    with Terminal(arch, sector, image, "caps-journal-legacy", profil_journal=True, journal=serial) as vm:
        vm.until(b"nsh> ")
        boot = demarrage(vm)
        for service in (9, 10, 16):
            etat(vm, service, "RUNNING")
        services = identities(vm)
        first = droits(vm, 16)
        verifie(first, "journal-store", "journal-store", "journal-store", "no")
        assert first["Stored generation"] == first["Effective generation"] == 2
        assert changer(vm, "restrict", 16, "journal-store") == 3
        verifie(droits(vm, 16), "journal-store", "aucune", "journal-store", "yes")
        sortie(vm, "service restart 16")
        etat(vm, 16, "FAILED")
        verifie(droits(vm, 16), "journal-store", "aucune", "aucune instance active")
        assert "boot=" not in sortie(vm, "journal status", (5, 10, 11, 12))
        assert identities(vm) == services
        assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
        assert changer(vm, "restore", 16, "journal-store") == 4
        sortie(vm, "service restart 16")
        etat(vm, 16, "RUNNING")
        assert demarrage(vm) == boot
        restored = droits(vm, 16)
        verifie(restored, "journal-store", "journal-store", "journal-store", "no")
        assert restored["Stored generation"] == restored["Effective generation"] == 4
        assert identities(vm) == services
        ack = sortie(vm, "journal emit 245")
        eid = re.search(r"émission confirmée eid=(\d+)", ack)[1]
        witness, = [e for e in rows(vm, "journal show self") if e["eid"] == eid]
        assert witness["seq"] == eid and witness["boot"] == boot and witness["données"] == "245,0"
        # La vue SELF numérote par producteur. La vue administrative conserve
        # la position globale et le même EID pour cette instance émettrice.
        global_witness, = [e for e in rows(vm, f"journal boot --boot {boot} --limit 128")
                           if e["émetteur"] == witness["émetteur"] and e["eid"] == eid]
        assert all(global_witness[k] == v for k, v in witness.items() if k != "seq")
        arret(vm, vcpus)
    first_boot = archive(image, sector, serial)
    final = read_slots(tool, image, sector, out / "slots-apres.bin")
    assert sorted(final) == [3, 4]
    for generation, journal_mask in [(3, 0), (4, 8)]:
        entries = final[generation]
        assert list(entries) == [1, 2, 9, 10, 16] and entries[16][0] == journal_mask
        assert all(entries[s][1] == raw for s, (_, raw) in before.items())
    relue = out / "relecture.serial"
    with Terminal(arch, sector, image, "caps-journal-relecture", profil_journal=True, journal=relue) as vm:
        vm.until(b"nsh> ")
        newer = demarrage(vm)
        assert newer != boot
        history = rows(vm, f"journal boot --boot {boot} --limit 128")
        assert [e for e in history if e["émetteur"] == witness["émetteur"]
                and e["eid"] == eid] == [global_witness]
        for s in (9, 10):
            etat(vm, s, "RUNNING")
        rebooted = identities(vm)
        assert all(services[s]["instance"] != v["instance"] for s, v in rebooted.items())
        arret(vm, vcpus)
    assert sha(original) == origin
    assert read_slots(tool, image, sector, out / "slots-relus.bin") == final
    assert (out / "slots-apres.bin").read_bytes() == (out / "slots-relus.bin").read_bytes()
    result = dict(architecture=arch, vcpus=vcpus, secteur=sector, noyau=sha(Path(os.environ["NOVA_KERNEL_ELF"])),
                  oracle=sha(tool), initial=origin, boot=boot, reboot=newer, services=services,
                  services_apres_reboot=rebooted, temoin=global_witness, temoin_prive=witness, memoire=memoire(serial), premiere_vm=first_boot,
                  seconde_vm=archive(image, sector, relue), generations=[1, 2, 3, 4],
                  sha_declarations={s: hashlib.sha256(raw).hexdigest() for s, (_, raw) in before.items()})
    (out / "resultats.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("aarch64", "x86_64"))
    p.add_argument("vcpus", type=int, choices=(1, 2, 4))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("preuves", type=Path)
    p.add_argument("oracle", type=Path)
    args = p.parse_args()
    trial(args.arch, args.vcpus, args.secteur, args.preuves.resolve(), args.oracle.resolve(strict=True))
