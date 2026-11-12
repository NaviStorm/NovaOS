# NovaOS v0.16.0 — Éditeur vi

Cette page décrit la livraison v0.16.0. Le guide de v0.16.1 présente le tampon dynamique, Ctrl+R, les mesures et la nouvelle image UTM. [Tester vi v0.16.1](vi-v0.16.1.md)

Au prompt `nsh>`, `vi bonjour.nsh` ouvre un fichier texte ou prépare un tampon vide si le nom est libre. Appuyer sur `i`, puis saisir ces deux lignes :

```sh
#!/bin/nsh
echo "Bonjour depuis vi"
```

Appuyer sur Échap, saisir `:wq`, puis Entrée. De retour au shell, `nsh bonjour.nsh` exécute le script. **Si le fichier existait déjà**, enregistrer la version modifiée avec `:w bonjour-2.nsh`, saisir `OUI` à la question de `nsh`, puis quitter avec `:q`. Une sauvegarde réussie fait du nouveau nom le nom courant. Pour réenregistrer après d’autres modifications, choisir encore un nom libre.

## Commandes

| Mode ou touche | Effet |
|---|---|
| Normal | Mode à l’ouverture. `h j k l` ou les flèches déplacent le curseur ; `0` et `$` atteignent les extrémités de la ligne, `gg` et `G` les première et dernière lignes. |
| `i`, `a`, `o`, `O` | Insérer sous le curseur, après lui, sur une nouvelle ligne en dessous ou au-dessus. Entrée insère un saut de ligne ; Retour arrière efface le scalaire précédent ou joint deux lignes. Les flèches restent actives. Échap revient en mode normal. |
| `x`, `dd`, `u` | Supprimer le scalaire courant, supprimer la ligne, annuler la dernière modification. Une session d’insertion compte pour une modification. Un mouvement seul ne consomme pas l’annulation. |
| `/mot`, `n` | Entrée valide une recherche littérale sensible à la casse. La recherche passe une fois par le début du fichier. `n` répète ; Échap annule la saisie. |
| `:w`, `:w autre-nom` | Publier sous le nom courant s’il est neuf, ou demander à `nsh` de confirmer un autre nom libre. Le texte après `w ` est un chemin littéral : espaces admis, sans expansion de variables ni de motifs. |
| `:q`, `:q!` | Quitter si le tampon est inchangé, ou abandonner explicitement les modifications. |
| `:wq`, `:x` | `:wq` quitte uniquement après une sauvegarde réussie. `:x` quitte directement un fichier existant inchangé ; sinon, il doit réussir sa sauvegarde, même pour créer un fichier neuf vide. |

Les commandes de la dernière ligne se valident avec Entrée. Une commande inconnue, un chemin inaccessible ou un écrasement interdit produit un message. Il n’y a ni mode forcé d’écriture, ni macros, commandes shell, expressions régulières, registres, répétitions numériques ou historique d’annulation multiple.

## Affichage et limites

L’ELF de l’éditeur utilise les flux texte du terminal série. Chaque rafraîchissement écrit une fenêtre d’au plus cinq lignes numérotées et 56 colonnes de texte, centrée verticalement autour du curseur dans la limite du fichier. La fenêtre suit aussi les déplacements horizontaux. `▏` indique le curseur logique ; la barre d’état affiche mode, nom, état et position à partir de 1. Les anciens cadres restent dans l’historique du terminal. Le contrat console ne fournit ni dimensions ni adressage libre du curseur : la fenêtre ne se redimensionne pas et ne requiert ni ANSI en sortie, ni ncurses, ni NovaTUI. Les flèches simples CSI et SS3 sont décodées en entrée ; le préfixe série Échap puis `O` suivi de `A` à `D` désigne une flèche SS3.

- **8 192 octets et 512 lignes logiques** au maximum, ligne vide finale comprise. Les contrôles précèdent la modification : une insertion trop grande laisse le tampon intact. Les lignes longues défilent sans troncature du fichier.
- Déplacements et suppressions travaillent sur les scalaires UTF-8, sans normalisation ni segmentation en graphèmes. Une tabulation occupe quatre colonnes à l’écran. Les caractères de largeur nulle et les marques directionnelles interdites par la console sont représentés par `�` dans le texte affiché ; cette représentation ne réécrit pas le fichier.
- UTF-8 malformé, CR isolé et contrôles autres que TAB et fins de ligne sont refusés à l’ouverture. Les fins LF ou CRLF existantes, même mélangées, restent intactes. Entrée insère la première convention rencontrée, ou LF par défaut. Aucun saut final n’est ajouté à la sauvegarde.
- **24 Kio réservés** aux trois textes : tampon courant, dernière version enregistrée et annulation. La ligne de commande et le dernier motif sont bornés à 1 024 octets chacun. L’analyse conserve au plus un préfixe de commande normale, sans récursion. Dans `nsh`, le texte ouvert et le chargement de sauvegarde sont limités à 8 Kio chacun ; le chemin est limité à 1 024 octets. Le rendu et les trames sont eux aussi bornés.
- **372 Kio privés observés pour l’ELF**, pile comprise (93 pages de 4 Kio), sur les quatre profils de développement. Ce sont des échantillons du noyau au lancement, pendant l’édition et avant la publication ; la mesure n’est ni celle du seul tampon ni une borne absolue du pic résident.

## Sauvegarde et capacités

`vi` est un ELF Rust `no_std` distinct du shell (Ring 3 ou EL0). Il reçoit les flux de sortie et d’erreur et deux capacités de session pour parler à `nsh`. Il n’a pas l’entrée UART brute, la racine de montage, une capacité de fichier générique ou le droit de lancer des processus. L’ouverture d’un fichier utilise un handle en lecture ; le shell vérifie identité et longueur et compare les octets si la révision a changé, notamment lors de l’actualisation de la date d’accès. Le dialogue ne permet de relire que le contenu sélectionné à l’ouverture. [Résolution et lecture](../../components/shell/paths.rs) · [Dialogue et délégations](../../components/shell/vi.rs)

1. L’éditeur transmet le nom et le texte en fragments d’au plus 127 octets. Le courtier vérifie l’ordre des messages, les tailles et l’UTF-8 avant de résoudre le chemin dans l’espace de noms du shell. Un répertoire, un nom déjà occupé ou une remontée hors racine sont refusés.
2. L’argument de lancement désigne le seul nom qui puisse être créé sans nouvelle confirmation. Pour tout autre nom, le courtier sélectionne le parent et le nom final dans une délégation temporaire, affiche ce nom et lit lui-même la réponse sur le terminal. Seule la ligne exacte `OUI` autorise la création. L’éditeur ne peut pas fournir cette réponse par IPC. Une réponse négative ne crée pas de temporaire.
3. Le service vérifie de nouveau la disponibilité du nom, crée un fichier temporaire par `CREATE_NEW`, écrit, synchronise et ferme son handle. `PUBLISH_COPY` vérifie l’identité du temporaire et le renomme **sans remplacement**. Les capacités temporaires sont fermées à la fin de l’opération. [Implémentation du service](../user/novafs/files/selected.rs) · [Protocole privé](../../components/libraries/vi_wire.rs)
4. Un refus, une révocation de la délégation de fichier ou une erreur de sauvegarde laisse l’éditeur ouvert avec son texte. Une perte d’acquittement de publication est signalée comme incertaine : vérifier le volume avant une nouvelle tentative. Il n’y a ni relance automatique de la publication ni journal du tampon en mémoire. Une perte du processus peut faire perdre les changements non enregistrés ; une coupure peut laisser un temporaire.

Les opérations P9 disponibles n’offrent pas à cet éditeur de remplacement conditionnel, comparant l’ancienne version et publiant tout le nouveau texte en une seule opération qualifiée. L’éditeur refuse donc d’écrire dans le fichier ouvert, même après confirmation et même si ses octets ont changé depuis son ouverture. Aucune suppression suivie de recréation. L’ABI native, les droits des autres clients, le format P9 et le moteur NovaFS restent inchangés. PC1 et le remplacement transactionnel restent désactivés. [Limites du stockage et de la publication](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

## Installation dans UTM

Depuis la racine du dépôt, sur Mac Apple Silicon avec UTM 5.0.8 et `qemu-img`. [Image installable avec vi](../../distribution/images/novaos-0.16.0-vi-utm-aarch64.tar.gz) — SHA-256 : `a5e8fa8fe2aab33a8daf375c52d5a0cc59dc4d8e85c1023388968d8e3244c29c`.

```sh
NOVA_TRAVAIL="$(mktemp -d "$HOME/Documents/NovaOS-vi-0.16.0.XXXXXXXX")"
NOVA_DIST="$NOVA_TRAVAIL/novaos-0.16.0-vi-utm-aarch64"
NOVA_VM="$NOVA_TRAVAIL/machine/NovaOS-installation-AArch64.utm"
tar -xzf distribution/images/novaos-0.16.0-vi-utm-aarch64.tar.gz -C "$NOVA_TRAVAIL"
(cd "$NOVA_DIST" && shasum -a 256 -c SHA256SUMS) &&
    python3 novaOS/scripts/paquet_installation.py creer "$NOVA_DIST" "$NOVA_TRAVAIL/machine"
```

1. Ouvrir le paquet dans UTM et démarrer. Attendre le contrôle complet du disque neuf de 64 Mio. Vérifier la série `NOVA-CIBLE-000000001`, le SID 56 et la capacité 67 108 864 octets avant de saisir `AUTORISER NOVA-CIBLE-000000001 67108864`.
2. Après `INSTALLATION bilan Ok(())`, arrêter la VM et retirer son raccourci de la bibliothèque UTM en conservant ses fichiers. Exécuter `python3 novaOS/scripts/paquet_installation.py systeme "$NOVA_VM"`, puis rouvrir le paquet dans UTM.
3. Démarrer sur le disque seul et créer le script de l’exemple. Exécuter `exit` au prompt, arrêter après `SYSTEME bilan Ok(())`, puis redémarrer et relancer `nsh bonjour.nsh`.

Le firmware EDK II reste expérimental ; ce profil utilise TPM CRB, GICv3 sans ITS et la révocation du domaine DMA firmware avant la prise en charge du disque par NovaOS. Pour reconstruire, utiliser le `NOVA_DIST` ci-dessus avec la [procédure de construction des deux ELF et du support](cli-v0.15.1.md#image-et-installation-dans-utm).

## Qualification

```sh
make -C novaOS ARCH=aarch64 vi-test
make -C novaOS ARCH=x86_64 vi-test
```

Le [banc de l’éditeur](../tests/vi.py) exerce des secteurs logiques de 512 et 4 096 octets sur chaque architecture. Par profil, 12 instances ELF ont été collectées et 12 fichiers relus octet par octet par un lecteur NovaFS indépendant, dont une copie de 8 192 octets et le script exécuté après redémarrage. Les essais couvrent les modes, l’annulation, les flèches, les lignes longues, les textes vides et invalides, le refus de quitter un tampon modifié, le refus d’écrasement, le refus de confirmation et la conservation du tampon. Un collage contenant CRLF suivi d’un LF vérifie le maintien d’une ligne vide : un double filtrage du LF entre le courtier et le décodeur clavier pouvait auparavant la supprimer. Seul le courtier filtre maintenant la paire CRLF.

Un acteur concurrent publie `tiers\n` entre le contrôle de disponibilité et la création. L’enregistrement de l’éditeur est refusé, et le lecteur hôte retrouve exactement ces cinq lettres et le saut de ligne. Un autre essai révoque les capacités d’une délégation avant création ; le noyau rejette le handle retiré. Sans ressaisir le texte, l’utilisateur peut ensuite l’enregistrer sous un autre nom explicitement confirmé.

Le [banc de l’application UTM](../tests/nsh_utm.py) installe sur une cible QCOW2/512 vierge, retire le support, écrit puis exécute `recettes/edite.nsh` depuis le terminal série, le sauvegarde aussi sous `recettes/autre.nsh` avec confirmation et exécute les deux scripts au deuxième démarrage autonome. Le moteur QEMU 10.0.12 d’UTM exerce séparément GPT et la persistance en 512 et 4 096 octets. L’essai de persistance porte sur un redémarrage de VM, pas sur une perte d’alimentation du Mac.

Sur AArch64, les 11 scénarios DMA/GICv3 et les 23 scénarios de transfert DMA séquentiel ont réussi avec les ELF de cette livraison. Pour chacune des deux géométries GPT, le banc a exercé 19 scénarios de sécurité ; les 13 comparaisons avant/après refus ont retrouvé la même empreinte SHA-256, avec zéro écriture et zéro FLUSH. [Banc GPT](../tests/installation_gpt.py) · [Contrat de protection DMA](installer-v0.15.0.md)

### Régressions par architecture

| Suite | AArch64 | x86_64 |
|---|---:|---:|
| Démarrages / groupes bloc / contrôles ELF, processus, IPC / essais CLI | 25 / 24 / 3 / 4 | 25 / 24 / 3 / 4 |
| Groupes fichiers et espaces de noms | 68 | 68 |
| Commandes nsh v0.15.1, secteurs 512 / 4 096 | 113 / 113 | 113 / 113 |
| Commandes de scripts nsh v0.16.0, secteurs 512 / 4 096 | 31 / 31 | 31 / 31 |
| NovaFS sur Darwin : standards / instrumentés | 196 / 197 | 196 / 197 |
| Injections sur les ancres / injections supplémentaires sur pool chiffré | 2 304 / 12 | 2 304 / 12 |
| Mémoire privée du terminal nsh seul, en Kio (contrôle CLI P9) | 436 | 448 |
| Opérations différentielles NovaFS | 1 000 000 | 1 000 000 |

Les invités NovaOS ont tourné sous QEMU 11.1.3. NovaFS a été exercé en ARM64 natif et en x86_64 sous Rosetta sur macOS 27.2. Les relectures croisées P8 et P9 ont réussi dans les deux sens, sur les deux géométries. [Matrice complète, empreintes et mesures](validation/vi-v0.16.0.json)
