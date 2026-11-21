# NovaAuth v0.24.0 — Défis et traces : qualification ciblée

Bilan du 21 novembre 2026. **Les deux blocages ciblés sont levés dans les quatre profils QEMU testés ; fusion toujours bloquée**, faute de campagne globale démontrée sur le candidat final. Aucun essai UTM, matériel ou de production n'est revendiqué.

## Références et périmètre

- Départ : `9b7c7a9b1e98c0947e9e5a3d714d35819edb9b4c`, branche `codex/novaauth-autorite-0240`.
- Correctif du transport privé et des traces : `2b4382a6f357f020103ab20a480e5d8cd02adaa1` ; normalisation documentaire du collecteur : `eb4c4cc089d9abd9f3ec45b4c121858b0b461c85`.
- Séparation des rejeux fonctionnels et cycles mémoire : `9ae021be953b817c8eadb1202eaaf03308167e32`.
- Calibration explicite de l'injection hôte : `512f8d8ebb7ac1114634967fc4a7c50b3742ec0c`, candidat de code et de banc de ce bilan.
- Binaires exacts : export EXEC QEMU de `eb4c4cc`. Les deux commits suivants modifient uniquement les scripts hôtes ; les sources invitées sont identiques. Le commit portant ce rapport ajoute seulement des documents.
- `origin/main` observé avant publication : `a3efdbb70d652f2687e7bbf31e27872fbe05c3a1`. Le candidat local de fusion `b08ba80f6edbd49f06fa7ad28a1810fd2157cd55` est préservé.

La [matrice structurée](validation/novaauth-auth-ciblee-512f8d8.json) donne les commits, profils, chemins et empreintes des preuves actuelles et historiques. Elle distingue explicitement leur portée. Les temporaires sont sous `OS-worktrees/tmp`, les preuves durables sous `OS-worktrees/preuves-auth-ciblee-9b7c7a9`.

## Cause des expirations

Le vérificateur crée un défi de 1 500 ms, obtient sa signature, puis consulte à nouveau l'autorité indépendante pour vérifier la fraîcheur de la liaison avant de vérifier la réponse. Cette consultation reste obligatoire. Le signataire utilise le transport privé pollé par NovaFS, tandis que NovaFS attend sur une autre route IPC.

Dans ce profil, `pair` vaut zéro. L'attente atteignait huit ticks, soit 32 ms, entre fragments ; elle s'accumulait pendant la consultation authentifiée. La sonde limitée à la collecte structurée, sans correction de cadence, mesure entre 1 428 et 1 532 ms après signature sur x86_64/P9-4096. Trois défis sur dix expirent, avec refus natif sûr ; le dernier échoue. L'ancienne ISO, sans cette nouvelle sonde, reproduit déjà quatre refus expirés parmi huit décisions observables, puis perd la neuvième trace. L'anomalie n'est donc pas exclusive à la nouvelle instrumentation. La charge hôte n'est pas retenue comme cause établie.

Le correctif généré exclusivement dans `exec_admission_profile.py` reprend la cadence bornée d'un tick déjà utilisée pour la paire VFS :

```rust
let timeout = if tmp_busy { 0 }
    else if pair || exec_files_lab::config(465) != 0 { 1 }
    else if rt::config(14) != 0 || rt::config(16) != 0 { 8 }
    else { 15000 };
```

Aucune consultation de fraîcheur n'est supprimée ou mise en cache. Aucun délai de défi, TLS ou session n'est augmenté. Avec la même sonde et cette seule différence de cadence, les dix connexions x86_64/P9-4096 réussissent, segment après signature de 732 à 788 ms. La dépendance temporelle au polling est ainsi démontrée ; cette expérience ne mesure pas séparément tous les coûts internes du TLS ou de l'ordonnanceur.

C'est une **correction fonctionnelle d'ordonnancement du profil QEMU**, pas seulement une modification de traces. Le coût CPU des réveils plus fréquents n'est pas mesuré. Aucune garantie d'absence d'expiration sous une charge arbitraire n'est annoncée : un défi réellement expiré doit toujours être refusé.

## Cause de saturation et collecte

`debug_write` limite chaque processus à seize messages. Le vérificateur émettait des succès de connexion, relais et fermeture redondants ; les refus expirés ajoutaient un second message. Le parcours long pouvait épuiser ce quota avant la décision suivante. Cela supprime une observation, pas une vérification cryptographique : l'oracle refusait de conclure, et ne doit jamais inférer un succès du silence.

Dans le seul export EXEC, `auth_trace_profile.py` produit une ligne native `NXAUTH` de **208 octets**, sans allocation dynamique : empreinte publique du défi (32 octets représentés en hexadécimal), puis huit champs hexadécimaux de 64 bits : tag, émission, échéance, signature, vérification, retour, bail et fin. Les dates sont monotones en millisecondes. Une étape non atteinte vaut zéro et ne doit pas être interprétée comme un instant de décision. L'empreinte identifie le défi sans publier de clé ou de signature privée.

Les succès facultatifs de relais/fermeture sont omis ; tous les autres refus restent tracés. Les événements natifs d'admission, révocation, création/PID et décision VFS restent produits par leurs effecteurs, sans suppression. Le banc collecte et structure les lignes ; une ligne incomplète ou malformée ne vaut jamais décision. Les anciennes lignes NXBAIL restent lisibles. Trois tests hôtes supplémentaires vérifient le format, les rejets et les identités de défis distinctes.

Le quota DEBUG reste **16**, le plafond IPC **4 racines**, le délai TLS **10 s**, le défi **1,5 s**, le bail **60 s**. La recette est bornée à dix connexions par instance ; ce changement ne constitue pas un journal illimité. Des erreurs additionnelles peuvent encore épuiser le quota et produire un résultat indéterminé plutôt qu'un succès artificiel.

## Rejeux ciblés

Chaque profil utilise une image P9 neuve et une autorité indépendante hors des disques restaurables. QEMU 11.1.3 ; commandes, versions, ISO et images initiales sont enregistrées dans les résultats et les pilotes archivés. Les redémarrages d'un même parcours conservent ses copies et sa seule autorité. Aucun test de six lancements mémoire n'est répété.

Le premier échange est volontairement retardé ; le défi expiré retourne `DENIED=5`. Neuf demandes explicites suivantes utilisent des défis distincts et réussissent dans le même `nsh`. Aucune commande ELF n'est demandée dans cette recette de connexion.

| Profil | Émission / échéance / vérification du premier défi, ms | Premier résultat | Neuf connexions fraîches | Segment signature→vérification après reprise |
|---|---|---|---|---|
| AArch64/P9-512 | 2 292 / 3 792 / 4 168 | Refus après expiration | 9/9 | 164 ms |
| AArch64/P9-4096 | 4 628 / 6 128 / 6 536 | Refus après expiration | 9/9 | 644–672 ms |
| x86_64/P9-512 | 2 560 / 4 060 / 4 368 | Refus après expiration | 9/9 | 164–184 ms |
| x86_64/P9-4096 | 5 036 / 6 536 / 7 404 | Refus après expiration | 9/9 | 728–828 ms |

Les quarante décisions sont conservées, sans disparition de trace. Les empreintes des dix défis de chaque profil sont distinctes. La récupération 2-sur-2 de préparation et le drainage du registre/signataire sont contrôlés par le banc, mais ce témoin ne remplace pas la recette complète VFS.

### Résultats négatifs conservés

- Deux premiers appels diagnostiques échouent avant les connexions : répertoire de sortie déjà créé, puis seconde attente d'un prompt déjà reçu. Les journaux restent archivés ; ils ne sont pas comptés comme échecs d'authentification.
- L'ancienne ISO échoue par saturation ; l'ISO avec instrumentation seule conserve trois expirations sur dix. Aucune transformation rétrospective en réussite.
- Sur AArch64/P9-512, 1,6 s de retard hôte produit seulement 1 408 ms entre signature et vérification invitée : le défi reste valide et son acceptation est correcte. L'oracle exigeant une expiration échoue ; ce passage ne prouve pas un refus après expiration.
- Une injection hôte de 3 s entraîne un refus avant d'atteindre la vérification du défi (`verification=0`). Elle est compatible avec la borne locale de 2 s par fragment dans `canal.rs`, mais cette cause précise n'a pas de sonde terminale dédiée : elle reste une hypothèse, pas une expiration de défi qualifiée.
- L'injection bornée de 2,2 s obtient ensuite le refus invité après 1 876 ms puis la reprise explicite. Seul le retard de panne hôte est calibré ; aucune borne invitée n'est modifiée.
- Le premier appel des 106 tests hôtes rencontre 25 erreurs de configuration (`NOVA_DETENTEUR` absent). Les trois appels correctement configurés passent ensuite. Les avertissements Python de connexions SQLite non fermées restent visibles ; ils ne sont pas assimilés à des fuites IPC invitées.

## Régressions directement affectées

Huit rejeux à deux VM réussissent : perte de réponse d'admission et admission acquittée avec création retardée, chacun sur les quatre profils. Ils utilisent `--sans-memoire-prolongee` : un seul lancement B, sans cycles 2–6 ni assertion de plateau prétendument exécutée. Le mode historique complet reste le défaut du banc, avec ses assertions inchangées.

Pour la réponse perdue, les quatre observations retrouvent l'admission par identifiant et son enfant identifié, sans seconde mutation ni seconde création. L'oracle conserve l'autre issue admissible, annulation irréversible durable, mais **aucune annulation n'est revendiquée comme observée dans ces quatre passages**.

Pour la création retardée, l'ordre durable est admission A → RETRAIT_EN_COURS → enfant A identifié/collecté → RETIREE → activation B. B reste refusée pendant le drainage. La réponse perdue, l'attente et le délai ne sont jamais utilisés comme preuves d'annulation. Le nouveau lancement B réussit ; les références historiques présentées par B et après redémarrage de A sont refusées sans enfant. Ces essais ne remplacent pas le scénario distinct du ticket non consommé dans une ancienne VM toujours vivante.

### Mesures IPC et mémoire

Pendant les dix connexions, le signataire reste à 4 racines/6 handles et le registre à 4 racines/7 handles. Les pages propres restent constantes : signataire 57 AArch64 / 65 x86_64 ; registre 125 / 132 ; pile 64 chacun. La cinquième racine est refusée, et le drainage de ces deux services réussit sur les quatre profils.

Pendant les huit rejeux, `nsh` passe de 2 à 3 racines puis revient à 2 ; ses handles passent de 8 à 9 puis reviennent à 8. Le registre reste à 4 racines, passe de 7 à 8 handles puis revient à 7 ; le signataire reste à 4/6. Les endpoints globaux reviennent de 50 à 49, les nœuds de 104 à 102 ; processus, threads et droits de collecte retrouvent leur référence. Les pages propres des trois acteurs restent stables. Les compteurs détaillés avant/pendant/après sont conservés dans chaque résultat, y compris les refus sans création et le drainage après redémarrage.

Le pool IPC passe de 58 à 59 cadres au premier lancement. Cette rétention d'un cadre appartient à l'oracle historique ; elle n'est pas présentée comme une restitution physique. Les cadres libres globaux passent de 59 260 à 59 278 sur AArch64 et de 59 753 à 59 772 sur x86_64 : aucune baisse nette inexpliquée dans ces passages. Le plateau prolongé et l'attribution des 32 cadres du tas NovaFS restent des **mesures historiques réutilisées**, pas de nouvelles mesures de cette campagne.

## Couverture consolidée et compatibilité

La matrice contient 32 lignes, une par scénario/profil. La colonne « candidat courant » fait autorité pour la nouvelle couverture :

| Scénario à deux VM | AArch64/512 | AArch64/4096 | x86_64/512 | x86_64/4096 |
|---|---|---|---|---|
| Réponse d'admission perdue | Réussi | Réussi | Réussi | Réussi |
| Admission acquittée, création retardée | Réussi | Réussi | Réussi | Réussi |
| Ticket non consommé, ancienne VM vivante | Non rejoué | Non rejoué | Non rejoué | Non rejoué |
| Double consommation | Non rejoué | Non rejoué | Non rejoué | Non rejoué |
| Réponse terminale perdue | Non rejoué | Non rejoué | Non rejoué | Non rejoué |
| Course, admission première | Non rejoué | Non rejoué | Non rejoué | Non rejoué |
| Course, retrait premier | Non rejoué | Non rejoué | Non rejoué | Non rejoué |
| Autorité indisponible avant admission | Non rejoué | Non rejoué | Non rejoué | Non rejoué |

Les huit résultats actuels ne s'additionnent pas aux résultats d'anciens arbres pour annoncer 32/32. La cadence fonctionnelle du profil EXEC a changé : ses scénarios temporels non rejoués restent à qualifier sur l'arbre final destiné à main. La campagne globale n'est pas lancée dans ce mandat ciblé.

Les **106 tests hôtes** et **9 contrôles ABI** réussissent. Les 48 sélecteurs et leurs contrats historiques restent inchangés. La comparaison des exports Rust ancien/final ne trouve que deux fichiers différents : NovaFS `files.rs` (cadence privée) et `shell_verifier.rs` (sonde). Aucun fichier du noyau, du registre des sélecteurs ou du moteur P9 ne change. Les deux fichiers générés sont archivés avant/après.

Les quatre recettes VFS et quatre ELF historiques restent associés à leurs binaires et commits antérieurs, donnés dans la matrice. Leurs profils ne passent pas par cet export EXEC modifié ; leurs preuves restent réutilisables. Les mesures de mémoire de l'allocateur/cache sont conservées car leur code et leurs invariants ne changent pas, avec la limite de couverture temporelle indiquée ci-dessus. La vérification d'identité ordinaire antérieure est conservée et complétée par le contrôle du périmètre des sources ; aucun binaire ordinaire nouveau n'a été construit ni activé ici.

## Budget et réserves

| Étape | Temps d'exécution mesuré | Plafond |
|---|---|---|
| A, diagnostic QEMU | 263,47 s | 1 200 s |
| B, ciblage et calibrations | 209,14 s | 1 800 s |
| C, huit rejeux | 340,68 s | 1 800 s |
| C, avec tous les contrôles hôtes/ABI journalisés | 381,44 s | 1 800 s |

Les temps sont ceux des commandes de banc, y compris les essais négatifs. Les compilations, analyses et archivages sont distingués du temps d'exécution du banc. Aucun plafond n'est atteint ; aucun succès n'est déduit d'une limite temporelle.

La réserve instrumentée AArch64 de **301 cadres** reste ouverte. Les coûts CPU du polling plus fréquent et la robustesse sous charge arbitraire ne sont pas mesurés. Les autorités et signataires sont de laboratoire, hors périmètre de restauration ; aucune qualification de production n'en découle. Les anciennes preuves de fraîcheur et de révocation ne sont pas transformées en garanties sans autorité indépendante.

**Verdict : fusion toujours bloquée.** Les deux défauts ciblés sont corrigés et leurs rejeux réussissent. Les scénarios globaux obligatoires restant non exécutés sur ce candidat empêchent une déclaration d'aptitude à fusionner. Aucun nouveau développement ni campagne prolongée n'est entrepris dans cette livraison.

## Archives et reproduction

Les **673 fichiers** de preuve nouveaux et leur manifeste sont conservés dans :

- `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/preuves-auth-ciblee-9b7c7a9`.
- Archive : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/qualification-auth-ciblee-512f8d8-preuves.tar.gz`.
- SHA-256 archive : `f3804cbc008b543473af15466418f64a3a1d22a10e11930c42d21efda2171a5b`.
- SHA-256 manifeste : `f46d69c24f6d7fd315c1437f1b88708e1d6e8ca6429c611fff634aa9236a6cc3`.

Les 674 membres ont été décompressés et relus, puis comparés aux originaux par SHA-256. Les journaux négatifs, images synthétiques, reçus, états de l'autorité, scripts et commandes exactes sont inclus. Le contrôle de confidentialité ne trouve aucune des clés connues de cette campagne dans ces preuves.

Les **235 artefacts privés** (dont ISO/ELF exacts et secrets fictifs de qualification) restent hors Git, dans `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/artefacts-prives-auth-ciblee-9b7c7a9`. Leur copie d'archive locale est protégée en lecture/écriture propriétaire ; aucun secret n'est publié.

- Archive privée : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/qualification-auth-ciblee-512f8d8-privee.tar.gz`.
- SHA-256 archive privée : `942ba0b25f2b849f66f3ecfb40de8a39b1a1cc38631e87457186b78504695fd8`.
- SHA-256 manifeste privé : `a4b15862b8e6612752c715fce67014a3c4de249bbea78571a23e1ceb911f16d8`.

Le [reçu de vérification](validation/novaauth-auth-ciblee-archives.json) détaille les deux archives. Les **1 774 preuves antérieures** et leur archive ont été relues avec empreintes concordantes (`acquis-verifies.json`). Les 1 800 preuves initiales et les branches parallèles restent préservées.

Les pilotes archivés `auth-ciblee-run.py` et `auth-rejeu-deux-vm.py` enregistrent leurs arguments exacts dans les fichiers `*-commande.json`. `build-auth-final.sh` reconstruit les deux architectures dans un export privé ; `artefacts-final.json` associe les six ISO/ELF finaux à leurs SHA-256. Les clés de laboratoire ne sont pas distribuées avec les sources. `auth-etape-C.py` borne les huit rejeux et s'arrête au premier échec ; `auth-bilan.py` et `auth-archive.py` reconstruisent les synthèses et vérifications dans un nouveau répertoire distinct. Le script d'archive refuse d'écraser une archive existante.


## Actualisation du 21 novembre 2026

La [qualification globale du candidat aa489ca](novaauth-qualification-globale-v0.24.0.md) complète les 24 scénarios alors manquants. Le verdict bloqué ci-dessus décrit la clôture du mandat ciblé ; le nouveau rapport donne le verdict consolidé et conserve les réserves. Aucune fusion ni clôture de version n’en découle.
