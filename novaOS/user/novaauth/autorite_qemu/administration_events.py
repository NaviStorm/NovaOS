"""Événements du banc R2 : journal conservé, curseur série indépendant.

Observation seulement. Aucun message, droit ou renouvellement de bail invité.
"""
import re
from reprise import Refus


def command_status_pattern(step):
    # __FIN termine déjà le statut. CONSOLE et DEBUG peuvent s'entrelacer
    # avant le LF suivant ; les octets des ELF restent collectés séparément.
    if re.fullmatch(r'NXR2_[0-9]+_', step) is None:
        raise Refus('étape de commande invalide')
    return re.escape(step.encode()) + rb'(\d+)__FIN'


def observed_command_status_pattern(step):
    # Émission DEBUG atomique ; le PID est fourni par le noyau, pas par argv.
    return rb'(?m)^DEBUG pid=(\d+) ' + command_status_pattern(step) + rb'\r?\n'


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


def membership_revision_pattern(phase, origin, code):
    if phase not in ('avant', 'apres', 'reprise', 'en-vol', 'apres-en-vol') or origin not in ('Direct', 'G', 'H') or code not in (0, 10):
        raise Refus('étape VFS invalide')
    prefix = f'NARM Q1 phase={phase} origine={origin} native={code} octets={0 if code else 12} revision='
    # Une lecture série peut s'arrêter après le premier chiffre d'une révision.
    # Attendre sa délimitation ; ne jamais accepter ce préfixe comme la valeur.
    return re.escape(prefix.encode()) + rb'(\d+)\r?\n'


def removal_race_results(data, generation):
    """Même intention confirmée concurremment : succès unique, préparation consommée."""
    if len(data)!=192:raise ValueError('deux resultats de retrait absents')
    rows=[];ids=[]
    for offset in (0,96):
        p=int.from_bytes(data[offset:offset+8],'little');c=int.from_bytes(data[offset+8:offset+16],'little')
        receipt=data[offset+16:offset+80];operation=data[offset+80:offset+96]
        if p!=0 or c not in (0,5) or operation==bytes(16):raise ValueError('statuts natifs de confirmation concurrente inattendus')
        if c==0:
            if receipt[:8]!=bytes(8) or receipt[8:24]!=operation or int.from_bytes(receipt[24:32],'little')!=generation or receipt[32]!=1 or receipt[33:]!=bytes(31):raise ValueError('recu terminal de retrait non canonique')
        elif receipt!=(5).to_bytes(8,'little')+bytes(56):raise ValueError('refus apres consommation non canonique')
        ids.append(operation);rows.append({'preparation':p,'confirmation':c,'operation':operation.hex(),'recu':receipt.hex()})
    if ids[0]!=ids[1] or sorted(r['confirmation'] for r in rows)!=[0,5]:raise ValueError('meme intention et confirmation unique non demontrees')
    return rows
