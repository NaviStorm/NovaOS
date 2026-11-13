"""Témoin du moteur UTM, avec le même disque système et sans l'interface de l'application."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import socket
import subprocess
import tempfile

from bloc import qmp
from es_groupees_elargies import operation, verifier
from es_groupees_utm import IDS, image, publier, relire_systeme
from installation_utm import FIRMWARE, lanceurs
from nsh_utm import Console, reouvrir_vi, transfert
from prototype_uefi import tpm
from publication_conditionnelle import convertir
from vi_ipc_sauvegarde import bilan


@contextmanager
def demarrer(binaires, sortie, config, disque, fmt, boot):
    serie = binaires / 'serie'
    code = sortie / 'code.fd'
    variables = sortie / 'variables.fd'
    with tpm(binaires, sortie, boot, sortie/'tpm') as sock:
        args = [binaires/'qemu-aarch64-softmmu', '-L', FIRMWARE, '-nodefaults', '-S', '-qmp', 'stdio',
                '-machine', config['System']['Target'] + ',' + config['QEMU']['MachinePropertyOverride'],
                '-accel', 'tcg,tb-size=64', '-cpu', config['System']['CPU'], '-smp', '1', '-m', '256',
                '-display', 'none', '-nic', 'none', '-vga', 'none', '-audio', 'none',
                '-chardev', f'socket,id=term,path={serie},server=on,wait=off', '-serial', 'chardev:term',
                '-drive', f'if=pflash,unit=0,format=raw,readonly=on,file={code}',
                '-drive', f'if=pflash,unit=1,format=raw,file={variables},discard=unmap,detect-zeroes=unmap',
                '-drive', f'if=none,id=drivecible,file={disque},format={fmt},discard=unmap,detect-zeroes=unmap',
                '-device', 'virtio-serial', '-chardev', 'null,id=agent',
                '-device', 'virtserialport,chardev=agent,name=org.qemu.guest_agent.0',
                '-chardev', f'socket,id=tch,path={sock}', '-tpmdev', 'emulator,id=tpm0,chardev=tch',
                '-device', 'tpm-crb-device,tpmdev=tpm0', *config['QEMU']['AdditionalArguments']]
        with (sortie/f'{boot}.qemu').open('xb') as log, subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, bufsize=0
        ) as vm:
            try:
                qmp(vm, 'qmp_capabilities')
                with socket.socket(socket.AF_UNIX) as canal, (sortie/f'{boot}.serial').open('xb') as journal:
                    canal.connect(str(serie))
                    qmp(vm, 'cont')
                    yield Console(canal.fileno(), journal), vm
            finally:
                if vm.poll() is None:
                    vm.kill()
                vm.wait(timeout=15)


def mesures_backend(vm):
    disque, = (d for d in qmp(vm, 'query-blockstats') if d['device'] == 'drivecible')
    champs = ('rd_operations', 'wr_operations', 'flush_operations', 'rd_bytes', 'wr_bytes',
              'rd_total_time_ns', 'wr_total_time_ns', 'flush_total_time_ns')
    return {k: disque['stats'][k] for k in champs}


def essai(a):
    out = a.sortie.resolve()
    out.mkdir()
    (out/'tpm').mkdir()
    config = plistlib.loads((a.profil/'config.plist').read_bytes())
    systeme = config['System']
    assert systeme['MemorySize'] == 256 and systeme['CPUCount'] == 1 and not config['QEMU']['Hypervisor']
    args = config['QEMU']['AdditionalArguments']
    rang, = (i for i, v in enumerate(args) if v.startswith('virtio-blk-pci,'))
    args[rang] += f',logical_block_size={a.secteur},physical_block_size={a.secteur}'
    for name in ('code.fd', 'variables.fd'):
        shutil.copyfile(a.profil/'Data'/name, out/name)
    volume, initial = out/'source.p9', out/'neuf.raw'
    etat = json.loads(subprocess.check_output([a.generateur.resolve(), '--image', volume, str(a.secteur), 'utm']))
    avant = verifier(volume, a.secteur, etat['blocs'], None, IDS)
    ids = image(initial, volume, a.elf, a.secteur)
    disque = out/('temoin.' + a.format)
    convertir(initial, 'raw', disque, a.format)
    with tempfile.TemporaryDirectory(prefix='nova-moteur-') as temporaire:
        binaires = Path(temporaire)
        lanceurs(binaires)
        version = subprocess.check_output([binaires/'qemu-aarch64-softmmu', '--version'], text=True).splitlines()[0]
        with demarrer(binaires, out, config, disque, a.format, 'sauvegarde') as (c, vm):
            transfert(c, b'nsh> ', 56)
            reouvrir_vi(c, 'source')
            attendu = operation(a.cas, lambda t: os.write(c.fd, t.encode()), lambda t: c.attendre(t.encode(), 180))
            avant_backend = mesures_backend(vm) if a.sonde else None
            compteurs = None
            debut, oui, fin = publier(c, 'source', 'copie', None)
            if avant_backend is not None:
                compteurs = {k: v - avant_backend[k] for k, v in mesures_backend(vm).items()}
                assert all(v >= 0 for v in compteurs.values()) and compteurs['flush_operations'] >= 2
            os.write(c.fd, b':q\r')
            c.attendre(b'nsh> ')
            c.ligne('exit', b'SYSTEME bilan Ok(())')
        with demarrer(binaires, out, config, disque, a.format, 'reprise') as (c, _):
            transfert(c, b'nsh> ', 56)
            reouvrir_vi(c, 'copie')
            os.write(c.fd, b':q\r')
            c.attendre(b'nsh> ')
            c.ligne('exit', b'SYSTEME bilan Ok(())')
    apres = relire_systeme(disque, a.format, out/'verification.raw', a.secteur, ids,
                           etat['blocs'], attendu, 'copie')
    mesure = out/'mesure.json'
    mesure.write_text(json.dumps([dict(octets=4 << 20, transfert_avant_oui_ms=(oui-debut)/1e6,
                                       publication_apres_oui_ms=(fin-oui)/1e6)]))
    cpt = bilan(out/'sauvegarde.serial', mesure) if a.sonde else None
    if not a.sonde:
        assert b'SAVE from=' not in (out/'sauvegarde.serial').read_bytes()
    return dict(moteur=version, secteur=a.secteur, format=a.format, cas=a.cas, sonde=a.sonde,
                sauvegarde_ms=round((fin-debut)/1e6, 3), apres_oui_ms=round((fin-oui)/1e6, 3),
                avant=avant, apres=apres, octets_relus=len(attendu),
                destination_sha256=hashlib.sha256(attendu).hexdigest(),
                noyau_sha256=hashlib.sha256(a.elf.read_bytes()).hexdigest(),
                compteur=cpt, backend_qmp=compteurs)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    for champ in ('profil', 'elf', 'generateur', 'sortie'):
        p.add_argument(champ, type=Path)
    p.add_argument('secteur', type=int, choices=(512, 4096))
    p.add_argument('--cas', choices=('remplacer-multiple', 'ajout-4m-multiple'), required=True)
    p.add_argument('--format', choices=('raw', 'qcow2'), default='qcow2')
    p.add_argument('--sonde', action='store_true')
    a = p.parse_args()
    resultat = essai(a)
    (a.sortie/'resultats.json').write_text(json.dumps(resultat, ensure_ascii=False, indent=2) + '\n')
