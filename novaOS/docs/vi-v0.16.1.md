# NovaOS v0.16.1 — Tester vi

Les essais historiques ci-dessous qualifient le tampon et le stockage. Le défaut de défilement observé dans la fenêtre Terminal UTM est corrigé en v0.16.2. [Image, procédure et captures du rendu corrigé](vi-v0.16.2.md)

## Essai au terminal

Au prompt `nsh>`, ouvrir `vi essai.txt` avec un nom libre. Appuyer sur `i`, écrire `été`, puis Échap. Appuyer sur `a`, ajouter `!` et revenir en mode normal avec Échap.

| Touches en mode normal | Texte attendu |
|---|---|
| `u` | `été` |
| `u` | Tampon vide |
| Ctrl+R | `été` |
| Ctrl+R | `été!` |
| `:wq` puis Entrée | Retour au shell. `cat essai.txt` relit `été!`. |

Ouvrir de nouveau le fichier, appuyer sur `x`, puis `u`. Une nouvelle modification après l’annulation abandonne la branche de rétablissement. Une recherche, un déplacement et une modification refusée conservent cette branche. Pour sauvegarder un fichier existant, saisir `:w essai-2.txt` puis confirmer le nouveau nom libre avec la ligne exacte `OUI` dans `nsh`. Après un refus, on peut continuer l’édition. [Commandes, modes, recherche et protections de sauvegarde](vi-v0.16.0.md#commandes)

## Tampon et allocations

Le tampon utilise un *gap buffer* UTF-8. Une seule allocation contient les deux morceaux de texte et une plage libre. Ouvrir un fichier transfère la propriété de la chaîne reçue. Pour modifier le texte, l’éditeur déplace la plage libre à la position voulue. Une frappe adjacente travaille sur les octets du scalaire. Un changement éloigné déplace les octets intermédiaires. Une recherche littérale ou une sauvegarde peut réunir les deux morceaux dans la même allocation. [Tampon](../../components/vi/buffer.rs) · [Stockage et frontières UTF-8](../../components/vi/gap.rs)

| Structure étudiée | Conséquence pour cet éditeur série |
|---|---|
| Gap buffer | Les frappes se concentrent autour d’un curseur. Les lignes longues et les sauts de ligne utilisent le même stockage. Le rendu peut parcourir deux tranches empruntées. |
| Piece table | L’original peut être partagé avec l’historique. Il faut cependant indexer, scinder et fusionner les segments, puis récupérer les fragments devenus inutiles. |
| Rope | Un arbre équilibré avec des compteurs facilite les accès dispersés par position ou numéro de ligne. Les nœuds et l’équilibrage ajoutent des allocations aux petites éditions. |

Il n’y a ni tableau de 512 lignes ni nombre maximal d’octets propre à `vi`. La plage libre croît à la demande. La réserve anticipée varie de 64 octets à 64 Kio et une allocation au strict nécessaire est essayée si elle échoue. Le marqueur de ligne du gap accélère le calcul du numéro de ligne pour les frappes proches. Le rendu reste borné à cinq lignes de 56 colonnes. Les recherches, la comparaison exacte avec la version sauvegardée et l’affichage de l’extrémité d’une très longue ligne peuvent parcourir le texte linéairement.

La première modification après ouverture ou sauvegarde conserve une copie exacte du dernier texte enregistré. Elle permet notamment de quitter avec `:q` après des opérations indépendantes ayant restauré les mêmes octets. La réserve du texte et du journal précède la mutation. Si l’allocation échoue, le texte, le curseur et les modifications encore rétablissables restent disponibles. Le message est `mémoire insuffisante`. La commande peut être réessayée. Le tampon vide n’alloue aucun octet pour le texte ou son historique. La fenêtre d’affichage est réservée une fois par session.

## Annulation et rétablissement

Une session `i`, `a`, `o` ou `O` forme un groupe jusqu’à Échap. `x` et `dd` forment chacun leur propre groupe. Le journal conserve les offsets, les octets retirés et insérés et la position du curseur. Les insertions contiguës d’un même groupe utilisent un seul enregistrement. `u` rejoue le groupe à l’envers, Ctrl+R dans l’ordre. Lors de la qualification, le rejeu d’une insertion avant un caractère existant restaurait le texte puis laissait le curseur un scalaire trop à droite. Le journal mémorise maintenant sa position au retour en mode normal. Le texte conserve la capacité nécessaire au rejeu. Une première annulation immédiatement après une sauvegarde peut avoir à allouer la copie du texte sauvegardé. [Journal](../../components/vi/history.rs) · [Gestion des modes](../../components/vi/editor.rs)

Le budget de rétention est de **256 Kio**, répartis entre 128 Kio de descripteurs et 128 Kio de capacités pour leurs chaînes. Les allocations se font à la demande. Quand le budget est atteint, les groupes les plus anciens sont retirés en entier. Si le groupe courant dépasse à lui seul sa part disponible après éviction, l’édition continue, l’éditeur affiche `groupe hors budget d’annulation` et ce groupe constitue une frontière d’annulation. La commande de modification suivante peut de nouveau être annulée. Ce budget concerne le journal retenu, les allocations transitoires de croissance et les métadonnées de l’allocateur s’y ajoutent.

## Fichiers et sécurité

L’ancien dialogue privé codait la position de lecture et la taille du texte en 16 bits. L’éditeur et `nsh` emploient désormais 64 bits pour ces deux valeurs. Les trames restent limitées à 127 octets utiles. Le courtier vérifie les longueurs et l’ordre des fragments. Il alloue le tampon d’envoi au fil des octets reçus. Un envoi incomplet ne peut pas déclencher de publication. [Dialogue privé](../../components/libraries/vi_wire.rs) · [Courtier](../../components/shell/vi.rs)

Avant de lancer l’ELF, `nsh` ouvre la source dans son espace de noms avec un handle en lecture, vérifie le contenu puis ferme le handle. Le texte vérifié reste dans le courtier pendant la session. Lors d’une sauvegarde, le texte à publier occupe aussi de la mémoire dans `nsh`. Pendant une sauvegarde, le service crée un temporaire sélectionné, écrit et synchronise par lots d’au plus 64 Kio, puis ferme le handle. Une seule publication vers le nom libre suit le dernier lot. Un nom occupé ou créé par un concurrent est refusé. Une perte d’acquittement reste signalée comme incertaine. Une coupure peut laisser un temporaire selon le contrat de v0.16.0. La primitive de remplacement, PC1 et le format NovaFS P9 gardent leur statut antérieur. [Publication et capacités](vi-v0.16.0.md#sauvegarde-et-capacités)

## Installation dans UTM

Sur Mac Apple Silicon, depuis la racine du dépôt, utiliser UTM 5.0.8 et `qemu-img`. L’archive contient l’installateur, le firmware EDK II expérimental et les notices. [Image AArch64](../../distribution/images/novaos-0.16.1-utm-aarch64.tar.gz)

```sh
NOVA_TRAVAIL="$(mktemp -d "$HOME/Documents/NovaOS-vi-0.16.1.XXXXXXXX")"
NOVA_DIST="$NOVA_TRAVAIL/novaos-0.16.1-utm-aarch64"
NOVA_VM="$NOVA_TRAVAIL/machine/NovaOS-installation-AArch64.utm"
tar -xzf distribution/images/novaos-0.16.1-utm-aarch64.tar.gz -C "$NOVA_TRAVAIL"
(cd "$NOVA_DIST" && shasum -a 256 -c SHA256SUMS) &&
    python3 novaOS/scripts/paquet_installation.py creer "$NOVA_DIST" "$NOVA_TRAVAIL/machine"
```

1. Ouvrir la VM dans UTM. Après vérification du disque vierge de 64 Mio, contrôler la série `NOVA-CIBLE-000000001`, le SID 56 et la capacité 67 108 864 octets. Saisir `AUTORISER NOVA-CIBLE-000000001 67108864` pour cette cible.
2. Attendre `INSTALLATION bilan Ok(())`, arrêter, puis retirer le raccourci de la bibliothèque en conservant les fichiers. Exécuter `python3 novaOS/scripts/paquet_installation.py systeme "$NOVA_VM"`. Rouvrir la VM et démarrer sur le disque seul.
3. Exécuter l’essai au terminal ci-dessus, puis `exit`. Après `SYSTEME bilan Ok(())`, arrêter et redémarrer. Relire les deux noms avec `cat`.

Pour reconstruire les deux ELF et le support, employer le `NOVA_DIST` de cette livraison avec les commandes de construction du profil GPT. [Construction depuis les sources](cli-v0.15.1.md#image-et-installation-dans-utm)

## Reproduire les mesures

```sh
for architecture in aarch64 x86_64; do
    make -C novaOS ARCH="$architecture" vi-memoire-test vi-test vi-volumes-test
done
```

Le banc de mémoire compile directement les modules du tampon avec Rust stable en mode optimisé pour Darwin ARM64 et Darwin x86_64 sous Rosetta. Il injecte des refus d’allocations successives, exerce une éviction de groupes complets et compare 600 modifications UTF-8 et leurs 1 200 rejeux à des chaînes de référence. Les mesures du tas comptent les capacités des allocations détenues par le tampon. Les durées hôtes excluent IPC, stockage et affichage. [Banc mémoire](../tests/vi_memoire.rs)

| Fichier | Tas du tampon ouvert | Tas après modifications, journal compris | Première insertion ARM64 / Rosetta |
|---:|---:|---:|---:|
| 8 Kio | 8 192 o | 160 224 o | 1,8 / 5,3 µs |
| 64 Kio | 65 536 o | 286 688 o | 7,6 / 25,8 µs |
| 1 Mio | 1 048 576 o | 2 301 920 o | 185 / 527 µs |
| 4 Mio | 4 194 304 o | 8 593 376 o | 768 / 2 236 µs |

À vide la structure occupe 152 octets. Chaque mesure utilise une longue ligne, place le curseur au milieu, insère un scalaire puis 1 024 autres et en efface 1 024. Le journal occupe alors 139 232 octets. Sur les quatre tailles, une insertion locale coûte en moyenne 12–17 ns sur ARM64 et 18–24 ns sous Rosetta, un effacement 37–39 ns et 70–119 ns, un déplacement horizontal 7 ns et 5–6 ns. La première insertion comprend la copie de référence et l’ouverture du gap. Les mesures par taille et les mesures de l’ELF dans la VM figurent au relevé de qualification.


## Qualification en VM

Les invités QEMU 11.1.3 disposent de 256 Mio de RAM. Chaque invité a ouvert quatre fichiers sur un volume P9 de 16 Mio à secteurs de 4 096 octets. Celui de 4 Mio compte 32 769 lignes logiques, ligne vide finale comprise. Pour chaque taille, le banc a inséré un scalaire, l’a effacé, a modifié le début du texte, navigué à la dernière ligne, annulé et rétabli plusieurs commandes, puis enregistré une copie sous un nom libre. Après arrêt, un lecteur P9 indépendant a vérifié les deux générations du volume et comparé les huit fichiers, source et copie, intégralement aux octets attendus. [Banc sur fichiers P9](../tests/vi_volumes.py)

| Invité | Fichier | Lancement / ouverture / avant publication (Kio privés) | Ouverture (s) | Insertion / effacement / déplacement G (ms) |
|---|---:|---:|---:|---:|
| aarch64 | 8 Kio | 312 / 376 / 376 | 1,5 | 1,73 / 0,77 / 5,08 |
| aarch64 | 64 Kio | 312 / 504 / 632 | 11,2 | 2,16 / 0,93 / 0,61 |
| aarch64 | 1 Mio | 312 / 1464 / 3704 | 184,3 | 3,92 / 5,06 / 3,55 |
| aarch64 | 4 Mio | 312 / 4536 / 12920 | 663,3 | 18,43 / 17,72 / 12,21 |
| x86_64 | 8 Kio | 312 / 376 / 376 | 1,3 | 1,40 / 0,50 / 0,40 |
| x86_64 | 64 Kio | 312 / 504 / 632 | 9,4 | 4,51 / 4,06 / 4,21 |
| x86_64 | 1 Mio | 312 / 1464 / 3704 | 160,2 | 3,77 / 2,72 / 3,65 |
| x86_64 | 4 Mio | 312 / 4536 / 12920 | 571,4 | 97,77 / 8,71 / 12,88 |

Le noyau a échantillonné les pages privées de l’ELF, pile de 64 pages comprise. Ces trois jalons excluent les autres processus. Les latences vont de l’envoi sur le terminal à l’affichage de la position attendue. L’ouverture comprend les lectures NovaFS, la vérification dans le courtier et le transfert fragmenté vers l’éditeur. Cette durée domine l’essai de 4 Mio. Les régressions s’exécutaient aussi sur le Mac. Les mesures hôtes sur ligne unique et les mesures invitées sur lignes de 128 octets couvrent des charges différentes.

Dans chaque invité et chaque géométrie, l’ouverture d’un fichier creux P9 de **536 870 912 octets** sur une VM de 256 Mio rend `mémoire insuffisante`. Le shell continue, relit la taille et lance les essais suivants. Les quatre tailles de la matrice ont passé la qualification avec les ressources existantes. Les essais de révocation, de publication concurrente, de confirmation refusée et de sauvegarde sous un autre nom ont conservé le contenu attendu. Au total, le banc de sécurité compte 14 processus vi collectés et 13 fichiers relus indépendamment par profil. [Essais sur les deux géométries](../tests/vi.py)

Dans **l’application UTM 5.0.8**, le banc a installé sur un disque vierge, retiré le support, saisi et enregistré 1 024 lignes de texte suivies d’une ligne finale vide, rouvert le fichier, effectué deux annulations et deux rétablissements et publié sous un autre nom. Au deuxième démarrage autonome, la lecture intégrale a retrouvé les 66 560 octets d’origine et les 66 562 octets de la copie, ainsi que les deux scripts édités. Le profil distribué offre une partition P9 de 2 Mio sur son disque de 64 Mio. Les fichiers de 4 Mio ont été qualifiés sur les volumes P9 des invités QEMU de développement. [Banc de l’application UTM](../tests/nsh_utm.py)

### Régressions

| Suite | AArch64 | x86_64 |
|---|---:|---:|
| Démarrages / groupes bloc / contrôles ELF, processus et IPC / essais CLI | 25 / 24 / 3 / 4 | 25 / 24 / 3 / 4 |
| Groupes fichiers et espace de noms | 68 | 68 |
| Commandes nsh / scripts, pour chacune des deux géométries | 113 / 31 | 113 / 31 |
| Tests du tampon sur Darwin | 6 | 6 |
| NovaFS de référence / instrumentés | 196 / 197 | 196 / 197 |
| Injections historiques sur ancres / sur pool chiffré | 2 304 / 12 | 2 304 / 12 |
| Opérations différentielles NovaFS | 1 000 000 | 1 000 000 |

Les 11 cas DMA/GICv3 et les 23 cas DMA séquentiels ont réussi sur AArch64. Pour chacun des secteurs 512 et 4 096, les 19 cas GPT comprennent 13 refus avec SHA-256 identique, zéro écriture et zéro FLUSH. Les échanges entre architectures ont aussi couvert les images CLI historiques et les migrations P9. [Matrice, latences et empreintes](validation/vi-v0.16.1.json)

SHA-256 de l’archive installable (3 672 809 octets, 27 fichiers contrôlés) `43d46a78beecd65b357821e98a9549231185972c47cc77c7fcba6902c6db01c2`.
