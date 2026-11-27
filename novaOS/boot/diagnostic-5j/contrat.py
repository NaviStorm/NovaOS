"""Vérificateur hors invité. Le producteur de confiance conserve ses baux depuis le reset."""

from dataclasses import dataclass
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "diagnostic-5i"))
from domaines import legacy


@dataclass(frozen=True)
class Bail:
    unite: int
    rid: int
    domaine: int
    # Triplets (IOVA, cadre physique, droits directionnels).
    feuilles: tuple


@dataclass(frozen=True)
class Constat:
    base: int
    registres: dict
    tables: dict
    # Pages de tables privées de l'autorité, inventoriées indépendamment du RTADDR.
    cadres_tables: frozenset
    # Tous les RID couverts, y compris les fonctions sans contexte.
    demandeurs: frozenset


def verifier(
    epoque,
    epoque_courante,
    zones,
    unites,
    baux,
    demandeurs,
    rmrr,
    attribution_complete,
    en_vol_inconnu,
):
    """Les entrées proviennent de l'autorité, jamais d'un champ déclaratif du PE."""
    if (
        not isinstance(epoque, bytes)
        or len(epoque) != 16
        or not any(epoque)
        or epoque != epoque_courante
        or not attribution_complete
        or en_vol_inconnu
    ):
        raise ValueError("Quarantaine : époque, attribution ou transaction incertaine")
    if not 1 <= len(unites) <= 16 or len(baux) > 256 or not 1 <= len(zones) <= 128:
        raise ValueError("Inventaire absent ou hors bornes")
    if len(demandeurs) > 256 or len(rmrr) > 256:
        raise ValueError("Topologie hors bornes")

    def intervalle(z):
        a, b = z
        if a < 0 or a >= b or b > 1 << 64 or (a | b) & 4095:
            raise ValueError("Plage non canonique")
        return a, b

    protege = sorted(intervalle(z) for z in zones)
    if any(x[1] > y[0] for x, y in zip(protege, protege[1:])):
        raise ValueError("Zones protégées ambiguës")

    def recoupe(page, spans):
        return any(a <= page < b for a, b in spans)

    reserves = [intervalle(r) for r in rmrr]
    if any(a < d and c < b for a, b in reserves for c, d in protege):
        raise ValueError("RMRR recouvrante")
    # Profil fermé. Une réservation même disjointe demande un bail firmware
    # et une preuve de maintien du mapping identité, hors de ce contrat.
    if reserves:
        raise ValueError("Bail RMRR absent")
    bases = {u.base for u in unites}
    if len(bases) != len(unites) or any(x <= 0 or x & 4095 for x in bases):
        raise ValueError("Registres remappeurs ambigus")
    affectes = set()
    cadres = set()
    for u in unites:
        if (
            not 1 <= len(u.tables) <= 64
            or any(not 0 < f < 1 << 48 or f & 4095 for f in u.cadres_tables)
            or u.demandeurs & affectes
            or set(u.tables) != u.cadres_tables
            or any(recoupe(f, protege) for f in u.cadres_tables)
            or cadres & u.cadres_tables
        ):
            raise ValueError("Couverture RID ou propriété des tables ambiguë")
        cadres.update(u.cadres_tables)
        affectes.update(u.demandeurs)
    if affectes != set(demandeurs) or any(not 0 <= sid <= 65535 for sid in affectes):
        raise ValueError("Demandeurs non réconciliés")
    attendus = {}
    data = set()
    for b in baux:
        cle = (b.unite, b.rid)
        if cle in attendus or b.unite not in bases or len(b.feuilles) > 4096:
            raise ValueError("Bail contradictoire")
        if len(data) + len(b.feuilles) > 4096:
            raise ValueError("Budget global de feuilles")
        for iova, frame, rights in b.feuilles:
            if (
                not 0 <= iova < 1 << 48
                or not 0 < frame < 1 << 48
                or (iova | frame) & 4095
                or frame in data
                or frame in cadres
                or recoupe(frame, protege)
                or rights not in (1, 2, 3)
            ):
                raise ValueError("Droits ou cadre incompatibles")
            data.add(frame)
        attendus[cle] = (b.domaine, tuple(sorted(b.feuilles)))
    observes = {}
    for u in unites:
        for sid, dom in legacy(u.registres, u.tables, data).items():
            if sid not in u.demandeurs:
                raise ValueError("Contexte hors groupe")
            feuilles = tuple(
                sorted((m["iova"], m["cadre"], m["droits"]) for m in dom["mappings"])
            )
            observes[u.base, sid] = (dom["did"], feuilles)
    if observes != attendus:
        raise ValueError("Permissions ou mappings hérités contradictoires")
    return len(cadres) * 4096
