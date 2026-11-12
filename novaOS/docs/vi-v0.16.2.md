# NovaOS v0.16.2 — Éditeur vi plein écran

## Installation dans UTM

Sur un Mac Apple Silicon avec UTM 5.0.8 et `qemu-img`, exécuter ces commandes depuis la racine du dépôt. Elles créent une VM et un disque vierge de 64 Mio dans un nouveau répertoire. [Archive AArch64](../../distribution/images/novaos-0.16.2-utm-aarch64.tar.gz)

```sh
NOVA_TRAVAIL="$(mktemp -d "$HOME/Documents/NovaOS-vi-0.16.2.XXXXXXXX")"
NOVA_DIST="$NOVA_TRAVAIL/novaos-0.16.2-utm-aarch64"
NOVA_VM="$NOVA_TRAVAIL/machine/NovaOS-installation-AArch64.utm"
tar -xzf distribution/images/novaos-0.16.2-utm-aarch64.tar.gz -C "$NOVA_TRAVAIL"
(cd "$NOVA_DIST" && shasum -a 256 -c SHA256SUMS) &&
    python3 novaOS/scripts/paquet_installation.py creer "$NOVA_DIST" "$NOVA_TRAVAIL/machine"
```

1. Ouvrir la VM dans UTM. Conserver le port série en mode **Terminal**. L’installateur doit afficher un disque vierge de 67 108 864 octets, série `NOVA-CIBLE-000000001`, SID 56. Pour ce seul disque, saisir `AUTORISER NOVA-CIBLE-000000001 67108864`.
2. Attendre `INSTALLATION bilan Ok(())`, arrêter la VM et retirer son raccourci de la bibliothèque en conservant ses fichiers.
3. Exécuter `python3 novaOS/scripts/paquet_installation.py systeme "$NOVA_VM"`. Rouvrir la VM et la démarrer. Le disque système présente l’invite `nsh>`. [Construction depuis les sources](cli-v0.15.1.md#image-et-installation-dans-utm)

## Tester vi dans la fenêtre Terminal

1. Saisir `vi toto` avec un nom libre, puis `i` et `abcdefghijklmnopqrst`. La première ligne contient les vingt caractères une seule fois. Le curseur est juste après le `t`, le statut indique `1:21`.
2. Ajouter vingt lignes, appuyer sur Échap, naviguer avec les flèches puis `hjkl`. Le statut reste sur l’avant-dernière ligne et la commande sur la dernière. `x` supprime un caractère, `u` l’annule, Ctrl+R le rétablit.
3. Saisir `:w` et Entrée, vérifier le message de sauvegarde, puis `:q`. Rouvrir le fichier, le modifier et essayer `:w`. Le nom occupé est protégé et le tampon reste disponible. `:q!` abandonne la modification. Pour la conserver sous un nom libre, saisir `:w autre-nom` et confirmer dans `nsh`.
4. Tester `:wq` sur un second nom libre. Quitter `nsh` avec `exit`, attendre `SYSTEME bilan Ok(())`, arrêter et redémarrer la VM. Rouvrir `toto` et relire le second fichier avec `cat`.

## Cause et correction

En v0.16.1, chaque frappe reconstruisait une vue de cinq lignes au plus, son curseur représenté par `▏`, le statut et le message. Chaque vue commençait par LF et comportait plusieurs LF. La console imprimable rejetait ESC et traduisait chaque LF en CR/LF sur l’UART. Aucun mouvement absolu ne revenait aux cellules déjà affichées. UTM ajoutait donc correctement chaque nouvelle vue au défilement. Le banc de l’application remplaçait son terminal graphique par un PTY et attendait des chaînes dans le flux. Il vérifiait le contenu P9 et les commandes, mais ne mesurait aucune position à l’écran.

Dans Vim, `drawscreen.c` distingue les régions valides de celles à recalculer. Une modification de texte, un changement de fenêtre et un redimensionnement ont des portées différentes. `term.c` réalise les opérations terminal, entre et sort du mode plein écran, positionne le curseur et traite les réponses aux requêtes. La licence demande de conserver son texte lors de la redistribution de sources de Vim, même partielles, et fixe des conditions pour distribuer un Vim modifié. L’implémentation NovaOS est écrite en Rust à partir de ces principes et du contrat VT. Elle contient zéro ligne importée de Vim et utilise les licences existantes du projet. [Rendu de Vim](https://github.com/vim/vim/blob/master/src/drawscreen.c) · [Terminal de Vim](https://github.com/vim/vim/blob/master/src/term.c) · [Licence de Vim, sections II et IV](https://github.com/vim/vim/blob/master/runtime/doc/uganda.txt)

### Contrat terminal

`CONSOLE` conserve son numéro et ses flux imprimables. Le sélecteur de présentation `SCREEN` (65) vérifie conjointement les trois droits de session `INPUT | OUTPUT | EDIT`. Le noyau construit les contrôles VT depuis des arguments numériques validés. L’ELF de `vi` conserve ses flux texte et deux ports privés vers `nsh`. Il demande la géométrie, le positionnement et l’effacement jusqu’en fin de ligne au courtier. Une commande ordinaire ne peut pas fournir ses propres séquences d’échappement. La confirmation d’une nouvelle destination se déroule sur l’écran du shell et relève de son entrée série. [ABI partagée](../abi/terminal.rs) · [Courtier de session](../../components/shell/vi.rs)

| Opération | Effet sur le terminal |
|---|---|
| Entrée / sortie | Écran alternatif DEC 1049. Le retour au shell restaure l’écran précédent. Le courtier le quitte aussi lors de la libération de la session. |
| Position / effacement | CUP utilise des lignes et colonnes à partir de 1. EL efface uniquement le reste de la ligne. |
| Taille | Sauvegarder la position, viser la cellule 999 × 999, demander la position effective (DSR/CPR), puis restaurer le curseur. Les frappes arrivées avec la réponse sont restituées dans l’ordre. |
| Redimensionnement | Sur un terminal répondant, une période d’inactivité de 500 ticks (2 s) déclenche une nouvelle mesure. Ctrl+L la demande immédiatement et repeint la vue. |
| Port série sans réponse | La première requête attend au plus 40 ticks pour le premier octet. À défaut de réponse, le rendu emploie 80 × 24. Les réponses et frappes anticipées sont bornées à 64 octets par requête. |

Le gap buffer et l’historique conservent leurs positions dans le texte. La barre d’état indique la ligne et la colonne à partir de 1. La vue garde l’origine verticale et horizontale, un cache des lignes et la position du curseur physique. Pour une ligne de texte modifiée, elle saute le préfixe commun, écrit le suffixe puis efface une ancienne fin de ligne si nécessaire. Les lignes inchangées restent en place. Le déplacement au-delà du bord ajuste la fenêtre. Le rendu ordinaire émet zéro CR/LF et zéro effacement global. À une ou deux lignes physiques, la vue donne la priorité au texte ou à la commande en cours. Le cache graphique peut réduire sa taille en cas de pénurie. [Rendu Rust](../../components/vi/screen.rs) · [Tampon, historique et sauvegarde](vi-v0.16.1.md)

Les dimensions adressables vont de 1 à 999 par axe. Les tabulations affichent quatre espaces. Les scalaires de largeur nulle ou impropres à l’affichage utilisent un caractère de remplacement dans la vue et conservent leurs octets dans le fichier. Le déplacement suit les scalaires UTF-8. L’éditeur ne compose pas les grappes de graphèmes et n’effectue pas de retour visuel à la ligne pour les longues lignes. [Contrôles VT](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Rejouer les contrôles du rendu

```sh
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" vi-ecran-test vi-test vi-memoire-test vi-volumes-test
done
```

Le banc adresse le véritable ELF dans chaque invité, décode CUP et EL et reconstruit les cellules. Il échoue sur un contrôle inattendu, un retour automatique ou un curseur hors écran. Il vérifie chaque frappe, l’effacement d’une ancienne fin de ligne, le déplacement de la fenêtre après vingt lignes, la ligne de commande et la restauration de `nsh`. Il injecte une frappe avant la réponse de géométrie et redimensionne une session modifiée de 48 × 12 à 36 × 8, puis à 1 × 1 et de nouveau à 36 × 8. [Test de flux et curseur](../tests/vi_ecran.py)

## Qualification dans l’application UTM

Le 12 novembre 2026, sur Mac Apple Silicon, l’installateur de l’archive a préparé un disque VirtIO QCOW2 vierge à secteurs de 512 octets sous UTM 5.0.8 / QEMU 10.0.12. Le système testé en fenêtre Terminal a démarré avec ce seul disque, TPM CRB et GICv3 sans ITS. Les captures ci-dessous proviennent de la fenêtre de la VM `NovaOS-vi-0.16.2-059065DD`.

| Observation | Preuve |
|---|---|
| Vingt frappes indépendantes dans `vi toto` laissent un seul exemplaire du texte à l’écran et le curseur en colonne 21. | [Mode insertion](validation/vi-v0.16.2/01-vingt-caracteres.jpg) |
| Après l’ajout de vingt lignes et la navigation par les flèches et `hjkl`, le curseur est sur le dernier caractère de la ligne 21. | [Mode normal](validation/vi-v0.16.2/02-navigation.jpg) |
| `x`, `u`, Ctrl+R puis une insertion ont transformé `Ligne 20` en `Ligne 2X`. La commande `:w` occupe la dernière ligne. | [Mode commande](validation/vi-v0.16.2/03-commande.jpg) |
| Après `:w`, le message de sauvegarde occupe la ligne de commande. `:q` rend l’écran principal à `nsh`. | [Sauvegarde acquittée](validation/vi-v0.16.2/04-sauvegarde.jpg) |
| Après agrandissement de la fenêtre de 560 × 376 à 2 992 × 1 801 pixels, le statut rejoint le bas de la nouvelle vue. | [Fenêtre agrandie](validation/vi-v0.16.2/05-redimensionnement.jpg) |
| Après réouverture, `:w` a refusé l’écrasement de `toto` et `:q!` a abandonné la modification. `:wq` a créé `nouveau`. Après redémarrage, `cat nouveau` a rendu `Temoin wq` et `vi toto` a retrouvé les 21 lignes. | [Réouverture après redémarrage](validation/vi-v0.16.2/06-apres-redemarrage.jpg) |

Après arrêt de la VM, le lecteur hôte a vérifié la GPT et le volume P9 exporté, puis comparé chaque octet des deux fichiers de la génération 13 aux données de l’essai.

| Fichier | Octets | SHA-256 |
|---|---:|---|
| `/toto` | 200 | `f0dc81a6cdc66d0678aca031b12d4b8c6eb5ef8fa4a9f3999d0edbd1c3e7babd` |
| `/nouveau` | 9 | `8f87cf38443e7f5e2ba2b758fc7c6858fe2f18b3db404a7b979aa5733f8e015e` |

Le banc UTM sur PTY a également relu les scripts et les fichiers de 66 560 et 66 562 octets après un démarrage à froid. Pendant un premier lancement simultané de deux VM de test, la fenêtre graphique est restée noire et la commande hôte d’arrêt du banc PTY a expiré après le bilan de succès de NovaOS. La reprise sur le même disque a abouti. Pour l’essai visuel de l’archive finale, l’opérateur a lancé le profil depuis le bouton Démarrer de sa fenêtre, puis redémarré depuis cette même fenêtre. Les observations ci-dessus portent sur cette session.

L’archive installable contient 27 fichiers contrôlés par le manifeste interne. Son SHA-256 est `0c97eeffebe82c488f43fe479e83d4758568d9d00599d53510bb1e920a9f3c06`. Le support `installateur.qcow2` a pour SHA-256 `060b2f0736a7a44d8e27c19ae8e2d65b392315f78c4b5e033abb9fc8c848ff97`.

## Régressions

| Suite | AArch64 | x86_64 |
|---|---:|---:|
| Démarrages historiques / groupes bloc / contrôles ELF, processus et IPC / profils CLI | 25 / 24 / 3 / 4 | 25 / 24 / 3 / 4 |
| Groupes fichiers et espaces de noms | 68 | 68 |
| Commandes `nsh` / scripts par géométrie de secteur (512 et 4 096) | 113 / 31 | 113 / 31 |
| Tampon et historique, exécutés sur Darwin natif / Rosetta | 6 | 6 |
| Géométries du banc de cellules VT | 3 | 3 |
| Profils de sécurité de vi, avec révocation et publication concurrente | 2 | 2 |
| Fichiers P9 modifiés, annulés, rétablis et publiés sous un autre nom | 8 Kio, 64 Kio, 1 Mio, 4 Mio | 8 Kio, 64 Kio, 1 Mio, 4 Mio |
| NovaFS standards / instrumentés / opérations différentielles | 196 / 197 / 1 000 000 | 196 / 197 / 1 000 000 |
| Injections sur ancres / pool chiffré | 2 304 / 12 | 2 304 / 12 |

Pour un fichier de 4 Mio, les sondes ont relevé 78, 1 134 puis 3 230 pages privées de 4 Kio, pile comprise, sur chaque architecture. Les trois mesures correspondent au lancement de l’ELF, à l’ouverture du tampon avant le premier rendu, puis à la sauvegarde après modification. Le lecteur hôte a comparé les quatre originaux et leurs quatre copies en entier.

Les invités de développement utilisent QEMU 11.1.3. Le moteur QEMU 10.0.12 d’UTM a aussi passé 11 scénarios GICv3, 23 scénarios de DMA séquentielle et 19 scénarios GPT par géométrie. Parmi ces derniers, 13 refus par géométrie ont conservé le SHA-256 du disque avec zéro écriture et zéro FLUSH. Les témoins GIC monodisque et DMA séquentielle emploient chacun leur firmware et leur injection de retrait propres. Un premier lancement croisé de ces profils a produit un refus d’identité SMBIOS et un jalon de quarantaine manquant. Les résultats indiqués ici proviennent des profils historiques appariés. [Résultats, empreintes, mesures sur fichiers et captures](validation/vi-v0.16.2/resultats.json)
