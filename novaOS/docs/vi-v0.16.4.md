# NovaOS v0.16.4 — Ouverture progressive de vi

Le premier écran d’un grand fichier ne dépend plus de la lecture intégrale du document. Le shell épingle la source ; l’éditeur affiche sa première fenêtre, accepte une modification locale et peut quitter sans lire le reste. Une sauvegarde sous un **nom libre** valide aussi les octets restés sur disque. Un conflit laisse les modifications dans l’éditeur. Aucun droit de processus, appel système ou format NovaFS n’a changé.

## Cause et correction

En v0.16.3, une lecture de fichier traversait le moteur NovaFS une fois par fragment de **88 octets**. Sur 64 Kio, avec mise à jour de la date d’accès et contre-lecture, cela représentait 1 491 lectures du moteur et 1 509 lectures de pages du service, avant même de lancer l’éditeur. Le coût du premier rendu était de quelques millisecondes. L’[audit intermédiaire](vi-v0.16.4-lecture-groupee.md) isole cet effet : regrouper la lecture par 4 Kio, tout en attendant encore l’intégralité du fichier, ramène déjà 4 Mio à 34,88 s sur AArch64 et 29,19 s sur x86_64. Ce sont les mesures du témoin intermédiaire, pas celles de l’image livrée.

- Jusqu’à 16 Kio, le courtier conserve la petite source vérifiée ; il vérifie également cette borne sur le handle ouvert. Au-delà, [le courtier](../../components/shell/vi_source.rs) ne réserve qu’une fenêtre de 4 Kio. Le seuil choisit une stratégie, il ne limite pas la taille du fichier.
- L’[éditeur](../../components/vi/paging.rs) affiche le préfixe valide. Il ne coupe ni un scalaire UTF-8 ni un CRLF. La mention « lecture partielle » indique que le tampon ne contient pas tout le fichier ; une ellipse distingue le texte encore absent de la fin effective. Les parties affichées passent par le même contrôle de caractères et le même rendu différentiel que les petits fichiers.
- Les déplacements locaux et les éditions au début utilisent le gap buffer et l’historique existants. Pour `G`, l’éditeur compte les sauts de ligne à la demande. Pour les fichiers de lignes ordinaires, il recycle **huit fenêtres de 4 Kio** et conserve la dernière vue et son numéro de ligne. `gg` retrouve le préfixe édité et son historique. Un second `G` vérifie la source avant de réutiliser cette vue. Le premier `G` reste un parcours linéaire. Les touches n’exécutent aucun travail d’indexation en arrière-plan.
- Une page récupérée n’est pas une modification utilisateur. Elle n’ajoute pas d’entrée à l’historique : l’original de comparaison et le texte sont étendus ensemble. Une annulation réserve, **avant la première mutation**, l’espace maximal requis par son groupe : le chargement d’une page a pu consommer l’espace laissé par une suppression. Le test d’allocation vérifie le tampon, l’annulation et le rétablissement après un refus.

## Cohérence, sauvegarde et coût mémoire

Une fenêtre conditionnelle READ (`IF_REVISION=3`) porte la révision attendue en little-endian au début de la charge utile. Le [service](../user/novafs/files.rs) contrôle le droit de lecture du handle, la taille, l’absence d’écriture en attente et la révision de l’objet **dans la même opération** que la lecture. La première réponse donne à +32 la révision résultant de cette lecture : une mise à jour légitime de la date d’accès ne se confond pas avec l’écriture d’un tiers. Les continuations sont liées au même client, handle, offset et à la même génération. Une révision périmée ne donne pas accès à de nouveaux octets. Le courtier mémorise le conflit et vérifie aussi identité, taille et révision du nom avant la publication.

Si un suffixe reste sur disque, `vi` transmet le préfixe édité et l’offset de reprise. `nsh` sélectionne d’abord la destination libre, puis uniquement l’objet source en lecture. Dans ce groupe exclusif, seule la source sélectionnée au slot 2 accepte les fenêtres conditionnelles ; les vues fichier ordinaires ne peuvent pas demander une lecture anticipée. Après le consentement au terminal, le courtier compare identité, taille et révision de la source sélectionnée, lit et valide le suffixe, écrit la candidate par lots d’au plus 64 Kio, synchronise, ferme et publie. Une erreur de validation empêche la publication. Un refus, une révocation ou une publication incertaine ne déclenchent pas de suppression de la destination. Le nom de l’original reste protégé. Le moteur est toujours **NovaFS v0.9.13 P9** ; PC1 et le remplacement transactionnel restent désactivés.

**Limite de la garantie mémoire.** L’ouverture, la modification du préfixe, la dernière vue d’un texte à lignes ordinaires et la publication de son suffixe ont un espace de travail indépendant de la longueur de ce suffixe. La lecture séquentielle qui étend le préfixe, la recherche globale, la modification de la vue de fin et certaines opérations sur une très longue ligne peuvent encore matérialiser tout le texte. La copie de référence utilisée pour détecter les modifications peut alors également grandir. Aucun quota n’a été augmenté. Une borne indépendante de la taille du fichier pour **toutes** les commandes n’est pas démontrée. L’étape suivante, si elle est requise, est de représenter aussi les modifications distantes par des intervalles de source et de texte édité, puis de qualifier l’historique à travers ces intervalles.

## Mesures

| Architecture | Taille dense | v0.16.3 : écran après lecture totale (ms) | v0.16.4 : premier écran (ms) | Premier G : parcours restant (s) | Second G (ms) | Sauvegarde complète avant → après (s) |
|---|---|---|---|---|---|---|
| aarch64 | 4 Mio | 673067,5 | 122,9 | 19,223 | 8,3 | 46,00 → 65,74 |
| aarch64 | 1 Mio | 168337,0 | 99,0 | 4,747 | 8,2 | 11,64 → 16,47 |
| aarch64 | 64 Kio | 10350,8 | 96,8 | 0,289 | 1,3 | 0,99 → 1,19 |
| aarch64 | 1 Kio | 242,4 | 100,0 | 0,001 | 0,3 | 0,30 → 0,30 |
| x86_64 | 4 Mio | 599160,5 | 95,3 | 16,958 | 7,3 | 40,00 → 57,54 |
| x86_64 | 1 Mio | 148638,6 | 83,3 | 4,245 | 7,2 | 10,21 → 14,42 |
| x86_64 | 64 Kio | 10430,3 | 86,8 | 0,255 | 1,3 | 0,91 → 1,03 |
| x86_64 | 1 Kio | 226,2 | 85,7 | 0,001 | 0,3 | 0,27 → 0,26 |

À 4 Mio, le temps de lecture intégrale se paie au premier parcours, pas à l’ouverture. Après retour en tête, le second saut utilise la vue déjà indexée. La sauvegarde d’un fichier dont seul le préfixe est en mémoire est **plus lente** dans ces séries : elle relit et valide le suffixe avant publication. Le gain du premier écran ne constitue pas un gain de même facteur sur la sauvegarde. Aucun FLUSH n’a été retiré.

| Architecture, 4 Mio | vi résident lancement / premier VIEW / sauvegarde (Kio) | Pic du tas de vi (Kio, sonde continue) | Allocations réussies de vi (session instrumentée) | Lancement / réception et préparation / premier rendu (ms) | Débit utile du premier G (Mio/s) |
|---|---|---|---|---|---|
| aarch64 | 316 / 380 / 444 | 128 | 52 | 32,9 / 82,1 / 7,9 | 0,21 |
| x86_64 | 320 / 384 / 448 | 128 | 52 | 20,4 / 70,2 / 4,7 | 0,24 |

Les échantillons du noyau incluent la pile ; le compteur du tas relève ses extensions entre les échantillons. Les deux mesures concernent le processus, pas toute la VM. Compter la taille logique du fichier comme si elle avait été intégralement lue au premier écran donnerait un débit fictif. Pour 4 Mio, l’éditeur demande **33 réponses de contenu** jusqu’au premier écran (127 octets utiles au plus, 4 Kio en tout), au lieu de 33 027 pour recevoir tout le fichier. Le courtier n’appelle le moteur qu’une fois pour cette première fenêtre. La demande de dimensions et le premier dessin viennent ensuite. Les trames, phases instrumentées et acquisitions du tas sont conservées dans le JSON. Les coûts de lancement fixes peuvent augmenter de quelques pages : la réduction de l’empreinte sur grand fichier ne signifie pas une réduction du binaire.

Les [résultats détaillés](validation/vi-v0.16.4.json) séparent témoins, image livrée, banque de sondes et banc à attente de montage prolongée. QEMU direct 11.1.3, TCG, un vCPU, 256 Mio, disque VirtIO brut à secteurs de 4 096 octets, terminal 24 × 80. Les fichiers denses du banc ont des lignes de 128 octets. Le temps part à l’envoi de la commande au shell ; le premier écran s’arrête à sa barre d’état. Le parcours complet s’arrête sur la dernière ligne après `G`. La sauvegarde inclut le dialogue, un accord immédiat et la publication sous un nouveau nom. La relecture de contrôle des octets est hors chronométrage. Les mesures sont des durées murales en charge sur ce Mac ; deux jalons UART peuvent arriver dans la même lecture. Les cibles sont indicatives, pas des percentiles garantis.

| Architecture | Taille logique | Cible écran (ms) | Fichier dense sur profil normal (ms) | Fichier creux, préfixe texte (ms, secteurs 512) | Variante dense distincte |
|---|---|---|---|---|---|
| aarch64 | 1 Mio | 100 | 99,0 | 149,4 | — |
| aarch64 | 4 Mio | 250 | 122,9 | 111,8 | — |
| aarch64 | 16 Mio | 750 | montage expiré | 106,2 | banc à attente prolongée : 121,6 ms |
| aarch64 | 100 Mio | 3000 | montage expiré | 105,1 | — |
| aarch64 | 1 Gio | 30000 | montage expiré | 101,4 | — |
| x86_64 | 1 Mio | 100 | 83,3 | 118,8 | — |
| x86_64 | 4 Mio | 250 | 95,3 | 88,9 | — |
| x86_64 | 16 Mio | 750 | montage expiré | 89,3 | banc à attente prolongée : 99,0 ms |
| x86_64 | 100 Mio | 3000 | montage expiré | 89,3 | — |
| x86_64 | 1 Gio | 30000 | montage expiré | 91,6 | — |

Les fichiers creux n’ont que deux zones écrites ; les trous contiennent des NUL. Le test affiche, modifie le début puis quitte en conservant les données du disque. Il ne qualifie pas ces fichiers comme des textes valides jusqu’à EOF. Le générateur a, séparément, écrit et relu **104 857 600 et 1 073 741 824 octets denses** sur P9. Pour chacune des deux architectures, MOUNT expire avec le statut client 8 avant tout lancement de vi sur ces volumes ; aucune vitesse d’ouverture dense à ces tailles n’est extrapolée.

Sur 16 Mio denses, seule la variante de laboratoire atteint le shell : la première navigation jusqu’à EOF prend 75,52 s et 67,10 s. L’outil hôte confirme les octets après fermeture. Il n’y a pas de mesure de sauvegarde invitée à cette taille.

Sur le Mac, Vim 9.1 a affiché la première ligne en **136,7 ms pour 100 Mo** et **143,8 ms pour 100 Mio** selon le protocole ci-dessous. Le chiffre indicatif de trois secondes n’est pas le résultat de cette exécution.

La référence utilisateur « environ 3 s pour 100 Mo » n’identifie ni l’éditeur ni l’état de son cache. Le [témoin macOS](../tests/vi_reference_macos.py) lance un Vim natif neuf, configuration et swap désactivés, terminal 24 × 80, et attend le premier texte réellement affiché. Les fichiers denses viennent d’être créés, cache hôte non purgé. Ce témoin n’utilise ni P9 ni TCG ; **100 000 000 octets** et **100 Mio** sont mesurés séparément.

Pour améliorer le démarrage à 100 Mio et 1 Gio, il faut d’abord mesurer la vérification du graphe P9 au montage et son transport bloc. Modifier le seul délai d’attente ne réduit pas le coût, et supprimer la vérification changerait la garantie d’intégrité. Le travail sur les recherches et les éditions distantes de vi est indépendant de ce blocage.

## Installation dans UTM

L’[archive AArch64](../../distribution/images/novaos-0.16.4-utm-aarch64.tar.gz) contient le support QCOW2 et le firmware expérimental. SHA-256 : `88f70a1f45b4b0a09027bc665cce5df6418f4f52af35cfeb51183f866d9a121c`. Depuis la racine du dépôt :

```sh
NOVA_TRAVAIL="$(mktemp -d "$HOME/Documents/NovaOS-vi-0.16.4.XXXXXXXX")"
NOVA_DIST="$NOVA_TRAVAIL/novaos-0.16.4-utm-aarch64"
tar -xzf distribution/images/novaos-0.16.4-utm-aarch64.tar.gz -C "$NOVA_TRAVAIL"
(cd "$NOVA_DIST" && shasum -a 256 -c SHA256SUMS) &&
    python3 novaOS/scripts/paquet_installation.py creer "$NOVA_DIST" "$NOVA_TRAVAIL/machine"
```

Suivre les [trois étapes d’installation et de retrait du support](nsh-v0.16.3.md#installation-dans-utm). Le volume P9 du profil installé fait 2 Mio. Les images de 100 Mio et 1 Gio du banc de stockage sont **des disques de données P9 distincts**, pas des supports d’installation.

Application UTM 5.0.8, QEMU 10.0.12, QCOW2, secteurs de 512 octets, TPM CRB, GICv3 sans ITS : après installation et retrait du support, le fichier de 66 560 octets créé au clavier est rouvert en **810,10 ms**. Après le deuxième démarrage autonome : **716,41 ms**, contre **48,54 s** avec v0.16.3. Le temps porte sur le premier écran. Le port série de l’application a ensuite restitué les 66 560 octets de l’original et les 66 562 octets de la copie après deux annulations et deux rétablissements. Le support d’installation a conservé son SHA-256.

## Intégrité et régressions

Les essais en invités ont relu 16 fichiers à l’octet près sur **chaque couple architecture/secteur**, avant et après un nouveau démarrage. Ils couvrent vide, absence de LF final, très longues lignes, UTF-8 coupé sur une fenêtre, octets invalides, NUL, annulations et rétablissements, protections de sauvegarde, concurrence, révocation et erreurs d’écriture et de FLUSH. Le fichier creux binaire de 512 Mio, qui épuisait la mémoire en v0.16.3, est désormais rejeté sur son premier contrôle interdit ; les erreurs d’allocation restent injectées distinctement sur le vrai tampon et son journal.

Sur **512 et 4 096 octets** et les deux architectures, deux écritures de même taille ont été injectées via un second handle : avant la lecture de la page encore absente, puis, indépendamment, avant la sauvegarde sans lecture supplémentaire. L’éditeur a signalé le conflit et gardé le texte modifié. Le lecteur indépendant a retrouvé exactement les octets concurrents et confirmé l’absence des destinations. Un suffixe invalide a également empêché la publication après confirmation. Le même banc a relu la copie enregistrée depuis la vue finale (131 073 octets), puis celle modifiée aux deux extrémités après annulation et rétablissement (131 074 octets).

| Architecture | Insertion dans 4 Mio avant → après (ms) | Suppression avant → après (ms) | Édition aux deux extrémités de 128 Kio : résident de vi (Kio, lancement / écran / première puis seconde sauvegarde) |
|---|---:|---:|---:|
| aarch64 | 15,89 → 3,28 | 16,81 → 1,75 | 316 / 380 / 444 / 636 |
| x86_64 | 95,31 → 1,77 | 2,06 → 0,75 | 320 / 384 / 448 / 640 |

Une révision périmée n’a fourni aucun premier fragment ; la bonne révision a fourni les octets et la révision postérieure à l’accès. Les vues de lecture ordinaires ont refusé les deux variantes de lecture anticipée. Pour rejouer ces contrôles sur une copie jetable, appliquer successivement [vi-fenetres.patch](../tests/fixtures/vi-fenetres.patch) et [vi-fenetres-pages.patch](../tests/fixtures/vi-fenetres-pages.patch), reconstruire, puis exécuter `tests/fichiers.py --only normal` et `tests/cli.py` pour chaque architecture. Un saut répété après ajout d’une ligne au préfixe a aussi vérifié la renumérotation de la dernière vue et son annulation.

Les temps de frappe proviennent du banc historique `vi-volumes`, terminal sans réponse de géométrie, avec la référence v0.16.3 enregistrée et la relecture des sources et copies. Ils ne sont pas des mesures macOS. Le dernier échantillon de mémoire illustre le coût du retour au tampon complet lorsqu’une modification touche une région distante.

**Empreinte fixe** : `nsh` passe de 452 à **460 Kio** sur AArch64 et de 468 à **472 Kio** sur x86_64. Le code ajouté coûte donc 8 et 4 Kio respectivement. Pour le scénario de 4 Mio, le pic du témoin intégral était de 12 920 Kio pour vi ; les pics du tas et les échantillons du nouveau scénario sont détaillés ci-dessus.

**Régressions NovaOS** : par architecture, 25 démarrages, 24 groupes bloc et trois contrôles ELF/processus/IPC ; quatre sessions CLI par géométrie, namespace, fichiers, CLI/P9, scripts, terminal, écran, sept tests de mémoire et quatre tailles du banc vi. Les 84 cas d’options/clavier et 21 mutations par couple architecture/secteur, ainsi que 11 scénarios GICv3, 23 DMA et 19 GPT à 4 Kio sur AArch64, ont été exécutés dans la campagne de lecture groupée. Leur portée et leurs résultats sont archivés séparément ; ils n’ont pas été recomptés comme des répétitions de la pagination.

**NovaFS inchangé**, Darwin ARM64 natif et x86_64 sous Rosetta : par architecture, **196 standards, 197 instrumentés, 2 304 injections de coupure et 1 000 000 d’opérations différentielles**. Les suites standard et instrumentée déclarent chacune six tests ignorés ; ces tests ne sont pas comptés parmi les réussites.

**Contrôle visuel** : la fenêtre Terminal de la copie graphique n’a pas pu être inspectée, la session macOS étant verrouillée. Le rendu a été testé sur les deux architectures avec le banc d’écran et dans l’application UTM par son port série. Ces résultats ne sont pas une capture visuelle de la fenêtre macOS.

## Reproduire

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --release --bin novafs
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" kernel
    python3 -B novaOS/tests/vi_performance.py "$arch" "novaOS/build/vi-perf-$arch.json" 4194304 1048576 65536 1024 --retour-fin
    python3 -B novaOS/tests/vi_pages.py "$arch" "novaOS/build/vi-pages-$arch.json" --secteur 4096
    make -C novaOS ARCH="$arch" vi-test vi-ecran-test vi-memoire-test vi-volumes-test
done
```

Pour compter les allocations et les IPC, copier le dépôt dans un répertoire de laboratoire, copier `novaOS/user/storage/fileapi.rs` vers `components/libraries/sonde_fileapi.rs`, puis y appliquer les patches [vi-profil-pages](../tests/fixtures/vi-profil-pages.patch) et [vi-service](../tests/fixtures/vi-service.patch) avec `patch -p1`. Reconstruire et ajouter `--sondes` au chronométrage. Aucun compteur de cette variante n’est livré.

Dans une autre copie, le [banc de conflit](../tests/fixtures/vi-conflit-source.patch) écrit et synchronise un octet via un second handle avant de servir la page demandée. Reconstruire puis ajouter `--injection` à `vi_pages.py`. Le [banc de montage 16 Mio](../tests/fixtures/vi-montage-exploratoire.patch) attend davantage **la même réponse** sans répéter la requête ni relâcher les contrôles. Pour l’application UTM, utiliser `python3 novaOS/tests/nsh_utm.py "$NOVA_DIST" nouveau-repertoire-de-collecte`, une seule VM de qualification à la fois.

Pour produire un vrai fichier dense sans tampon hôte de 1 Gio, le [générateur de volumes](../tests/vi_volume_dense.rs) utilise le moteur P9 et un fichier image créé exclusivement. Il écrit des transactions de 64 Kio, démonte, remonte en lecture seule et compare chaque lot :
```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --release --lib
NOVA_DEP=novaFS/target/aarch64-apple-darwin/release
rustup run stable rustc --edition 2024 -O --target aarch64-apple-darwin novaOS/tests/vi_volume_dense.rs \
    --extern novafs="$NOVA_DEP/libnovafs.rlib" -L "dependency=$NOVA_DEP/deps" -o novaOS/build/vi-dense
novaOS/build/vi-dense novaOS/build/vi-neuf-100.raw 100
python3 novaOS/tests/vi_montage_dense.py aarch64 novaOS/build/vi-neuf-100.raw 100 novaOS/build/montage-100.json
```
