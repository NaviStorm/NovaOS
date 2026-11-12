"""Encodeur de vecteurs et inspection RO de l'étendue PC1, hors moteur NovaFS."""
from dataclasses import dataclass
from hashlib import sha256
import hmac
import struct

PAGE = 4096
ZERO = bytes(PAGE)
N = 256
J = N // 64
M = bytes(range(32))
FS, VOL, LIG, KID = (bytes([i]) * 16 for i in range(1, 5))
VECTEUR = b"NVTST001" + bytes(8)
CATALOGUE = b"NVCATLAB" + bytes(8)


def le(n):
    return struct.pack("<Q", n)


def entier(b, p):
    return struct.unpack_from("<Q", b, p)[0]


def digest(*parties):
    h = sha256()
    for p in parties:
        h.update(p)
    return h.digest()


def mac(k, data):
    return hmac.digest(k, data, "sha256")


def identifiant(c):
    return digest(b"NovaFS/PC1/id\0", c)


def positions(g):
    return (J + g - 1, g - 1, N - J + g - 1)


def racine(g):
    return b"NVTST001" + le(g) + bytes(PAGE - 16)


class Profil:
    def __init__(self, volume=VOL, codec=VECTEUR, maitre=M):
        if any(len(v) != 16 or v == bytes(16) for v in (volume, codec)):
            raise ValueError("identifiant de profil invalide")
        if maitre is not None and len(maitre) != 32:
            raise ValueError("taille de maîtresse invalide")
        self.contexte = FS + volume + LIG + KID + codec
        self.codec = codec
        self.cles = None
        if maitre is not None:
            prk = mac(self.contexte[:48], maitre)
            self.cles = tuple(mac(prk, b"NovaFS/PC1/" + nom + b"-key\0" + KID + b"\1")
                              for nom in (b"intent", b"decision"))

    def empreinte(self, p, g, page):
        return digest(b"NovaFS/PC1/root\0", self.contexte[:48], self.codec, le(p), le(g), page)

    def corps(self, g, parent, page, p=None, tx=None):
        p = 7 + g if p is None else p
        tx = bytes([5 + g]) * 16 if tx is None else tx
        return (b"NVPUBC01" + struct.pack("<HHI", 1, 1, 0) + self.contexte + le(N) + le(g)
                + tx + parent + le(p) + le(g) + self.empreinte(p, g, page))

    def enregistrement(self, c, role):
        p = positions(entier(c, 104))[role]
        debut = c + bytes([role]) + bytes(7) + le(p) + bytes(3840)
        return debut + mac(self.cles[int(role != 0)], b"NovaFS/PC1/page\0" + debut)

    def decoder(self, page, p, g, role):
        if (len(page) != PAGE or page[:16] != b"NVPUBC01\1\0\1\0\0\0\0\0"
                or page[16:96] != self.contexte or entier(page, 96) != N
                or entier(page, 104) != g or page[112:128] == bytes(16)
                or not 2 * J <= entier(page, 160) < N - J or entier(page, 168) != g
                or (page[128:160] == bytes(32)) != (g == 1)
                or page[208:216] != bytes([role]) + bytes(7) or entier(page, 216) != p
                or any(page[224:4064])):
            return None
        tag = mac(self.cles[int(role != 0)], b"NovaFS/PC1/page\0" + page[:4064])
        return page[:208] if hmac.compare_digest(tag, page[4064:]) else None


PROFIL = Profil()


def generation(g, parent, profil=PROFIL):
    w = racine(g)
    c = profil.corps(g, parent, w)
    pages = {7 + g: w}
    for role, p in enumerate(positions(g)):
        pages[p] = profil.enregistrement(c, role)
    return c, pages


C1, ANCIEN = generation(1, bytes(32))
C2, AJOUTS = generation(2, identifiant(C1))
NOUVEAU = {**ANCIEN, **AJOUTS}


def octets(pages):
    return b"".join(pages.get(i, ZERO) for i in range(N))


def image(data):
    if len(data) != N * PAGE:
        raise ValueError("étendue incompatible")
    return {i: data[i*PAGE:(i+1)*PAGE] for i in range(N) if data[i*PAGE:(i+1)*PAGE] != ZERO}


@dataclass(frozen=True)
class Lecture:
    etat: str
    rang: int = 0
    corps: bytes = b""
    anomalies: tuple = ()


def inspecter(pages, profil=PROFIL, erreurs=(), graphe=None, extrait=None, recu=None):
    """Le fournisseur ne délivre que des pages, sans primitive de correction."""
    if profil.cles is None:
        return Lecture("verrouillé")
    if profil.codec not in (VECTEUR, CATALOGUE) or (profil.codec == CATALOGUE and graphe is None):
        return Lecture("incompatible")
    rangs = {}
    anomalies = []
    traces = set()
    for g in range(1, J + 1):
        roles = {}
        for role, p in enumerate(positions(g)):
            page = pages.get(p, ZERO)
            if p in erreurs:
                anomalies.append((g, p, "EIO"))
            elif page == ZERO:
                continue
            else:
                c = profil.decoder(page, p, g, role)
                if c is not None:
                    roles[role] = c
                else:
                    anomalies.append((g, p, "intégrité"))
            traces.add(g)
        rangs[g] = roles
    tetes = [g for g, roles in rangs.items() if 1 in roles or 2 in roles]
    if not tetes:
        return Lecture("refus", anomalies=tuple(anomalies))
    h = max(tetes)
    if extrait is not None and not 1 <= extrait <= h:
        return Lecture("refus")
    parent = bytes(32)
    transactions = set()
    historique = {}
    for g in range(1, h + 1):
        roles = rangs[g]
        corps = set(roles.values())
        if len(corps) != 1:
            return Lecture("refus", anomalies=((g, -1, "filiation ou conflit"),))
        c = corps.pop()
        if c[128:160] != parent or c[112:128] in transactions:
            return Lecture("refus", anomalies=((g, -1, "parent ou transaction"),))
        parent = identifiant(c)
        transactions.add(c[112:128])
        historique[g] = c
        for role, p in enumerate(positions(g)):
            if role not in roles and not any(a[1] == p for a in anomalies):
                anomalies.append((g, p, "absent"))
    k = h if extrait is None else extrait
    c = historique[k]
    def graphe_valide(rang):
        corps = historique[rang]
        pos = entier(corps, 160)
        w = pages.get(pos, ZERO)
        if pos in erreurs or len(w) != PAGE or not hmac.compare_digest(
                corps[176:208], profil.empreinte(pos, rang, w)):
            return False
        return graphe(w, rang, pages, erreurs) if graphe else w == racine(rang)

    p = entier(c, 160)
    if not graphe_valide(k):
        return Lecture("refus", anomalies=((k, p, "graphe"),))
    if k < h and not graphe_valide(h):
        anomalies.append((h, entier(historique[h], 160), "graphe de tête"))
    if recu is not None and recu != (c[16:64], k, identifiant(c), c[112:128]):
        return Lecture("refus", anomalies=((k, p, "reçu"),))
    queue = sorted(g for g in traces if g > h)
    if queue:
        for g in queue:
            for role in rangs[g]:
                anomalies.append((g, positions(g)[role], "queue"))
    etat = "indéterminé" if queue else "dégradé" if anomalies else "sain"
    return Lecture(etat, k, c, tuple(anomalies))
