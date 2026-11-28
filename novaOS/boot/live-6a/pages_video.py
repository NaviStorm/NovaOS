"""Lecture indépendante des PTE de la surface Limine via la RAM physique QEMU."""
from pathlib import Path
import re
import struct


def controler(q, d, syms, etape, gmem, hmp):
    def mots(addr, n):
        if addr < 0xffff800000000000 or addr % 8:
            raise ValueError("Pointeur Limine hors profil")
        return struct.unpack(f"<{n}Q", gmem(d, addr, 8*n))

    rep, = mots(syms["novaos::boot::FRAMEBUFFER"] + 40, 1)
    _, count, liste = mots(rep, 3)
    if not 1 <= count <= 16:
        raise ValueError("Nombre de surfaces")
    fb, = mots(liste, 1)
    base, w, h, pitch = mots(fb, 4)
    if not (base % 4096 == 0 and base >= 0xffff800000000000 and 0 < w*4 <= pitch
            and 0 < h*pitch <= 128 << 20):
        raise ValueError("Géométrie de surface")
    mr, = mots(syms["novaos::boot::MAP"] + 40, 1)
    _, n, ptr = mots(mr, 3)
    if not 0 < n <= 512:
        raise ValueError("Carte mémoire hors borne")
    carte = [mots(p, 3) for p in mots(ptr, n)]
    master = int(re.search(r"CR3=([0-9a-fA-F]+)", hmp(q, "info registers"))[1], 16)
    masque = 0x000ffffffffff000
    root = master & masque
    tables = {}
    type_table = 5 if etape == "entree" else 0
    def table(pa):
        if pa not in tables:
            if len(tables) >= 256 or [k for a,l,k in carte if a <= pa and pa+4096 <= a+l] != [type_table]:
                raise ValueError("Table CPU hors des cadres RAM attendus")
            path = Path(f"video-{etape}-{pa:x}.bin")
            q.call("pmemsave", val=pa, size=4096, filename=path.name)
            tables[pa] = struct.unpack("<512Q", path.read_bytes())
        return tables[pa]

    premiere = None
    pages = 0
    non_exec = 0
    for delta in range(0, pitch*h, 4096):
        va, t, user, nx = base + delta, root, True, False
        for shift in (39, 30, 21, 12):
            e = table(t)[va >> shift & 511]
            if (e & 3) != 3:
                raise ValueError("Surface absente ou non inscriptible")
            user &= bool(e & 4)
            nx |= bool(e >> 63)
            if shift == 12 or (shift in (21, 30) and e & 0x80):
                mask = (1 << shift) - 1
                pa = (e & masque & ~mask) | (va & mask)
                break
            t = e & masque
        if premiere is None:
            premiere = pa
        if user or pa != premiere + delta or [k for a,l,k in carte if a <= pa and pa+4096 <= a+l] != [7]:
            raise ValueError("Alias, droits utilisateur ou type physique du framebuffer")
        if etape == "memoire" and not nx:
            raise ValueError("Framebuffer exécutable après installation du CR3 noyau")
        non_exec += int(nx)
        pages += 1
    return dict(cr3=root, surface_va=base, surface_pa=premiere, largeur=w, hauteur=h,
                pas=pitch, pages=pages, pages_nx=non_exec, pages_superviseur_rw=pages,
                tables=sorted(tables))
