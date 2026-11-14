"""Reconstitue les ablations IPC dans une copie Git et archive les mesures de présélection."""
import argparse
import json
import os
from pathlib import Path
import random
import shutil
import statistics
import subprocess
import sys
import tarfile

DEPOT = Path(__file__).resolve().parents[2]
HISTORIQUE = 'f356970d27cf8e8fad24d487a3debdc4d2a0be18'
BASE = '4e741fbb2e40df84f69f1a2e8ee40b5182968650'
CANDIDAT = 'a8dc2534ac9fabcca2e480d1054fd0b60f180f17'
STANDARD_PHASE3 = '86a9abaddc0cc85e0afc067b1894c81245cdc198'
SONDES_RAM = '17e8de27238355d2dbf6cf07589bfa15e6294bd0'
SERIES = {
    'mesures': (100, ('base', 'pool-fixe', 'gate-avant', 'syscall-avant', 'diffusion', 'garde', 'historique')),
    'comparaison': (180, ('base', 'historique', 'garde', 'ram', 'loc', 'g-ram', 'g-ram-loc')),
}
CADRES = ('cadres-avant', 'cadres-apres')
VARIANTES = sorted(set(SERIES['mesures'][1] + SERIES['comparaison'][1] + CADRES))
CIBLES = {'x86_64': 'x86_64-unknown-none', 'aarch64': 'aarch64-unknown-none-softfloat'}
SCHED = 'novaOS/kernel/src/sched/'
IPC = SCHED + 'process/ipc/mod.rs'
RAM = 'novaOS/kernel/src/memory/frames.rs'


def blob(revision, chemin):
    return subprocess.check_output(['git', 'show', f'{revision}:{chemin}'], cwd=DEPOT)


def preparer(nom, sortie):
    revision = {'historique': HISTORIQUE, 'cadres-avant': STANDARD_PHASE3,
                'cadres-apres': CANDIDAT}.get(nom, BASE)
    sortie.mkdir(parents=True, exist_ok=False)
    with subprocess.Popen(['git', 'archive', revision, 'components', 'novaFS', 'novaOS',
                           ':(exclude)novaOS/docs/**', ':(exclude)novaFS/docs/**'],
                          cwd=DEPOT, stdout=subprocess.PIPE) as git:
        with tarfile.open(fileobj=git.stdout, mode='r|') as archive:
            archive.extractall(sortie, filter='data')
        if git.wait():
            raise RuntimeError(f'Export incomplet pour {nom}')
    if nom == 'pool-fixe':
        def declaration(rev):
            return next(l for l in blob(rev, IPC).splitlines() if l.startswith(b'type Waits ='))
        chemin = sortie / IPC
        chemin.write_bytes(chemin.read_bytes().replace(declaration(BASE), declaration(HISTORIQUE), 1))
    if nom in ('gate-avant', 'syscall-avant'):
        chemin = SCHED + ('gate.rs' if nom == 'gate-avant' else 'process/syscall.rs')
        (sortie / chemin).write_bytes(blob(HISTORIQUE, chemin))
    if nom == 'diffusion':
        chemin = sortie / (SCHED + 'mod.rs')
        source = chemin.read_text()
        debut = source.index('        if other != 0 {')
        fin = source.index('\n    }\n    fn signal', debut)
        source = source[:debut] + '''        if (0..crate::smp::count())
            .any(|cpu| other & (1 << cpu) != 0 && self.currents[cpu] == Self::idle_slot(cpu))
        {
            profile::kick(me);
            crate::arch::kick();
        }''' + source[fin:]
        chemin.write_text(source)
    if nom in ('garde', 'g-ram', 'g-ram-loc'):
        patch = blob(BASE, 'novaOS/tests/fixtures/ipc-smp-phase3-garde-x86.patch')
        # patch -p1 fonctionne dans une copie sans index, même hors du dépôt.
        subprocess.run(['patch', '-p1'], cwd=sortie, input=patch, check=True)
    if nom in ('ram', 'g-ram', 'g-ram-loc'):
        (sortie / RAM).write_bytes(blob(CANDIDAT, RAM))
    if nom in ('loc', 'g-ram-loc'):
        source = blob(CANDIDAT, IPC).decode()
        ligne = '            self.locals_to_check |= 1 << p.slot;\n'
        # Lors de la présélection, la création était marquée avant sa tentative.
        assert source.count(ligne) == 1 and source.count('            self.locals.insert(\n') == 1
        source = source.replace(ligne, '').replace('            self.locals.insert(\n',
                                                    ligne + '            self.locals.insert(\n')
        (sortie / IPC).write_text(source)
    if nom == 'cadres-apres':
        for chemin in ('novaOS/kernel/src/memory/probes.rs', SCHED + 'process/abi_probe.rs'):
            (sortie / chemin).write_bytes(blob(SONDES_RAM, chemin))


def compiler(nom, source, arch):
    rustc = subprocess.check_output(['rustup', 'which', '--toolchain', 'stable', 'rustc'], text=True).strip()
    cible = CIBLES[arch]
    build = source.parent / (source.name + '-elf')
    args = ['rustup', 'run', 'stable', 'cargo', 'build', '--release', '--offline', '--locked',
            '--target', cible, '--target-dir', build]
    if nom not in CADRES:
        args += ['--features', 'ipc-smp-lab']
    subprocess.run(args, cwd=source/'novaOS', env=dict(os.environ, RUSTC=rustc), check=True)
    return build/cible/'release/novaos'


def ordre(serie, rep):
    graine, noms = SERIES[serie]
    noms = list(noms)
    random.Random(graine + rep).shuffle(noms)
    return noms


def mesurer(serie, sortie, variante):
    noms = (variante,) if variante else SERIES[serie][1]
    if variante and variante not in SERIES[serie][1]:
        raise ValueError(f'{variante} absent de {serie}')
    # Même lecteur de série, même ISO et même lanceur hôte pour tous les noyaux.
    base = sortie/'sources'/serie/'base'
    preparer('base', base)
    sys.path.insert(0, str(base/'novaOS/tests'))
    from ipc_multicoeur import mesurer as mesurer_vm
    elfs = {}
    for nom in noms:
        source = base.parent/nom
        if nom != 'base':
            preparer(nom, source)
        elfs[nom] = compiler(nom, source, 'x86_64')
    os.environ['NOVA_RAM'] = '256'
    os.environ['NOVA_X86_CPU'] = 'max'
    for rep in range(3):
        for nom in ordre(serie, rep):
            if nom in elfs:
                dossier = sortie/serie/nom
                dossier.mkdir(parents=True, exist_ok=True)
                resultat = mesurer_vm('x86_64', 4, 'paires', elfs[nom], rep, dossier)
                ecrire(dossier/f'x86_64-4-paires-{rep}.json', resultat)


def mesurer_cadres(sortie):
    # Le contrôleur de phase 4 exécutait les deux ELF avec les mêmes tests.
    sources = {nom: sortie/nom for nom in CADRES}
    for nom, source in sources.items():
        preparer(nom, source)
    controleur = sources['cadres-apres']/'novaOS'
    for arch in CIBLES:
        for nom, source in sources.items():
            elf = compiler(nom, source, arch)
            env = dict(os.environ, NOVA_ARCH=arch, NOVA_VCPUS='4', NOVA_KERNEL_ELF=str(elf))
            subprocess.run([sys.executable, '-B', 'tests/ipc.py', 'test', '--ram', '128'],
                           cwd=controleur, env=env, check=True)
            shutil.copyfile(controleur/'build'/f'ipc-{arch}-test-128.json', sortie/f'{nom}-{arch}.json')


def ecrire(fichier, objet):
    with fichier.open('x', encoding='utf-8') as f:
        json.dump(objet, f, ensure_ascii=False, separators=(',', ':'), allow_nan=False)
        f.write('\n')


def archiver(racine):
    demarrages, synthese = {}, {}
    for serie, (_, noms) in SERIES.items():
        demarrages[serie], synthese[serie] = {}, {}
        for nom in noms:
            boots, debits, medianes = [], [], []
            for rep in range(3):
                fichier = racine/serie/nom/f'x86_64-4-paires-{rep}.json'
                boot = json.loads(fichier.read_bytes())
                conf = boot['config']
                lots = boot['releves']
                if ((boot['arch'], boot['vcpu'], boot['cas'], boot['repetition']) != ('x86_64', 4, 'paires', rep)
                    or (conf['cpu'], conf['couples'], conf['tours']) != (4, 4, 1000)
                    or [lot['serie'] for lot in lots] != list(range(8))
                    or any(len(lot['couples']) != 4 or min(lot['couples']) <= 0
                           or max(lot['couples']) != lot['ns'] for lot in lots)):
                    raise ValueError(f'Relevé incomplet ou incohérent dans {fichier}')
                vitesses = [4e12 / lot['ns'] for lot in lots[1:]]
                debits.extend(vitesses)
                medianes.append(statistics.median(vitesses))
                boots.append(boot)
            if len({b['elf'] for b in boots}) != 1:
                raise ValueError(f'Noyau différent entre démarrages de {nom}')
            demarrages[serie][nom] = boots
            synthese[serie][nom] = dict(medianes_par_demarrage=medianes, mediane=statistics.median(debits),
                                       minimum=min(debits), maximum=max(debits))
    cadres, bilan = {}, {}
    for arch in CIBLES:
        cadres[arch] = {nom: json.loads((racine/f'{nom}-{arch}.json').read_bytes()) for nom in CADRES}
        avant, apres = (cadres[arch][nom]['phases'] for nom in CADRES)
        for nom, resultat in cadres[arch].items():
            phases = resultat['phases']
            if not (resultat['architecture'] == arch and resultat['ram_mio'] == 128 and resultat['mode'] == 'test'
                    and phases['ipc_repos'] == phases['ipc_detruits'] == phases['ipc_reprise']):
                raise ValueError(f'Relevé mémoire incohérent pour {arch}/{nom}')
        bilan[arch] = {phase: {cle: apres[phase][cle] - v for cle, v in valeurs.items()}
                       for phase, valeurs in avant.items()}
    revisions = dict(historique=HISTORIQUE, base=BASE, candidat=CANDIDAT,
                     cadres_avant=STANDARD_PHASE3, sondes_cadres_apres=SONDES_RAM)
    return dict(revisions=revisions, demarrages=demarrages, synthese_tours_s=synthese,
                cadres=cadres, ecarts_cadres=bilan,
                ordres={serie: [ordre(serie, r) for r in range(3)] for serie in SERIES})


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    commandes = cli.add_subparsers(dest='action', required=True)
    prep = commandes.add_parser('preparer', help='export Git dans un répertoire neuf')
    prep.add_argument('variante', choices=VARIANTES)
    prep.add_argument('sortie', type=Path)
    prep.add_argument('--compiler', action='store_true')
    prep.add_argument('--arch', choices=CIBLES, default='x86_64')
    banc = commandes.add_parser('mesurer', help='trois démarrages par variante, série complète par défaut')
    banc.add_argument('serie', choices=SERIES)
    banc.add_argument('sortie', type=Path)
    banc.add_argument('--variante', choices=VARIANTES)
    mem = commandes.add_parser('mesurer-cadres', help='quatre démarrages avec les noyaux ordinaires')
    mem.add_argument('sortie', type=Path)
    archive = commandes.add_parser('archiver', help='vérifier et rassembler les deux séries et les quatre mesures mémoire')
    archive.add_argument('racine', type=Path)
    archive.add_argument('json', type=Path)
    args = cli.parse_args()
    if args.action == 'archiver':
        ecrire(args.json, archiver(args.racine))
    elif args.action == 'preparer':
        source = args.sortie.resolve()
        preparer(args.variante, source)
        if args.compiler:
            print(compiler(args.variante, source, args.arch))
    elif args.action == 'mesurer':
        mesurer(args.serie, args.sortie.resolve(), args.variante)
    else:
        mesurer_cadres(args.sortie.resolve())


if __name__ == '__main__':
    main()
