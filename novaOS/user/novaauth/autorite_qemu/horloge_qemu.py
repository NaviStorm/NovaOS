"""Mesure de la liaison TLS invitée sur une copie P9, état de droit hors périmètre."""

import time, hashlib, json, os, sys, re, shutil, socket, ssl, subprocess, tempfile, threading
from pathlib import Path

root = Path(__file__).resolve().parents[4]
sys.path[:0] = [
    str(root / "novaOS/user/novaauth/service"),
    str(root / "novaOS/user/novaauth/transport_qemu"),
]
from adaptateur import servir, lire
from paquet_qemu import Serial
from shell_qemu import command
import argparse

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("arch", choices=("aarch64", "x86_64"))
parser.add_argument("sector", type=int, choices=(512, 4096))
parser.add_argument("mode", choices=("nominal", "lent"))
for name in ("iso", "genese", "cles", "sortie"):
    parser.add_argument(name, type=Path)
args = parser.parse_args()
arch, sector, mode = args.arch, args.sector, args.mode
out, iso, keys, seed = (
    x.resolve() for x in (args.sortie, args.iso, args.cles, args.genese)
)
out.mkdir(exist_ok=False, parents=True)
disk = out / "identites.p9"
shutil.copyfile(seed / disk.name, disk)
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ctx.minimum_version = ctx.maximum_version = ssl.TLSVersion.TLSv1_3
ctx.set_groups("P-256")
ctx.set_client_sigalgs("ed25519")
ctx.set_server_sigalgs("ed25519")
ctx.set_ciphersuites("TLS_AES_128_GCM_SHA256")


def audit(conn, direction, version, kind, code, data):
    if int(code) == 8 and int(kind) == 22 and direction == "write":
        raw = memoryview(data)[6:]
        while len(raw) >= 4:
            ext = int.from_bytes(raw[:2], "big")
            n = int.from_bytes(raw[2:4], "big")
            if n > len(raw) - 4:
                raise ValueError("extension TLS tronquée")
            if ext == 1:
                observed["fragment_2048"] = bytes(raw[4 : 4 + n]) == b"\x03"
            raw = raw[4 + n :]
    with (out / "tls-public.log").open("a") as f:
        print(direction, int(version), int(kind), int(code), len(data), file=f)


ctx._msg_callback = audit
ctx.verify_mode = ssl.CERT_REQUIRED
ctx.num_tickets = 0
ctx.load_cert_chain(keys / "autorite.pem", keys / "autorite-cle.pem")
ctx.load_verify_locations(keys / "ca.pem")
with iso.open("rb") as image:
    iso_hash = hashlib.file_digest(image, "sha256").hexdigest()
observed = {
    "architecture": arch,
    "secteur": sector,
    "mode": mode,
    "iso_sha256": iso_hash,
}
with (
    socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener,
    tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as tmp,
):
    os.chdir(tmp)
    listener.bind("port.sock")
    os.chdir(root)
    listener.listen(1)
    listener.settimeout(80)
    left, right = socket.socketpair()

    def host():
        try:
            with (
                right,
                ctx.wrap_socket(
                    right, server_side=True, do_handshake_on_connect=False
                ) as conn,
            ):
                conn.settimeout(75)
                conn.do_handshake()
                if (
                    conn.getpeercert(True) != (keys / "serveur.der").read_bytes()
                    or conn.session_reused
                ):
                    raise ValueError("pair TLS")
                observed.update(tls=conn.version(), certificat_epingle=True)
                ident = bytes(lire(conn, 32))
                if not observed.get("fragment_2048"):
                    raise ValueError("négociation des 2048 octets absente")
                observed["instance"] = ident.hex()
                if mode == "lent":
                    for octet in ident:
                        time.sleep(0.65)
                        conn.sendall(bytes([octet]))
                else:
                    conn.sendall(ident)
                try:
                    conn.recv(1)
                except ssl.SSLError:
                    pass
        except Exception as exc:
            observed["erreur_tls"] = str(exc)

    def adapter():
        try:
            with disk.open("r+b", buffering=0) as f, (out / "nbd.log").open("x") as log:

                def trace(*args):
                    print(*args, file=log, flush=True)

                servir(f.fileno(), listener, left, sector, trace)
        except Exception as exc:
            observed["erreur_nbd"] = str(exc)

    workers = [threading.Thread(target=host), threading.Thread(target=adapter)]
    for worker in workers:
        worker.start()
    args = command(
        arch,
        sector,
        disk,
        controls=["-display", "none", "-monitor", "none", "-serial", "stdio"],
    )
    i = args.index("-drive") + 1
    args[i] = (
        "if=none,id=d,format=raw,file.driver=nbd,file.server.type=unix,file.server.path=port.sock,cache=none,werror=report,rerror=report"
    )

    env = dict(
        os.environ,
        NOVA_ISO=str(iso),
        NOVA_ARCH=arch,
        NOVA_ARM_CPU="max",
        NOVA_VCPUS="2",
    )
    with (
        (out / "invite.serial").open("xb") as raw,
        subprocess.Popen(
            args,
            cwd=tmp,
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            bufsize=0,
        ) as vm,
    ):
        try:
            t = Serial(vm, raw)
            m = t.expect(rb"NAUTH canal (authentifie|refuse)", 65)
            observed["invite"] = m[1].decode()
            acknowledgement = t.expect(rb"NAUTH registre canal (confirme|refuse)", 15)
            observed["registre"] = acknowledgement[1].decode()
            if b"NAUTH transport ferme" not in t.data:
                raise ValueError("fermeture non confirmee")
            snapshots = re.findall(
                rb"NAUTH mesure NAUTH TLS (avant|actif|apres) roots=(\d+) handles=(\d+) waits=(\d+) pages=(\d+) pile=(\d+) cadres_ipc_globaux=(\d+)",
                t.data,
            )
            observed["ressources"] = {
                row[0].decode(): list(map(int, row[1:])) for row in snapshots
            }
            res = observed["ressources"]
            observed["drainage"] = (
                set(res) == {"avant", "actif", "apres"}
                and res["avant"][:5] == res["apres"][:5]
                and res["apres"][2] == 0
            )
        except Exception as exc:
            observed["erreur_invite"] = str(exc)
        finally:
            if vm.poll() is None:
                vm.terminate()
            vm.wait(5)
            raw.write(vm.stdout.read())
    for worker in workers:
        worker.join(80)
(out / "resultat.json").write_text(json.dumps(observed, indent=2) + "\n")
print(observed)
raw = (out / "invite.serial").read_bytes()
times = re.search(
    rb"NAUTH TLS debut=([0-9a-f]+) echeance=([0-9a-f]+) fin=([0-9a-f]+)", raw
)
if not times:
    sys.exit(1)
start, limit, end = (int(v, 16) for v in times.groups())
success = (
    observed.get("tls") == "TLSv1.3"
    and observed.get("certificat_epingle")
    and observed.get("registre") == ("refuse" if mode == "lent" else "confirme")
    and observed.get("fragment_2048")
    and observed.get("drainage")
    and limit - start == 10000
    and b"NOW autorise gestion et arguments refuses 8/8" in raw
)
if mode == "lent":
    success = (
        success
        and observed.get("invite") == "refuse"
        and end >= limit
        and b"NAUTH diagnostic Io(TimedOut)" in raw
    )
else:
    success = (
        success and observed.get("invite") == "authentifie" and start <= end < limit
    )
observed.update(debut_ms=start, echeance_ms=limit, fin_ms=end, succes=bool(success))
(out / "resultat.json").write_text(json.dumps(observed, indent=2) + "\n")
if not success:
    sys.exit(1)
