"""Événements du banc R2 : journal conservé, curseur série indépendant.

Observation seulement. Aucun message, droit ou renouvellement de bail invité.
"""
import re
from reprise import Refus


class Events:
    def __init__(self, serial, start):
        self.serial, self.start = serial, start
        self.records, self.used = [], set()

    @classmethod
    def before_ready(cls, serial, pattern, seconds):
        # Le Serial appartient à un seul démarrage. Conserver sa frontière
        # avant de consommer le marqueur prêt, jamais celle d'une ancienne VM.
        events = cls(serial, serial.offset)
        serial.expect(pattern, seconds)
        return events

    def pattern(self, message, pid=None):
        actor = rb'(\d+)' if pid is None else str(pid).encode()
        return rb'DEBUG pid=' + actor + b' ' + re.escape(message) + rb'\r?\n'

    def actor(self, message):
        matches = list(re.finditer(self.pattern(message), self.serial.data[self.start:]))
        if len(matches) != 1:
            raise Refus('acteur de l’événement absent ou ambigu')
        return int(matches[0][1])

    def expect(self, pid, message, seconds):
        key = (pid, message)
        if key in self.used:
            raise Refus('étape du scénario déjà consommée')
        pattern = self.pattern(message, pid)
        saved = self.serial.offset
        try:
            # Rechercher aussi les événements reçus pendant une commande.
            # Le curseur de commande ne perd ni ne reconsomme son propre flux.
            self.serial.offset = self.start
            self.serial.expect(pattern, seconds)
            matches = list(re.finditer(pattern, self.serial.data[self.start:]))
            if len(matches) != 1:
                raise Refus('événement du scénario ambigu')
            position = self.start + matches[0].start()
            self.used.add(key)
            self.records.append({'pid': pid, 'etape': message.decode(),
                                 'position_trace': position,
                                 'deja_recu': position < saved})
        finally:
            self.serial.offset = saved
