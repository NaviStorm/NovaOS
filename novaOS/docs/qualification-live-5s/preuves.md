# Registre des preuves 5S et reproduction

**Question expérimentale.** Le PE 5R accepté par Secure Boot atteint-il un verdict visible si OVMF l’amorce depuis un vrai chemin xHCI émulé, sans émetteur 5Q, sans ancre 5M et sans bail graphique 5O/5R ? Les archives 5R testaient une remise valide ou défectueuse sur le firmware spécialisé. Les mesures USB ordinaires antérieures portaient sur d’autres PE. Deux cellules supplémentaires suffisent à tester cet écart. Elles n’exercent ni le silicium ni le firmware OEM.

## Configuration et résultat

Essai du 28 novembre 2026 à la base `27e5a9fcf8ee6906e604d9314c562d4d7f053903`. QEMU 11.1.3, Q35/SMM x86_64 en TCG, un vCPU, 128 Mio, modèle Intel IOMMU présent, xHCI, VGA et SATA émulés, sans NIC. Le firmware OVMF USB de référence 5E est utilisé octet pour octet. L’option de boot strict et le disque canari synthétique désignent ce profil. Le moniteur QEMU et les points d’arrêt observent le chargement. Ils ne déposent ni reçu ni tables DMA de 5Q. La protection précoce reste indéterminée. Le disque USB est un fichier ordinaire ouvert en lecture seule par la VM. [Banc de démarrage réutilisé](../../boot/diagnostic-5n/demarrage.py) · [Recette 5S](temoin.py)

| Entrée contrôlée | SHA-256 |
|---|---|
| Firmware OVMF USB | `32807682a9e5c0e2d192ecb6077941d4be8bcfa6b9cd6e60830c3a9f48c0555a` |
| PE 5R | `7f80bf3c42c89162d67fb229b9080e9706c0f7f6a3574978051c1c0c555477a5` |
| Image GPT 68 Mio avec NOVAOS et NOVALOGS | `9440a1030b530174d97e85e4e55cb6566c514dfd3a143a68fff77ae8b5f1b404` |
| Magasin actif déjà qualifié en 5R, copié pour chaque VM | `7b7250bb70f9dc0f8e604c30f06041b7f91afc1f57c84e7e45b0ac6079265a52` |
| Magasin inactif déjà qualifié, copié pour chaque VM | `8c09b326b45951f539619b4d0f33e5e8fdbafa1cca9b173468f9df7e4a7725e0` |

| Observation | Secure Boot actif | Secure Boot inactif |
|---|---:|---:|
| Calibrations de l’adresse, exclues des résultats | 1 | 1 |
| Démarrages mesurés entre première instruction et HLT | 1 | 1 |
| Sections du PE relogées et contrôlées | 5 | 5 |
| Lectures USB comptées à l’entrée | 35 / 538 624 octets | 35 / 538 624 octets |
| Statut en RAM et confiance rapportée par l’EFI | 16 / indéterminée | 16 / indéterminée |
| Arène utilisée / allocations vivantes à l’arrêt | 2 880 octets / 0 | 2 880 octets / 0 |
| Compteurs internes au reset, à l’entrée et à HLT | tous à zéro | tous à zéro |
| Trace ATA/AHCI dans l’intervalle EFI | vide | vide |
| Pixels 1280 × 800 entre les deux captures | identiques | identiques |

Le statut 16 correspond à l’échec du contrôle de taille ou d’accessibilité de l’ancre LoadOptions requise par 5M. L’EFI s’arrête avant d’accorder un bail d’affichage ou de publier un inventaire. Le mode Secure Boot indiqué en tête des colonnes est celui du banc. C’est précisément dans le cas actif que l’on voit que l’admission du PE par OVMF ne suffit pas à produire un état de confiance dans le PE. Les deux images d’écran bornent l’observation des pixels. Elles ne constituent pas une trace de chaque écriture graphique. L’arrêt à HLT ne couvre pas un futur délai de watchdog. Le canari SATA est resté identique. La preuve d’absence d’E/S combine compteurs aux bornes et trace de commandes dans la fenêtre. [Archives des deux cellules](../validation/diagnostic-uefi-5s/temoin.zip) · [Contrôle de réception 5M](../../boot/diagnostic-5m/entree.rs)

## Contre-lecture documentaire

Les empreintes des dix dossiers 5I à 5R ont été recalculées sur leurs fichiers. Quatre cellules 5R ont été revérifiées avec le PE exact, les signatures, les tables VT-d, les rapports et les pixels archivés pour les scénarios nominal et surface contradictoire de chaque mode. Les refus 5F et 5N ont été relus sur leurs bilans. ARM64 Darwin natif et x86_64 sous Rosetta donnent la même décision pour chaque cellule. Le registre canonique garde ses 48 sélecteurs et le générateur ABI est à jour. Le contrôle PowerShell de la fiche a été exécuté sur six jeux fictifs sous PowerShell 7.7.0. Il analyse les deux blocs publiés puis exerce uniquement la lecture hors machine du premier. Windows PowerShell 5.1, les commandes PnP en direct, les essais physiques et les scénarios de coupures réelles n’ont pas été exécutés en 5S. Les campagnes QEMU historiques n’ont pas été relancées. [Journal des contrôles](../validation/diagnostic-uefi-5s/controle.txt) · [Empreintes des livrables de preuve](../validation/diagnostic-uefi-5s/SHA256SUMS)

Depuis la racine du worktree 5S, fournir le binaire qualifié 5R ou le reconstruire selon sa recette. Le juge hors VM utilise Python 3.9 ou ultérieur et OpenSSL 3. Le bloc suivant n’ouvre que des fichiers du projet. [Construction reproductible du PE](../../boot/diagnostic-5r/README.md)

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1
d=novaOS/docs/qualification-live-5s
v=novaOS/docs/validation/diagnostic-uefi-5s
r=../diagnostic-uefi-phase5r/novaOS/build/5r
pe="$r/rust/x86_64-unknown-uefi/release/diagnostic-5r.efi"
(cd "$v" && shasum -a 256 -c SHA256SUMS)
for isa in arm64 x86_64; do
  /usr/bin/arch "-$isa" /usr/bin/python3 -B "$d/controler.py" "$pe"
done
make -C novaOS abi-check
for phase in i j k l m n o p q r; do
  (cd "novaOS/docs/validation/diagnostic-uefi-5$phase" && shasum -a 256 -c SHA256SUMS)
done
```

Pour reproduire uniquement les deux cellules QEMU, utiliser l’environnement Python historique et une copie existante du magasin actif exact 5R. Aucun signataire privé n’est nécessaire. Le banc crée un répertoire de série neuf sous `novaOS/build`, prend le verrou partagé et produit son archive après les deux mesures. Le magasin de référence reste intact. Les deux calibrations et leurs images de travail restent privées. La comparaison documentaire peut ensuite être rejouée sur l’archive produite.

```sh
py=../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools/venv/bin/python
"$py" -B "$d/temoin.py" "$pe" "$r/magasin/actif.fd" reproduction
python3 -B "$d/controler.py" "$pe" --archive novaOS/build/qualification-5s-reproduction/temoin.zip
```

Pour le contrôle PowerShell sur un hôte de laboratoire, fournir un dossier temporaire neuf. Le script écrit un faux inventaire uniquement dans ce dossier et n’appelle aucun fournisseur Windows.

```sh
export POWERSHELL_TELEMETRY_OPTOUT=1 POWERSHELL_UPDATECHECK=Off
pwsh -NoLogo -NoProfile -NonInteractive -File "$d/essai-inventaire.ps1" "$NOVAOS_TMPDIR/inventaire-5s-reproduction"
```
