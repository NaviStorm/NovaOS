# NovaFS P2 sur macOS

NovaFS v0.9.2 conserve le format disque P0 et les deux barrières de publication P1. Il fonctionne sur une image logique de LV dont les blocs mesurent 4 096 octets. Le paquet possède son propre workspace Cargo. Le lanceur `lab/run` sélectionne explicitement la cible Darwin.

La commande `check` examine les deux générations d’une image hors ligne et rend un diagnostic JSON, y compris lorsqu’une seule génération reste lisible. `check-pool` contrôle le catalogue du pool de laboratoire et le contenu des volumes déclarés prêts. Ces deux commandes prennent un **fichier ordinaire en entrée standard**. [Utilisation, codes de sortie et stratégie de récupération](docs/novafs-p2-integrite.md) · [Injections de corruption](docs/novafs-p2-injections.md) · [Contrôle des volumes](docs/novafs-p2-pool.md)

P1 exerce les ruptures aux frontières de publication, les erreurs d’E/S partielles et la perte du cache volatil. Les commandes et les limites de chaque essai figurent dans le [rapport de durabilité](docs/novafs-p1-durabilite.md), la [matrice des injections](docs/novafs-p1-coupures.md) et les [mesures comparées avec P0](docs/novafs-p1-mesures.md).

## Créer, inspecter et vérifier une image jetable

Depuis la racine du dépôt, avec Rust stable installé via rustup, le script suivant crée une image temporaire de 2 Mio. Les chemins `/notes` et `/notes/exemple` se trouvent **dans NovaFS**.

```sh
lab=novaFS/lab/run
image=$(mktemp -t novafs-p0)
log=$(mktemp -t novafs-p0-identites)

printf '%s\n' 'mkdir /notes' 'create /notes/exemple' \
  'write /notes/exemple 0 48656c6c6f' 'verify' \
  | "$lab" run -- script 512 4096 > "$image" 2> "$log"

"$lab" run -- inspect 512 512 < "$image"
fs_id=$(sed -n 's/^FS=\([^ ]*\) LV=.*/\1/p' "$log")
lv_id=$(sed -n 's/^FS=[^ ]* LV=\(.*\)/\1/p' "$log")
"$lab" run -- verify 512 512 "$fs_id" "$lv_id" < "$image"
"$lab" run -- check 512 512 "$fs_id" "$lv_id" < "$image"

rm "$image" "$log"
```

`create BLOCS SECTEUR` produit sur stdout une image vierge formatée. `script` effectue ses commandes dans un nouveau fichier anonyme, vérifie puis exporte le résultat. Le formateur génère deux identités de 128 bits et les rend sur stderr. Le moteur refuse de formater une image comportant un seul octet non nul. Une erreur du script empêche l’export de la nouvelle image.

`inspect` lit les deux enveloppes et signale individuellement un slot illisible. `verify` exige les identités attendues, la paire d’ancres, la structure complète et les checksums des données des deux générations. Utiliser les identités enregistrées lors du formatage : récupérer les seules identités d’un support suspect ne prouve pas sa provenance. `salvage BLOCS SECTEUR FS_ID LV_ID SLOT NOM` vérifie le slot explicitement choisi et extrait le fichier sur stdout en lecture seule. Un résultat de sauvetage porte une génération et une fraîcheur indéterminée. Si une lecture de secours échoue en cours de flux, écarter la sortie partielle et consulter l’erreur. Le détail des arguments est accessible depuis la racine via `novaFS/lab/run run -- --help` et `novaFS/lab/run run -- salvage --help`.

L’outil accepte 133 à 32 768 blocs pour une image formatée, des secteurs logiques de 512 ou 4 096 octets et des transferts par flux. Il ne prend aucun chemin de périphérique macOS. L’import copie exactement le flux vers un fichier ordinaire temporaire et conserve le descripteur ouvert. Les commandes d’inspection et de sauvetage ne modifient pas l’image source. L’API de bloc du moteur accepte jusqu’à 8 Gio. Le banc peut tester la géométrie maximale avec un fichier creux.

## Moteur et pannes

- `Engine::format`, `mount`, `unmount` et `salvage` travaillent sur un périphérique possédé exclusivement. Une transaction réserve au plus 64 blocs CoW, y compris les index et le manifeste. La réserve de 128 blocs limite les allocations ordinaires. `unlink` retire le nom, puis `collect` rend les extents par lots récupérables après coupure.
- L’ordre de publication est écritures des dépendances, flush F1, écriture de l’ancre alternée, flush F2, puis retour du succès. Une erreur avant l’ancre interrompt le montage courant. Une erreur à la publication transporte la phase, sa cause et le caractère indéterminé du résultat. Le remontage RW valide les deux générations et stabilise la tête retenue avant de rendre l’écrivain.
- Les opérations ordinaires utilisent des références (bloc, naissance, SHA-256) et un cache de lecture propre de 256 blocs au plus, à seconde chance. `reclaim_cache` restitue le contenu et l’index. `verify` contourne ce cache, relit le support et compare la tête à celle observée par l’instance en mémoire. Après un défaut d’intégrité, rouvrir une nouvelle instance pour poursuivre un diagnostic indépendant.
- Le banc conserve ses reçus hors du support simulé. Un nouveau montage sans reçu externe peut accepter une paire ancienne mais parfaitement cohérente. La lecture en RO ne transforme pas une telle observation en preuve de fraîcheur.

[Format sérialisé](docs/novafs-disk-format.md) · [Rapport des quinze scénarios](docs/novafs-prototype-macos.md) · [Injections de panne](docs/novafs-prototype-pannes.md)

## Reproduire sur les deux cibles

```sh
novaFS/lab/run test
NOVAFS_TARGET=x86_64-apple-darwin novaFS/lab/run test
novaFS/lab/run test --test properties -- --ignored --nocapture
NOVAFS_TARGET=x86_64-apple-darwin novaFS/lab/run test --test properties -- --ignored --nocapture
python3 novaFS/lab/test-interop.py
novaFS/lab/run run --bin novafs-bench -- 4096 complet
NOVAFS_TARGET=x86_64-apple-darwin novaFS/lab/run run --bin novafs-bench -- 512 complet
```

Sur un Mac Apple Silicon, l’essai x86_64 demande la cible Rust `x86_64-apple-darwin`, l’éditeur de liens macOS et un environnement capable d’exécuter le Mach-O x86_64. Le test d’échange contrôle le type de CPU de chaque binaire avant de le lancer. Le mode `--offline` de Cargo est utilisable une fois les dépendances du verrou disponibles localement.
