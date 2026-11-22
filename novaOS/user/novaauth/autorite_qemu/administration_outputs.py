"""Collecteur R2 exclusivement hôte : aucune remise de handle à une session.

La sonde noyau observe les RECEIVE effectivement livrés. Aucune lecture sur la
console d'Alice, aucun DEBUG_WRITE de Bob, aucune écriture GDB dans l'invité.
Un relevé de flux ne prouve ni WAIT, ni révocation, ni restitution des ressources.
"""
from dataclasses import dataclass, field
import struct


class Refus(ValueError):
    pass


WIRE = 272
HEADER = struct.Struct('<8sHHBBHQQ32s32s32sQQ128s')
assert HEADER.size == WIRE


@dataclass(frozen=True)
class Binding:
    shell: int
    child: int
    shell_instance: bytes
    child_instance: bytes
    session: bytes
    endpoint: int
    fd: int


@dataclass
class Stream:
    binding: Binding
    sequence: int = 0
    data: bytearray = field(default_factory=bytearray)
    closed: bool = False


class Collector:
    """Récepteur de preuves, jamais une autorité de session ou de lancement."""
    def __init__(self, sessions, byte_limit=1024 * 1024):
        # Triplets relus par le banc sur les routes noyau, jamais depuis argv.
        sessions = tuple(sessions)
        self.sessions = frozenset(sessions)
        if len(sessions) != len(self.sessions):
            raise Refus("session dupliquée")
        if not self.sessions or len(self.sessions) > 2 or not 0 < byte_limit <= 1024 * 1024:
            raise Refus('configuration de collecte invalide')
        for pid, instance, sid in self.sessions:
            if pid <= 0 or len(instance) != 32 or not any(instance) or len(sid) != 32 or not any(sid):
                raise Refus('identité de session invalide')
        if len({p for p, _, _ in self.sessions}) != len(self.sessions) or len({i for _, i, _ in self.sessions}) != len(self.sessions) or len({s for _, _, s in self.sessions}) != len(self.sessions):
            raise Refus('sessions non indépendantes')
        self.streams = {}
        self.byte_limit = byte_limit
        self.bytes = 0

    def accept(self, record):
        if len(record) != WIRE:
            raise Refus('taille de sonde invalide')
        magic, version, size, event, fd, length, shell, child, instance, child_instance, sid, endpoint, seq, payload = HEADER.unpack(record)
        if magic != b'NXR2OUT1' or version != 1 or size != WIRE or event not in (0, 1, 2) or fd not in (1, 2):
            raise Refus('format de sonde invalide')
        if not 0 < child or child == shell or not any(child_instance) or not endpoint:
            raise Refus('identité enfant invalide')
        if length > 128 or any(payload[length:]) or (event != 1 and length) or (event == 1 and length == 0):
            raise Refus('payload non canonique')
        if (shell, instance, sid) not in self.sessions:
            raise Refus('sortie étrangère à la session attendue')
        binding = Binding(shell, child, instance, child_instance, sid, endpoint, fd)
        stream = self.streams.get(endpoint)
        if event == 0:
            if stream is not None or seq != 0 or len(self.streams) >= 256 or sum(not s.closed for s in self.streams.values()) >= 8:
                raise Refus('liaison répétée ou saturée')
            for other in self.streams.values():
                b = other.binding
                if b.child == child and (b.shell, b.shell_instance, b.child_instance, b.session) != (shell, instance, child_instance, sid):
                    raise Refus('enfant associé à deux sessions')
                if not other.closed and (b.child, b.fd) == (child, fd):
                    raise Refus('flux enfant ambigu')
            self.streams[endpoint] = Stream(binding)
            return
        if stream is None or stream.closed or stream.binding != binding or seq != stream.sequence + 1:
            raise Refus('substitution, rejeu ou fragment manquant')
        if event == 1:
            if self.bytes + length > self.byte_limit:
                raise Refus('budget de collecte dépassé')
            stream.data.extend(payload[:length])
            self.bytes += length
        else:
            stream.closed = True
        stream.sequence = seq

    def output(self, shell, instance, sid, child, fd):
        """Utilisé uniquement par le banc ; aucun protocole invité ne l'expose."""
        matches = [s for s in self.streams.values() if
                   (s.binding.shell, s.binding.shell_instance, s.binding.session, s.binding.child, s.binding.fd) ==
                   (shell, instance, sid, child, fd)]
        if len(matches) != 1:
            raise Refus('flux absent ou ambigu')
        return bytes(matches[0].data)


def read_stopped(debug):
    """QEMU arrêté sur nova_r2_output_observe ; m lit, jamais M/X ni P/G."""
    if debug.arch == 'aarch64':
        pointer, size = debug.register(0), debug.register(1)  # x0, x1
    elif debug.arch == 'x86_64':
        pointer, size = debug.register(5), debug.register(4)  # rdi, rsi SysV
    else:
        raise Refus('architecture de sonde non admise')
    if not pointer or size != WIRE:
        raise Refus('arguments de sonde invalides')
    raw = debug.command(f'm{pointer:x},{size:x}')
    if len(raw) != WIRE * 2:
        raise Refus('lecture GDB incomplète')
    try:
        result = bytes.fromhex(raw)
        if len(result) != WIRE:
            raise Refus("lecture GDB non canonique")
        return result
    except ValueError as error:
        raise Refus('réponse GDB invalide') from error
