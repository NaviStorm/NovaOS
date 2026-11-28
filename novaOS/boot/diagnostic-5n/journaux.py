"""Publication hors invité. Huit emplacements fixes. Seul le banc possède le fichier de destination."""

from dataclasses import dataclass
from datetime import datetime, timezone
import fcntl
import json
import os
import secrets
import stat
import subprocess

from demarrage import boot
from support import MIN_MIO, MIO, S, empreinte, require, verify


def read_at(f, area):
    pos, length = area
    f.seek(pos)
    data = f.read(length)
    require(len(data) == length, "Lecture de journal tronquée")
    return data


def sums(data, text):
    return (
        f"{empreinte(data)}  rapport.json\n{empreinte(text)}  JOURNAL.TXT\n"
    ).encode()


def pad(data, size):
    require(len(data) <= size - 1, "Rapport trop grand")
    return data.ljust(size - 1, b" ") + b"\n"


def history(f, areas):
    states = []
    for slot in areas:
        data, text, seal = (read_at(f, p) for p in slot)
        if any(seal):
            require(
                seal == pad(sums(data, text), len(seal)),
                "Sceau ou ancien rapport altéré",
            )
            state = "valide"
        elif any(data) or any(text):
            state = "interrompu"
        else:
            state = "libre"
        states.append((state, empreinte(data + text + seal)))
    return states


def report(observation, boot_id, date=None):
    # Schéma fermé. Aucune chaîne issue du firmware, adresse, GUID, époque signée ou série.
    return dict(
        schema="novaos.diagnostic/5n",
        demarrage=boot_id,
        date_utc=date,
        origine_date="UTC du banc attestée par l’opérateur"
        if date
        else "horloge invitée non attestée",
        verdict="refus",
        motif="Transfert de confiance 5M absent sur USB (16)",
        secure_boot_diagnostic="indéterminé",
        secure_boot_banc=observation["mode_banc"],
        dma_avant_chargement="indéterminée",
        inventaire_source="QMP",
        classes_pci=observation["inventaire_classes_pci"],
        io_interne_pendant_diagnostic=0,
        activation_pilotes=False,
    )


def render(r):
    return (
        "NovaOS | Diagnostic x86_64 de laboratoire\n"
        f"Démarrage {r['demarrage']} | REFUS\n{r['motif']}\n"
        f"Secure Boot du diagnostic : {r['secure_boot_diagnostic']}\n"
        f"Mode du banc : {r['secure_boot_banc']} | DMA précoce USB : {r['dma_avant_chargement']}\n"
        "E/S internes pendant le diagnostic : 0\n"
        "Inventaire PCI du banc (classes) : "
        + ", ".join(f"{c:04x}" for c in r["classes_pci"])
        + "\n"
    )


def sync(f):
    f.flush()
    os.fsync(f.fileno())
    if hasattr(fcntl, "F_FULLFSYNC"):
        fcntl.fcntl(f, fcntl.F_FULLFSYNC)


def publish(f, previous, r, fault=None):
    """La destination dérive exclusivement de la géométrie autorisée relue sur ce fichier."""
    areas = verify(f)
    now = history(f, areas)
    require(previous == now, "Anciens journaux modifiés pendant le démarrage")
    target = next((i for i, (state, _) in enumerate(now) if state == "libre"), None)
    require(target is not None, "NOVALOGS saturée")
    slot = areas[target]
    data = pad(
        (json.dumps(r, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode(),
        slot[0][1],
    )
    text = pad(render(r).encode(), slot[1][1])
    seal = pad(sums(data, text), slot[2][1])
    for step, (area, payload) in enumerate(zip(slot, (data, text, seal)), 1):
        pos, length = area
        require(length == len(payload), "Longueur d’export")
        f.seek(pos)
        if fault == "sceau-dechire" and step == 3:
            f.write(payload[:17])
            sync(f)
            raise OSError("Coupure simulée pendant le sceau")
        if f.write(payload) != length:
            raise OSError("Écriture courte")
        sync(f)
        require(read_at(f, area) == payload, "Relecture du secteur")
        if fault == step:
            raise OSError("Interruption simulée après synchronisation")
    final = history(f, verify(f))
    require(
        final[target][0] == "valide"
        and all(a == b for i, (a, b) in enumerate(zip(now, final)) if i != target),
        "Postcondition des journaux",
    )
    return f"BOOT{target + 1:04d}"


@dataclass
class Result:
    observation: dict
    rapport: dict
    emplacement: str = ""
    refus_export: str = ""


class SupportPrive:
    """Le banc crée une copie neuve dans un répertoire privé et en garde le descripteur."""

    def __init__(self, folder, source, size):
        folder.mkdir(mode=0o700)
        self.path = folder / "usb.img"
        # Création exclusive avant dd. Aucune cible existante, y compris un périphérique.
        self.f = self.path.open("x+b", buffering=0)
        try:
            os.fchmod(self.f.fileno(), 0o600)
            fcntl.flock(self.f, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with source.open("rb") as src:
                src_size = os.fstat(src.fileno()).st_size
                require(
                    stat.S_ISREG(os.fstat(src.fileno()).st_mode)
                    and MIN_MIO * MIO <= src_size <= size
                    and size % S == 0,
                    "Taille du support",
                )
                subprocess.run(
                    ["dd", "bs=1m"],
                    stdin=src,
                    stdout=self.f,
                    stderr=subprocess.PIPE,
                    check=True,
                )
            self.f.truncate(size)
            sync(self.f)
        except BaseException:
            self.f.close()
            raise

    def close(self):
        self.f.close()

    def reader(self):
        fd = os.open(self.path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        r = os.fdopen(fd, "rb")
        if not self.same_file(r):
            r.close()
            raise ValueError("Propriété du support incertaine")
        return r

    def same_file(self, r):
        st = os.fstat(self.f.fileno())
        return (
            stat.S_ISREG(st.st_mode)
            and st.st_uid == os.getuid()
            and st.st_nlink == 1
            and os.path.samestat(st, os.fstat(r.fileno()))
            and os.path.samestat(st, self.path.lstat())
        )

    def run(
        self,
        folder,
        firmware,
        mode,
        base,
        duplicate=None,
        fault=None,
        horloge_attestee=False,
    ):
        # Une autorisation d’export ne peut pas être reconstruite depuis un rapport archivé.
        before = None
        try:
            before = history(self.f, verify(self.f))
            reason = ""
        except ValueError as exc:
            reason = str(exc)
        boot_id = secrets.token_hex(8)
        date = (
            datetime.now(timezone.utc).isoformat(timespec="seconds")
            if horloge_attestee
            else None
        )
        with self.reader() as read_only:
            obs = boot(folder, firmware, mode, read_only, base, duplicate)
            require(
                obs["vm_arretee"] and obs["interne_intact"],
                "VM active ou stockage altéré",
            )
            r = report(obs, boot_id, date)
            print(render(r), flush=True)
            result = Result(obs, r)
            try:
                require(
                    self.same_file(read_only) and obs["supports_usb"] == 1,
                    "Identité USB ambiguë",
                )
                require(before is not None, reason)
                result.emplacement = publish(self.f, before, r, fault)
            except (ValueError, OSError) as exc:
                result.refus_export = str(exc)
            return result
