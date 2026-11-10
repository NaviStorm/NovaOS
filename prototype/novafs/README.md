# NovaFS P0 sur macOS

Ce paquet Rust possède son propre workspace Cargo et sa propre version 0.9.0. Il fonctionne sur une image logique de LV dont les blocs mesurent 4 096 octets. Le lanceur `lab` choisit explicitement la cible Darwin : la configuration Cargo du dépôt parent cible le noyau bare-metal.

## Créer, inspecter et vérifier une image jetable

Depuis la racine du dépôt, avec Rust stable installé via rustup, le script suivant crée une image temporaire de 2 Mio. Les chemins `/notes` et `/notes/exemple` se trouvent **dans NovaFS**.

```sh
lab=prototype/novafs/lab
image=$(mktemp -t novafs-p0)
log=$(mktemp -t novafs-p0-identites)

printf '%s\n' 'mkdir /notes' 'create /notes/exemple' \
  'write /notes/exemple 0 48656c6c6f' 'verify' \
  | "$lab" run -- script 512 4096 > "$image" 2> "$log"

"$lab" run -- inspect 512 512 < "$image"
fs_id=$(sed -n 's/^FS=\([^ ]*\) LV=.*/\1/p' "$log")
lv_id=$(sed -n 's/^FS=[^ ]* LV=\(.*\)/\1/p' "$log")
"$lab" run -- verify 512 512 "$fs_id" "$lv_id" < "$image"

rm "$image" "$log"
```

`create BLOCS SECTEUR` produit sur stdout une image vierge formatée. `script` effectue ses commandes dans un nouveau fichier anonyme, vérifie puis exporte le résultat. Le formateur génère deux identités de 128 bits et les rend sur stderr. Le moteur refuse de formater une image comportant un seul octet non nul. Une erreur du script empêche l’export de la nouvelle image.

`inspect` lit les deux enveloppes et signale individuellement un slot illisible. `verify` exige les identités attendues, la paire d’ancres, la structure complète et les checksums des données des deux générations. Utiliser les identités enregistrées lors du formatage : récupérer les seules identités d’un support suspect ne prouve pas sa provenance. `salvage BLOCS SECTEUR FS_ID LV_ID SLOT NOM` vérifie le slot explicitement choisi et extrait le fichier sur stdout en lecture seule. Un résultat de sauvetage porte une génération et une fraîcheur indéterminée. Si une lecture de secours échoue en cours de flux, écarter la sortie partielle et consulter l’erreur. Le détail des arguments est accessible via `lab run -- --help` et `lab run -- salvage --help`.

L’outil accepte 133 à 32 768 blocs pour une image formatée, des secteurs logiques de 512 ou 4 096 octets et des transferts par flux. Il ne prend aucun chemin de périphérique macOS. L’import copie exactement le flux vers un fichier ordinaire temporaire et conserve le descripteur ouvert. Les commandes d’inspection et de sauvetage ne modifient pas l’image source. L’API de bloc du moteur accepte jusqu’à 8 Gio. Le banc peut tester la géométrie maximale avec un fichier creux.

## Moteur et pannes

- `Engine::format`, `mount`, `unmount` et `salvage` travaillent sur un périphérique possédé exclusivement. Une transaction réserve au plus 64 blocs CoW, y compris les index et le manifeste. La réserve de 128 blocs limite les allocations ordinaires. `unlink` retire le nom, puis `collect` rend les extents par lots récupérables après coupure.
- L’ordre de publication est écritures des dépendances, flush F1, écriture de l’ancre alternée, flush F2, puis retour du succès. Une erreur avant l’ancre interrompt le montage courant. Une erreur à la publication transporte la phase, sa cause et le caractère indéterminé du résultat. Le remontage RW valide les deux générations et stabilise la tête retenue avant de rendre l’écrivain.
- Les opérations ordinaires utilisent des références (bloc, naissance, SHA-256) et un cache de lecture propre de 256 blocs au plus, à seconde chance. `reclaim_cache` restitue le contenu et l’index. `verify` contourne ce cache, relit le support et compare la tête à celle observée par l’instance en mémoire. Après un défaut d’intégrité, rouvrir une nouvelle instance pour poursuivre un diagnostic indépendant.
- Le banc conserve ses reçus hors du support simulé. Un nouveau montage sans reçu externe peut accepter une paire ancienne mais parfaitement cohérente. La lecture en RO ne transforme pas une telle observation en preuve de fraîcheur.

[Format sérialisé](../../docs/novafs-disk-format.md) · [Rapport des quinze scénarios](../../docs/novafs-prototype-macos.md) · [Injections de panne](../../docs/novafs-prototype-pannes.md)

## Reproduire sur les deux cibles

```sh
prototype/novafs/lab test
NOVAFS_TARGET=x86_64-apple-darwin prototype/novafs/lab test
prototype/novafs/lab test --test properties -- --ignored --nocapture
NOVAFS_TARGET=x86_64-apple-darwin prototype/novafs/lab test --test properties -- --ignored --nocapture
python3 prototype/novafs/test-interop.py
prototype/novafs/lab run --bin novafs-bench -- 4096 complet
NOVAFS_TARGET=x86_64-apple-darwin prototype/novafs/lab run --bin novafs-bench -- 512 complet
```

Sur un Mac Apple Silicon, l’essai x86_64 demande la cible Rust `x86_64-apple-darwin`, l’éditeur de liens macOS et un environnement capable d’exécuter le Mach-O x86_64. Le test d’échange contrôle le type de CPU de chaque binaire avant de le lancer. Le mode `--offline` de Cargo est utilisable une fois les dépendances du verrou disponibles localement.
