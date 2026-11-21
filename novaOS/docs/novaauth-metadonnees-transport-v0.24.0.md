# NovaAuth v0.24.0 — Métadonnées du transport QEMU

## Budget par liaison

Mesure du 17 novembre 2026. Le constructeur Python de la base `974ce9cd0186ae8dc776b7561405c702d714c591` conservait 8 763 octets sur CPython 3.15.2 ARM64. Les deux allocations de 4 193 octets comprennent chacune 4 096 octets utiles et 97 octets de représentation. L'allocation de l'instance représente 328 octets. L'objet `bytes` d'incarnation en représente 49, dont 16 pour l'identifiant. Les références Python et l'état des compteurs font partie de l'instance. Le dépassement reproduit vaut 97 + 97 + 328 + 49 − 256 = **315 octets**.

Le processus Rust possède le socket de relais, les compteurs et les deux pages dans une structure sans allocation indirecte. Sur Darwin ARM64 natif et x86_64 sous Rosetta, `size_of::<State>()` vaut 48 et `size_of::<Link>()` vaut 8 248. **56 octets** couvrent les métadonnées de liaison, descripteur et alignement compris. L'enregistrement de diagnostic sérialisé occupe également 56 octets. La marge dans les deux mesures est de 200 octets. Deux assertions de compilation vérifient les bornes.

Ce budget porte sur l'état conservé pour le protocole des pages. Le processus adaptateur dispose aussi d'un tampon de transit NBD de 128 Kio. Les processus du banc, leurs piles, les buffers de sockets du système et les bibliothèques TLS des deux extrémités sont hors de cette mesure. La mesure ne donne pas le RSS d'un déploiement à plusieurs liaisons.

## Encodage canonique NATM/1

Les offsets comptent depuis zéro. Les entiers multioctets sont non signés, en petit-boutiste. Le code partagé écrit explicitement chaque champ et exclut le remplissage natif de la sérialisation.

| Offset | Octets | Champ |
|---:|---:|---|
| 0 | 4 | Magie ASCII `NATM` |
| 4 | 1 | Version 1 |
| 5 | 1 | Liaison fermée, 0 ou 1 |
| 6 | 2 | Taille totale, exactement 56 |
| 8 | 16 | Incarnation complète du transport |
| 24, 32, 40 | 8 chacun | Séquence TX admise, séquence RX produite, dernière RX acquittée |
| 48, 50, 52 | 2 chacun | Octets TX déjà transmis, taille TX, taille RX en attente |
| 54 | 2 | Réservés, zéro |

Le décodeur exige exactement 56 octets, la version connue, les réserves nulles, des longueurs cohérentes et des compteurs compatibles avec une seule réception en attente. Le diagnostic sert à observer la liaison. Il ne confère aucune autorité de reprise.

Sur les pages, le format `NA24/1` garde son en-tête de 64 octets et jusqu'à 448 octets de données par cellule, pour les deux tailles de secteur. Une enveloppe plus longue traverse plusieurs cellules. Une taille déclarée illégale, une version inconnue, un remplissage non nul ou une séquence incohérente rompent la liaison. Une répétition strictement identique de la dernière cellule ne transmet pas deux fois les octets. L'acquittement TX attend la remise complète au socket. La sonde vérifie la version, les encodages extrêmes et les tailles invalides dans l'invité.

Le serveur, le domaine, la session, l'identité du registre, la génération, l'empreinte de l'état, l'identifiant d'opération, le nonce et la signature restent dans l'enveloppe authentifiée. L'adaptateur transporte tous ses octets. Aucun de ces identifiants n'est dérivé des compteurs de transport. Les deux pairs de laboratoire établissent TLS 1.3 mutuel, vérifient le certificat attendu et vérifient la signature applicative Ed25519. Chaque pair crée ses clés privées dans son propre processus hôte. NovaFS reçoit des cellules opaques.

## Essais

Les quatre profils utilisent deux vCPU et une copie privée P9. La sonde NovaFS est construite dans un export de qualification. Les tests du transport font circuler les données via les E/S de l'invité. Les clés TLS, la signature et la décision témoin sont exercées dans deux processus hôtes distincts. Le registre de décisions de cette recette TLS vit en mémoire.

| Contrôle | AArch64/512 | AArch64/4096 | x86_64/512 | x86_64/4096 |
|---|---|---|---|---|
| Aller-retour exact de 1, 448, 449 et 2 048 octets, 24 échanges par profil | réussi | réussi | réussi | réussi |
| Rejet d'une signature invalide, d'un rejeu applicatif, d'un enregistrement TLS rejoué ou altéré | réussi | réussi | réussi | réussi |
| Réponse perdue, nouvelle connexion mutuellement authentifiée, relecture du reçu par identifiant avec un nonce frais | réussi | réussi | réussi | réussi |
| Saturation réelle du socket, drainage vérifié, coupure et rupture observée par l'invité | 10 cellules | 10 cellules | 10 cellules | 10 cellules |
| Empreinte P9 identique avant et après transport | oui | oui | oui | oui |

L'empreinte de la copie P9 du témoin de transport vaut `df389d30ef2a2bdcb07958d05c55f73adeaeb465e5c5e8bfec613b180bbff827`. Les cinq tests Rust du relais ont réussi sur chacune des deux architectures hôtes. Ils couvrent notamment les tailles tronquées et excédentaires, la version inconnue, la substitution de liaison, la coupure à trois étapes et l'effacement des deux pages. Les neuf contrôles ABI et les treize tests de composants du registre et des sessions ont réussi. L'entrée de test ignorée lance le sous-processus privé utilisé par la recette du reçu.

Le banc historique de sessions a ensuite exécuté sept contrôles avec chacune des quatre ISO/géométries du candidat. L'installation et l'admission réussissent, le bail expiré refuse avant création, l'ancien handle reste révoqué, la nouvelle authentification est explicite, la double fermeture traite le descendant, la réponse perdue est retrouvée sans répétition de la mutation et les ressources sont drainées. Les premières tentatives avec une image dépourvue du paquet attendu ont échoué. Le rejeu utilise les images initiales du banc historique, dont les empreintes sont enregistrées.

### Mémoire de l'invité et régression IPC

Le banc historique `ipc.py test` utilise son mode d'amorçage dédié, indépendant de la géométrie P9. Les compteurs de repos, de destruction et de reprise sont identiques dans chaque passage. Le compteur IPC revient à zéro. Les noyaux ordinaires passent sur AArch64 (1 761 cadres retenus) et x86_64 (1 373). Avec les noyaux instrumentés de qualification, x86_64 passe sur la base et le candidat (1 824 et 1 827). AArch64 échoue au plafond absolu de 2 048 cadres sur les deux arbres (2 261 et 2 264). L'image de qualification augmente de trois cadres de 4 Kio sur les deux architectures. La réserve AArch64 instrumentée reste ouverte. Les attentes du banc historique sont inchangées.

## Reproduction et limites

Depuis la racine du worktree et avec la politique TMPDIR du projet active, construire l'exécutable de mesure et le banc natif avec le même compilateur que pour la qualification.

```sh
sources=novaOS/user/novaauth/transport_qemu
export NOVA_ADAPTATEUR="$NOVAOS_TMPDIR/nova-transport-natif"
rustup run stable rustc --edition=2024 -O "$sources/adaptateur.rs" -o "$NOVA_ADAPTATEUR"
python3 -B "$sources/mesure_memoire.py"
rustup run stable rustc --edition=2024 --test "$sources/adaptateur.rs" -o "$NOVAOS_TMPDIR/nova-transport-tests"
"$NOVAOS_TMPDIR/nova-transport-tests"
```

Le profil est produit par `service/profile.py --transport-autorite` à partir d'un commit, puis compilé avec `novaauth-sessions-lab`. Le banc `essai_qemu.py ARCH SECTEUR ISO IMAGE_P9 SORTIE` exige une destination neuve et consigne la commande QEMU, les empreintes et les contrôles par profil.

La recette du dossier, de la restauration des disques locaux et du refus lorsque l'autorité est injoignable reste qualifiée au niveau du composant P9 et de l'autorité hôte. Son raccord complet au signataire invité, à l'autorité durable et à l'effecteur VFS reste à qualifier. Cette étape conserve les 48 sélecteurs ABI v1, les limites IPC et le format P9. Elle ne qualifie ni le matériel physique ni une autorité de production.

## Preuves durables

Sous la racine des worktrees, le dossier `archives-novaauth-autorite-0240/metadonnees-natives-974ce9c/` regroupe le manifeste et l'archive. Les **46 fichiers** ont été relus après décompression et comparés aux copies durables. Le manifeste a aussi été relu dans l'archive.

| Objet | SHA-256 |
|---|---|
| `metadonnees-transport.tar.gz` | `5e0aac2e8e955261f2d1131592ce1da646e474da864a5995f60ca935d1ed0ef4` |
| `SHA256SUMS` | `e1c5d764789dd3d7cb9db78a9d72829ba01707609f0d068e198c4020e1061532` |

Le fichier `originaux-durables.json` référence les ISO, séries et disques originaux conservés dans `preuves-transport-natif-974ce9c/final/`. Les résultats du transport, les extraits IPC, les deux binaires natifs, les sept contrôles de session par profil et les empreintes des sources sont dans le paquet de preuves. Les images comportant les composants de laboratoire restent sur le stockage privé.
