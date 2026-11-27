# Preuves disponibles au 27 novembre 2026

Les vérifications portent sur la révision `5b7c30103b5ab624b15fc4dec7e1878785acdc25`. Les contrôles 5D portent sur les sources, les modèles en mémoire et les fichiers de distribution du dépôt. Les résultats Windows sont ceux communiqués par la MOA. Aucune donnée brute de la machine ne fait partie de cette livraison.

## Essais exécutés en 5D

| Contrôle | Darwin ARM64 natif | Darwin x86_64 sous Rosetta | Portée |
|---|---:|---:|---|
| Modèle ACPI | 16/16 | 16/16 | RSDT/XSDT, sommes, longueurs, recouvrements, fenêtres MCFG, DRHD hors profil et refus de plusieurs unités pour le superviseur Q35 |
| Inventaire PCI | 19/19 | 19/19 | Configuration fictive, ponts, fonctions multiples, capacités, budgets, BAR décrites sans écriture |
| Formats VT-d Q35 | 4/4 | 4/4 | Décodage des capacités et des fautes, bornes d’époque et d’IOVA |
| **Total** | **39/39** | **39/39** | Même code source, zéro échec et zéro test ignoré. Aucune transaction matérielle par ces modèles. |

Compilateur `rustc 1.99.1 (58cf8f9c3 2026-10-13)`. Les sorties de test par cas sont conservées dans le [journal des six suites](essais-hotes.txt). Depuis la racine du dépôt sur le Mac de développement avec les deux cibles Darwin installées, le parcours suivant reproduit les suites ciblées. [Méthode hôte du projet](../../tests/materiel_modeles.py)

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
essai=$(mktemp -d "$NOVAOS_TMPDIR/killer-5d.XXXXXXXX")
for suite in acpi pci_inventory vtd_lab/format; do
  unite=${suite##*/}
  printf '#[path = "%s/novaOS/kernel/src/hardware/%s.rs"] pub mod %s;\n' \
    "$PWD" "$suite" "$unite" > "$essai/$unite.rs"
  for isa in aarch64 x86_64; do
    lanceur=$isa
    if [ "$isa" = aarch64 ]; then lanceur=arm64; fi
    rustup run stable rustc --edition 2024 --test --target "$isa-apple-darwin" \
      "$essai/$unite.rs" -o "$essai/$unite-$isa"
    /usr/bin/arch "-$lanceur" "$essai/$unite-$isa"
  done
done
```

## Inspection du candidat du laboratoire

La vérification de `SHA256SUMS` porte sur les binaires tiers. Pour le chemin x64 copié par le script d’ISO, lecture de l’en-tête PE et comparaison octet par octet avec le fichier de même nom **extrait de la FAT12** de `limine-uefi-cd.bin`. L’image FAT elle-même a pour SHA-256 `c25328b0e58f188d6b1a588827596d1c19a9e1d0d5e9dbc16a3aab238deb4aa4`. Les deux chemins livrent 307 200 octets identiques de SHA-256 `91ef81c1f95c4ac8bf77c4393a0b2fcd2628550827d7abf89a43194400e17a85`. `llvm-readobj` 23.1.6 relève AMD64, six entrées de répertoire, position et taille de `CertificateTable` nulles. [Sommes des entrées](../../third_party/limine/SHA256SUMS) · [Assemblage x86](../../scripts/iso.sh)

Commandes depuis la racine, sur les seuls fichiers du dépôt. L’extraction suit les répertoires et la chaîne de clusters de cette image FAT12 connue.

```sh
(cd novaOS/third_party/limine && shasum -a 256 -c SHA256SUMS)
llvm-readobj --file-headers novaOS/third_party/limine/BOOTX64.EFI
python3.11 -B - <<'PY'
from pathlib import Path
from struct import unpack_from
from hashlib import sha256

base = Path("novaOS/third_party/limine")
image = (base / "limine-uefi-cd.bin").read_bytes()
sector, spc, reserved, copies, entries, total, _, fat_sectors = unpack_from("<HBHBHHBH", image, 11)
assert total * sector == len(image)
fat = image[reserved * sector:(reserved + fat_sectors) * sector]
root = (reserved + copies * fat_sectors) * sector
data = root + ((entries * 32 + sector - 1) // sector) * sector
size = sector * spc
directory = image[root:root + entries * 32]
for name in (b'EFI        ', b'BOOT       ', b'BOOTX64 EFI'):
    pos = next(i for i in range(0, len(directory), 32) if directory[i:i+11] == name)
    cluster, length = unpack_from("<HI", directory, pos + 26)
    chain, seen = bytearray(), set()
    while 2 <= cluster < 0xff0:
        assert cluster not in seen
        seen.add(cluster)
        start = data + (cluster - 2) * size
        assert start + size <= len(image)
        chain.extend(image[start:start+size])
        cell = unpack_from("<H", fat, cluster + cluster // 2)[0]
        cluster = cell >> 4 if cluster % 2 else cell & 4095
    assert cluster >= 0xff8
    directory = chain
embedded = directory[:length]
assert embedded == (base / "BOOTX64.EFI").read_bytes()
print(len(embedded), sha256(embedded).hexdigest())
PY
```

Ces vérifications identifient les octets du banc. L’autorisation db/dbx de la future image sur le MSI, l’intégrité transitive du contenu et l’absence de repli restent à démontrer. [Critères Secure Boot](distribution-et-diagnostic.md)

## Relecture des preuves antérieures

- **5B et 5C.** `python3.11 -B novaOS/docs/killer-e2400-5c/verifier.py` a réussi. Il vérifie les SHA-256 des archives et de leurs membres référencés, les 48 sélecteurs et le périmètre des sources protégées. Dans les traces Q35 double déjà acquises, chaque scénario contrôlé compte 22 READ, 7 WRITE et 3 FLUSH sur le stockage émulé. La consommation du reçu attendu laisse zéro cadre NIC retenu et le reçu absent en conserve 26. Le cas E2400 simulé passe par 259, 130 puis zéro cadre en quarantaine. Le témoin VirtIO ARM compte 13 312 trames par sens sur chacun de trois démarrages historiques. [Vérificateur](../killer-e2400-5c/verifier.py) · [Archives et empreintes](../validation/killer-e2400-phase5b/SHA256SUMS)
- **5C.1.** Le procès-verbal du collecteur comporte 32 contrôles `OK` sur données fictives et PnP simulé sous PowerShell 7.7.0. Il conserve séparément les empreintes des scripts. Les résultats physiques d’inventaire ultérieurs proviennent de la synthèse MOA, sans lecture directe du MSI en 5D. [Procès-verbal 5C.1](../killer-e2400-5c/inventaire/qualification-acpi-5c1.txt)
- **4F.** La qualification x86_64 du gestionnaire, le maintien du bail NVMe sur Q35 et les limites de réadmission conservent leur provenance. [Résultats Q35 4F](../gestionnaire-pilotes/phase4f-v0.25.0.md)

**Non exécuté en 5D.** Démarrage QEMU ou physique, préparation d’un support amovible, vérification des bases de confiance de la machine, observation live de VT-d, trafic réseau ou accès aux disques du MSI. Les tests de l’image générique et de ses refus sont spécifiés pour le futur candidat. [Matrice d’acceptation](acceptation.md)
