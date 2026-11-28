# Phase 5Y · Premier démarrage physique du diagnostic STANDARD sur MSI

Le 28 novembre 2026, la MOA a relevé l’inventaire complet affiché par le premier EFI STANDARD x86_64 sur le MSI GE62 6QC, puis confirmé le retour à Windows. **A8 est clôturé pour ce seul essai.** Le périmètre physique s’arrête au diagnostic EFI, à son journal en RAM et à son arrêt local. Un autre démarrage, une écriture sur clé ou un essai du noyau nécessitent une nouvelle autorisation. [Registre des qualifications](validation/qualifications-demarrage.md)

## Relevé physique transmis par la MOA

| Élément | Valeur observée ou vérifiée par la MOA |
|---|---|
| Plateforme | MSI GE62 6QC, Intel Core i5-6300HQ, BIOS E16J5IMS.11D |
| Support | Kingston DataTraveler 2.0, 4 005 560 320 octets. SHA-256 du support entier `0b04a9b2b65c1943575f5429c043d0b80a67d666d307b5e963a4347ed4a75ff4` |
| Confiance | Profil STANDARD x86_64, Secure Boot inactif, origine USB déclarée par UEFI. Le diagnostic affiche la réussite des vérifications des signatures des manifestes et des ELF. L’authentification du premier EFI par Secure Boot reste hors du périmètre de cet essai. |
| CPU, mémoire | Signature CPU `000506e3`. RAM UEFI 16 280 Mio, dont 16 216 Mio libres. Quatre CPU actifs décrits par la MADT. |
| Inventaire | 23 tables ACPI, une fenêtre PCIe, 16 fonctions PCIe |
| Fonctions affichées par catégorie | Graphique `10de:139b` à `0000:01:00.0` · USB `8086:a12f` à `0000:00:14:00.0` · SATA `8086:a103` à `0000:00:17.0` · réseau `8086:3165` à `0000:02:00.0` |
| Décompte | USB 1, réseau 2, graphique 2, stockage 1, autres 10. Le PE affiche la première fonction de chaque catégorie nommée. |
| Sélection et droits | Un pilote sélectionné, un conflit, zéro activation. L’écran indique une attribution VT-d indéterminée (`Ambigu`) et une isolation pour activation non établie. |
| Fin du parcours | Inventaire terminé, journal en RAM, arrêt local. Retour à Windows confirmé par la MOA. |

Le relevé fourni est le compte rendu de l’opérateur. La clôture 5Y n’ajoute aucune capture du bus physique, du contenu du journal volatil ou des tables de traduction actives. L’état Secure Boot inactif décrit cet essai précis. Le dossier préparatoire 5X citait un inventaire antérieur en mode actif. [Entrée du PE et logique d’affichage](../boot/diagnostic-5w/entree.rs) · [Dossier préparatoire 5X](qualification-live-5x/a8-msi.md)

## Ce que le laboratoire permet de recouper

- **Identité de l’image.** Depuis la base 5X `c25b6db8b2cb40c93542319bbeee8b667e9304aa`, la MOE a reconstruit un fichier image à la capacité exacte. Le SHA-256 du fichier entier concorde avec celui du support communiqué par la MOA. Le PE `EFI/BOOT/BOOTX64.EFI` a pour SHA-256 `da718b3944579c3b53e82c1f489eeecd7899ea5097b8a899bb5f884e49665355`. La relecture valide les deux FAT32 NOVAOS et NOVALOGS, les 24 emplacements de journal vides dans l’image initiale et la GPT secondaire au LBA 7 823 359. La clé physique n’a pas été relue en 5Y. [Générateur 5W](../boot/diagnostic-5w/image.py) · [Contrôle à capacité exacte](validation/diagnostic-uefi-5y/copie.json)
- **Disques internes.** En 5W/5X, QEMU a mesuré six lectures SATA avant l’entrée du PE et zéro E/S SATA supplémentaire dans les intervalles EFI instrumentés. En cas de rejet par OVMF, le témoin a compté 58 lectures depuis le reset. Sur le MSI, le retour à Windows établit le résultat opérateur. Les E/S internes avant, pendant et après le diagnostic et l’intégrité exhaustive des disques Windows n’ont pas été mesurées. [Mesures et limites QEMU](diagnostic-uefi-5x.md)
- **DMA et noyau.** L’affichage de zéro activation porte sur le diagnostic. Il laisse ouverts le bus mastering hérité du firmware, les transactions en vol et les droits réels des domaines. La protection DMA du MSI avant chargement ou pendant l’inventaire reste indéterminée. L’EFI s’est arrêté localement sans transfert au noyau NovaOS complet. Les qualifications antérieures du profil RENFORCÉ gardent leur périmètre. [Portée des deux profils](diagnostic-uefi-5w.md)

## Préparer la levée de l’ambiguïté VT-d

1. Reprendre hors ligne les collectes privées déjà disponibles et leur synthèse ACPI. Elles décrivent deux DRHD et une unité par défaut candidate commune au NIC et au SATA. Associer à chaque portée son segment, son type, son bus initial et le chemin de ponts jusqu’au RID. Inclure xHCI, SATA, les deux fonctions réseau, les fonctions graphiques et les portées spéciales IOAPIC/HPET. L’écran 5Y donne quatre exemples sur seize fonctions. Marquer tout chemin manquant comme inconnu. [Étude multi-unités 5U](diagnostic-uefi-5u.md)
2. Construire un jeu de données synthétiques minimal pour `novaos_vtd_multi::decrire`. Distinguer un conflit de ponts, de portées, d’unité par défaut ou d’identifiant de portée spéciale. Conserver un cas témoin attribuable et un cas qui doit rester ambigu. Demander au futur rapport d’identifier la règle et les deux candidats concernés. Le conflit du catalogue de pilotes et le refus d’attribution VT-d sont produits par deux mécanismes distincts. La cause de `Ambigu` sur ce MSI reste à démontrer. [Résolveur et cas de refus](../boot/diagnostic-5u/lib.rs) · [Tests synthétiques](../boot/diagnostic-5u/tests.rs)
3. Retenir comme critère de résolution une attribution déclarative unique pour chaque RID couvert, avec justification des alias et réservations. Garder la garde réseau/stockage et zéro activation tant que titulaires, domaines, permissions et ressources en vol ne sont pas établis indépendamment. Une ambiguïté persistante doit rester visible.

## Prochaine étape vers le noyau

Préparer un essai QEMU distinct du diagnostic terminal. Réutiliser le chargeur Limine, la configuration et le noyau ELF existants. Figer et vérifier les artefacts attendus, limiter leur origine à l’image externe, puis établir le transfert de la carte mémoire et la fin des services UEFI. Réserver explicitement les pages encore utilisées par le chargeur, le noyau, la pile et la surface d’affichage. Qualifier la clé de carte mémoire périmée et le refus avant transfert en cas de composant invalide. [Configuration de démarrage existante](../boot/limine.conf) · [Services de démarrage UEFI](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html)

Exiger deux observations indépendantes dans le même démarrage de laboratoire : atteinte de `nova_start` dans l’ELF identifié et émission par le noyau d’un jalon après initialisation mémoire. Un jalon ultérieur devra prouver le passage à l’ordonnanceur. Suivre en parallèle les compteurs d’E/S internes et l’absence d’activation réseau ou stockage. Un résultat d’écran du premier EFI seul ne satisfait aucun de ces critères. La candidature physique du noyau fera l’objet d’un mandat séparé. [Entrée et initialisation du noyau](../kernel/src/main.rs)

## Préparation différentielle d’une future clé

Une image clairsemée économise de l’espace sur l’hôte. Sur un support réemployé, ignorer ses trous laisserait subsister des données anciennes. La méthode proposée compare **toute la capacité** en fenêtres de 1 Mio, puis écrit uniquement les blocs différents de 4 Kio, y compris les blocs attendus à zéro. Une dernière fraction de bloc conserve sa longueur exacte. Après synchronisation, relire le support entier, contrôler son SHA-256, les GPT et les FAT32. Une lecture courte, une coupure, une reconnexion ou une différence interdit de valider la copie. Après interruption, réidentifier la même cible et reprendre la comparaison complète.

Sur fichier ordinaire de 4 005 560 320 octets, le témoin simule un préfixe déjà copié et un octet résiduel dans une plage attendue à zéro. **659 456 octets écrits à la première passe, zéro à la seconde** pour ce contenu initial. Sur les deux architectures hôtes, un secteur final de 512 octets a également été corrigé seul et une lecture tronquée d’un octet a provoqué un refus. Le gain concerne les écritures émises par le programme. Les lectures intégrales, les écritures internes du contrôleur flash et la durée sur une vraie clé restent à distinguer. Un média rempli de données différentes peut nécessiter une réécriture complète.

Le témoin crée lui-même son fichier cible temporaire et ne prend aucun périphérique en argument de destination. Avant toute adaptation physique, une autorisation distincte devra couvrir la clé à effacer, l’identité stable du disque entier, le bus externe, l’exclusivité, la géométrie, le FLUSH et la relecture après écriture. Aucune commande d’écriture USB physique n’est livrée en 5Y. [Témoin sur fichier](../boot/diagnostic-5y/simuler_copie.py)

### Reproduire sur fichiers ordinaires

Depuis la racine du worktree, utiliser un répertoire de sortie neuf. Les contrôles hôtes ARM64 natif et x86_64 sous Rosetta ont exercé la même image. Aucune nouvelle campagne QEMU n’était nécessaire au compte rendu physique.

```sh
set -eu
export PYTHONDONTWRITEBYTECODE=1
b=novaOS/build/rejeu-5y
mkdir -p novaOS/build
mkdir "$b"
gzip -dc novaOS/docs/validation/diagnostic-uefi-5w/standard.efi.gz > "$b/standard.efi"
python3 -B novaOS/boot/diagnostic-5w/image.py creer "$b/standard.efi" "$b/standard.img" --octets 4005560320
for isa in arm64 x86_64; do
    /usr/bin/arch "-$isa" /usr/bin/python3 -B novaOS/boot/diagnostic-5y/simuler_copie.py \
        "$b/standard.img" --octets 4005560320 \
        --sha256 0b04a9b2b65c1943575f5429c043d0b80a67d666d307b5e963a4347ed4a75ff4 > "$b/$isa.json"
    cmp "$b/$isa.json" novaOS/docs/validation/diagnostic-uefi-5y/copie.json
done
```
