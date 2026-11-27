"""Lecture indépendante du fichier /BOOT/NOVAOS.ELF dans la FAT32 de la copie QEMU."""
import hashlib
import struct


def verifier(fp, origine, attendu):
    def lire(offset, taille):
        fp.seek(origine + offset)
        data = fp.read(taille)
        if len(data) != taille:
            raise ValueError("lecture EFI tronquée")
        return data

    boot = lire(0, 512)
    secteur, par_cluster, reserve, tables = struct.unpack_from("<HBHB", boot, 11)
    total = struct.unpack_from("<I", boot, 32)[0]
    par_fat = struct.unpack_from("<I", boot, 36)[0]
    racine = struct.unpack_from("<I", boot, 44)[0]
    if (secteur, par_cluster, tables) != (512, 1, 2) or not 0 < par_fat < total <= 122880:
        raise ValueError("géométrie EFI inattendue")
    premier = reserve + tables * par_fat
    table = lire(reserve * secteur, par_fat * secteur)
    if table != lire((reserve + par_fat) * secteur, len(table)):
        raise ValueError("copies FAT différentes")

    def chaine(cluster):
        vus = set()
        while cluster < 0xffffff8:
            if not 2 <= cluster < 2 + total - premier or cluster in vus:
                raise ValueError("chaîne FAT invalide")
            vus.add(cluster)
            yield lire((premier + cluster - 2) * secteur, secteur)
            cluster = struct.unpack_from("<I", table, 4 * cluster)[0] & 0xfffffff

    def entree(cluster, nom, repertoire):
        for bloc in chaine(cluster):
            for offset in range(0, secteur, 32):
                e = bloc[offset:offset+32]
                if not e[0]:
                    raise ValueError("entrée EFI absente")
                if e[:11] == nom and bool(e[11] & 16) == repertoire:
                    return (struct.unpack_from("<H", e, 20)[0] << 16 |
                            struct.unpack_from("<H", e, 26)[0]), struct.unpack_from("<I", e, 28)[0]
        raise ValueError("répertoire EFI incomplet")

    dossier, _ = entree(racine, b"BOOT       ", True)
    debut, taille = entree(dossier, b"NOVAOS  ELF", False)
    digest = hashlib.sha256()
    reste = taille
    for bloc in chaine(debut):
        if reste == 0:
            raise ValueError("chaîne ELF trop longue")
        n = min(reste, secteur)
        digest.update(bloc[:n])
        reste -= n
    if reste or digest.hexdigest() != attendu:
        raise ValueError("ELF relu sur l’ESP différent du fichier sélectionné")
    return digest.hexdigest()
