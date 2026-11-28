"""Autorité privée d'une VM mono-vCPU. Les preuves matérielles sont acquises par le courtier."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Bail:
    epoque: bytes
    titulaire: str
    rid: int
    domaine: int
    incarnation: int
    generation: int
    pages: tuple


@dataclass(frozen=True)
class Recu:
    bail: Bail
    sequence: int
    cookie: int


class Autorite:
    def __init__(self, bail):
        if (
            type(bail.epoque) is not bytes
            or type(bail.pages) is not tuple
            or len(bail.epoque) != 16
            or not any(bail.epoque)
            or not bail.titulaire
            or not 0 <= bail.rid <= 65535
            or not 0 < bail.domaine <= 65535
            or min(bail.incarnation, bail.generation) <= 0
            or not bail.pages
            or len(set(bail.pages)) != len(bail.pages)
            or any(
                type(p) is not int or not 0 < p < 1 << 64 or p % 4096
                for p in bail.pages
            )
        ):
            raise ValueError("Bail invalide")
        self.bail = bail
        self.etat = "ouvert"
        self.sequence = 0
        self.attendu = None
        self.recu = None
        self.invalidation = None

    def soumettre(self, preuve_attendue):
        if self.etat != "ouvert" or self.attendu is not None or not preuve_attendue:
            raise ValueError("Admission fermée ou commande pendante")
        self.sequence += 1
        self.attendu = preuve_attendue

    def fermer(self):
        if self.etat != "ouvert":
            raise ValueError("Clôture déjà engagée")
        self.etat = "drainage"

    def achever(self, occupe, resultat):
        if self.attendu is None or occupe or resultat != self.attendu:
            raise ValueError("Achèvement non démontré")
        self.attendu = None

    def drainer(self):
        if self.etat != "drainage" or self.attendu is not None:
            raise ValueError("Transaction encore en vol")
        self.etat = "invalidation"

    def preparer_invalidation(self, cookie, queue):
        if (
            self.etat != "invalidation"
            or self.invalidation is not None
            or not 0 < cookie <= 0xFFFFFFFF
            or not 0 < queue < 4096
            or queue % 16
        ):
            raise ValueError("Commande d'invalidation incohérente")
        self.invalidation = (cookie, queue)

    def confirmer(self, marque, tete, queue, defaut, tables_ok):
        if self.etat == "invalidation" and (defaut or not tables_ok):
            self.etat = "quarantaine"
        if (
            self.etat != "invalidation"
            or self.invalidation != (marque, queue)
            or tete != queue
        ):
            raise ValueError("Invalidation non confirmée")
        self.recu = Recu(self.bail, self.sequence, marque)
        self.etat = "clos"
        return self.recu

    def restituer(self, recu, nettoyer):
        # L'objet authentique n'est délivré qu'après relecture matérielle.
        if self.etat != "clos" or recu is not self.recu or recu.bail != self.bail:
            raise ValueError("Reçu absent, étranger ou consommé")
        self.recu = None
        self.etat = "quarantaine"
        for page in self.bail.pages:
            if nettoyer(page) is not True:
                raise ValueError("Effacement non confirmé")
        self.etat = "rendu"
        return self.bail.pages
