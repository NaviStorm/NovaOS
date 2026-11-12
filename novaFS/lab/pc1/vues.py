"""Catalogue de vues réservé à S08/S09. Les paires référencées utilisent le codec NovaFS."""
import json
import subprocess
from lecture import (CATALOGUE, J, N, PAGE, ZERO, Profil, digest, identifiant, inspecter,
                     octets, positions)

PROFIL_VUES = Profil(codec=CATALOGUE)


def encoder(vues, g):
    b = json.dumps([g, vues], sort_keys=True, separators=(",", ":")).encode()
    return len(b).to_bytes(4, "little") + b + bytes(PAGE - 4 - len(b))


def decoder(w):
    n = int.from_bytes(w[:4], "little")
    if not 1 <= n <= PAGE - 4 or any(w[4+n:]):
        raise ValueError("taille du catalogue")
    return json.loads(w[4:4+n])


class Graphe:
    def __init__(self, controle, dossier):
        self.controle = controle
        self.objets = {}
        self.refs = {}
        for p in (32, 34, 36, 38):
            paire = (dossier / f"{p}.paire").read_bytes()
            assert len(paire) == 2 * PAGE
            self.objets[p], self.objets[p+1] = paire[:PAGE], paire[PAGE:]
            self.refs[p] = digest(b"NovaFS/P7/paire/v1", paire).hex()

    def verifier(self, w, g, support, erreurs):
        try:
            rang, vues = decoder(w)
            if rang != g or not isinstance(vues, dict) or not vues or encoder(vues, g) != w:
                return False
            vus = {}
            for nom, liens in vues.items():
                if nom not in ("origine", "snapshot", "clone") or not isinstance(liens, dict):
                    return False
                ids = set()
                for chemin, lien in liens.items():
                    inode, date, uid, mode, p, naissance, empreinte = lien
                    if (chemin not in ("source", "destination") or not all(type(n) is int for n in lien[:6])
                            or inode < 1 or inode in ids or date < 1 or uid != 7 or mode != 0o600
                            or naissance != 1 or p not in self.refs or not 2*J <= p < N-J-1
                            or p in erreurs or p+1 in erreurs or p not in support or p+1 not in support
                            or p in vus and vus[p] != empreinte):
                        return False
                    ids.add(inode)
                    if p not in vus:
                        paire = support[p] + support[p+1]
                        if digest(b"NovaFS/P7/paire/v1", paire).hex() != empreinte:
                            return False
                        r = subprocess.run([self.controle, "ouvrir", str(p), empreinte], input=paire)
                        if r.returncode:
                            return False
                    vus[p] = empreinte
            return True
        except (ValueError, TypeError, KeyError, IndexError):
            return False

    def lien(self, inode, p, date=100):
        return [inode, date, 7, 0o600, p, 1, self.refs[p]]


def consulter(pages, graph, vue, nom, uid):
    r = inspecter(pages, PROFIL_VUES, graphe=graph.verifier)
    if r.etat not in ("sain", "dégradé"):
        raise ValueError("graphe refusé")
    entree = decoder(pages[7+r.rang])[1][vue][nom]
    if uid != entree[2]:
        raise PermissionError
    return entree


def campagne(graph, controle, sortie):
    from copy import deepcopy
    horloge = 101
    for op in ("cp", "mv"):
        g1 = {"source": graph.lien(1, 34), "destination": graph.lien(2, 36)}
        copie = {"destination": graph.lien(3 if op == "cp" else 1, 34, horloge)}
        if op == "cp":
            copie["source"] = g1["source"]
        vues = {"origine": deepcopy(g1), "snapshot": deepcopy(g1), "clone": deepcopy(g1)}
        support = dict(graph.objets)
        parent = bytes(32)
        for g in range(1, 5):
            if g == 2:
                vues["origine"] = deepcopy(copie)
            if g == 3:
                vues["clone"]["destination"] = graph.lien(2, 38, horloge + 1)
            if g == 4:
                del vues["origine"]
            w = encoder(vues, g)
            c = PROFIL_VUES.corps(g, parent, w)
            page = 7 + g
            support[page] = w
            if g > 1:
                controle(support, "sain", g-1, profil=PROFIL_VUES, graphe=graph.verifier)
            for role, p in enumerate(positions(g)):
                support[p] = PROFIL_VUES.enregistrement(c, role)
                if g > 1 and role != 2:
                    r = controle(support, "indéterminé" if role == 0 else "dégradé",
                                 g-1 if role == 0 else g, profil=PROFIL_VUES, graphe=graph.verifier)
                    assert decoder(support[int.from_bytes(r.corps[160:168], "little")])[1] == (
                        vues if role == 1 else precedent)
            recu = (c[16:64], g, identifiant(c), c[112:128])
            r = controle(support, "sain", g, profil=PROFIL_VUES, graphe=graph.verifier, recu=recu)
            assert r.corps == c and decoder(support[page])[1] == vues
            if g >= 2:
                historique = controle(support, "sain", 1, profil=PROFIL_VUES, graphe=graph.verifier, extrait=1)
                assert historique.rang == 1 and decoder(support[8])[1]["origine"] == g1
                assert consulter(support, graph, "snapshot", "destination", 7) == g1["destination"]
                if g < 4:
                    assert consulter(support, graph, "origine", "destination", 7) == copie["destination"]
                # L'appelant a perdu sa réponse. Il connaît seulement l'identifiant de son opération.
                tx = bytes([7])*16
                trouves = [v for n in range(1,g+1)
                           for v in [inspecter(support,PROFIL_VUES,graphe=graph.verifier,extrait=n)]
                           if v.corps[112:128] == tx]
                assert len(trouves) == 1 and trouves[0].rang == 2
                try:
                    consulter(support, graph, "clone", "destination", 8)
                except PermissionError:
                    pass
                else:
                    raise AssertionError("lecteur sans droit")
            parent = identifiant(c)
            precedent = deepcopy(vues)
        (sortie / (op + ".pc1")).write_bytes(octets(support))
        assert vues["clone"]["destination"] == graph.lien(2, 38, 102)
        assert all(support[p] == b for p,b in graph.objets.items())
        for p in (34, 35, 36, 37, 38, 39):
            perdu = {**support, p: ZERO}
            controle(perdu, "refus", 0, profil=PROFIL_VUES, graphe=graph.verifier)
        controle({**support, 35:support[39]}, "refus", 0, profil=PROFIL_VUES, graphe=graph.verifier)
    # Les quatre combinaisons clair/scellé et brut/LZ4 traversent le lecteur de graphe.
    for p in (32, 34, 36, 38):
        w = encoder({"origine": {"source": graph.lien(1, p)}}, 1)
        c = PROFIL_VUES.corps(1, bytes(32), w)
        pages = {**graph.objets, 8:w}
        for role, pos in enumerate(positions(1)):
            pages[pos] = PROFIL_VUES.enregistrement(c, role)
        controle(pages, "sain", 1, profil=PROFIL_VUES, graphe=graph.verifier)
        controle(pages, "refus", 0, profil=PROFIL_VUES, graphe=graph.verifier, erreurs=(p+1,))
