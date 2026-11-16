# Audit final des sources NovaOS v0.23.0

**Dossier soumis à décision MOA/MOE le 16 novembre 2026.** La stabilisation est intégrée à `origin/main`. Le périmètre proposé pour la clôture comprend la documentation native des 37 commandes du catalogue, deux formes de pipe et la correction des lectures interrompues à tort. La décision officielle de clôture et le tag relèvent d’une autorisation distincte.

## Source intégrée et produits disponibles

| Repère | Valeur vérifiée |
|---|---|
| Base officielle v0.22.0 | `ce8f2d83792e4d83f6e4d7ffbce6400a288fbd27` |
| Fusion de stabilisation publiée, relue sur GitHub | `e5d7124a53e9b0d6a896b7001f717f655c371367` |
| Premier parent, précédent `main` | `816843fa6f5a8ae35f64ef0c3f96a67652bbc384` |
| Second parent, candidat accepté | `d21852c0349e9d70bf2211259df40077c73a53bf` |
| Arbre commun au candidat et à la fusion | `e73ce9e2209449bda3e1e1475ac15053ac3e312e` |
| Versions de construction | Noyau, nsh et nova-cli 0.20.0. SDK Rust nova-system 0.2.0. ABI native v1, 48 sélecteurs. NovaFS 0.9.13, format P9 |
| Dernière image UTM distribuée | v0.17.0 AArch64, générée le 14 novembre 2026 depuis `b76c863c9302bf6ead53fac3b80b26df0513fe0d`. L’archive contient le shell et vi de cette version. Le manuel bilingue, les deux pipes et les corrections de lecture de v0.23.0 relèvent des sources intégrées. |

Le push sans force a publié le seul commit de fusion à deux parents. Les 24 autres références annoncées par GitHub étaient identiques juste après publication. Le worktree principal a conservé son HEAD local, sa modification de `.gitignore` et ses deux liens non suivis. Les trois chantiers GREP, SED et AWK ont reçu le SHA publié et la demande de vérifier leurs dépendances et de requalifier les comportements affectés avant toute proposition de fusion. Leurs travaux suivent leurs branches respectives. [Produits distribués et provenance](audit-0220-abi-livraisons.md) · [Contrats et versions de l’ABI](registre-abi.md)

## Fonctionnalités intégrées

| Lot | Commandes couvertes ou fonction livrée | Preuve de qualification |
|---|---|---|
| ARC-026, huit pilotes | `nsh`, `service`, `journal`, `ls`, `rm`, `cd`, `vi`, `man` | 71 observations et neuf parcours de pages par ISA. Les versions indiquent le composant réel. |
| Extension 1, lecture | `pwd`, `cat`, `head`, `tail`, `wc`, `more` | 129 observations par ISA |
| Extension 2, mutations | `mkdir`, `rmdir`, `touch`, `cp`, `mv` | 108 observations par ISA |
| Extension 3, terminal et observation | `help`, `history`, `clear`, `ps`, `top`, `true`, `false` | 99 observations par ISA |
| Extension 4, langage et session | `alias`, `unalias`, `export`, `unset`, `echo`, `test`, `[`, `break`, `continue`, `exit`, `return` | 174 observations par ISA |
| Premier pipe | `cat fichier \| wc -l` transmet les octets entre deux ELF isolés et compte les LF. | 40 observations par profil, saturation, ordre des octets, EOF et collecte |
| Second pipe | `cat fichier \| head -n 1` ferme la lecture après la première ligne. Le producteur suspendu est réveillé. | 41 commandes et 11 témoins relus par profil, comparaison avec GNU head |
| Service de fichiers | READDIR lie son curseur à la révision du répertoire. READ/CONTINUE revalide le fichier tenu par le client après un changement de génération du volume. | Deux clients réels, modifications concurrentes de la cible ou d’un objet distinct, révocation et relecture |

**Documentation.** Un catalogue commun fournit `--help`, `--version` et `man` aux commandes internes et autonomes. Le manuel permet défilement, recherche, navigation au clavier et retour à l’invite. Une page absente donne le statut 20. L’option inconnue en position d’option donne 3 avant l’action. Pour les commandes à chemins, `--` permet de désigner un nom commençant par un tiret. Les consultations qualifiées retournent 0 sans droit métier. Les fenêtres de mesure ont donné zéro écriture et zéro FLUSH, avec le même SHA-256 du disque avant et après. L’aide de `service stop` émet zéro requête STOP et un témoin positif vérifie la sonde. Les ELF documentaires isolés reçoivent la préférence linguistique sans création de temporaire P9. [Catalogue et tests des pilotes](documentation-v0.23.0-phase1.md) · [Quatre lots et contrôles des scripts](documentation-v0.23.0-phase2.md)

**Langue et supervision.** Une étiquette BCP 47 admise de langue principale exactement `fr`, sans distinction de casse, choisit le français. Les scripts, régions et variantes explicites sont contrôlés dans les tables IANA embarquées. `fr-CA` et `fr-Zzzz` donnent le français, `fr-zzzzz`, `frr`, une locale absente ou invalide donnent l’anglais. Le catalogue documente les aides imbriquées de service et de journal. Le profil ordinaire de `service list` a donné les entrées 1 clock, 2 tmp-health, 9 storage-info, 10 cache et 16 journal. La recette a exercé aussi le nom canonique `storage-info`. Les sondes facultatives de supervision 3 à 8 et la recette intégrale des mutations de service restent des points distincts pour la stable serveur. [Politique de langue éprouvée](cloture-documentation-v0.23.0-phase1.md) · [Matrice serveur](reconciliation-v0300/acceptation-serveur.md)

**Flux et droits.** Un pipeline au premier plan emploie deux processus, un contrôleur et une file de huit trames de 128 octets, plus au maximum une offre suspendue. Le lecteur reçoit le droit de réception et ses sorties terminal. L’écrivain reçoit le droit d’envoi et les droits de lecture sur ses sources. Le shell collecte les deux statuts et expose celui du lecteur dans `$?`. Sur le témoin de 64 Kio, le lecteur consomme une trame, affiche les neuf octets de la première ligne et quitte. Une écriture interrompue donne `cat=11 (CLOSED), head=0`. Le petit fichier déjà acquitté donne `0/0`. Les refus de lancement, les clés révoquées, la collecte unique et la restitution des ressources ont été testés. L’extension de SPAWN porte sur ses variantes de programme et le descripteur de flux. Le registre v1 et les descripteurs ordinaires restent compatibles. [Premier pipe](pipe-natif-v0.23.x.md) · [Second pipe](pipe-head-v0.23.x.md)

## Stabilisation et non-régression

Quatre observations relevaient du curseur de répertoire qui comparait la génération globale après une publication indépendante du journal. Elles concernaient `ls -a /`, `ls -latr --color /`, le glob récursif depuis la racine et `ls src/`. Le dernier cas a échoué dans 4 des 18 essais ciblés x86_64/P9-4096 sur le code instrumenté initial. Sur le correctif, les 18 essais ciblés et cinq parcours complets de nsh sur ce profil ont réussi. Le refus de `vi tronque` venait d’un contrôle distinct entre fragments du fichier. Sur le correctif, vi atteint son diagnostic UTF-8 et la modification effective du fichier ou la révocation de son handle provoque toujours un refus. [Causes, essais individuels et séries](stabilisation-v0.23.x.md)

L’identité de l’arbre et la relecture des archives permettent de reprendre les 24 passages réussis des cinq suites concernées, huit passages de navigation et 32 régressions complémentaires. Ils couvrent les deux ISA, les secteurs de 512 et 4 096 octets, les pipes, l’IPC, les processus, les coupures F1/F2, les remontages et la compatibilité P8. Les neuf contrôles du registre ont été rejoués avant et après publication. Les quatre ELF SDK historiques ont été exécutés sans recompilation dans la qualification du même arbre. Le moteur NovaFS et le registre des sélecteurs ont des arbres ou des objets Git identiques à v0.22.0. [Résultats et références de preuve](validation/audit-cloture-v0.23.0.json)

Les qualifications précédentes référencent 153 fichiers pour ARC-026 phase 1, 406 pour la phase 2, 172 pour le premier pipe et 170 dans l’archive publiée du second. Des scénarios et des fichiers figurent dans plusieurs campagnes. Les 2 753 originaux recomparés dans cet audit constituent le lot de stabilisation. Le script d’installation `/verification.nsh` est inventorié à part des 37 noms du catalogue public. Toute nouvelle commande ajoutée à la distribution devra passer la recette ARC-026. [Inventaire des commandes et du script](reconciliation-v0300/inventaire-commandes.md)

## Mémoire et performances

Les chiffres ci-dessous concernent les invités QEMU 11.1.3 TCG avec 256 Mio. La mémoire de nsh provient de `ps` dans la session du banc terminal, celle de vi du pic de pages privées observé par son banc. La mémoire du pipe provient des compteurs de tout l’invité à deux vCPU. Les pics de session comptent des cadres de 4 Kio. Le témoin de `head` exécute `pwd` dans une session distincte. Ces mesures proviennent de l’archive de stabilisation relue.

| ISA / secteur P9 | nsh privé (Kio) | vi, pic observé (Kio) | Pic de cadres, témoin / session ligne longue | Cadres libres après collecte | Pages DMA après collecte |
|---|---:|---:|---:|---:|---:|
| AArch64 / 512 | 544 | 416 | 1 053 / 1 273 | 61 876 | 0 |
| AArch64 / 4 096 | 544 | 416 | 764 / 1 289 | 61 876 | 0 |
| x86_64 / 512 | 564 | 416 | 1 056 / 1 292 | 62 418 | 9 |
| x86_64 / 4 096 | 564 | 416 | 1 050 / 1 276 | 62 418 | 9 |

Après collecte, chaque profil retrouve les cadres libres et les pages DMA de son témoin, avec zéro page IPC, zéro page utilisateur et zéro processus dans le relevé final du banc. Les neuf pages DMA x86_64 étaient déjà présentes dans le témoin. En phase 1, nsh après consultations affichait 512 Kio sur AArch64 et 532 Kio sur x86_64. Les scénarios diffèrent. Le pic propre au lecteur `man` pendant le rendu reste à mesurer.

**Chronométrage complémentaire du 16 novembre.** Le lanceur de la fusion a redémarré les ISO ordinaires qualifiées, identifiées par SHA-256, sur des copies des volumes P9 fictifs. Une VM à la fois, deux vCPU, 256 Mio, français `fr-CA`, terminal série. Chaque profil a réalisé un passage de chauffe puis huit observations par commande, dans le même ordre. Les cellules indiquent la **médiane / le maximum**, en millisecondes. L’annexe conserve les 144 observations individuelles, chauffe comprise.

| ISA / secteur P9 | `rm --help` | `man rm`, premier écran | `cat grand \| wc -l` | `cat grand \| head -n 1` |
|---|---:|---:|---:|---:|
| AArch64 / 512 | 6,02 / 7,01 | 201,08 / 205,55 | 271,60 / 278,64 | 76,92 / 77,46 |
| AArch64 / 4 096 | 5,97 / 7,20 | 204,40 / 205,91 | 266,60 / 275,83 | 78,58 / 81,49 |
| x86_64 / 512 | 4,19 / 5,65 | 163,45 / 167,04 | 247,60 / 298,06 | 76,91 / 79,08 |
| x86_64 / 4 096 | 3,41 / 6,25 | 163,97 / 164,99 | 227,62 / 235,49 | 68,56 / 70,87 |

L’horloge monotone hôte couvre l’envoi de la commande, la sortie UART, la vérification du statut 0 par un marqueur et le retour de l’invite. Pour `man`, la durée publiée s’arrête à la réception du pied du premier écran. La sonde de géométrie reste sans réponse en série et le lecteur emploie 80 × 24. Pour les pipes, `grand` contient 65 536 octets et deux LF. `wc` compte les deux LF après lecture complète et `head` affiche uniquement `premiere` suivie du LF. Les temporaires ordinaires et la collecte sont inclus dans les latences de commande. Une seule session par profil et huit échantillons après chauffe permettent de décrire ces mesures. Le débit interne du transport, les queues de distribution en charge et le temps CPU propre à chaque processus restent non mesurés. [Séries, empreintes et profils](validation/audit-cloture-v0.23.0.json)

## Limites et décision soumise

- **Portée de v0.23.0.** La base documentaire ARC-027 validée réserve à v0.23.x une réalisation ciblée des pipes. La proposition antérieure de poste d’administration texte mentionne aussi NovaTUI, les redirections et les jobs. Les pièces de qualification de ce jalon couvrent les deux formes de pipe décrites ici. Les chaînes arbitraires, les pipelines à trois processus, les redirections, les jobs et `pipefail` restent hors de ce périmètre. Les lots GREP, SED et AWK nécessitent leurs propres décisions d’admission et d’intégration. [Feuille de route validée](https://github.com/NaviStorm/NovaOS/blob/89d5924f97b2dfc35806c697f439f6503c301118/novaOS/docs/reconciliation-v0300/arc-027/lots.md) · [Proposition antérieure](reconciliation-v0300/proposition.md)
- **Portée de qualification.** Les campagnes du jalon portent sur QEMU direct. L’application UTM, un serveur physique, une coupure électrique réelle et les performances sous charge soutenue restent à qualifier. La campagne longue du moteur NovaFS a été conservée à sa révision de référence. Le nouveau chronométrage emploie les binaires archivés de l’arbre intégré.
- **Décision demandée.** La MOA/MOE peut prononcer la clôture des **sources** pour ce périmètre et ces réserves. L’image distribuée, le SDK et les versions affichées gardent les numéros du tableau de provenance. Le tag officiel attend une décision séparée.

**Conservation.** Avant la fusion, les 2 753 originaux et les 2 753 membres décompressés de la copie complète ont correspondu au manifeste du candidat. Les 1 545 fichiers de preuve de l’archive versionnée ont également été relus. La nouvelle archive durable `OS-worktrees/archives-audit-cloture-0230/audit-cloture-v0.23.0.tar.gz` conserve les 35 fichiers de ce mandat, dont les quatre séries UART, les copies de disques de mesure, le programme de mesure, les contrôles ABI et les relevés Git. Tous ses membres ont été relus. Son SHA-256 est `55243f773be2e4fc4a184d02e10fa6783a201dcddf8c47666bea1cba3bea9238`. La copie est sur le même Mac que les originaux. [Preuves versionnées de stabilisation](validation/stabilisation-v0.23.x.tar.xz) · [Annexe chiffrée de l’audit](validation/audit-cloture-v0.23.0.json)
