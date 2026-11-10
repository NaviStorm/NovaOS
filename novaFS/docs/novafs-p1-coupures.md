# P1 v0.9.1 — résultats des coupures et des erreurs simulées

Les cinq essais P1 et les trois essais N02/N03/N05 ont réussi dans deux exécutables macOS distincts, sur ARM64 natif et sur x86_64 sous Rosetta. Les résultats ci-dessous sont donnés **par exécutable**. Le simulateur sépare les blocs stables du cache d’écriture perdu à la coupure. La géométrie désigne la taille de secteur du modèle. Les observations sur fichiers hôtes et les hypothèses matérielles sont consignées dans le [modèle de durabilité](novafs-p1-durabilite.md).

## Oracle et garantie testée

L’observateur crée un catalogue indépendant avec deux fichiers. Le premier conserve des octets déjà acquittés. Le second comporte un trou initial de 4 Kio suivi de deux blocs non nuls. Une écriture de 4 Kio qui commence 512 octets à l’intérieur du second bloc modifie deux blocs logiques et laisse intacts le trou et les octets voisins. L’oracle applique le remplacement à un vecteur d’octets en mémoire et compte lui-même les générations. Il prédit le contenu et la révision du fichier, sa taille, les deux noms, le marqueur, la fin de fichier et le numéro de la tête. Aucune lecture de l’image candidate n’alimente ces valeurs. Le journal du périphérique sert à cibler les adresses et les frontières d’injection. [Essais P1](../tests/durability.rs)

Lors d’une coupure à une frontière, l’oracle exige l’ancienne génération avant acceptation de l’ancre et la nouvelle après l’effet stable de F2. Dans la fenêtre intermédiaire, la politique d’injection indique exactement si l’ancre entière a persisté. Un succès d’API exige la nouvelle génération. Une erreur d’écriture de l’ancre ou de F2 peut cacher une transaction **intégralement publiée** dont le client n’a reçu aucun succès. Elle ne permet aucune nouvelle mutation sur cette instance. Un assemblage de données et métadonnées provenant de deux états incompatibles échoue au test.

Chaque image récupérable du scénario d’écriture est montée en lecture seule, vérifiée intégralement, démontée et montée en écriture. Après stabilisation de cette tête et nouvelle perte du cache, un troisième montage doit restituer le même catalogue et les mêmes octets. Lorsqu’une déchirure de l’ancre altère son enveloppe, les deux modes normaux doivent refuser le montage. Le secours sur le slot survivant doit retrouver la génération antérieure, vérification intégrale comprise.

## Matrice exécutée

| Injection ou contrôle | Secteurs de 512 octets | Secteurs de 4 096 octets | Critère observé |
| --- | ---: | ---: | --- |
| Formatage initial, coupures sur 21 frontières | 63 | 63 | Par géométrie, 54 refus et 9 racines vides vérifiées en génération 1. Une ancre initiale isolée ne suffit pas. |
| Mutation, coupures sur 27 frontières | 81 | 81 | À chaque frontière, perte du cache, persistance de la dernière écriture ou de toutes les écritures en ordre inverse. Vue exacte à chaque remontage. |
| Coupure après acquittement | 1 | 1 | La génération 6 survit intégralement. |
| Erreurs d’écriture ciblant chacun des sept blocs soumis | 70 | 56 | Six dépendances, données et métadonnées comprises, puis une ancre. Test de préfixes nuls, partiels ou complets suivis d’EIO. Trois ancres mixtes refusées dans chaque géométrie. |
| Échecs de F1 et de F2 | 9 | 9 | F1 persiste de zéro à six dépendances avant l’erreur, puis coupure. F2 persiste zéro ou une ancre avant l’erreur. Les données déjà acquittées restent lisibles. |
| EIO de lecture | 4 | 4 | Les deux slots et le manifeste sont testés au montage. Un défaut sur des données à la lecture interrompt la lecture et gèle l’instance. Après retrait du défaut, le catalogue entier est retrouvé. |
| Remises à zéro de blocs stables | 3 | 3 | Les métadonnées et l’ancre effacées provoquent un refus de montage. Les données effacées provoquent un refus à la lecture et à la vérification intégrale indépendante. |
| Redémarrages successifs sur la même lignée | 48 | 48 | 12 succès reçus, 12 états entièrement publiés sans réponse positive, 24 échecs précédant la persistance de l’ancre. Génération finale 29 et conservation de tous les octets acquittés. |

Les 27 frontières de mutation couvrent trois événements par écriture (avant soumission, après acceptation et après complétion) et trois par flush (soumission, effet, complétion). L’EIO d’une écriture partielle peut laisser des octets modifiés. Le test les autorise à tous atteindre le support avant la perte du cache. Pour l’ancre, il reconstruit le bloc résultant du mélange des préfixes et vérifie les octets de l’image. Les longueurs exercées incluent les frontières de secteur ainsi que les offsets 64, 96, 183 et 184 du profil P0. Une coupure qui ne modifie aucun octet différent de l’ancienne ou de la nouvelle enveloppe garde une ancre valide. Les contrôles d’EIO vérifient aussi la phase renvoyée et le refus d’une mutation ultérieure.

N02 conserve ses six mutations et ses 459 coupures sur secteurs de 512 octets. Les listes et contenus attendus pour la création qui scinde le B+Tree, le renommage, la suppression, la collecte et les deux cas d’écriture sont maintenant construits hors du moteur. Le rejeu sans défaillance fournit uniquement les événements et les reçus physiques à comparer. N03 conserve les 256 masques en géométrie 512, les deux masques en géométrie 4 096 et les témoins négatifs de barrières mensongères. N05 conserve le témoin externe de fraîcheur. [Essais N02/N03/N05](../tests/crash.rs) · [Détail des scénarios P0](novafs-prototype-pannes.md)

## Reproduire depuis la racine du dépôt

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run test --offline --test durability --test crash -- --nocapture
NOVAFS_TARGET=x86_64-apple-darwin novaFS/lab/run test --offline --test durability --test crash -- --nocapture
```

## Portée des résultats

Une erreur sur un bloc ne s’étend pas à son voisin dans cette campagne. Les essais contrôlent des préfixes, sous-ensembles et ordres déterminés. Les contrôles de checksum détectent les mélanges effectivement exercés. Une panne corrélée du support, un retour intégral à une image plus ancienne, un contrôleur qui acquitte une barrière prématurément ou une panne électrique de la machine exigent des preuves supplémentaires. Les campagnes mesurent la récupération sur les images générées et ne constituent pas une qualification physique.
