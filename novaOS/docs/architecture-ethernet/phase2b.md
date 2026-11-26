# Ethernet 2B — transport du premier port expérimental

**Recommandation du 26 novembre 2026.** Utiliser une boîte privée de 4 Kio avec `IPC_PUT/TAKE` pour le premier port. Prévoir DATA si les deux extrémités autorisées négocient dès l’ouverture une session sans boîte. Une révocation ou une réponse incertaine pendant un lot doit suivre le protocole de retrait ou de consultation du reçu. Un basculement à chaud risquerait d’admettre deux fois le même lot.

Les fenêtres multipages accélèrent chacun des douze essais décisionnels de grands lots. Le meilleur gain vaut 63,4 %. Au premier démarrage AArch64, le p99 d’une trame de 60 octets atteint 115,01 µs avec les fenêtres et 90,00 µs avec la boîte. L’écart de **+27,8 %** dépasse la tolérance préétablie de +10 %. Les cinq autres démarrages satisfont les quatre seuils. La règle porte sur chaque démarrage. [Protocole fixé avant mesure](criteres-phase2b.md) · [Résultats par démarrage et par profil](mesures-phase2b.md)

| Gain du débit utile des fenêtres face au meilleur concurrent | 4 × 1 514 octets | 8 × 1 514 octets |
|---|---:|---:|
| AArch64, minimum et maximum sur trois démarrages | +44,1 à +45,3 % | +56,6 à +63,4 % |
| x86_64, minimum et maximum sur trois démarrages | +31,1 à +48,2 % | +50,3 à +54,0 % |

**Option hybride.** Les deux lots gagnants occupent 6 056 et 12 112 octets. Une session pourrait réserver respectivement deux ou trois pages pour ces géométries, garder une seule voie pour les unités et les petits lots et négocier DATA si la réservation d’une boîte est impossible. Les gains de cadence, pauses de travail comprises, restent compris entre +28,4 et +59,1 % sur les mêmes grands lots. Pour une charge de 4 096 octets ou moins, les deux chemins de boîte utilisent une seule voie et font les mêmes copies. La dispersion du p99 à 60 octets ne permet pas d’en attribuer la cause à la multiplicité des pages. Les tailles intermédiaires, les lots mixtes, la concurrence entre plusieurs grands lots et le coût d’un redimensionnement de session demanderaient des mesures propres. La décision du premier port ne dépend pas de la mise en œuvre de cette option.

## Médiation et propriété

Deux processus NovaOS distincts exécutent le même ELF en EL0 ou Ring 3. Le banc monte la session à un seul pair, préalloue les zones, établit les voies avec le mécanisme interne existant `bind_lane`, lance les rôles et lit leurs compteurs. Ensuite le processus émetteur prépare le lot, envoie le contrôle TX v1 et publie les octets. Le récepteur vérifie le contrôle, récupère les trames dans sa propre zone, compare chaque octet à son oracle privé et valide le lot entier avant d’émettre son reçu. Les numéros de requête avancent depuis la chauffe jusqu’à la calibration. Un seul lot reste sans reçu. Le dernier reçu reste récupérable par QUERY dans la session vivante. La source, la candidate et l’oracle appartiennent à leurs espaces respectifs. [Acteur utilisateur](../../tests/ethernet_v1/phase2b/acteur.rs) · [Lanceur et sondes](../../tests/ethernet_v1/phase2b/invite.rs)

La boîte vérifie l’identité du processus appelant, la capacité locale exacte, sa direction et les plages mémoire. Un PUT copie vers la page superviseur liée. Le TAKE correspondant vérifie l’étiquette et la longueur, copie dans la mémoire privée du destinataire puis efface les octets de la boîte. Les fenêtres qualifiées enchaînent de une à trois de ces voies, chacune bornée à 4 Kio. La frontière de page peut couper une trame. Le reçu porte sur la totalité du lot. L’autorité de création et de retrait des racines reste chez le créateur. Le contrôle de trafic repose sur l’attribution initiale d’une paire dédiée. [Implémentation des voies](../../kernel/src/sched/process/ipc/bulk.rs) · [Contrat Ethernet v1](contrat-v1-phase2.md)

La copie directe entre deux mémoires privées, simulée en phase 2, n’est accessible par aucun des 48 sélecteurs v1. La porte privée du pilote bloc vérifie son bail et son rôle. La réutiliser pour Ethernet changerait les droits. Cette qualification établit l’autonomie des acteurs pour les données après montage, avec les appels ordinaires. Une copie directe ou la création autonome d’une liaison par un client demanderait un arbitrage de l’autorité de liaison, de la validation, de la propriété et du retrait. [Registre ABI](../../abi/syscalls.toml) · [Porte privée du bloc](../../kernel/src/sched/process/syscall.rs) · [Comparaison avec la phase 2](phase2.md)

## Débit, latences et médiation

Chaque série rapide compare les trois transports sur les mêmes géométries, le même contenu et les mêmes conditions de validation. L’ordre des transports tourne à chaque démarrage. Les six démarrages décisionnels, trois par architecture, comprennent 32 lots de chauffe par cellule, puis 10 000 lots pour chaque cellule en rafale, en saturation et sous trafic concurrent. Le premier passage en trafic rare comporte 1 000 lots par cellule. Six démarrages supplémentaires portent ce dernier profil à 10 000 lots pour chacun des trois transports et des deux longueurs unitaires. Au total, 1 476 000 lots des séries principales et 360 000 lots de trafic rare prolongé entrent dans les résultats, hors chauffe, calibration et voisin. Les trois démarrages d’une architecture emploient le même noyau pour un profil donné. L’ELF des acteurs reste identique entre les deux profils de performance de la même architecture.

- **Rafale.** Les lots s’enchaînent après réception du reçu. La latence va du premier envoi de contrôle à la sortie du gestionnaire de réception du reçu. Elle comprend l’ordonnancement et le travail du destinataire. La préparation du client et sa vérification finale du reçu se situent hors de ces bornes.
- **Trafic rare.** Le client attend sur une boîte de réponse vide avec un timeout d’un tick de l’ordonnanceur à 250 Hz. Le débit utile rapporte les octets aux durées d’admission cumulées. La cadence divise les mêmes octets par toute la durée entre les bornes du premier et du dernier lot. Elle inclut les intervalles entre lots.
- **Saturation.** Le récepteur attend pendant que l’émetteur remplit la file de huit dépôts DATA ou la totalité des voies réservées. Une tentative supplémentaire doit rendre `WOULD_BLOCK` avant la reprise du récepteur. L’essai impose le même échange de synchronisation aux trois transports.
- **Concurrence et attentes.** Une deuxième paire indépendante, sur sa propre boîte, échange et vérifie des trames de 60 octets. Sa progression est relevée pendant la mesure. Dans chaque cellule, le premier destinataire est observé bloqué avant l’envoi, puis les réveils et commutations du vCPU sont comptés. En concurrence ces compteurs incluent le voisin.
- **Quantiles.** Les p50, p95, p99 et p99,9 sont calculés par rang supérieur dans chaque série, sans fusion des démarrages. Sur 10 000 lots, le p99,9 est la 9 990e observation triée. Les journaux gardent l’ordre d’origine et la fréquence du compteur pour chaque cellule. Ce sont des temps écoulés dans QEMU TCG. Les trois démarrages donnent une indication de la dispersion entre séries. [Dépouillement détaillé](mesures-phase2b.md) · [Compteurs par série](../validation/ethernet-v1-phase2b/mesures.json)

À faible trafic, les trois transports donnent une cadence médiane de 0,0120 Mo/s sur AArch64 et 0,0150 Mo/s sur x86_64 à 60 octets. Sur ces trames, le p99,9 de la boîte va de 1,37 à 2,26 ms sur AArch64 et de 1,24 à 1,69 ms sur x86_64. La dispersion et l’intervalle imposé entre admissions comptent donc dans l’interprétation du débit et de la latence. Les seuils décisionnels restent ceux des séries en rafale.

Pour huit trames de 1 514 octets en rafale, la boîte et les fenêtres copient toutes deux 24 224 octets en six copies logiques. Les trois voies réduisent les reprises à vide et les commutations. DATA traite 128 fragments et copie 48 448 octets en 512 copies logiques, préparation et réassemblage compris. Ces décomptes sont déduits des segments effectivement échangés et vérifiés avec les appels observés. Ils excluent les métadonnées et les lectures de l’oracle.

| Huit grandes trames, médiane des trois séries | Débit DATA / boîte / fenêtres (Mo/s) | Commutations DATA / boîte / fenêtres par lot | Coût moyen PUT / TAKE réussi, boîte (µs) | Coût moyen PUT / TAKE réussi, fenêtres (µs) |
|---|---:|---:|---:|---:|
| AArch64 | 6,28 / 53,57 / 84,31 | 30,42 / 6,07 / 2,04 | 5,00 / 6,14 | 3,30 / 3,78 |
| x86_64 | 7,97 / 69,14 / 103,91 | 30,41 / 6,06 / 2,04 | 10,26 / 11,11 | 8,66 / 9,48 |

La mesure de médiation chronomètre le véritable gestionnaire des voies dans une passe séparée de 128 lots après la série principale. Les appels réussis et refusés disposent chacun de compteurs et de temps cumulés. La mesure principale garde le coût de l’appel, de ses vérifications et de la copie. Pour DATA, le temps publié porte sur la remise complète par les appels de messages. La calibration interne PUT/TAKE s’applique aux deux transports qui les utilisent.

## Retrait, réponses et isolation

| Situation exercée sur les deux architectures | Observation vérifiée |
|---|---|
| Fermeture après la première page d’un lot de trois | Le récepteur constate la première page complète, l’absence de modification sur le reste et le refus de la suite. Il efface sa candidate. Zéro admission |
| Disparition de l’émetteur, puis essai distinct de disparition du récepteur | Sorties réelles des processus avec codes 23 et 24 au point de transfert. Le survivant observe le retrait, la paire est libérée et le voisin continue |
| Réponse d’admission supprimée | Le client expire, envoie QUERY avec son identifiant et récupère le reçu. Une seule admission, sans réémission de données. Le reçu est conservé en mémoire de la session |
| Révocation explicite | Les voies du lot deviennent inutilisables avec les anciens handles. Deux tentatives inter-paires sont aussi refusées dans chacun des cinq cas. Le voisin continue pendant et après le retrait |
| Fermeture ou révocation engagée au milieu d’une copie | Sur deux vCPU, le CPU 0 a effectivement copié 2 048 des 4 096 octets lorsque le CPU 1 tente et échoue à prendre le verrou. TAKE/fermeture, TAKE/révocation et PUT/fermeture terminent la copie avant le retrait. Le TAKE rend la destination intégrale. La fermeture après PUT laisse la destination privée du pair intacte. Les deux anciens accès sont refusés |

Chaque essai retrouve son nombre initial de cadres libres et ses tables. La course utilise deux acteurs de la sonde IPC générique et les mêmes appels noyau, avec rendez-vous de test dans la copie. Ses latences ne participent pas aux seuils. Le témoin d’admission prouve une remise CPU dans une session vivante. La remise externe, la persistance d’un reçu après la mort du récepteur et la complétion par une carte relèvent d’autres modèles de panne. [Scénarios de retrait](../../tests/ethernet_v1/phase2b/invite.rs) · [Course SMP](../../tests/ethernet_v1/phase2b/course.rs)

Un essai préparatoire de saturation a révélé une erreur du banc. Le montage essayait de transférer la capacité de réveil sur une feuille ne possédant que SEND. IPC_GIVE exige aussi GRANT sur le canal de délégation. Le montage utilise désormais une feuille temporaire SEND|GRANT, la ferme avant la chauffe et conserve la feuille de réveil SEND. Les traces de cet essai restent dans l’archive locale `essai-2`. Les chiffres publiés utilisent le montage corrigé.

## Pages et plafonds

Pages de 4 Kio, pour une paire de deux acteurs, huit trames de 1 514 octets en rafale. Les colonnes couvrent les deux architectures. Les pages d’IPC incluent les boîtes et l’attente bloquante.

| Poste ou ressource | DATA | Boîte | Fenêtres |
|---|---:|---:|---:|
| Pages privées émetteur hors pile, dont 3 de charge utile | 9 | 9 | 9 |
| Pages privées récepteur hors pile, dont 3 de candidate et 3 d’oracle | 12 | 12 | 12 |
| Piles utilisateur / piles noyau des deux acteurs | 128 / 32 | 128 / 32 | 128 / 32 |
| Tables de traduction supplémentaires | 24 | 24 | 24 |
| Pages IPC en attente, dont boîtes privées | 5 dont 0 | 6 dont 1 | 10 dont 3 |
| Total de cadres additionnels à l’ouverture / en attente | 209 / 210 | 210 / 211 | 214 / 215 |
| Racines créées par l’émetteur / le récepteur | 2 / 0 | 2 / 0 | 4 / 0 |

En phase 2, le banc allouait ses vecteurs source et de contrôle avant de relever les cadres libres et réutilisait le second comme instantané. En 2B, la référence précède la création des acteurs. Le tas utile de l’émetteur occupe le volume du lot arrondi à la page. Le récepteur en réserve deux fois autant. À huit grandes trames, les neuf pages de source, de candidate et d’oracle entrent donc toutes dans le bilan. Les six autres pages privées hors pile de chaque acteur couvrent son image et ses données, dont le contrôle et l’état du dernier reçu. Le profil saturé ajoute une racine de synchronisation au récepteur. Dans le profil concurrent unitaire, les quatre acteurs consomment 404 cadres en attente pour DATA et 405 pour chaque chemin par boîte. Les vues globales rendent également les pages de tables, de piles, de tas noyau, de buddy, d’IPC, d’espace utilisateur, de préparation ELF et de DMA.

Les images de mesure comptent 1 319 pages sur AArch64 et 1 567 sur x86_64, BSS comprise. Les images ordinaires correspondantes comptent 1 110 et 1 192 pages. L’écart global de profil inclut le code, les images utilisateur embarquées et les sondes. Le tableau statique des 10 000 durées occupe déjà 80 000 octets dans l’image, en plus des compteurs. Les photos de référence incluent aussi les pages déjà retenues par le système. Le rapprochement de chaque photo d’attente avec sa référence explique le delta de cadres libres. Le relevé couvre les points d’ouverture, d’attente et de fermeture. Il ne donne pas un maximum continu entre ces points. [Séries mémoire complètes](../validation/ethernet-v1-phase2b/mesures.json) · [Calcul des pages](../../kernel/src/memory/mod.rs)

Les plafonds restent de 32 processus, 64 endpoints, 256 nœuds, 32 handles par table locale, 32 attentes IPC, huit dépôts par endpoint, huit boîtes globales et quatre racines par créateur. Pour un lot de quatre grandes trames, deux voies et trois racines du créateur suffisent. Pour huit grandes trames, une seule paire utilise les quatre racines de son créateur. Même avec des créateurs distincts, les huit boîtes globales permettent au plus deux paires à trois voies avant déduction des réservations des autres services. Le besoin d’une deuxième liaison au port limite aussi la généralisation du mode multipage. [Capacité et budgets](capacite.md)

## Reproduction et provenance

La référence de phase 2 est `a8405f92473ba1400249f12aef93bd9d09c2eb53`. Le protocole est figé à `fe83b48d1b5d753abed27118ffffaa498a46164a`. Les six démarrages principaux et les deux essais de sûreté utilisent le code de `defc086ba1901d7b038523b1378953ac6a942b89`. Les six démarrages de trafic rare prolongé et les deux confirmations SMP utilisent `b6bd9bfcc7e23499629f49561533c53929d0d849`. Les empreintes des ELF, des ISO, des firmwares et des journaux identifient chaque démarrage. [Provenance](../validation/ethernet-v1-phase2b/provenance.json) · [Journaux comprimés et sommes SHA-256](../validation/ethernet-v1-phase2b/)

Le noyau ordinaire a été reconstruit sur les deux ISA. Par rapport aux ELF ordinaires archivés de phase 2, chaque segment chargeable a la même géométrie. Les segments exécutables et inscriptibles sont identiques octet pour octet. Les seuls écarts chargés sont des numéros de ligne de fichiers Rust en lecture seule, 168 octets sur AArch64 et 169 sur x86_64. Les neuf vérifications du registre ABI passent. Le manifeste des 48 sélecteurs est identique à la référence. La campagne ciblée utilise QEMU TCG 11.1.3, 256 Mio, virt/GICv2 avec Cortex-A72 ou Q35 avec CPU max, sans NIC. Les 17 tests historiques et les campagnes complètes sont restés suspendus. [Contrôle ABI](../validation/ethernet-v1-phase2b/abi-registre.txt)

Depuis la racine du worktree, avec Python 3.11 ou ultérieur, ces commandes vérifient les preuves publiées et régénèrent les deux résultats dans un répertoire neuf.

```sh
export NOVAOS_TMPDIR="$(dirname "$(pwd -P)")/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
preuves=novaOS/docs/validation/ethernet-v1-phase2b
rapport=novaOS/docs/architecture-ethernet/mesures-phase2b.md
sortie=$(mktemp -d "$NOVAOS_TMPDIR/ethernet2b-bilan.XXXXXXXX")
(cd "$preuves" && shasum -a 256 -c SHA256SUMS)
python3 -B novaOS/tests/ethernet_v1/phase2b/bilan.py "$preuves" "$sortie/mesures.md" "$sortie/mesures.json"
cmp "$sortie/mesures.md" "$rapport"
cmp "$sortie/mesures.json" "$preuves/mesures.json"
```

Pour reconstruire un profil, utiliser le commit associé ci-dessus et le même chemin de worktree pour une comparaison d’ELF avec informations de débogage. Exemple du premier démarrage décisionnel. Les autres rotations sélectionnent `mesure-1` et `mesure-2`. La sûreté sélectionne `surete`, le trafic rare prolongé `rare`, `rare-1` et `rare-2`. La course sélectionne `course` après compilation avec `ethernet-2b-course` dans un répertoire cible propre. Chaque exécution demande une destination d’archive neuve et prend le verrou commun.

```sh
export RUSTC="$(rustup which --toolchain stable rustc)"
export CARGO_TARGET_DIR="$PWD/novaOS/target"
(cd novaOS && for cible in aarch64-unknown-none-softfloat x86_64-unknown-none
do
  rustup run stable cargo build --offline --locked --release --features ethernet-2b-lab --target "$cible" || exit 1
done)
archive="$(dirname "$PWD")/archives-ethernet-phase2b-rejeu"
python3 -B novaOS/tests/ethernet_v1/invite.py "$archive" aarch64 x86_64 --phase2b mesure --cibles "$CARGO_TARGET_DIR"
```

Les seize démarrages probants et leur contre-vérification ont abouti en moins de 61 minutes écoulées depuis le début du mandat, attente du verrou comprise, dans le budget de 120 minutes actives.

Les binaires et les ISO des essais sont conservés dans `OS-worktrees/archives-ethernet-phase2b`, dans les répertoires `serie-1` à `serie-3`, `retrait-candidat`, `rare-1` à `rare-3`, `course-finale` et `binaires`.
