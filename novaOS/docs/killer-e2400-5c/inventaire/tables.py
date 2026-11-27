"""Lecture locale Windows. Signatures, entêtes et octets hexadécimaux sur stdout."""
import ctypes
import hashlib
import json
import sys

MAX = 1024 * 1024


def examiner(nom, data):
    if not 36 <= len(data) <= MAX or data[:4] != nom.encode("ascii"):
        raise ValueError("signature ou taille incorrecte")
    if int.from_bytes(data[4:8], "little") != len(data) or sum(data) % 256:
        raise ValueError("longueur ou somme ACPI incorrecte")
    if nom == "DMAR" and len(data) < 48:
        raise ValueError("entête DMAR tronqué")
    return {"octets": len(data), "revision": data[8], "sha256": hashlib.sha256(data).hexdigest(),
            "table_hex": data.hex()}


def lire(api, *args):
    n = api(*args, None, 0)
    if not 0 < n <= MAX:
        raise ValueError("taille indisponible ou hors limite")
    buf = ctypes.create_string_buffer(n)
    if api(*args, buf, n) != n:
        raise ValueError("taille modifiée ou lecture incomplète")
    return buf.raw


def main():
    if sys.platform != "win32":
        raise SystemExit("Collecte réservée à Windows.")
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    enum, get = kernel.EnumSystemFirmwareTables, kernel.GetSystemFirmwareTable
    u32 = ctypes.c_uint32
    enum.argtypes = (u32, ctypes.c_void_p, u32)
    get.argtypes = (u32, u32, ctypes.c_void_p, u32)
    enum.restype = get.restype = u32
    provider = int.from_bytes(b"ACPI", "big")
    ids = lire(enum, provider)
    if len(ids) % 4:
        raise SystemExit("Liste ACPI tronquée.")
    noms = [ids[i:i+4] for i in range(0, len(ids), 4)]
    rapport = {}
    for nom in ("DMAR", "MCFG", "APIC"):
        tag = nom.encode("ascii")
        try:
            if noms.count(tag) != 1:
                raise ValueError("table absente ou ambiguë")
            data = lire(get, provider, int.from_bytes(tag, "little"))
            rapport[nom] = examiner(nom, data)
        except ValueError as err:
            rapport[nom] = {"indetermine": str(err)}
    print(json.dumps(rapport))


if __name__ == "__main__":
    main()
