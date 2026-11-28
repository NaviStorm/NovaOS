"""Modèle fini du retrait avec ATS. Les événements proviennent exclusivement des tests hôtes."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Bail:
    epoque: int
    titulaire: int
    incarnation: int
    generation: int
    domaine: int
    # Ensemble exhaustif de (RID, PASID ou None) attesté à l'admission.
    caches: frozenset


class Retrait:
    def __init__(self, bail, sequence):
        if (
            not isinstance(bail.caches, frozenset)
            or not 1 <= len(bail.caches) <= 32
            or min(
                bail.epoque, bail.titulaire, bail.incarnation, bail.generation, sequence
            )
            <= 0
            or not 0 < bail.domaine < 65536
            or any(
                not 0 <= rid < 65536 or pasid is not None and not 0 <= pasid < 1 << 20
                for rid, pasid in bail.caches
            )
        ):
            raise ValueError("Identité, portée ou géométrie inconnue")
        self.bail = bail
        self.sequence = sequence
        self.phase = "ouvert"
        self.pendants = set()

    def fermer(self):
        if self.phase != "ouvert":
            return False
        self.phase = "drainage"
        return True

    def retirer(self, *, fin_transactions, portee_exclusive, contexte_et_iotlb):
        if self.phase != "drainage" or not fin_transactions:
            return False
        if not portee_exclusive or not contexte_et_iotlb:
            self.panne()
            return False
        self.pendants = set(self.bail.caches)
        self.phase = "invalidation"
        return True

    def reponse_atc(self, bail, sequence, cache, *, achevement_endpoint):
        if self.phase != "invalidation" or (bail, sequence) != (
            self.bail,
            self.sequence,
        ):
            return False
        if cache not in self.pendants or not achevement_endpoint:
            return False
        self.pendants.remove(cache)
        return True

    def rendre(self, bail, sequence, *, wait_ordonne):
        if (
            self.phase != "invalidation"
            or self.pendants
            or not wait_ordonne
            or (bail, sequence) != (self.bail, self.sequence)
        ):
            return False
        self.phase = "rendu"
        return True

    def panne(self):
        if self.phase != "rendu":
            self.phase = "quarantaine"
