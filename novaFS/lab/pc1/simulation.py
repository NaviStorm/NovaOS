"""S01–S11 et relecture R03. Le banc n'ouvre en écriture que sa nouvelle sortie."""
import argparse
from collections import Counter
import itertools
import json
from pathlib import Path
import platform
import re
import subprocess

from lecture import (ANCIEN, AJOUTS, C1, C2, J, N, NOUVEAU, PAGE, PROFIL, ZERO, Profil,
                     digest, entier, generation, identifiant, image, inspecter, mac, octets,
                     positions, racine)
from vues import Graphe, campagne

ROOT = Path(__file__).resolve().parents[3]
VECTEURS = ROOT / "novaFS/docs/novafs-publication-conditionnelle.md"


def chronologie(preparation=(9,5), publication=(1,253), erreur="", effets=0, mensonge=False):
    """Cache et support distincts, un seul résultat livré, lecture de chaque dépendance."""
    stable = dict(ANCIEN)
    cache = {}
    signes = set()
    succes = False

    def vue():
        return {**stable, **cache}

    def etape(nom):
        return nom, dict(stable), dict(cache), set(signes), succes

    def ecriture(p, contenu):
        yield etape(f"{p}/soumission")
        cache[p] = contenu[:96] + ZERO[96:]
        yield etape(f"{p}/partielle")
        cache[p] = contenu
        yield etape(f"{p}/complétion")

    def barriere(nom):
        yield etape(nom + "/soumission")
        if mensonge and nom == "F2":
            yield etape(nom + "/complétion")
            return
        for i, p in enumerate(list(reversed(cache))):
            if nom == erreur and i >= effets:
                return
            stable[p] = cache.pop(p)
            yield etape(nom + "/effet")
        if nom != erreur:
            yield etape(nom + "/complétion")

    for p in preparation:
        if erreur in (f"EIO-{p}", f"délai-{p}"):
            yield etape(erreur)
            return
        yield from ecriture(p, AJOUTS[p])
    yield from barriere("F1")
    if erreur == "F1":
        return
    for p in (5, 9):
        yield etape(f"relecture-{p}/avant")
        if erreur == f"lecture-{p}" or vue()[p] != AJOUTS[p]:
            return
        yield etape(f"relecture-{p}/après")
    for p in publication:
        role = 1 if p == 1 else 2
        contenu = PROFIL.enregistrement(C2, role)
        signes.add(p)
        if erreur == f"EIO-{p}":
            yield etape("EIO")
            return
        yield from ecriture(p, contenu)
    yield from barriere("F2")
    if erreur == "F2":
        return
    for p in (1, 253):
        yield etape(f"relecture-{p}/avant")
        if erreur == f"lecture-{p}" or PROFIL.decoder(vue()[p], p, 2, 1 if p == 1 else 2) != C2:
            return
        yield etape(f"relecture-{p}/après")
    yield etape("réponse/avant")
    succes = True
    yield etape("réponse/reçue")


def oracle(support):
    # Vérité du scénario à deux générations, indépendante du code de parcours/MAC.
    d = [support.get(p, ZERO) == AJOUTS[p] for p in (1,253)]
    if any(d):
        if support.get(9) != AJOUTS[9]:
            return "refus", 0
        return ("sain" if all(d) and support.get(5) == AJOUTS[5] else "dégradé"), 2
    if any(support.get(p, ZERO) != ZERO for p in (5,1,253)):
        return "indéterminé", 1
    return "sain", 1


class Banc:
    def __init__(self, s, out, arch):
        self.s = s
        self.n = Counter()
        self.groupe = ""
        self.out = out
        self.arch = arch
        self.bin = ROOT / f"novaFS/target/{arch}-apple-darwin/release"

    def controle(self, support, etat, rang, **args):
        avant = digest(octets(support))
        r = inspecter(support, **args)
        assert (r.etat, r.rang) == (etat, rang), (self.groupe, r, etat, rang)
        if rang and args.get("profil", PROFIL) is PROFIL and rang <= 2:
            assert r.corps == (C1, C2)[rang-1]
        assert digest(octets(support)) == avant
        self.n[self.groupe] += 1
        return r

    def verifier(self, support):
        return self.controle(support, *oracle(support))

    def s01(self):
        colonnes = []
        for label in ("Empreinte de la racine", "ID du corps", "Tag I,", "Tag A,", "Tag B,"):
            row = next(l for l in VECTEURS.read_text().splitlines() if l.startswith("| " + label))
            colonnes.append(re.findall(r"\x60([0-9a-f]{64})\x60", row))
        for col, (c, pages) in enumerate(((C1, ANCIEN),(C2, AJOUTS))):
            mesures = [c[176:], identifiant(c)] + [pages[p][-32:] for p in positions(col+1)]
            assert [x.hex() for x in mesures] == [row[col] for row in colonnes]
        self.controle({}, "refus", 0)
        self.controle({4:ANCIEN[4],8:ANCIEN[8]}, "refus", 0)
        self.controle(ANCIEN, "sain", 1)
        for at in (0,8,10,12,16,32,48,64,80,96,104,112,128,160,168,208,209,216,224,4064):
            mauvais = bytearray(ANCIEN[0])
            mauvais[at] ^= 1
            # Pour les champs canoniques, essayer aussi une page resignée par le banc.
            for signe in (False, True):
                if signe:
                    mauvais[-32:] = mac(PROFIL.cles[1], b"NovaFS/PC1/page\0"+mauvais[:-32])
                r = inspecter({0:bytes(mauvais),8:ANCIEN[8]})
                # Un id de transaction modifié et resigné est une autre genèse légitime.
                assert r.etat == ("dégradé" if signe and at in (112,4064) else "refus"), (at,signe)
        self.controle(ANCIEN, "refus", 0, profil=Profil(volume=bytes([19])*16))
        self.controle(ANCIEN, "verrouillé", 0, profil=Profil(maitre=None))
        for invalide in (dict(volume=bytes(16)), dict(codec=b""), dict(maitre=b"x")):
            try:
                Profil(**invalide)
            except ValueError:
                pass
            else:
                raise AssertionError("contexte de confiance invalide")
        self.controle(ANCIEN, "incompatible", 0, profil=Profil(codec=b"INCONNU!"+bytes(8)))
        try:
            image(octets(ANCIEN)[:-1])
        except ValueError:
            pass
        else:
            raise AssertionError("capacité tronquée")

    def s02(self):
        f1 = f2 = False
        for etape, stable, cache, signes, ack in chronologie():
            f1 |= etape == "F1/complétion"
            f2 |= etape == "F2/complétion"
            assert (not signes or f1) and (not ack or f2)
            for n in range(len(cache)+1):
                for survie in itertools.combinations(cache,n):
                    disque = {**stable, **{p:cache[p] for p in survie}}
                    r = self.verifier(disque)
                    assert not ack or (r.rang == 2 and r.etat == "sain")
        assert ack
        for reussi in (False, True):
            e = Exclusivite(dict(NOUVEAU))
            assert e.activer("premier", barriere=reussi) == reussi
            self.controle(e.stable, "sain", 2)
        for frontiere in ("avant", "pendant", "après"):
            e = Exclusivite(dict(ANCIEN))
            e.cache.update(AJOUTS)
            assert not e.activer("nouveau", coupure=frontiere)
            self.verifier(e.stable)
            assert e.titulaire is None
        # Le lecteur n'a aucun état réutilisable après une coupure à une lecture du scan.
        class Interrompu(dict):
            def get(self, p, valeur):
                if self.restant == 0:
                    raise InterruptedError
                self.restant -= 1
                return super().get(p, valeur)

        for lecture in range(3*J+1):
            support = Interrompu(NOUVEAU)
            support.restant = lecture
            try:
                inspecter(support)
            except InterruptedError:
                pass
            else:
                raise AssertionError("interruption de lecture attendue")
            self.controle(dict(support), "sain", 2)

    def s03(self):
        for role, p in enumerate(positions(2)):
            fond = dict(ANCIEN)
            fond[9] = AJOUTS[9]
            if role:
                fond[5] = AJOUTS[5]
            page = AJOUTS[p]
            def partiel(debut, fin):
                v = bytearray(ZERO)
                v[debut:fin] = page[debut:fin]
                self.verifier({**fond, p:bytes(v)})
            for taille in (1,32,64,96,128,512,4096):
                partiel(0, taille)
                partiel(PAGE-taille, PAGE)
            for milieu in (208,216,512,4064):
                partiel(milieu-1,milieu+1)
                partiel(milieu-16,milieu+16)
            for masque in range(1 << (PAGE // self.s)):
                def conserver(adr, mask):
                    b = AJOUTS[adr]
                    return b"".join(b[o:o+self.s] if mask & (1 << (o//self.s))
                                    else bytes(self.s) for o in range(0,PAGE,self.s))
                self.verifier({**fond, p:conserver(p,masque)})
                if role == 1:
                    self.verifier({**fond, p:conserver(p,masque), 253:conserver(253,~masque)})

    def s04(self):
        for pre in ((5,9), (9,5)):
            for dec in ((253,1),(1,253)):
                for err in ("", "F1", "F2", "EIO-9", "EIO-5", "délai-5", "EIO-253", "EIO-1",
                            "lecture-9", "lecture-5", "lecture-1","lecture-253"):
                    for effets in (0,1,2) if err in ("F1","F2") else (0,):
                        etapes = list(chronologie(pre,dec,err,effets))
                        _, stable, _, signes, ack = etapes[-1]
                        assert ack == (not err)
                        if err in ("F1","EIO-9","EIO-5","délai-5","lecture-9","lecture-5"):
                            assert not signes
                        self.verifier(stable)
        for p in (9,5,8,4):
            manque = dict(NOUVEAU)
            del manque[p]
            if p == 8:
                self.controle(manque, "refus", 0, extrait=1)
            else:
                self.verifier(manque) if p != 4 else self.controle(manque,"dégradé",2)

    def s05(self):
        autre = Profil(volume=bytes([31])*16)
        ca, _ = generation(2, identifiant(C1), autre)
        for role,p in enumerate(positions(2)):
            for corruption in (ZERO, AJOUTS[p][:-1]+bytes([AJOUTS[p][-1]^1]),
                               AJOUTS[positions(2)[(role+1)%3]], autre.enregistrement(ca,role)):
                r = self.controle({**NOUVEAU,p:corruption}, "dégradé", 2)
                assert p in [a[1] for a in r.anomalies]
            self.controle(NOUVEAU,"dégradé",2,erreurs=(p,))
        self.controle(NOUVEAU, "refus", 0, erreurs=(9,))
        r = self.controle(NOUVEAU, "dégradé", 1, erreurs=(9,), extrait=1)
        assert (2,9,"graphe de tête") in r.anomalies

    def s06(self):
        apres_f1 = next(s for n,s,_,_,_ in chronologie() if n == "F1/complétion")
        confirme = next(s for _,s,_,_,ok in chronologie() if ok)
        perte = {p:b for p,b in confirme.items() if p not in (1,253)}
        assert octets(perte) == octets(apres_f1)
        _, mensonger, volatile, _, ack = list(chronologie(mensonge=True))[-1]
        assert ack and set(volatile) == {1,253} and octets(mensonger) == octets(perte)
        self.controle(perte, "indéterminé", 1)
        self.controle(apres_f1, "indéterminé", 1)
        objet = {**ANCIEN,9:AJOUTS[9]}
        assert octets(objet) == octets({p:b for p,b in perte.items() if p != 5})
        self.verifier(objet)
        for faux, disque in (("F2",apres_f1),("intégral",ANCIEN)):
            r = self.verifier(disque)
            assert r.rang < 2
            self.controle(disque, "refus", 0, extrait=2, recu=recu(C2))

    def s07(self):
        for parent in (identifiant(C1),digest(b"autre parent")):
            conflit = PROFIL.corps(2,parent,AJOUTS[9],tx=bytes([8])*16)
            self.controle({**NOUVEAU,253:PROFIL.enregistrement(conflit,2)}, "refus", 0)
        self.controle({**NOUVEAU,**{p:PROFIL.enregistrement(conflit,r)
                                    for r,p in enumerate(positions(2))}}, "refus", 0)
        for p in (0,252):
            assert p in ANCIEN
        v = {p:b for p,b in NOUVEAU.items() if p not in (0,252)}
        self.controle(v,"dégradé",2)
        self.controle(v,"dégradé",1,extrait=1)
        del v[4]
        self.controle(v,"refus",0)
        v = dict(NOUVEAU)
        parent = identifiant(C2)
        for g in (3,4):
            c, a = generation(g,parent)
            parent = identifiant(c)
            v.update(a)
        self.controle(v,"sain",4)
        assert not reserver(v, 5, (16,))
        assert not reserver(NOUVEAU, 3, tuple(range(N)))
        assert not reserver(NOUVEAU, 3, (8,))
        assert reserver(NOUVEAU, 3, (10,))
        assert not reserver(NOUVEAU, 3, (10,), tx=C2[112:128])
        c = PROFIL.corps(2,identifiant(C1),AJOUTS[9],tx=C1[112:128])
        self.controle({**NOUVEAU,**{p:PROFIL.enregistrement(c,r) for r,p in enumerate(positions(2))}},"refus",0)
        self.controle({**ANCIEN,positions(4)[0]:v[positions(4)[0]]},"indéterminé",1)
        self.controle({**ANCIEN,positions(4)[1]:v[positions(4)[1]]},"refus",0)

    def s08_s09(self):
        helper = self.bin / "examples/pc1-controles"
        paires = self.out / f"paires-{self.s}"
        resultat = subprocess.check_output([helper,"créer",paires], text=True).strip()
        assert resultat.startswith("4 paires, 12 demandes, 10 nonces distincts")
        graphe = Graphe(helper, paires)
        self.groupe = "S08"
        campagne(graphe,self.controle,paires)
        self.groupe = "S09"
        # Diagnostic individuel : substitution et déchiffrement sur un vrai graphe scellé.
        from vues import PROFIL_VUES, encoder
        for p in (32,34,36,38):
            w = encoder({"origine":{"destination":graphe.lien(1,p)}},1)
            c = PROFIL_VUES.corps(1,bytes(32),w)
            base = {**graphe.objets,8:w,**{pos:PROFIL_VUES.enregistrement(c,r)
                                              for r,pos in enumerate(positions(1))}}
            self.controle(base,"sain",1,profil=PROFIL_VUES,graphe=graphe.verifier)
            for off in (16,32,72,76,88,112):
                cor = bytearray(base[p]); cor[off] ^= 1
                self.controle({**base,p:bytes(cor)},"refus",0,profil=PROFIL_VUES,graphe=graphe.verifier)
        return resultat

    def s10(self, ancien):
        cibles = (("complet",octets(NOUVEAU)),("préfixe",octets(NOUVEAU)[:252*PAGE]),
                  ("genèse-partielle",octets({0:ANCIEN[0][:96]+ZERO[96:]})),
                  ("fausse-étiquette",b"NOVAP0FS"+octets(NOUVEAU)[8:]))
        for _, data in cibles:
            h = digest(data)
            subprocess.run([self.bin/"examples/pc1-controles","p9",str(self.s)], input=data, check=True)
            for op, extra in (("verify",()),("salvage",("0","/audit")),("salvage",("1","/audit"))):
                r = subprocess.run([self.bin/"novafs",op,str(len(data)//PAGE),str(self.s),"01"*16,"02"*16,*extra],
                                   input=data,capture_output=True)
                assert r.returncode and not r.stdout and "image incohérente" in r.stderr.decode(), r.stderr
            assert digest(data) == h
            self.n["S10"] += 7
        self.controle({}, "refus", 0)
        historique = ancien.read_bytes()
        assert historique.startswith(b"NOVAP0FS") and len(historique) >= N*PAGE
        # Géométrie du journal NVLABTX1 précédent : LV P9 suivi de trois paires.
        # Reconstituer en RAM les deux checkpoints de genèse avec les références de la fixture.
        taille = len(historique)//PAGE
        refs = sorted((historique[i*PAGE+136:i*PAGE+184] for i in (0,1)),key=lambda ref:entier(ref,8))
        assert entier(refs[0],8)+1 == entier(refs[1],8)
        journal = bytearray(6*PAGE)
        for slot in (0,3):
            debut = (b"NVLABTX1" + historique[16:48] + taille.to_bytes(8,"little")
                     + (taille+slot).to_bytes(8,"little") + b"".join(refs))
            debut += bytes(4064-len(debut))
            journal[slot*PAGE:(slot+1)*PAGE] = debut + digest(b"NovaFS.lab.publication.v1\0",debut)
        for ancien_format in (historique,historique+journal):
            try:
                image(ancien_format)
            except ValueError:
                self.n["S10"] += 1
            else:
                raise AssertionError("géométrie de l'ancien profil acceptée")
            self.controle(image(ancien_format[:N*PAGE]), "refus", 0)
        assert digest(ancien.read_bytes()) == digest(historique)

    def s11(self):
        e = Exclusivite(dict(ANCIEN))
        assert e.activer("un")
        assert not e.activer("deux")
        e.differee("un",9,AJOUTS[9])
        e.differee("un",5,AJOUTS[5])
        assert not e.activer("deux", evince=True, drainage=False)
        assert not e.activer("deux", evince=True)
        assert not e.differee("un",1,AJOUTS[1]) and not e.differee("deux",1,AJOUTS[1])
        assert e.file == [] and e.titulaire is None
        fige = {**e.stable, **e.cache}
        self.controle(fige,"indéterminé",1)
        c = PROFIL.corps(2,identifiant(C1),AJOUTS[9],tx=b"x"*16)
        assert fige[5][:208] != c and e.signables(c) == ()
        # Même avec une identité d'écrivain, l'intention authentique fixe le corps.
        titulaire = Exclusivite(dict(fige))
        titulaire.titulaire = "instance suspendue après F1"
        assert not titulaire.signables(c) and len(titulaire.signables(C2)) == 2
        self.verifier(e.stable)


def recu(c):
    return c[16:64], entier(c,104), identifiant(c), c[112:128]


def reserver(pages,g,places,tx=None):
    r = inspecter(pages)
    return (r.etat == "sain" and r.rang+1 == g and g <= J and bool(places)
            and (tx is None or all(inspecter(pages,extrait=n).corps[112:128] != tx for n in range(1,g)))
            and len(set(places)) == len(places) and all(2*J <= p < N-J and pages.get(p,ZERO) == ZERO for p in places)
            and all(pages.get(p,ZERO) == ZERO for p in positions(g)))


class Exclusivite:
    def __init__(self, stable):
        self.stable = stable
        self.titulaire = None
        self.file = []
        self.cache = {}

    def activer(self, nom, evince=False, drainage=True, barriere=True, coupure=None):
        if self.titulaire is not None and not evince:
            return False
        self.titulaire = None
        if not drainage:
            return False
        for p,b in self.file:
            self.cache[p] = b
        self.file.clear()
        avant = inspecter({**self.stable, **self.cache})
        if avant.etat != "sain" or coupure == "avant":
            return False
        for p in list(self.cache):
            self.stable[p] = self.cache.pop(p)
            if coupure == "pendant":
                return False
        if not barriere or coupure == "après" or inspecter(self.stable) != avant:
            return False
        self.titulaire = nom
        return True

    def differee(self, nom,p,b):
        if self.titulaire != nom:
            return False
        self.file.append((p,b))
        return True

    def signables(self, c):
        g = entier(c, 104)
        p = positions(g)[0]
        w = self.stable.get(entier(c, 160), ZERO)
        if (self.titulaire is None or PROFIL.decoder(self.stable.get(p, ZERO), p, g, 0) != c
                or w != racine(g) or PROFIL.empreinte(entier(c, 160), g, w) != c[176:208]):
            return ()
        return tuple(PROFIL.enregistrement(c,r) for r in (1,2))


def croiser(index, sortie):
    lignes = json.loads(index.read_text())["essais"]
    arch = {"arm64":"aarch64","x86_64":"x86_64"}[platform.machine()]
    auxiliaire = ROOT/f"novaFS/target/{arch}-apple-darwin/release/examples/pc1-controles"
    resultats = []
    for row in lignes:
        data = (index.parent/row["fichier"]).read_bytes()
        h = digest(data).hex()
        assert h == row["sha256"]
        pages = image(data)
        r = inspecter(pages, recu=recu(C2) if row["ack"] else None)
        assert (r.etat,r.rang) == (row["etat"],row["rang"]) == oracle(pages)
        rust = subprocess.check_output([auxiliaire,"vecteur"],input=data).decode().strip()
        assert rust == f"{r.etat} {r.rang}"
        if row["ack"]:
            assert r.corps == C2
            cor = {**pages,1:ZERO}
            assert inspecter(cor).etat == "dégradé" and inspecter(cor).corps == C2
            assert inspecter({**cor,253:ZERO}).etat == "indéterminé"
            assert subprocess.check_output([auxiliaire,"vecteur"],input=octets(cor)).decode().strip() == "dégradé 2"
        assert digest((index.parent/row["fichier"]).read_bytes()).hex() == h
        resultats.append([row["nom"],h,r.etat,r.rang])
    sortie.write_text(json.dumps(resultats,ensure_ascii=False)+"\n")


def main():
    a = argparse.ArgumentParser(description=__doc__)
    a.add_argument("sortie",type=Path)
    a.add_argument("--ancien",type=Path)
    a.add_argument("--croiser",type=Path)
    a.add_argument("--echanger",type=Path)
    p = a.parse_args()
    assert __debug__
    if p.croiser:
        return croiser(p.croiser,p.sortie)
    arch = {"arm64":"aarch64","x86_64":"x86_64"}[platform.machine()]
    if p.echanger:
        from vues import PROFIL_VUES, consulter
        resultat = {}
        for s in (512,4096):
            dossier = p.echanger/f"paires-{s}"
            graph = Graphe(ROOT/f"novaFS/target/{arch}-apple-darwin/release/examples/pc1-controles", dossier)
            for op in ("cp","mv"):
                f = dossier/(op+".pc1")
                h = digest(f.read_bytes())
                pages = image(f.read_bytes())
                r = inspecter(pages,PROFIL_VUES,graphe=graph.verifier)
                assert r.etat == "sain" and r.rang == 4
                assert consulter(pages,graph,"clone","destination",7) == graph.lien(2,38,102)
                assert consulter(pages,graph,"snapshot","destination",7) == graph.lien(2,36)
                r2 = inspecter(pages,PROFIL_VUES,graphe=graph.verifier,extrait=2)
                assert r2.etat == "sain" and r2.rang == 2
                assert h == digest(f.read_bytes())
                resultat[f"{s}-{op}"] = h.hex()
        p.sortie.write_text(json.dumps(resultat)+"\n")
        return
    p.sortie.mkdir()
    res = {}
    for s in (512,4096):
        b = Banc(s,p.sortie,arch)
        for i in (1,2,3,4,5,6,7,8,10,11):
            b.groupe = f"S{i:02}"
            if i == 8:
                crypto = b.s08_s09()
            elif i == 10:
                b.s10(p.ancien / f"{s}-cp/avant.raw")
            else:
                getattr(b,b.groupe.lower())()
        res[s] = {"inspections":dict(sorted(b.n.items())), "unités":crypto}
    (p.sortie/"simulation.json").write_text(json.dumps(dict(architecture=arch,secteurs=res),
        indent=2,ensure_ascii=False)+"\n")
    print(json.dumps(res,ensure_ascii=False))


if __name__ == "__main__":
    main()
