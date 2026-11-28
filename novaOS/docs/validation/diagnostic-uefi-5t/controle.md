# Contrôles documentaires 5T

Base NovaOS `684b446a24d68448721122e22b090876208f15ea`, 28 novembre 2026. Les contrôles ont porté sur les sources, les entrées du registre ABI et les octets des preuves archivées. La question nouvelle était de déterminer ce qu’une interface UEFI ordinaire permet d’inférer sur la protection précoce, puis de comparer les hypothèses du PE livré à la description physique disponible. Aucun changement de binaire. Aucune VM, machine physique ou clé USB utilisée en 5T. Une nouvelle exécution Q35 ne permettrait pas d’identifier le dépositaire du démarrage OEM.

## Portée réelle de Map dans EDK II

Révision amont TianoCore `fbe0805b2091393406952e84724188f8c1941837`, référence du banc 5Q. La vérification a lu les octets des trois fichiers publics et ceux de la copie de construction 5Q. Deux fichiers sont identiques. Dans le troisième, le diff contient un seul ajout de huit lignes dans le traitement des attributs PCI. Cette garde locale interdit l’activation de bus mastering pour un emplacement réseau réservé. Le corps de PciIoMap étudié est identique à l’amont. Ces sources décrivent cette révision d’EDK II. Le firmware MSI reste inconnu. [Recette 5Q](../../../boot/diagnostic-5q/README.md)

| Fichier sous MdeModulePkg | SHA-256 amont | SHA-256 copie de laboratoire |
|---|---|---|
| Bus/Pci/PciBusDxe/PciIo.c | `051ffead677c42a769c4bcf0a03de32e762b1f96554361517c403aba911d49d3` | `46b9087ff9e264e4623d3e3e12c6865996252e2708e30a11230d54396d20da0a` |
| Bus/Pci/PciHostBridgeDxe/PciRootBridgeIo.c | `0d0c14aa77427e225005bfe21d623f1cfb4991d17b0387e126826c48887a8789` | identique |
| Include/Protocol/IoMmu.h | `d4227a7ad2dd8faa35a4a014fae03410bfd9851a140a402fac652a3fb143bfc9` | identique |

- **Bus PCI.** PciIoMap appelle Map du pont racine. Si le protocole IOMMU est disponible et le mapping réussi, SetAttribute applique les droits de lecture, d’écriture ou les deux selon l’opération, pour le handle du périphérique. [PciIo.c, révision examinée](https://github.com/tianocore/edk2/blob/fbe0805b2091393406952e84724188f8c1941837/MdeModulePkg/Bus/Pci/PciBusDxe/PciIo.c#L964-L1037)
- **Pont racine.** RootBridgeIoMap peut déléguer au protocole IOMMU. Dans le chemin sans ce protocole, il peut utiliser un tampon de rebond ou retourner directement l’adresse physique. Ces branches existent dans les sources. Elles n’ont pas été instrumentées sur le MSI. [PciRootBridgeIo.c, révision examinée](https://github.com/tianocore/edk2/blob/fbe0805b2091393406952e84724188f8c1941837/MdeModulePkg/Bus/Pci/PciHostBridgeDxe/PciRootBridgeIo.c#L1331-L1477)
- **Portée du protocole.** L’interface EDK II distingue la création du mapping et les droits DMA du périphérique. Même une exécution attestée de SetAttribute sur un buffer particulier ne démontre pas l’exclusion de tous les autres demandeurs, la protection de la pile et du PE depuis le reset ou la continuité à ExitBootServices. [Interface IoMmu.h](https://github.com/tianocore/edk2/blob/fbe0805b2091393406952e84724188f8c1941837/MdeModulePkg/Include/Protocol/IoMmu.h) · [Contrat PCI I/O UEFI](https://uefi.org/specs/UEFI/2.11/14_Protocols_PCI_Bus_Support.html)

Pour reproduire la comparaison hors ligne avec un checkout amont EDK II déjà disponible et la copie du banc, exécuter depuis la racine de ce worktree. Définir `EDK2_AMONT` sur le dépôt amont qui contient la révision. Les commandes Git lisent les blobs originaux, indépendamment des conversions de fins de ligne d’un checkout.

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/"
: "${EDK2_AMONT:?fournir le chemin du depot EDK II existant}"
r=fbe0805b2091393406952e84724188f8c1941837
lab=../diagnostic-uefi-phase5q/novaOS/build/reference-compacte-5q/edk
for f in Bus/Pci/PciBusDxe/PciIo.c Bus/Pci/PciHostBridgeDxe/PciRootBridgeIo.c Include/Protocol/IoMmu.h; do
  p=MdeModulePkg/$f
  git -C "$EDK2_AMONT" cat-file -e "$r:$p"
  git -C "$EDK2_AMONT" cat-file blob "$r:$p" | shasum -a 256
  shasum -a 256 "$lab/$p"
done
```

## Archives et ABI

Les onze manifestes SHA-256 ont été relus sur les fichiers du worktree, y compris les archives volumineuses. Les 55 fichiers référencés ont concordé. Les comptages de 5I à 5S dans l’ordre sont **5, 3, 3, 4, 3, 12, 11, 2, 3, 7, 2**. Le contrôle du générateur ABI a réussi et le registre contient 48 sélecteurs. Les preuves 5I–5S conservent leur auteur, leur profil et leur date.

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1
for phase in i j k l m n o p q r s; do
  (cd "novaOS/docs/validation/diagnostic-uefi-5$phase" && shasum -a 256 -c SHA256SUMS)
done
make -C novaOS abi-check
test "$(rg -c '^\[call\.' novaOS/abi/syscalls.toml)" = 48
```

## Même média et deux autorités de démarrage

La comparaison ciblée a relu les bilans nominaux 5R et les observations 5S dans les deux modes. Sur Darwin ARM64 natif et x86_64 sous Rosetta, les quatre comparaisons ont abouti. Dans chaque paire, le PE, l’image disque et le magasin initial ont les mêmes empreintes. 5R indique un DRHD, un verdict 0 et aucune activation autorisée. 5S indique le statut 16 et la confiance indéterminée. Sa trace de commandes SATA entre l’entrée et HLT est vide, ses compteurs internes terminaux sont nuls, ses captures d’entrée et de fin ont la même empreinte. Les deux firmwares et leurs dispositifs d’observation diffèrent. Cette comparaison ne mesure aucun temps physique et ne prouve pas l’état du portable. [Preuve 5R](../diagnostic-uefi-5r/observations.zip) · [Témoin 5S](../diagnostic-uefi-5s/temoin.zip) · [Sens des champs et bornes du témoin](../../qualification-live-5s/preuves.md)

Reproduction sur le Mac du laboratoire depuis la racine du worktree, avec les variables temporaires du bloc précédent et sans optimisation Python.

```sh
set -eu
for isa in arm64 x86_64; do
  /usr/bin/arch "-$isa" /usr/bin/python3 -B - <<'PY'
import json
from pathlib import Path
import platform
from zipfile import ZipFile

root = Path("novaOS/docs/validation")
with ZipFile(root / "diagnostic-uefi-5r/observations.zip") as r, ZipFile(root / "diagnostic-uefi-5s/temoin.zip") as s:
    for mode in ("actif", "inactif"):
        p = mode + "-nominal/"
        bilan = json.loads(r.read(p + "bilan.json"))
        rapport = json.loads(r.read(p + "rapport.json"))
        obs = json.loads(s.read(mode + "/observation.json"))
        assert bilan["mode"] == rapport["secure_boot"] == obs["mode_banc"] == mode
        assert bilan["pe"] == rapport["pe_sha256"] == obs["pe"]
        assert bilan["image"] == bilan["image_apres"] == obs["image"]
        assert bilan["magasin"] == obs["magasin_initial"]
        assert (rapport["statut"], rapport["drhd"], rapport["activation_autorisee"]) == (0, 1, False)
        assert (obs["verdict"]["statut"], obs["verdict"]["confiance"]) == (16, 2)
        assert obs["sha256_ppm_entree"] == obs["sha256_ppm_hlt"]
        assert not any(obs["compteurs_hlt"]["interne"].values())
        assert not s.read(mode + "/commandes-intervalle.log")
        print(platform.machine(), mode, "5R=0 DRHD=1 ; 5S=16 écran identique ; PE, image et magasin identiques")
PY
done
```
