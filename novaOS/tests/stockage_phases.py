"""Injecter une faute de sauvegarde groupée puis vérifier l'image et son redémarrage.

Exécuter depuis un export diagnostic-stockage construit avec stockage-0170-phases.patch.
L'ISO du second démarrage reçoit une copie du disque immobile et aucun injecteur QEMU.
"""
import argparse
from contextlib import ExitStack
import hashlib
from pathlib import Path
import re
import socket
import subprocess
from types import SimpleNamespace

from bloc import qmp
from cli import Terminal
from es_groupees_elargies import grande_insertion
from preuves_stockage import copie_reprise, dossier_essai, empreinte, enregistrer, figer_initiale, copier_image
import vi_volumes as volumes

TAILLE = 4 << 20
# Nom de destination reconnu uniquement par l'invité de diagnostic ; code public attendu.
CAS = {
    'avant-ecriture': ('q-avant', 26),
    'ecriture': ('q-ecriture', 26),
    'apres-ecriture': ('q-apres', 26),
    'expiration-f1': ('coupure-vol-f1', 26),
    'publication': ('q-publication', 22),
    'avant-f2': ('q-avant-f2', 22),
    'expiration-f2': ('coupure-vol-f2', 22),
    'durable': ('q-temoin', 0),
}
PUBLIC = {0: 'fichier enregistré', 22: 'publication incertaine', 26: 'erreur d’E/S'}


def vue(image, secteur, ids, destination):
    def lire(action, *args, check=True):
        with image.open('rb') as disque:
            return subprocess.run([volumes.HOST, action, str(volumes.BLOCKS), str(secteur), *args],
                                  stdin=disque, check=check, capture_output=True)

    inspection = lire('inspect').stdout.decode()
    ancres = [(int(i), int(g)) for i, g in re.findall(r'slot=(\d+) génération=(\d+)', inspection)]
    assert {i for i, _ in ancres} == {0, 1} and abs(ancres[0][1] - ancres[1][1]) == 1, inspection
    lire('verify', *ids)
    slot, generation = max(ancres, key=lambda a: a[1])
    original = volumes.contents(TAILLE)
    contenu_source = lire('salvage', *ids, str(slot), f'/f{TAILLE}').stdout
    assert contenu_source == original, 'source altérée'
    resultat = lire('salvage', *ids, str(slot), '/' + destination, check=False)
    if resultat.returncode:
        assert b'NotFound' in resultat.stderr, resultat.stderr
    else:
        assert resultat.stdout == grande_insertion() + original, 'destination incomplète ou mélangée'
    return dict(inspection=inspection.splitlines(), slot=slot, generation=generation,
                source_sha256=hashlib.sha256(contenu_source).hexdigest(),
                destination_sha256=None if resultat.returncode else hashlib.sha256(resultat.stdout).hexdigest())


def injection_qemu(dossier, cas, slot_actif):
    if cas not in ('ecriture', 'publication'):
        return None
    # L'ouverture de la source publie son atime (F1/F2). N'armer EIO que pour la
    # sauvegarde suivante. Le banc contrôle les FLUSH de cette préparation avant OUI.
    apres_ouverture = ''.join(
        f'[set-state]\nevent = "flush_to_disk"\nstate = "{i}"\nnew_state = "{i + 1}"\n'
        for i in (1, 2))
    # Secteurs QEMU de 512 octets pour les deux géométries. L'ouverture a alterné
    # les ancres une fois : la sauvegarde revient au slot initial, vérifié avant OUI.
    secteur = f'sector = "{slot_actif * 8}"\n' if cas == 'publication' else ''
    config = dossier / 'blkdebug.conf'
    config.write_text(apres_ouverture + '[inject-error]\nevent = "write_aio"\nstate = "3"\nerrno = "5"\n' + secteur + 'once = "on"\n')
    return config


def compteurs(moniteur):
    etat = qmp(moniteur, 'query-blockstats')
    stats = next(d['stats'] for d in etat if d['device'] == 'd')
    return {k: stats[k] for k in ('wr_operations', 'failed_wr_operations', 'flush_operations', 'failed_flush_operations')}


def tentative(dossier, arch, secteur, image, cas, config, ids, initial):
    nom, statut = CAS[cas]
    suspendre = cas.startswith('expiration-')
    with ExitStack() as pile:
        proche, distant = socket.socketpair()
        pile.enter_context(proche)
        pile.enter_context(distant)
        flux = pile.enter_context(proche.makefile('rwb', buffering=0))
        moniteur = SimpleNamespace(stdin=flux, stdout=flux)
        serie = dossier / 'sauvegarde.serial'
        t = pile.enter_context(Terminal(arch, secteur, image, 'phases-' + cas, test=True,
                                        qmp_bloc=distant.fileno(), defaut_bloc=config, journal=serie, diagnostic=True))
        distant.close()
        qmp(moniteur, 'qmp_capabilities')
        t.until(b'nsh> ', 180)
        demarrage = compteurs(moniteur)
        t.vm.stdin.write(f'vi f{TAILLE}\n'.encode())
        t.until(b'\x1b[6n')
        t.vm.stdin.write(b'\x1b[24;80R')
        t.until(f'f{TAILLE} [enregistré] 1:1'.encode())
        t.vm.stdin.write(f'\x10:w {nom}\r'.encode())
        t.until('Saisir OUI puis Entrée : '.encode())
        # Geler uniquement l'invité à l'invite. À cet instant l'atime d'ouverture
        # est publié et aucun appel de sauvegarde n'a été envoyé.
        qmp(moniteur, 'stop')
        assert qmp(moniteur, 'query-status')['status'] == 'paused'
        avant = vue(copier_image(image, 'avant-oui.raw'), secteur, ids, nom)
        stats = compteurs(moniteur)
        enregistrer(dossier, 'avant-oui.json', dict(volume=avant, apres_montage=demarrage, confirmation=stats))
        assert avant['generation'] == initial['generation'] + 1 and avant['destination_sha256'] is None
        assert stats['flush_operations'] - demarrage['flush_operations'] == 2, (stats, demarrage)
        qmp(moniteur, 'cont')
        t.vm.stdin.write(b'OUI\r')
        if suspendre:
            t.until(b'COUPURE groupe armer', 45, refus=PUBLIC[26].encode())

            def controle(commande):
                retour = qmp(moniteur, 'human-monitor-command', **{'command-line': f'qemu-io d "{commande}"'})
                assert retour == '', retour

            controle('break flush_to_disk attente')
            t.until(b'COUPURE groupe soumission')
        # Toute erreur attendue doit atteindre l'éditeur avant de tuer QEMU.
        t.until(PUBLIC[statut].encode(), 45)
        if suspendre:
            # Le périphérique reste suspendu, même lorsque le pilote a déjà expiré.
            controle('wait_break attente')
            enregistrer(dossier, 'bloc-suspendu.json', qmp(moniteur, 'query-blockstats'))
        fin = compteurs(moniteur)
        delta = {k: fin[k] - stats[k] for k in stats}
        enregistrer(dossier, 'issue-qemu.json', dict(apres=fin, delta=delta))
    texte = serie.read_text(errors='replace').split('Saisir OUI puis Entrée : ')[-1]
    for code, libelle in PUBLIC.items():
        assert (libelle in texte) == (code == statut), (cas, libelle, texte[-900:])
    if statut:
        assert f'FS0170 plan resultat statut={statut}' in texte, texte[-1200:]
    if suspendre:
        assert 'BLOC0170 finish used op=2' in texte and 'erreur=Timeout' in texte, texte[-1200:]
    if cas in ('ecriture', 'publication'):
        assert 'BLOC erreur peripherique op=1 statut=1' in texte, texte[-1200:]
    if cas in ('avant-ecriture', 'apres-ecriture', 'avant-f2'):
        position = {'avant-ecriture': 'avant_premiere_ecriture', 'apres-ecriture': 'apres_lot_avant_F1',
                    'avant-f2': 'ancre_ecrite_avant_F2'}[cas]
        assert f'INJECT refus position={position}' in texte, texte[-1200:]
    acquits = [int(f) for f in re.findall('FS0170 barriere_confirmee F([12])', texte)]
    attendus = [1, 2] if not statut else ([1] if statut == 22 else [])
    assert acquits == attendus, (cas, acquits, attendus)
    assert delta['flush_operations'] == len(acquits), (cas, delta, acquits)
    assert delta['failed_wr_operations'] == int(cas in ('ecriture', 'publication')), (cas, delta)
    if cas == 'avant-ecriture':
        assert delta['wr_operations'] == 0, delta
    if cas == 'apres-ecriture':
        assert delta['wr_operations'] > 0, delta
    return dict(message=PUBLIC[statut], statut=statut, barrieres_confirmees=acquits,
                generation_avant_oui=avant['generation'], compteurs_apres_oui=delta)


def verifier_cellule(arch, secteur, cas, rapport):
    nom, _ = CAS[cas]
    with dossier_essai(rapport, arch, secteur, cas, conserver=True,
                       parametres=dict(destination=nom, octets_origine=TAILLE, insertion=TAILLE)) as dossier:
        volumes.SIZES = (TAILLE,)
        volumes.BLOCKS = 4096
        image, ids = volumes.prepare(dossier, secteur)
        depart = vue(figer_initiale(image), secteur, ids, nom)
        assert depart['destination_sha256'] is None
        config = injection_qemu(dossier, cas, depart['slot'])
        # Une issue applicative inattendue doit également laisser les preuves de récupération.
        try:
            public = tentative(dossier, arch, secteur, image, cas, config, ids, depart)
        finally:
            reprise = copie_reprise(image)
            apres = vue(image, secteur, ids, nom)
            with Terminal(arch, secteur, reprise, 'relecture-' + cas, test=True,
                          journal=dossier / 'reprise.serial', diagnostic=True) as t:
                t.until(b'nsh> ', 180)
                # Le montage en écriture a réussi. ls toucherait l'atime du
                # répertoire et publierait une transaction étrangère à cet essai.
                t.exit()
            relu = vue(reprise, secteur, ids, nom)
            enregistrer(dossier, 'relecture.json', dict(initial=depart, apres=apres, apres_redemarrage=relu))
            assert relu == apres
        publie = apres['destination_sha256'] is not None
        assert apres['generation'] == public['generation_avant_oui'] + int(publie)
        if cas == 'durable':
            assert publie
        elif cas not in ('avant-f2', 'expiration-f2'):
            assert not publie
        ligne = dict(architecture=arch, secteur=secteur, scenario=cas, **public, apres=apres,
                     generation_initiale=depart['generation'], redemarrage_verifie=True, dossier=dossier.name,
                     sha256_initiale=empreinte(dossier / 'initiale.raw'), sha256_apres=empreinte(image),
                     sha256_reprise=empreinte(reprise))
        enregistrer(dossier, 'resultat.json', ligne)
    return ligne


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('architecture', choices=('aarch64', 'x86_64'))
    p.add_argument('secteur', type=int, choices=(512, 4096))
    p.add_argument('rapport', type=Path)
    p.add_argument('--cas', choices=CAS, nargs='+', default=tuple(CAS))
    args = p.parse_args()
    if args.rapport.exists():
        p.error('choisir un nouveau fichier de rapport')
    args.rapport.parent.mkdir(parents=True, exist_ok=True)
    rows = []
    for cas in args.cas:
        rows.append(verifier_cellule(args.architecture, args.secteur, cas, args.rapport))
        print(args.architecture, args.secteur, cas, rows[-1]['statut'], flush=True)
    enregistrer(args.rapport.parent, args.rapport.name, rows)
