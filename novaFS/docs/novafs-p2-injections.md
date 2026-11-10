# P2 v0.9.2 — résultats des injections de corruption

Les 14 cas ci-dessous ont été exécutés sur macOS ARM64 natif puis sur x86_64 sous Rosetta. Chaque exécutable a contrôlé les secteurs de 512 et de 4 096 octets. Les sept tests Rust de la matrice LV ont réussi sur chaque architecture. Ils regroupent de nombreuses images et 168 invocations de la commande `novafs check` par architecture. Les campagnes relatives au catalogue physique s’y ajoutent. [Matrice LV](../tests/check_corruption.rs) · [Essais PD et LV](../tests/volumes.rs) · [Mutations du catalogue](../tests/support/pool.rs)

## Oracle de contenu et absence d’écriture

L’image LV ordinaire comporte 256 blocs de 4 Kio. Le modèle connaît les deux générations avant de modifier les octets du disque. G3, dans le slot 1, possède un fichier de 8 Kio rempli de `0x17`. G4, dans le slot 0, conserve ce préfixe, ajoute un trou nul de 4 Kio et une page remplie de `0x74`. Les injections changent des champs à leurs offsets documentés. Pour les mutations sémantiques, le banc calcule lui-même les SHA-256 et propage les références jusqu’à l’ancre. L’oracle ne demande pas au vérificateur de choisir les générations de référence.

Chaque appel contrôle le résultat individuel des slots, puis le verdict sur la paire. Une vue annoncée récupérable est ouverte explicitement en secours et comparée au contenu et au répertoire prévus. Un support instrumenté fait échouer le test au moindre `write` ou `flush`, ainsi qu’au-delà de 4 096 lectures sur cette petite fixture. Le banc compare tous les octets du support avant et après. Pour chaque image LV soumise à la CLI, il ouvre un fichier temporaire en lecture seule, décode le JSON, vérifie le code de sortie puis relit le fichier **octet par octet**, y compris ses blocs libres. Les EIO ponctuelles sont injectées à l’interface de lecture du simulateur.

## Résultats observés

| Cas | Injection | Diagnostic exigé et obtenu sur les deux architectures |
| --- | --- | --- |
| C01 | Altération de la signature d’une ancre, successivement dans chaque slot. Essai supplémentaire avec version inconnue et somme recalculée. | Seul le slot atteint est rejeté. Le slot survivant restitue G3 ou G4. La paire est indisponible, y compris lorsque G4 est encore lisible. |
| C02 | Signature du slot 0 et checksum du slot 1 altérés simultanément. | Deux diagnostics distincts, aucune tête retenue. |
| C03 | Un bit de la somme du manifeste de G4 est inversé. | Le manifeste est rejeté pour checksum. G3 reste récupérable. |
| C04 | Génération précédente du manifeste de G4 fausse. Dans une seconde image, génération correcte mais digest du prédécesseur divergent. | La première image perd la vue G4. La seconde conserve deux vues intégralement lisibles mais la paire est rejetée. |
| C05 | La racine objets du manifeste pointe vers le bloc 261 d’une image de 256 blocs, avec des sommes correctes. | Référence hors limites rejetée avant déréférencement. |
| C06 | Deux répertoires se désignent comme parents. Les deux index inverses sont cohérents, les entrées triées et les sommes valides. | La vérification de profondeur/parenté détecte le cycle. La génération antérieure conserve le seul répertoire `/a`. |
| C07 | Un deuxième extent empiète sur la plage logique du premier. Dans une autre image, leurs plages logiques sont disjointes mais ils revendiquent la même adresse physique. | Le premier cas signale un recouvrement logique. Le second détecte un bloc multiplement possédé. |
| C08 | Retrait d’un bloc de la longueur du dernier intervalle libre, sans lui donner de propriétaire. | La partition de l’espace est incomplète, malgré les checksums recalculés. P0 n’encode pas de total libre global indépendant. |
| C09 | Identité de LV remplacée dans l’ancre G4, puis somme recalculée. Autres images sur PD avec catalogue de pool, grains, propriétaire et LV prêt. | L’identité attendue du LV exclut G4. Côté pool, sept catalogues malformés avec empreinte valide sont rejetés en géométrie 512. Les deux géométries refusent cinq successions de catalogues pourtant cohérents pris séparément. La CLI teste aussi un LV prêt aux données altérées, une ancre de pool altérée et une erreur réelle de lecture. |
| C10 | Un bit du dernier bloc de données appartenant uniquement à G4 est inversé, sans changer les métadonnées. | Le contrôle complet de la donnée rejette G4 et préserve G3. |
| C11 | EIO ponctuelle sur le bloc de données de G4, puis sur chacune des deux ancres dans deux essais distincts. | Le diagnostic indique une erreur d’E/S et le numéro de bloc exact. L’autre slot est lu et vérifié indépendamment. Les octets du support restent intacts. |
| C12 | Image réelle amputée de 1, 512 ou 4 096 octets. | Dans les six cas par architecture, JSON de longueur incohérente, code 1 et fichier tronqué intégralement préservé. |
| C13 | 64 inversions déterministes par géométrie, avec bit et offset pseudo-aléatoires. Les cibles alternent entre les deux ancres, la page privée de G4 et le dernier bloc libre. | Dans chaque géométrie, les 48 altérations de zones protégées invalident le slot prévu, les 16 modifications d’espace libre laissent valides les deux vues. |
| C14 | Une transaction en G5 échoue à F2. Le cache perd la nouvelle ancre, puis l’ancre G4 confirmée est endommagée. | Avant corruption, la paire confirme la vue G4. Après corruption, seule G3 reste extractible et la paire est indisponible. Le vérificateur ne ressuscite pas G5 et ne présente pas G3 comme la dernière génération confirmée. |

## Limites mises en évidence

Deux écritures légitimes différentes sur deux copies de la même base produisent des images intégralement vérifiables de G5 avec des contenus et des reçus différents. Ce témoin a réussi sur les deux architectures et les deux géométries. Une autorité extérieure qui conserve le reçu peut comparer les digests. Sans ce témoin, l’intégrité interne ne donne ni l’authenticité des octets ni la fraîcheur de la copie présentée. Les octets d’un bloc libre, le contenu de volumes RAW et les données rendues inaccessibles aux deux générations ne sont pas attestés par les checksums du FS. Le corpus de mutations constitue un échantillon déterministe.

## Pénuries effectives de l’allocateur

Un exécutable de test isolé délègue les allocations à `System`. Après la lecture d’un bloc déterminé, il refuse une seule demande de taille exacte sur le thread de vérification. Pour un volume de 513 blocs, les cibles ARM64 et x86_64 ont chacune exercé huit refus, quatre par géométrie. Les demandes visées étaient le bitmap de 72 octets, la table de deux cellules de 96 octets, une clé de 8 octets et un record de 229 octets. Le décodeur réserve falliblement la table, les clés et les valeurs. [Injection dans l’allocateur de test](../tests/check_oom.rs)

Dans chaque essai, le slot affecté signale `NoMemory`, l’autre slot termine sa vérification et la paire reste indisponible. Une seconde exécution complète sur le même périphérique retrouve la génération attendue et les octets sont identiques. L’essai porte sur ces demandes du moteur. Il ne démontre pas la survie du processus à un épuisement de toutes les allocations, notamment celles du runtime, de la construction JSON ou du système hôte.

## Reproduction

```sh
for arch in aarch64 x86_64; do
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline \
        --test check_corruption --test check_oom --test volumes -- --nocapture || exit 1
done
```

Les budgets globaux du vérificateur et les garanties du montage sont décrits dans le [contrat d’intégrité](novafs-p2-integrite.md).
