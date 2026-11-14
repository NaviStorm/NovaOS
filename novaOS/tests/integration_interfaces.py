"""Croiser les programmes publiés et ceux du prototype dans deux archives de laboratoire isolées."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import select
import subprocess
import sys
import tempfile
import time

ARCHS = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}
COMPOSANTS = {"vi": "vi.elf", "nsh": "nsh.elf", "novafs": "storage-media.bin",
              "relais": "storage-relay.bin", "pilote": "storage-disk.bin"}
# Une composition = base du noyau et des services, programmes pris dans l'autre archive.
CONFIGURATIONS = (
    ("publie", "publie", ()), ("prototype", "prototype", ()),
    ("vi-publie", "prototype", ("vi",)), ("nsh-publie", "prototype", ("nsh",)),
    ("clients-publies", "prototype", ("nsh", "vi")), ("novafs-publie", "prototype", ("novafs",)),
    ("bloc-publie", "prototype", ("relais", "pilote")), ("vi-prototype", "publie", ("vi",)),
)
COMPLEMENTS = (
    ("relais-publie", "prototype", ("relais",)), ("pilote-publie", "prototype", ("pilote",)),
    ("novafs-prototype", "publie", ("novafs",)), ("relais-prototype", "publie", ("relais",)),
    ("pilote-prototype", "publie", ("pilote",)),
    ("chaine-prototype", "publie", tuple(COMPOSANTS)),
)
TAILLES_PAGES = (12 * 1024, 24 * 1024)
FRONTIERE = 4096
AJOUT = ("édition-" + "0123456789" * 6 + "-terminée-").encode()


def source_pages(taille):
    # La deuxième ligne commence à 4096. Numéroter les lignes distingue les pages
    # à la relecture, y compris en cas de décalage du suffixe resté sur disque.
    premiere = b"premier" + b"a" * (FRONTIERE - 1 - len(b"premier")) + b"\n"
    milieu = b"frontiere-4096\n" + b"".join(
        (f"ligne-{n:05d}-" + "texte" * 6 + "\n").encode() for n in range(taille // 32)
    )
    fin = b"derniers-octets"
    source = premiere + milieu[:taille - FRONTIERE - len(fin)] + fin
    assert len(source) == taille and source.index(b"frontiere-4096") == FRONTIERE
    return source


def fixture_pages(folder, secteur, taille, lecteur):
    source = source_pages(taille)
    commandes = "create /source\nwrite /source 0 " + source.hex() + "\n"
    base, image = folder / "base.raw", folder / "volume.raw"
    with base.open("xb") as raw:
        r = subprocess.run([lecteur, "script", "1024", str(secteur)], input=commandes.encode(),
                           stdout=raw, stderr=subprocess.PIPE, check=True)
    fs, lv = re.search(rb"FS=(\w+) LV=(\w+)", r.stderr).groups()
    with base.open("rb") as raw, image.open("xb") as out:
        subprocess.run([lecteur, "migrate-content-p9", "1024", str(secteur), fs, lv],
                       stdin=raw, stdout=out, check=True)
    return image, fs, lv, source


def verifier_cat_pages(serie, attendu):
    # Le noyau peut écrire ses mesures bloc sur le même UART entre deux appels
    # console de cat. Le texte contrôlé de la fixture ne contient pas ce motif.
    trace = rb"BLOC parcours i=\d+ octets=\d+ ns=\d+\r\n"
    assert b"BLOC parcours" not in attendu
    sortie = re.sub(trace, b"", serie)
    assert sortie.count(attendu.replace(b"\n", b"\r\n")) == 1


def sha(donnees):
    return hashlib.sha256(donnees).hexdigest()


def outils(archive, arch):
    cible = archive / "novaOS/target" / ARCHS[arch] / "release"
    (catalogue,) = cible.glob("build/novaos-*/out/cli_catalog.rs")
    return catalogue.parent, cible / "novaos"


def reconstruire(archive, arch, log):
    # Les ELF sont injectés uniquement dans les produits temporaires du build.
    # Forcer une nouvelle liaison du noyau sans réexécuter le générateur des ELF.
    (archive / "novaOS/kernel/src/sched/process/storage.rs").touch()
    with log.open("ab") as f:
        subprocess.run(["make", "ARCH="+arch, "kernel"], cwd=archive/"novaOS", stdout=f,
                       stderr=subprocess.STDOUT, env=dict(os.environ, CARGO_NET_OFFLINE="true"), check=True)


def attendre(t, messages, secondes=45):
    fin = time.monotonic() + secondes
    while True:
        presents = [(t.pending.index(m), m) for m in messages if m in t.pending]
        if presents:
            _, message = min(presents)
            return message, t.until(message)
        if time.monotonic() >= fin or t.vm.poll() is not None:
            raise RuntimeError(f"attente terminal {messages!r}, fin {t.pending[-500:]!r}")
        if select.select([t.vm.stdout], [], [], .1)[0]:
            morceau = os.read(t.vm.stdout.fileno(), 65536)
            if not morceau:
                raise RuntimeError(f"fin anticipée {t.pending[-500:]!r}")
            t.log.write(morceau)
            t.log.flush()
            t.pending += morceau


def observer(archive, arch, secteur, qualification, dossier, id_essai, remontage=False, taille=None):
    # L'appel s'effectue dans un processus frais, pour lier cli.Terminal au bon export.
    sys.path.insert(0, str(archive / "novaOS/tests"))
    from cli import HOST, Terminal
    from vi import SOURCE, fixture

    with tempfile.TemporaryDirectory(dir=dossier) as tmp:
        if taille is None:
            image, fs, lv = fixture(Path(tmp), secteur)
            source = SOURCE
            attendu = b"Z" + source
            temoins = ["Zpremier", "été", "fin"]
        else:
            image, fs, lv, source = fixture_pages(Path(tmp), secteur, taille, HOST)
            attendu = source[:FRONTIERE] + AJOUT + source[FRONTIERE:]
            temoins = [b"premier", AJOUT + b"frontiere-4096", b"derniers-octets"]
        invite = b"nsh> "
        ferme = b"BLOC session terminee ok="
        consentement = False
        with Terminal(arch, secteur, image, id_essai, test=qualification) as t:
            def envoyer(data):
                t.vm.stdin.write(data)
            premier, _ = attendre(t, [invite, ferme])
            if premier == ferme:
                etat = "session_refusee"
            else:
                envoyer(b"vi source\r")
                t.until(b"\r\n")
                marque, _ = attendre(t, [b"\x1b[6n", b"vi : ", b"[statut ", ferme])
                if marque == b"\x1b[6n":
                    envoyer(b"\x1b[24;80R")
                    t.until("source [enregistré] 1:1".encode())
                    if taille is None:
                        envoyer(b"iZ\x1b:w copieaudit\r")
                    else:
                        envoyer(b"j0")
                        t.until("source [enregistré] 2:1".encode())
                        envoyer(b"i")
                        texte = AJOUT.decode()
                        for i in range(0, len(texte), 12):
                            morceau = texte[i:i + 12]
                            envoyer(morceau.encode())
                            t.until(f"source [modifié] 2:{i + len(morceau) + 1}".encode())
                        envoyer(b"\x1b:w copieaudit\r")
                    erreurs = [s.encode() for s in ("échec de l’opération", "destination non autorisée",
                                                       "argument invalide", "capacité révoquée",
                                                       "issue de sauvegarde incertaine", "modifications non sauvegardées")]
                    question = b"Saisir OUI puis "
                    succes = "fichier enregistré".encode()
                    m, _ = attendre(t, [question, succes, *erreurs])
                    consentement = m == question
                    if consentement:
                        t.until("Entrée : ".encode())
                        envoyer(b"OUI\r")
                        m, _ = attendre(t, [succes, *erreurs])
                    etat = "sauvegarde_validee" if m == succes else "sauvegarde_refusee"
                    envoyer(b":q!\r")
                elif marque == ferme:
                    etat = "session_refusee"
                else:
                    etat = "ouverture_refusee"
                if etat != "session_refusee":
                    t.until(invite)
                    t.exit()
        trace = t.archive.read_bytes()
        (dossier/(id_essai+".serial")).write_bytes(trace)
        reprise = None
        if remontage and etat == "sauvegarde_validee":
            with Terminal(arch, secteur, image, id_essai + "-remontage", test=qualification) as t:
                t.until(invite)
                t.line("cat copieaudit", has=temoins)
                t.exit()
            serie = t.archive.read_bytes()
            if taille is not None:
                verifier_cat_pages(serie, attendu)
            (dossier / (id_essai + "-remontage.serial")).write_bytes(serie)
            reprise = sha(serie)
        def lecteur(op, *args, check=True):
            with image.open("rb") as flux:
                return subprocess.run([HOST, op, "1024", str(secteur), *args], stdin=flux,
                                      capture_output=True, check=check)
        lecteur("verify", fs, lv)
        slots = re.findall(rb"slot=(\d+) g\xc3\xa9n\xc3\xa9ration=(\d+)", lecteur("inspect").stdout)
        assert len(slots) == 2
        slot, _ = max(slots, key=lambda x: int(x[1]))
        assert lecteur("salvage", fs, lv, slot, "/source").stdout == source
        dst = lecteur("salvage", fs, lv, slot, "/copieaudit", check=False)
        if etat == "sauvegarde_validee":
            assert dst.returncode == 0 and dst.stdout == attendu
        else:
            assert dst.returncode != 0 and b"NotFound" in dst.stderr
        return dict(resultat=etat, confirmation_affichee=consentement,
                    sha256_source=sha(source), destination="octets égaux" if dst.returncode==0 else "absente",
                    sha256_destination=sha(dst.stdout) if dst.returncode == 0 else None,
                    sha256_serie=sha(trace), sha256_remontage=reprise,
                    generations=sorted(int(n) for _,n in slots))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("publie", type=Path, help="archive jetable du dépôt publié, compilée pour les deux cibles")
    p.add_argument("prototype", type=Path, help="archive jetable avec fixtures, compilée pour les deux cibles")
    p.add_argument("sortie", type=Path)
    p.add_argument("architecture", choices=ARCHS)
    p.add_argument("--session", nargs=3, metavar=("BASE", "ID", "MODE"))
    p.add_argument("--etendue", action="store_true", help="croiser aussi le relais et le pilote séparément")
    p.add_argument("--remontage", action="store_true", help="redémarrer avec le même volume et relire la copie")
    p.add_argument("--secteur", type=int, choices=(512, 4096), help="remplacer la géométrie de référence")
    p.add_argument("--frontieres", action="store_true", help="cinq croisements des frontières de montage en profil 42")
    p.add_argument("--pages", action="store_true",
                   help="tester les croisements sur 12 et 24 Kio avec édition à l’octet 4096 et remontage")
    p.add_argument("--taille-session", type=int, choices=TAILLES_PAGES, help=argparse.SUPPRESS)
    p.add_argument("--branche-brute", type=Path,
                   help="tester uniquement le profil 42 d'un troisième export compilé sans fixture")
    args = p.parse_args()
    if args.pages and (args.frontieres or args.etendue or args.branche_brute):
        p.error("--pages utilise sa propre sélection de compositions")
    a = args.architecture
    secteur = args.secteur or (512 if a == "aarch64" else 4096)
    exports = {v:getattr(args, v).resolve() for v in ("publie", "prototype")}
    dossier = args.sortie.resolve()
    dossier.mkdir(exist_ok=True, parents=True)
    if args.branche_brute:
        archive = args.branche_brute.resolve()
        if (archive / ".git").exists() or archive in exports.values() or args.session:
            p.error("Le témoin brut utilise un troisième export jetable distinct.")
        rid = f"{a}-branche-brute-42"
        out, elf = outils(archive, a)
        r = observer(archive, a, secteur, True, dossier, rid, args.remontage)
        trace = (dossier / (rid + ".serial")).read_bytes()
        bilan = re.search(rb"BLOC bilan mode=42 ([^\r\n]+)", trace)
        r.update(architecture=a, secteur=secteur, mode=42,
                 sha256_elf=sha(elf.read_bytes()),
                 sha256_composants={c: sha((out / f).read_bytes()) for c, f in COMPOSANTS.items()},
                 chaine_demarre=b"BLOC chaine novaFS=" in trace,
                 invite_nsh=b"nsh> " in trace,
                 bilan=bilan[1].decode() if bilan else None,
                 session_fermee=b"BLOC session terminee ok=true" in trace)
        (dossier / (rid + ".json")).write_text(json.dumps(r, ensure_ascii=False, indent=2) + "\n")
        print(rid, r["resultat"], flush=True)
        attendu = "service=Exit(0) bloc=Exit(0) pilote=Exit(0) client=Exit(0) retrait=Ok(())"
        if (r["resultat"] != "sauvegarde_validee" or not r["confirmation_affichee"]
                or not r["chaine_demarre"] or not r["invite_nsh"] or not r["session_fermee"]
                or not (r["bilan"] or "").startswith(attendu)):
            raise SystemExit("Échec du témoin brut. Consulter le bilan et la trace série.")
        return
    if args.session:
        base, rid, mode = args.session
        r = observer(exports[base], a, secteur, mode=="42", dossier, rid, args.remontage, args.taille_session)
        print(json.dumps(r, ensure_ascii=False))
        return
    if any((d/".git").exists() for d in exports.values()) or len(set(exports.values())) != 2:
        p.error("Deux exports jetables distincts, sans .git, sont requis.")
    originaux = {nom:{c:(out/f).read_bytes() for c,f in COMPOSANTS.items()}
                 for nom,d in exports.items() for out,_ in [outils(d,a)]}
    resultats = []
    configurations = CONFIGURATIONS + (COMPLEMENTS if args.etendue or args.frontieres or args.pages else ())
    if args.pages:
        inclus = {"prototype", "vi-publie", "vi-prototype", "novafs-publie", "chaine-prototype"}
        configurations = tuple(c for c in configurations if c[0] in inclus)
    if args.frontieres:
        inclus = {"relais-publie", "pilote-publie", "bloc-publie", "novafs-publie", "chaine-prototype"}
        configurations = tuple(c for c in configurations if c[0] in inclus)
    for cas, base, autres in configurations:
        autre = "prototype" if base=="publie" else "publie"
        archive = exports[base]
        out, elf = outils(archive,a)
        try:
            for c in autres:
                (out/COMPOSANTS[c]).write_bytes(originaux[autre][c])
            if autres:
                reconstruire(archive,a,dossier/(cas+"-"+a+"-build.log"))
            identites = {c:sha((out/f).read_bytes()) for c,f in COMPOSANTS.items()}
            assert all(identites[c]==sha(originaux[autre if c in autres else base][c]) for c in COMPOSANTS)
            essais = [("42", t) for t in TAILLES_PAGES] if args.pages else [
                (mode, None) for mode in (("42",) if args.frontieres else ("41", "42"))
            ]
            for mode, taille in essais:
                rid = f"{a}-{cas}-{mode}"
                if taille:
                    rid += f"-{taille}"
                cmd = [sys.executable,"-B",__file__, str(exports["publie"]), str(exports["prototype"]),
                       str(dossier),a,"--session",base,rid,mode,"--secteur",str(secteur)]
                if taille:
                    cmd.extend(["--taille-session", str(taille)])
                if args.remontage or taille:
                    cmd.append("--remontage")
                enfant = subprocess.run(cmd, capture_output=True,text=True)
                (dossier/(rid+".log")).write_text(enfant.stderr)
                r = json.loads(enfant.stdout) if enfant.returncode==0 else dict(resultat="banc_incomplet",
                                                                                 erreur=enfant.stderr[-900:])
                r.update(architecture=a,secteur=secteur,composition=cas,base=base,
                         mode=int(mode),sha256_elf=sha(elf.read_bytes()),sha256_composants=identites)
                if taille:
                    r.update(octets_source=taille, offset=FRONTIERE, octets_ajoutes=len(AJOUT))
                resultats.append(r)
                print(rid, r["resultat"], flush=True)
                (dossier/(a+".json")).write_text(json.dumps(resultats, ensure_ascii=False,indent=2)+"\n")
        finally:
            if autres:
                for c in autres:
                    (out/COMPOSANTS[c]).write_bytes(originaux[base][c])
                reconstruire(archive,a,dossier/(cas+"-"+a+"-restauration.log"))
    if any(r["resultat"] == "banc_incomplet" for r in resultats):
        raise SystemExit("Au moins une observation incomplète. Voir les journaux de la campagne.")
    if args.pages and not all(r["resultat"] == "sauvegarde_validee" and r["confirmation_affichee"]
                              and r["sha256_remontage"] for r in resultats):
        raise SystemExit("Au moins une sauvegarde ou un remontage multipage a échoué.")


if __name__ == "__main__":
    main()
