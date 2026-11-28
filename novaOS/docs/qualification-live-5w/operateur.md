# Premier diagnostic STANDARD sur le MSI GE62 6QC

**Procédure préparatoire. A8 est fermé.** Les essais 5W ont utilisé exclusivement des fichiers ordinaires et des périphériques émulés. La préparation d’une clé réelle et le démarrage du MSI nécessitent une autorisation physique explicite précisant l’image, la clé à effacer, le mode Secure Boot, le comportement de repli accepté et la personne qui peut décider l’arrêt. [Résultats et limites du profil](../diagnostic-uefi-5w.md)

## Conditions à vérifier avant une candidature A8

| Condition | Vérification attendue |
|---|---|
| Profil choisi | Choisir explicitement STANDARD. Accepter que la protection DMA antérieure au chargement soit indéterminée et que l’affichage relève du firmware. Pour une exigence d’isolation attestée dès le chargement, la décision 5V du MSI reste indéterminée. |
| Démarrage existant | Mode UEFI x86_64 et option ponctuelle USB disponibles sur la configuration existante. Le constructeur documente la touche F11 pour la sélection de démarrage de portables MSI. Confirmer l’entrée UEFI de la clé sur cet exemplaire. En cas d’ambiguïté, quitter le menu sans démarrer. [Procédure MSI pour le menu de démarrage](https://www.msi.com/faq/11370) |
| Secure Boot | Conserver le choix existant du propriétaire. Sur le MSI inventorié, Secure Boot était actif. Avant de proposer le PE à ce firmware, établir son acceptation par la politique existante, révocations comprises. L’autorisation par empreinte dans OVMF est locale au banc. Si la confiance manque, différer l’essai. Sur une machine déjà en mode inactif, l’opérateur doit accepter distinctement la chaîne initiale non authentifiée. |
| Repli et stockage | Faire accepter la conduite à tenir si le firmware refuse le PE ou ne trouve plus la clé. Dans le banc, OVMF a essayé le SATA et effectué 58 lectures après un refus USB, comptées depuis le reset. Un firmware OEM peut lire un disque, démarrer Windows ou déclencher sa récupération avant que NovaOS puisse afficher quoi que ce soit. En présence d’un repli interne imprévu incompatible avec la politique convenue, différer A8. |
| Windows et BitLocker | Disposer d’une sauvegarde vérifiée des données et d’un moyen de récupération Windows. Le propriétaire doit pouvoir retrouver hors de la machine la clé de récupération BitLocker correspondant au volume. Garder cette clé privée. Aucune suspension, désactivation ni modification de BitLocker n’est requise par le diagnostic. [Microsoft, retrouver la clé de récupération BitLocker](https://support.microsoft.com/en-us/windows/security/encryption/find-your-bitlocker-recovery-key) |
| Verdict et arrêt | Prévoir une personne sur place, une alimentation stable et la possibilité d’arrêter le PC et d’enlever la clé après extinction. Convenir des motifs d’abandon, notamment écran vide, origine indéterminée, refus, redémarrage ou menu de récupération imprévu. Le support contient un catalogue de laboratoire et n’active aucun pilote. |

## Vérifier la livraison et préparer le fichier image sur le Mac

Depuis la racine du worktree qualifié, créer un répertoire neuf sur le Mac. Ces commandes manipulent seulement des fichiers ordinaires. L’empreinte attendue doit provenir du commit retenu dans la décision A8.

```sh
set -eu
v=novaOS/docs/validation/diagnostic-uefi-5w
d=novaOS/boot/diagnostic-5w
travail=novaOS/build/preparation-operateur-5w
mkdir -p novaOS/build
mkdir "$travail"
(cd "$v" && shasum -a 256 -c SHA256SUMS)
gzip -dc "$v/standard.efi.gz" > "$travail/standard.efi"
(cd "$travail" && shasum -a 256 standard.efi)
```

Comparer les 64 caractères du dernier résultat à `standard.sha256`. Pour contrôler l’image témoin de 68 Mio, décompresser `novaos-standard-68mio.img.gz` dans ce répertoire et comparer le SHA-256 brut à `79ee5d200f5e90bca19be8f06011dd8cd0b080f086e4f300005a338b55e20060`. Une vraie clé de plus grande capacité exige la génération à **son nombre exact d’octets**.

## Écriture avec dd — uniquement après autorisation physique

1. Utiliser une clé USB dédiée dont l’effacement intégral est autorisé. Débrancher les autres supports externes amovibles. Relever `diskutil list external physical` avant et après insertion puis `diskutil info /dev/diskN`. Remplacer `N` par le numéro effectivement constaté. Vérifier le disque **entier**, le bus USB, le caractère physique et externe, l’identité de la clé, la capacité exacte en octets et des secteurs logiques de 512 octets. Une partition telle que `diskNs1`, un disque interne, une géométrie incompatible ou plusieurs candidats imposent l’arrêt.
2. Dans le même shell, renseigner `OCTETS` avec la capacité vérifiée et `DISQUE_USB` avec le nom du disque entier, sous la forme `diskN`. Générer et relire l’image avant de démonter la clé. Garder le SHA-256 du fichier ainsi produit comme référence de ce support.

```sh
: "${OCTETS:?capacité exacte en octets à renseigner}"
: "${DISQUE_USB:?nom du disque entier externe à renseigner}"
image="$travail/novaos-standard.img"
python3 -B "$d/image.py" creer "$travail/standard.efi" "$image" --octets "$OCTETS"
python3 -B "$d/image.py" verifier "$travail/standard.efi" "$image" --octets "$OCTETS"
shasum -a 256 "$image"
```

3. Contrôler de nouveau l’identité de la cible dans `diskutil info` immédiatement avant l’écriture. Les commandes ci-dessous **écrasent toutes les données du disque sélectionné**. Arrêter si la clé a été retirée ou réattribuée. Conserver le fichier source sur le Mac.

```sh
set -eu
diskutil info "/dev/$DISQUE_USB"
python3 - "$DISQUE_USB" "$OCTETS" <<'CONTROLE_USB'
import plistlib, re, subprocess, sys
disque, taille = sys.argv[1:]
if not re.fullmatch(r"disk[0-9]+", disque):
    sys.exit("Refus : nom de disque entier attendu")
info = plistlib.loads(subprocess.check_output(["diskutil", "info", "-plist", "/dev/" + disque]))
if not (info.get("DeviceIdentifier") == disque and info.get("Whole") is True
        and info.get("Internal") is False and info.get("BusProtocol") == "USB"
        and info.get("DeviceBlockSize") == 512 and info.get("TotalSize", info.get("Size")) == int(taille)):
    sys.exit("Refus : identité, bus ou géométrie non confirmés")
CONTROLE_USB
diskutil unmountDisk "/dev/$DISQUE_USB"
sudo dd if="$image" of="/dev/r$DISQUE_USB" bs=4m
sync
diskutil unmountDisk "/dev/$DISQUE_USB"
sudo shasum -a 256 "/dev/r$DISQUE_USB"
```

4. Exiger l’égalité du SHA-256 de **tout le disque** avec celui de l’image à capacité exacte, puis exécuter `diskutil eject "/dev/$DISQUE_USB"`. Une erreur d’écriture, une copie courte ou une différence après remontage automatique invalide la préparation. Ne pas réparer les partitions pour faire passer le contrôle.

## Démarrage ponctuel et retour à Windows — uniquement après A8

1. Terminer les travaux Windows et effectuer un arrêt normal. Insérer la clé contrôlée. Au démarrage, ouvrir le menu ponctuel MSI et sélectionner l’entrée UEFI de cette clé. Garder l’ordre permanent, les clés UEFI, Secure Boot, le mode SATA et les réglages du TPM tels qu’ils sont.
2. Vérifier à l’écran **STANDARD**, le mode Secure Boot attendu et l’origine USB déclarée. Relever le résultat DMA, les contrôleurs, les candidats et le verdict terminal. Le journal détaillé reste en RAM. Recopier le verdict ou prendre une photo cadrée sur le diagnostic avec l’accord du propriétaire. Éviter tout identifiant privé. NOVALOGS reste vide.
3. Si NovaOS affiche un refus, photographier ou noter sa cause puis terminer l’essai. Si le firmware refuse l’image avant son entrée, un écran NovaOS est impossible. Si Windows ou la récupération démarre déjà, suivre la procédure de retour convenue, sans couper l’alimentation pendant des E/S Windows. En cas d’écran vide ou bloqué dont l’origine est inconnue, suivre la procédure d’arrêt convenue avec l’opérateur. N’accepter aucune réparation ni modification de sécurité pour poursuivre le diagnostic.
4. Une fois l’arrêt local NovaOS constaté, éteindre avec le bouton du PC suivant la procédure prévue. Retirer la clé **machine éteinte**, puis redémarrer volontairement Windows sur son entrée habituelle. Si BitLocker demande la récupération, vérifier l’identifiant présenté et utiliser la procédure privée du propriétaire. Consigner le retour obtenu et toute anomalie.

Les résultats QEMU ne mesurent ni les accès internes indirects du firmware MSI, ni la visibilité réelle de sa console, ni l’immunité du premier PE à un DMA antérieur. Une autorisation d’essai STANDARD ne vaudrait pas qualification de ces propriétés.
