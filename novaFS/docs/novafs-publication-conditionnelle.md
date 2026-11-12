# Publication transactionnelle à garanties conditionnelles

**Spécification proposée, révision 1.** Base de l’étude `fc7ce0c64ae51af10b77641f4cfb2991c5561150`, NovaOS v0.15.0 et NovaFS v0.9.13 P9. Le protocole décrit ici publie une racine CoW sur un volume neuf d’un type distinct. Le remplacement d’une destination sert de cas d’essai. Le service NovaOS distribué le refuse. Le statut de ce document est documentaire.

## 1. Propriété recherchée

Une **génération** désigne tout l’état du volume à l’issue d’une transaction, y compris son catalogue de vues. L’écrivain unique ordonne les générations d’une même lignée. Une génération est **préparée** quand ses objets et son intention sont prêts, **publiée** dès qu’une décision authentique complète est observable avec ses dépendances, **durable** quand les écritures requises ont atteint le domaine de persistance annoncé, et **confirmée** quand l’appelant reçoit le succès. Une réponse affirmative à une invite utilisateur autorise la tentative.

Soit `a` la dernière génération dont le succès a été reçu. Sous les trois préconditions ci-dessous, la reprise retrouve l’état de `a`. Toute tête présentée au montage est `a` ou un descendant vérifié de `a`. Si une décision ultérieure a persisté sans que sa réponse soit parvenue à l’appelant, la tête peut inclure cette opération. L’état précis de `a` reste consultable dans l’historique retenu. Le disque seul ne détermine pas qui a reçu une réponse. Une destination confirmée garde son identité et son contenu jusqu’à une opération ultérieure qui les modifie légitimement.

| Précondition obligatoire | Portée exacte |
| --- | --- |
| H1. Persistance et ordre | Une barrière réussie conserve, à leurs adresses, **toutes** les écritures antérieures dont la complétion a été attendue. Cette propriété couvre les données et toutes les métadonnées de traduction, jusqu’au support qui doit survivre à la panne déclarée. Une écriture en cours peut se déchirer à n’importe quel octet. Les écritures restent confinées aux plages soumises. |
| H2. Survie de la décision | Pour la génération confirmée à retrouver, au moins un des deux exemplaires de la décision est entier, authentique, lisible à sa position prévue et lié à l’identité attendue. La reprise tolère la perte ou la corruption de l’autre. Les deux adresses se trouvent dans le même volume. |
| H3. Disponibilité de ce que la décision engage | Le catalogue, les objets et leurs dépendances transitives, y compris les vues retenues et les valeurs des générations parentes engagées par empreinte, restent intègres et lisibles. Une valeur parente peut se retrouver dans son intention ou dans l’un de ses exemplaires. Les clés de vérification et de déchiffrement nécessaires sont disponibles au lecteur habilité. |

Le modèle suppose un écrivain conforme, une image stable pendant le contrôle, des clés protégées et les propriétés usuelles de HMAC, de SHA-256 et du chiffrement authentifié. Le lecteur peut subir une panne à chaque étape. Les complétions peuvent être perdues et les écritures d’une phase réordonnées. Un accès concurrent par deux écrivains indépendants à la même lignée enfreint le contrat. Deux branches authentiques contradictoires observées imposent le refus. Aucune hypothèse d’atomicité des secteurs de 512 ou 4 096 octets n’intervient dans la preuve.

L’audit a comparé l’alternance des ancres, l’ajout seul, les décisions redondantes, la publication en phases, le contrôle croisé des manifestes, la conservation des générations, l’authentification et la reprise en lecture seule. Le prototype a obtenu 2 032 refus sur 10 056 coupures par architecture avec deux décisions hachées. Le présent protocole ajoute une intention authentifiée et un type de volume distinct. L’intention permet notamment de signaler un rang préparé dont les deux décisions manquent. [Comparaison des huit stratégies](novafs-p9-ancres-dechirees.md#comparaison-des-stratégies) · [Prototype et résultats](novafs-p9-ancres-dechirees.md#résultats-expérimentaux)

## 2. Identification et géométrie

Le type GPT privé proposé pour ce protocole est `6d427fc3-1c6d-4fd0-a615-c8ac98347d2c`, en notation UUID usuelle. Le format des enregistrements est identifié par les huit octets ASCII `NVPUBC01` et la version numérique 1. Le gestionnaire fournit l’étendue exacte et le contexte de confiance défini au §3. Une image brute demande une ouverture explicite de ce profil. Une partition GPT demande aussi la concordance du GUID de type. L’échec d’identification ou d’authentification termine l’ouverture du périphérique sélectionné.

Toutes les adresses comptent des pages de 4 096 octets depuis le début de l’étendue. Le début est aligné sur 4 096 octets et la longueur est un multiple exact de 4 096. Les secteurs logiques admis font 512 ou 4 096 octets. Soit `N` la longueur en pages et `J = ⌊N/64⌋`. Exiger `N ≥ 128` et vérifier chaque multiplication, addition et limite d’étendue avant l’E/S. La révision 1 accepte les générations `1 ≤ g ≤ J`.

| Usage | Numéro de page | Règle |
| --- | --- | --- |
| Exemplaire A de la décision | `A(g) = g − 1` | Zone `[0,J)`. |
| Intention | `I(g) = J + g − 1` | Zone `[J,2J)`. Une intention n’autorise jamais, seule, la publication de sa propre génération. |
| Catalogues, données et métadonnées CoW | `[2J,N−J)` | Pages immuables une fois engagées. Aucune référence d’objet ne peut sortir de la zone. |
| Exemplaire B de la décision | `B(g) = N − J + g − 1` | Zone `[N−J,N)`. A et B sont distants de `N−J` pages. |

Cette géométrie se déduit de la capacité réelle. Chaque copie peut être localisée même si l’autre est illisible. Les blocs 0 et 1 du volume sont réservés à A(1) et A(2). Depuis la création sur support vierge, ils ne portent que des zéros ou une page de ce protocole, éventuellement partielle. Il est interdit d’y poser une ancre P9, d’exposer un sous-volume hébergeant une paire d’ancres P9 ou d’exporter un préfixe permettant un second montage par l’ancien service.

Le lecteur NovaOS v0.15.0 exige `NOVAP0FS` au bloc 0. Le décodeur P9 exige cette même signature au slot sélectionné, puis le type, la géométrie et la somme. L’ancien chemin GPT exige aussi le type P9 à l’entrée attendue. Le nouveau placement provoque ainsi le refus sur l’étendue brute comme sur son préfixe, même si la sélection par GUID est contournée. L’ancien banc conservait des ancres P9 au début de son volume et son préfixe montait une destination périmée. Sa disposition est exclue. Une image P9, une version inconnue ou un profil incohérent ne peuvent déclencher ni conversion, ni formatage, ni essai automatique d’un autre lecteur. [Sélection GPT](../../novaOS/kernel/src/hardware/block/gpt.rs) · [Montage du service](../../novaOS/user/novafs/files.rs) · [Décodage des ancres](../src/codec.rs) · [Essai du préfixe historique](../src/fs/remplacement/protocole/injections.rs)

## 3. Encodage canonique et authentification

Le contexte attendu comprend les identités **FS, volume, lignée, clé de publication et type de graphe**, chacune sur 16 octets opaques non nuls, ainsi que la capacité attendue. Le lecteur reçoit ce contexte et la clé par un canal de provisionnement de confiance. Ils sont statiques pour la lignée. Ce provisionnement ne contient ni numéro de dernière génération ni reçu de transaction. La capacité attendue doit correspondre à celle du périphérique.

I, A et B portent le même corps logique `C` de 208 octets. Les intervalles sont semi-ouverts. Les entiers sont non signés, little-endian. Les champs réservés doivent être nuls. Toute page est lue sur exactement 4 096 octets.

| Octets | Valeur obligatoire |
| --- | --- |
| 0–8 | ASCII `NVPUBC01`. |
| 8–10, 10–12, 12–16 | Version u16 = 1, suite u16 = 1, fonctionnalités u32 = 0. |
| 16–32, 32–48, 48–64 | Identités FS, volume, lignée dans cet ordre. |
| 64–80, 80–96 | Identifiant de la clé de publication, identifiant du type de graphe. |
| 96–104, 104–112 | Capacité `N` et génération `g`, en u64. |
| 112–128 | Identifiant de transaction non nul, unique dans la lignée et identique lors de la répétition du même résultat. |
| 128–160 | Identifiant du corps parent. À la genèse seulement, 32 zéros. Pour `g > 1`, valeur exacte de `ID(C[g−1])`. |
| 160–168, 168–176, 176–208 | Adresse de la racine du catalogue, naissance de cette racine et empreinte de sa page. La racine est neuve à chaque génération, sa naissance vaut `g`. |
| 208–209, 209–216, 216–224 | Rôle u8 (0 pour I, 1 pour A, 2 pour B), sept zéros, adresse réelle de la page en u64. Le rôle et l’adresse doivent correspondre à la zone et au rang lus. |
| 224–4064, 4064–4096 | Remplissage nul, puis HMAC-SHA-256 complet de 32 octets. |

Dans les formules, `||` concatène les octets, `LE64` code huit octets et `\0` désigne un seul octet nul terminant un libellé ASCII. `W` est l’intégralité de la page de racine telle que stockée. Pour un graphe chiffré, elle engage les unités scellées. Les formules constituent le contrat d’interopérabilité.

```text
PRK = HKDF-Extract-SHA256(salt = FS || volume || lignée, IKM = M)
K_I = HKDF-Expand-SHA256(PRK, "NovaFS/PC1/intent-key\0"   || id_clé, 32)
K_D = HKDF-Expand-SHA256(PRK, "NovaFS/PC1/decision-key\0" || id_clé, 32)
ID(C) = SHA256("NovaFS/PC1/id\0" || C)
empreinte_racine = SHA256("NovaFS/PC1/root\0" || FS || volume || lignée ||
                          type_graphe || LE64(adresse) || LE64(naissance) || W)
tag = HMAC-SHA256(clé_du_rôle, "NovaFS/PC1/page\0" || octets[0,4064))
```

`M` est une maîtresse de publication indépendante de 32 octets obtenue par CSPRNG. Le rôle I utilise `K_I`, les rôles A et B utilisent `K_D`. L’authentificateur compare les tags en temps constant. Il refuse les champs non canoniques, une naissance ou position invalide, un identifiant logique nul et toute divergence du contexte attendu. Le déplacement de A vers B, le changement de rang, de volume ou de lignée et la promotion d’une intention en décision échouent à la vérification. Les deux copies authentiques d’un rang doivent avoir un **corps identique octet par octet**. Leurs tags diffèrent. L’identifiant logique et les empreintes d’objets sont publics. Seul le MAC habilite une décision. Une partie qui connaît la clé de décision peut en fabriquer. [HKDF, RFC 5869](https://www.rfc-editor.org/rfc/rfc5869) · [HMAC, RFC 2104](https://www.rfc-editor.org/rfc/rfc2104)

Le type de graphe identifie le décodeur du catalogue. Son contrat exige un parcours fini, intégral et borné des références depuis la racine, avec contrôle de l’adresse, de la naissance, du rôle et de l’empreinte attendue à chaque arête. Il vérifie données, espace, propriété, identités de vues, relations de partage et règles de chiffrement. Il refuse cycle illicite, alias incompatible, dépendance manquante, budget de vérification dépassé ou codec inconnu. Le catalogue est l’unique point d’entrée. Le protocole de publication se teste indépendamment du codec NovaFS avec le vecteur du §8. L’affectation d’un type de graphe de production nécessitera le contrat de son codec.

## 4. Écriture, synchronisation et acquittement

1. Obtenir l’accès exclusif à la lignée, évincer l’ancien écrivain et drainer ses E/S. Sur support neuf, contrôler toute l’étendue vierge et préparer la genèse au rang 1. Sur un volume existant, appliquer le §5. Refuser avant écriture une lignée dégradée, une identité déjà employée pour une autre opération, un journal plein ou une réservation insuffisante. Réserver trois emplacements vierges et la place CoW nécessaire. La révision 1 conserve les générations et exclut rotation du journal, réemploi des objets engagés et TRIM sur ces plages.
2. Construire le catalogue complet et ses objets sur des pages libres hors des graphes protégés. Le remplacement change le lien de destination, les identités concernées, les métadonnées et la comptabilité dans un seul catalogue. Écrire les nouvelles pages et l’intention `I(g)`, puis attendre **toutes** les complétions. À ce stade un manifeste lisible, même vérifié, et une intention authentique indiquent une préparation.
3. Soumettre **F1** sur l’ensemble du domaine concerné et attendre son succès. Relire l’intention, comparer son corps à la préparation et vérifier le graphe qu’elle engage. L’écrivain ne crée un tag de décision qu’après ces contrôles. Une intention stabilisée fixe définitivement l’unique corps admissible à ce rang. Après une interruption, il est interdit de signer un autre corps pour cette intention.
4. Écrire la décision complète en A et en B, attendre les deux complétions, puis soumettre **F2** couvrant les deux copies. L’ordre entre A et B est libre. Ni une complétion d’écriture, ni une seule copie relue, ni un FLUSH en vol n’autorise le succès. Après succès de F2, relire les deux décisions, vérifier leurs tags, leurs rôles et l’égalité des corps avec l’intention. La relecture détecte certains défauts de transmission. La persistance à travers la panne dépend de H1.
5. Livrer le reçu `(FS, volume, lignée, g, ID(C), id_transaction)` seulement après l’étape 4. Avant d’engager un rang suivant, le précédent doit avoir franchi cette étape. Une réponse perdue laisse l’issue indéterminée pour le demandeur. L’identifiant déjà employé sert à interroger le résultat. Toute demande de nouvelle mutation avec cet identifiant est refusée. Une nouvelle confirmation de succès exige une tête stabilisée et la vérification des deux copies de la décision concernée.

Une erreur, un délai expiré ou une complétion manquante arrête les mutations et le réemploi dans l’instance. Un échec avant émission de A ou B ne peut pas publier ce rang. Après soumission de l’une des copies, rendre « issue indéterminée ». Une lecture ultérieure peut retrouver le nouveau catalogue même après l’erreur. La reprise ne répare rien en place. Une page orpheline ne devient jamais une autorité de publication.

## 5. Algorithme de reprise

1. Sur une vue figée, commencer en lecture seule (RO). Parcourir **les trois zones sur leurs J rangs entiers**, même après une page vide. Classer chaque lecture comme zéro intégral, page canonique authentifiée pour le rôle et la position, page altérée, ou erreur de lecture. Une erreur d’E/S reste distincte d’une page zéro. Une géométrie ou une version de profil non admise à l’ouverture rend « incompatible ». Dans un profil 1 identifié, un bit de version altéré dans une seule copie invalide cette copie. Une clé indisponible rend « verrouillé ». Aucune de ces issues ne permet de sélectionner une génération inférieure.
2. Prendre le plus grand rang `h` portant un A ou un B authentique. En l’absence de décision, refuser le montage, y compris si une intention ou un catalogue de genèse est intact. Pour chaque rang de `h` à 1, comparer toutes ses pages authentiques. Deux corps différents au même rang font refuser le volume, même si l’un est soutenu par deux pages. Contrôler la filiation exacte, la genèse, l’unicité des identifiants de transaction et le contexte. Un rang antérieur dont A et B sont perdus peut être reconstruit à partir de I **si et seulement si** son corps égale par empreinte le parent exigé par le descendant déjà vérifié. Le descendant authentique atteste ce passage. Un trou dans les trois sources ou un parent différent impose le refus.
3. Vérifier l’empreinte de la racine de `h`, puis le catalogue complet et toutes ses références, y compris les snapshots et clones. Pour ouvrir une génération historique, refaire la même vérification sur sa racine, après avoir établi sa place dans la filiation. Une décision valide qui désigne un graphe incomplet fait refuser le montage. Interdiction de descendre d’un rang et de réessayer comme si la publication n’avait pas eu lieu.
4. Classer le résultat suivant la table. Rapporter l’identité de la lignée, le rang, l’identifiant du corps retenu et la position de chaque anomalie. Pendant l’inspection et l’extraction, le compteur d’écritures, de FLUSH et de libérations doit rester à zéro.

| Observation sur le support figé | Réponse obligatoire |
| --- | --- |
| Chaîne et graphes requis vérifiés, I/A/B concordants, queue entièrement nulle | Montage RO de `h`. Une demande de lecture-écriture (RW) distincte exige l’exclusivité, le drainage, un FLUSH d’activation réussi et une nouvelle vérification des mêmes conditions, y compris l’intégrité de la queue et du graphe. Cette étape couvre le redémarrage du service avec cache du périphérique encore alimenté. |
| Une copie authentique de la décision subsiste, l’autre manque, échoue à la lecture ou au MAC | Récupérer `h` en RO, signaler exactement la copie dégradée. Appliquer la même restriction si I manque ou est altérée à un rang publié, ou si un descendant a permis d’établir un ancêtre. Suspendre RW. |
| Au-delà de `h`, présence d’une intention seule, de fragments ou d’emplacements illisibles sans décision valide | **Refuser le montage normal, même RO.** Rendre « publication indéterminée » et le dernier préfixe vérifiable. La récupération explicite peut livrer son contenu en RO avec ce diagnostic, sans le présenter comme une tête certaine hors des hypothèses. Un reçu extérieur n’est pas nécessaire pour extraire ce préfixe. |
| Corps authentiques en conflit, parent invérifiable, graphe invalide, clé requise absente | Refuser le montage. Un rapport peut nommer des générations contrôlables séparément. Aucun accès à des octets non vérifiés, aucune correction et aucun acquittement. |

Le refus pour une trace terminale prévaut sur un résultat RO dégradé. L’extraction explicite d’un rang `g` exige un A/B authentique et unique pour ce corps, ou son engagement par un descendant authentique dont la filiation jusqu’à `g` est unique. Vérifier le préfixe de `g` à la genèse et tout le graphe de `g`. Si un reçu est fourni, exiger l’égalité exacte de ses champs. Si une anomalie existe au-delà de `g`, la rapporter avec l’état extrait. Une demande sans reçu peut utiliser le rang du dernier préfixe contrôlé. L’extraction ne peut délivrer un nouveau succès transactionnel.

La récupération d’un état confirmé et la disponibilité d’un montage normal sont deux résultats mesurés séparément. Une queue partielle peut empêcher ce montage alors que l’état confirmé reste intégralement récupérable. Si les hypothèses du stockage sont inconnues, le diagnostic rend seulement compte de la cohérence observée. Toute revendication de fraîcheur ou de durabilité conditionnelle exige un profil de stockage qualifié pour la panne concernée.

### Preuve sous H1 à H3

À la genèse, une décision entière ne peut provenir de l’écrivain qu’après F1. Sa racine authentifiée engage un seul graphe. Par induction, l’écrivain du rang suivant vérifie le corps du prédécesseur stabilisé et inscrit son identifiant dans le nouveau corps. F1 précède tout tag de décision, F2 et le contrôle des deux copies précèdent chaque succès. Une décision authentique peut donc avancer la chaîne d’un rang avant la livraison d’un succès, sans y introduire une préparation isolée.

Au redémarrage, H2 place au moins une décision authentique au rang `a`. Le balayage intégral la rencontre. H3 fournit son graphe et sa filiation. Une tête qui réussirait le montage à un rang inférieur aurait omis cette page. Une tête différente au même rang, un descendant d’une autre branche ou une dépendance contradictoire déclenchent le refus. La récupération de `a` peut suivre la filiation en lecture seule, y compris si une intention déchirée au rang suivant bloque le montage. L’autorité de publication porte sur **une** racine. Chacun des noms, contenus, identités et droits exposés provient du même catalogue vérifié. Une racine d’un état et les pages d’un autre sont rejetées à la première référence incompatible.

### Pannes observables et limites

| Interruption ou dommage | Interprétation après remise sous tension |
| --- | --- |
| Avant publication, y compris pendant F1 | Les objets épars n’avancent pas la tête. Avec une queue vide, la génération précédente monte. Toute trace d’intention terminale entraîne le diagnostic du §5. |
| Après F1, avant les décisions | Le candidat, ses snapshots et son intention peuvent être entièrement vérifiables. Il reste préparé. Aucun succès pour ce rang. |
| Pendant A/B ou F2, écriture partielle ou réordonnée dans la phase | Zéro décision entière donne une queue vide ou indéterminée selon les octets lus. Une ou deux décisions entières concordantes donnent la nouvelle tête si son graphe vérifie. Une copie absente ou altérée est signalée. L’ancienne destination et la nouvelle ne sont jamais combinées. |
| Après F2 et avant réception du succès, puis après réception | La même image peut correspondre aux deux arrêts. Elle rend la nouvelle tête dans les deux cas. L’état de livraison de la réponse demeure inconnu du lecteur. |
| Perte ou corruption de A seule, puis de B seule | Sous H2 et H3 l’autre copie publie le même état. L’inspection localise la dégradation et conserve la vue en RO. |
| Perte des deux copies du dernier rang après confirmation | H2 est violée. Si I subsiste, la trace est la même qu’après F1 avant A/B. Le montage refuse et le diagnostic signale ce rang. Si les trois pages et toute trace de fin de journal reviennent à zéro, l’ancien préfixe peut paraître sain. |
| Perte d’un bloc référencé ou décision arrivée avant ses dépendances à travers F1 | Une erreur de lecture, de digest, de tag ou de structure impose le refus. La copie d’une décision ne recrée pas ses données. La réorganisation au travers d’une F1 réussie viole H1. |
| FLUSH qui annonce à tort le succès ou recul physique intégral | Une ancienne image complète, avec ses MAC et ses clés valides, peut être identique octet pour octet à celle d’une exécution qui n’a jamais confirmé la suite. Les deux lectures ont les mêmes entrées. Aucun algorithme interne ne distingue les deux histoires. |

La disparition conjointe de I/A/B illustre la limite même si les objets préparés survivent. Partir d’une coupure pendant la première phase, avec seulement ces objets persistants. Sur une autre exécution, confirmer la transaction puis effacer ses trois enregistrements de contrôle. La comparaison porte sur **tout** le volume. La présence des mêmes objets CoW n’autorise pas leur promotion. Le prototype antérieur a effectivement reproduit l’égalité sur 2 121 728 octets avec ses deux enregistrements. Cette mesure constitue un antécédent expérimental pour la limite, pas un essai du présent protocole. [Historiques comparés](novafs-p9-ancres-dechirees.md#résultats-expérimentaux)

## 6. Snapshots, clones et clés

Le catalogue engage, pour chaque vue, son identifiant, sa lignée de contenu, son propriétaire, ses droits, sa classe, sa relation de parenté et sa racine. Un snapshot fixe ces références dans la transaction qui le crée. Un clone reçoit un identifiant de vue neuf et conserve initialement les références immuables héritées. Une divergence écrit de nouveaux objets et publie en un seul rang le catalogue qui les désigne. Les vues d’un même volume passent par le même écrivain et le même journal. Le lecteur contrôle les droits de la vue demandée avant de livrer ses données. Un reçu de publication ne confère aucun droit sur une vue.

Tant qu’un état est retenu, conserver ses objets, les règles de décompression, les enveloppes de clés et toutes les racines dont dépendent les descendants, même après une suppression logique de l’origine. Un bloc partagé doit conserver son rôle, son contexte de scellement d’origine et son empreinte dans chacune de ses références. La vérification du catalogue et cette rétention donnent, sous H3, l’ancien contenu du snapshot et celui du clone après divergence. La révision 1 garde tout l’historique jusqu’à saturation. L’accès à une ancienne vue reste soumis à l’autorisation actuelle. [Contrat de partage des vues](novafs-p5-partage.md) · [Contrat de chiffrement du pool](novafs-p7-chiffrement.md)

Pour un graphe chiffré, le codec doit conserver au moins les règles du pool NovaFS. Il sépare les clés de données, de métadonnées, d’engagement de vue et de publication. Il lie par données associées la suite, le domaine, la portée logique, l’instance physique, la naissance, le type, l’encodage et les longueurs. La compression précède le scellement. Le lecteur vérifie la référence de l’instance et le tag avant de décompresser, borne la sortie, puis valide les extents. Le catalogue protège la confidentialité des noms, du contenu et des métadonnées internes. Les identifiants publics sont opaques. La géométrie, les rangs et le rythme des publications restent observables.

Les MAC du journal ne consomment aucun nonce. Pour XChaCha20-Poly1305, tirer un nonce indépendant de 24 octets pour **chaque** nouveau scellement sous une clé, y compris lors d’un réessai ou après une coupure. Reprendre par référence des octets déjà scellés conserve leur nonce et ne déclenche aucun nouveau chiffrement. La garantie statistique suppose un CSPRNG frais après redémarrage ou duplication de la VM. Si cette fraîcheur manque, suspendre le scellement. Une génération, un LBA ou l’identité d’un clone ne fournissent pas à eux seuls un nonce unique. [Règles NovaFS et hypothèse d’entropie](novafs-p7-chiffrement.md#contrat-de-sécurité-et-décision-de-nonces) · [Construction XChaCha20-Poly1305](https://doc.libsodium.org/secret-key_cryptography/aead/chacha20-poly1305/xchacha20-poly1305_construction)

Les descendants autorisés peuvent partager les anciennes unités de leur domaine comme dans le pool actuel. Une copie brute ou un snapshot de VM de la même lignée est admissible pour inspection figée. Deux copies inscriptibles indépendantes exigent des identités de lignée et des maîtresses de publication fraîches. Une restauration autonome suit également les règles de nouveaux domaines et de nouvelles maîtresses de contenu pour les unités réencodées. Une clé administrative de publication ne déverrouille pas les données. Les secrets et sous-clés restent protégés, les tampons privés sont effacés à la libération et les clés de récupération doivent rester disponibles aux seuls titulaires. La perte d’une clé indispensable provoque un refus explicite. Le renouvellement en place de l’identité ou de la maîtresse de publication et la réécriture de l’historique exigeraient une révision distincte. [Autorités et conservation des clés](novafs-p7-chiffrement.md#autorités-clés-et-mémoire)

## 7. Matrice de conformité et prérequis matériels

Cette matrice définit les essais de qualification. La rédaction initiale a permis le recalcul indépendant des vecteurs et un essai de refus par le lecteur P9 (§8). La campagne du 12 novembre 2026 exerce cette révision dans un banc privé sur support VirtIO. Les résultats et le domaine de panne effectivement couvert figurent au §10.

### Qualification en simulation

Exécuter chaque ligne applicable dans les quatre combinaisons ARM64 natif ou x86_64 sous Rosetta, secteurs logiques de 512 ou 4 096 octets. Échanger les images dans les deux sens entre architectures. Employer des images neuves, une horloge d’essai fixée et des clés publiques de test. L’oracle séparé du lecteur conserve les succès **effectivement livrés**, les deux graphes attendus et les événements d’E/S. Après chaque coupure, repartir d’une instance sans cache. Comparer reçu, destination, identité, contenu, dates, droits, source, snapshot et clone. Contrôler le SHA-256 du support entier avant et après toute inspection ou tentative refusée.

| Essai | Injection ou action | Observation exigée |
| --- | --- | --- |
| S01. Encodage et genèse | Vecteurs du §8 dans les deux sens. Essayer mauvaise identité, capacité, suite, version, bornes, rôle, adresse et octets réservés. Genèse vide, partielle puis complète. | Octets et tags de référence identiques. Une genèse dépourvue de décision, un profil inconnu ou une étendue tronquée ne montent pas. |
| S02. Chronologie | Interrompre avant soumission, pendant l’effet et après complétion de chaque écriture, de F1 et de F2. Couper aussi avant/après les relectures et avant/après la livraison du reçu. Inclure une deuxième coupure au remontage ou pendant le FLUSH d’activation. | Aucun tag de décision émis avant F1. Aucun succès rendu avant F2 et le double contrôle. Avec F1/F2 fidèles, chaque succès livré figure dans la tête ou conduit au diagnostic explicite prévu au §5. |
| S03. Déchirures | Dans I, A et B séparément puis dans les copies en vol, conserver des préfixes de 1, 32, 64, 96, 128, 512 ou 4 096 octets, des suffixes et des plages intérieures. Traverser les offsets 208, 216, 512 et 4 064. Essayer tous les masques des secteurs d’une page (256 en 512, 2 en 4 096). | Un mélange ne s’authentifie que si tous les octets sont effectivement ceux d’une page canonique valide. Graphes exacts ou refus prévu. Une trace partielle n’est jamais effacée au montage. |
| S04. Ordre et erreurs | Permuter les dépendances et l’ordre de A/B. Simuler EIO, délai, F1 en erreur, F2 en erreur ayant stabilisé zéro, une ou deux copies. Imposer séparément une décision persistante avec chaque dépendance manquante. | Le retour en erreur ne confirme rien. L’erreur de F1 empêche d’émettre A/B. Une décision authentique avec graphe incomplet provoque le refus sans repli. |
| S05. Dommage local après succès | Corrompre, effacer ou rendre illisible A puis B. Déplacer une page d’une zone à l’autre ou d’un autre volume au même rang. Tester aussi I détruite avec une décision valide. | Une copie authentique survivante et son graphe rendent le nouveau contenu RO. Rang et défaut signalés. Aucune ouverture RW dégradée. |
| S06. Double perte et recul | Après succès, effacer A+B en conservant I, puis comparer au cas après F1. Effacer ensuite I+A+B et comparer à une préparation où seuls les objets ont persisté. Restaurer enfin une image complète antérieure. Introduire un FLUSH mensonger. | I seule donne « publication indéterminée » dans les deux historiques. Dans les cas à images entièrement identiques, le lecteur doit rendre le même résultat dans les deux exécutions. L’oracle constate la perte hors H1/H2. Aucune affirmation de détection universelle. |
| S07. Filiation et saturation | Deux décisions authentiques incompatibles, parent différent, trace après trou, rang dépassé. Perdre A+B d’un ancêtre dont I est engagé par un descendant, puis perdre aussi I. Remplir le journal et l’espace objet. | Conflit ou filiation incomplète refusés. L’ancêtre entièrement engagé peut être vérifié via le descendant. Saturation refusée avant écriture. Aucun wraparound, recyclage ni balayage d’objets pour élire une tête. |
| S08. Transactions et vues | Déplacement et publication d’une copie, avec destination existante. Conserver un snapshot, écrire dans un clone, retirer l’origine et rouvrir chaque vue. Perdre la réponse et rechercher le même identifiant de transaction. | Une seule opération logique. L’identité et le contenu correspondent à chaque vue attendue. Un lecteur non habilité ne gagne aucun droit. Aucun bloc engagé ou partagé ne devient libre. |
| S09. Chiffrement et compression | Clair et scellé, données brutes et LZ4, mauvais secret, domaine verrouillé, tag ou longueur modifiés, bloc substitué. Réessai après coupure, branches divergentes, restauration et panne d’entropie. | Échec explicite sans livrer de clair non vérifié. Contextes et domaines vérifiés. Mesurer que chaque nouvel appel de scellement demande un nonce frais, même pour les écritures abandonnées. Vérifier le refus si l’entropie échoue et l’absence d’appel pour un partage immuable. |
| S10. Compatibilité | Présenter volume entier, préfixe à la taille d’un ancien LV et entrées GPT typées au lecteur/écrivain P9 distribué. Essayer les deux slots et les outils historiques, une fausse étiquette P9 et une genèse partielle. Présenter P9 ordinaire, le banc historique et une version inconnue au nouveau lecteur. | Aucun montage obsolète, formatage, repli ou écriture. Vérifier les octets des images P9 sources et du nouveau volume avant/après. Une acceptation par un ancien lecteur arrête l’intégration. |
| S11. Clôture de l’écrivain | Redémarrer le service avec commandes retardées et cache encore alimenté. Tenter un second écrivain sur la même image et une relance sur intention stabilisée avec un corps différent. | Drainage avant lecture stable. Un seul propriétaire peut émettre. L’état ou l’exclusivité indéterminés bloquent les mutations. Le second corps ne reçoit aucun tag de décision. |

### Qualification du chemin de stockage réel

| Prérequis ou essai | Preuve à exiger pour la configuration exacte |
| --- | --- |
| Domaine de panne déclaré | Identifier disque et firmware, contrôleur, caches, alimentation protégée éventuelle, mode de cache, ponts, image, FS hôte, versions et options. Exiger un contrat écrit de persistance pour le domaine annoncé. Relevés de taille logique, taille physique annoncée et unité de dommage éventuelle ont des significations distinctes. |
| Persistance de bout en bout | Suivre l’achèvement des écritures puis celui de chaque FLUSH à travers l’invité, VirtIO, QEMU, le format d’image, le FS hôte et le contrôleur. Couvrir l’allocation des clusters, tables et autres métadonnées. Écarter un cache qui acquitte hors du domaine promis. Une batterie ou un cache protégé doit couvrir le composant qui acquitte effectivement. |
| Survie d’une copie | Relever les emplacements et les unités susceptibles d’être détruites ensemble, y compris les métadonnées de traduction partagées et le support physique sous les deux copies. Pour annoncer une tolérance matérielle à une panne donnée, il faut justifier que ce défaut laisse une copie accessible et le graphe intact. La distance logique A–B ne démontre pas cette propriété. |
| R01. Moteur UTM | Sur cibles sacrifiables, UTM 5.0.8 / QEMU 10.0.12 AArch64, brut et QCOW2, 512 et 4 096. Tracer OUT, FLUSH soumis et terminés, cache et géométrie. Séparer arrêt ordonné, arrêt forcé du processus et redémarrage du système hôte. Réouvrir les images à froid et comparer aux reçus de l’oracle. |
| R02. Perte d’alimentation | Seulement sur banc matériel dédié, provoquer une coupure avant, pendant et après les barrières et après remise du reçu. Couper le domaine dont la persistance est revendiquée, puis relire après remise sous tension sans caches antérieurs. Conserver les images, traces, identifiants et résultats, y compris tout écart au contrat. |
| R03. Relecture croisée | Relire les images des essais réels en RO sur ARM64 et x86_64 Rosetta. Distinguer un défaut spontané sur le matériel d’une corruption imposée après acquisition sur une copie. Pour prétendre couvrir l’interface UTM, consigner aussi la configuration lancée par celle-ci. |

Le profil UTM archivé a produit 142 images et quatre relectures de disques système. Les 142 cas incluent 134 SIGKILL et 8 arrêts ordonnés. Aucun n’a coupé l’alimentation du Mac. Le chemin mesuré passait par `fsync` dans le backend fichier. Ces observations ne qualifient pas H1 pour une perte électrique, ni H2/H3 pour un défaut physique commun. Un FLUSH réussi ordonne et stabilise selon son contrat. Une garantie d’indivisibilité pendant l’écriture exigerait un engagement distinct. [Audit du chemin UTM](novafs-p9-ancres-dechirees.md#de-lancre-au-support) · [Mesures historiques](validation/v0.9.13/atomicite-utm.json) · [Contrat VirtIO Block, §5.2.6](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)

## 8. Vecteurs d’interopérabilité

Ce codec minimal de test contient une seule page racine et aucune arête. Son identifiant sur 16 octets est ASCII `NVTST001` suivi de huit zéros. Sa page `W` est ASCII `NVTST001` suivi de `LE64(g)` et de 4 080 zéros. Un décodeur de ce codec vérifie ces octets exactement. Les tests de vues et de chiffrement demandent le codec de catalogue approprié.

Utiliser `N=256`, donc `J=4`, et les mêmes paramètres dans les deux géométries. La maîtresse publique du vecteur est la suite des 32 octets `00 01 … 1f`. Les identités FS, volume, lignée et clé valent respectivement 16 répétitions de `01`, `02`, `03` et `04`. Pour les rangs 1 puis 2, les identifiants de transaction valent 16 répétitions de `06` puis `07`. Les racines sont aux pages 8 puis 9. Les trois pages de contrôle sont calculées avec les adresses du §2. Toutes les sorties ci-dessous sont en hexadécimal, octet de tête en premier.

| Grandeur | Génération 1 | Génération 2 |
| --- | --- | --- |
| Empreinte de la racine | `a8f1aeb2e4b90ce8065c0f88ef2b727ce4d0c812760c00487fc466c7e899bc00` | `596a789219975b5d34f7c996c6320752d11ef514eba25c13d7ed80c5154ca395` |
| ID du corps | `4b05695c5809db3257b3b455e9ab6bb3e0a42f672da65f7b1a16b482df69af1f` | `61711ad4d14bbaabdbbf5242095cb946fdf7a8d777c2f35cdde4eeaf506b37db` |
| Tag I, pages 4 et 5 | `51640d347b68a585d1edb53fabbdb9e8ff96783310b3a66607f5ac6d82c88825` | `08a2875a73b6e2a11e65b413e0af9d16d58923d9e4dcdfd6d7db4bb532e0dccb` |
| Tag A, pages 0 et 1 | `3467bb04f74ae18543dc6a47f84630401fecd3c7e10671e912d98d5a15ebe643` | `a514fc1862b38adfdd8d4011257da9277e5760f448aff9068eb56b0d00ef6109` |
| Tag B, pages 252 et 253 | `e63802e36f968510a283e7e1c5534cd05c8ca98e68b2647b3f9d115ef4e373fd` | `ccdf89d0535e356b717b24407572523705ae789fbbf3582d1be8274e3a55bd86` |

Ainsi, après confirmation du rang 2, effacer la page 1 laisse la décision de la page 253 et donne le rang 2 en RO dégradé. Effacer aussi la page 253 en conservant la page 5 impose le refus de montage et signale le rang 2 indéterminé. Copier la page 253 à la position 1 échoue au contrôle de rôle et d’adresse.

**Contrôles exécutés sur ces vecteurs.** Deux calculs indépendants (Python et Node.js avec HKDF) donnent les dix valeurs du tableau. Les binaires hôtes `novafs` v0.9.13 ARM64 et x86_64 ont reçu l’image complète des deux rangs (256 pages) et son préfixe de 252 pages, qui exclut la zone B. Dans chaque géométrie, `verify` a tenté le montage RO et `salvage` a tenté séparément les slots 0 et 1. Les 24 tentatives ont rendu le diagnostic « signature, version ou type de métadonnée », sans extraire d’octets. Le SHA-256 de chaque image d’entrée est resté identique. Ce contrôle couvre le moteur hôte. Le parcours GPT dans NovaOS et les coupures du nouveau protocole figurent dans la qualification à venir.

## 9. Conditions d’intégration future

La révision proposée consomme trois pages de contrôle par génération, soit 12 Kio, en plus de la racine et des objets CoW neufs. F1 et F2 sont sur le chemin de confirmation. Le lecteur parcourt `3J` pages de contrôle, puis les objets nécessaires. Le plafond de générations et l’absence de recyclage bornent la durée d’usage. Ces choix permettent une lecture indépendante et une comparaison de toutes les preuves avant d’étudier une collecte.

Le banc isolé utilise le codec de test, deux implémentations de lecture et les essais S01 à S11. Une proposition de liaison à NovaFS devrait documenter son codec de catalogue, le périmètre des clés, l’exclusivité, le dimensionnement et la preuve de refus des lecteurs historiques. Une qualification physique réclame à la fois un contrat couvrant la panne annoncée et les essais R01 à R03 cohérents avec ce contrat.

Arrêter la qualification si une ancienne voie de montage expose une tête périmée, si un succès manque sous H1 à H3, si la liaison proposée exige de changer P9, si les frontières de persistance restent imprécises ou si un défaut cryptographique permet la substitution, la réutilisation d’un nonce ou l’accès à une vue sans ses droits. Chaque écart doit être soumis à arbitrage avec l’image, le reçu et le chemin d’E/S concernés. Les décisions sur NovaOS v0.15.1, NovaFS P10 et l’activation du remplacement relèvent d’un mandat ultérieur.

## 10. Qualification UTM du 12 novembre 2026

**Décision de qualification.** Sur les 96 essais PC1 du banc VirtIO, les 24 reçus effectivement remis désignent exactement la génération 2 relue. L’application UTM a remis un reçu supplémentaire, vérifié après arrêt et export. Les essais couvrent la perte de la VM sur un Mac qui reste alimenté. La persistance après coupure électrique, la survie d’une copie et la disponibilité de son graphe après un dommage physique commun restent non qualifiées. Le remplacement d’une destination par `cp` ou `mv` reste désactivé dans NovaOS v0.15.0. [Matrice par démarrage](validation/v0.9.13/publication-conditionnelle-utm.tsv) · [Résultats et empreintes](validation/v0.9.13/publication-conditionnelle-utm.json) · [Images, traces et reçus](validation/v0.9.13/publication-conditionnelle-utm-preuves.tar.gz)

### Chemin de persistance

Le banc part du commit `f6570774edee724cfdac78f72fc8304f065b1bb4` sur `main`. Le framework AArch64 provient d’UTM 5.0.8, build 128, QEMU 10.0.12. L’outil `qemu-img` 11.1.3 intervient hors VM pour la préparation, la vérification QCOW2 et l’extraction. Les quatre profils utilisent le firmware EDK2 expérimental archivé, GICv3 sans ITS, TPM CRB et VirtIO-IOMMU sans bypass. Le support EFI est en lecture seule. Le firmware libère son domaine pour le SID 48 et la sonde rattache la cible de test au domaine du SID 56. Chaque nouveau disque porte un identifiant de banc à vérifier avant toute sortie. [Sonde invitée](../../novaOS/kernel/src/hardware/block/qualification/ancres/pc1.rs) · [Lanceur et contrôles QMP](../../novaOS/tests/publication_conditionnelle.py)

| Composant | Configuration constatée | Portée du contrat |
| --- | --- | --- |
| VirtIO Block et QEMU | OUT ordinaires et FLUSH. QMP confirme des secteurs logiques et physiques annoncés de 512/512 ou 4 096/4 096 octets, `writeback=true`, `direct=false`, `no-flush=false`. | Une écriture volatile peut être partielle. L’écrivain attend ses complétions et clôt la phase par un FLUSH. Le backend détermine le domaine de persistance effectif. [VirtIO 1.2, §5.2.6](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html) · [Modes de cache QEMU](https://qemu.readthedocs.io/en/stable-10.0/system/invocation.html) |
| Image brute ou QCOW2 | Fichiers neufs sur le même volume APFS. QCOW2 compat 1.1, clusters de 64 Kio, sans fichier parent. Les fichiers QCOW2 acquis ont passé `qemu-img check` sans réparation. | Les tables et les compteurs de références QCOW2 participent à la disponibilité des deux copies. Dans la fenêtre d’1 Mio, A(2), I(2) et la racine occupent le premier cluster invité. B(2) occupe le seizième. A et B sont distants de 1 008 Kio. [Format QCOW2](https://www.qemu.org/docs/master/interop/qcow2.html) |
| Backend Darwin | Empreinte du framework `16d48f6e3e83fdabb760ac01f17f2c69af844f3403be77fad51c692189896f5f`. Désassemblage ARM64 revérifié, appel de `_qemu_fdatasync` à `0x4967b8`, branche vers `_fsync` à `0x8d69d0`. | Darwin prévoit une demande supplémentaire au périphérique avec `F_FULLFSYNC`. Le chemin observé termine par `fsync`. La persistance physique de F2 sur perte électrique reste à établir. [Backend fichier](https://github.com/qemu/qemu/blob/v10.0.12/block/file-posix.c) · [Contrat Darwin](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html) |
| Mac et stockage | Mac16,5, Apple M4 Max, firmware système 20457.41.3, macOS 27.2 (26B123), APFS 3288.41.1. Volume `disk3s5` sur `disk0s2`, SSD interne APPLE SSD AP2048Z, Apple Fabric, révision 241.40.4. Unité adressable hôte de 4 096 octets. | Le contrat de protection des caches du contrôleur, une unité de dommage physique et la séparation physique de A, B et des objets restent inconnus pour cette configuration. |

**H1.** L’ordre a été observé jusqu’aux complétions invitées et aux compteurs du backend. Les écritures acquittées se retrouvent après destruction du processus. **H2.** Chaque acquisition après reçu contient deux décisions complètes. Des corruptions imposées sur des copies en mémoire confirment la récupération avec une seule décision authentique. **H3.** La sonde a relu la racine du vecteur et les décisions. La simulation a étendu la vérification au catalogue et aux unités brutes, compressées et scellées. Ces observations laissent ouverte la survie de la traduction QCOW2, de la racine et des unités après une panne physique commune.

### Matrice S01 à S11

Les onze scénarios ont été exécutés en ARM64 natif et en x86_64 sous Rosetta, avec les deux tailles de secteur. Le lecteur ouvre une image figée et vérifie les zones I/A/B avant d’accorder un état. L’oracle de coupure conserve séparément les reçus remis. Une inspection compare le SHA-256 du support entier avant et après. Les catalogues de déplacement et de copie produits sur chaque architecture ont aussi été rouverts sur l’autre. [Simulateur et injections](../lab/pc1/simulation.py) · [Lecteur RO](../lab/pc1/lecture.py) · [Catalogue de vues du banc](../lab/pc1/vues.py)

| Scénario | Résultat |
| --- | --- |
| S01. Encodage | Les dix valeurs du §8 concordent. Les essais couvrent identité, rôle, version, capacité, adresse, champs réservés et genèse privée de décision. |
| S02. Chronologie | Coupures aux soumissions, effets, complétions, relectures et remise du reçu. Un redémarrage de l’inspection ou une seconde coupure à l’activation laisse un graphe vérifiable ou un diagnostic explicite. |
| S03. Déchirures | 1 090 inspections en 512 et 74 en 4 096 par architecture. Les cas incluent préfixes, suffixes, plages internes, les 256 ou 2 masques de secteurs de chaque page et des déchirures simultanées de A et B. |
| S04. Ordre et erreurs | Les deux ordres de préparation et de publication ont été permutés. EIO, délai et échec de F1 empêchent le reçu. Après une F2 en erreur, zéro, une ou deux copies peuvent avoir persisté. Une tête dont la racine manque est refusée. |
| S05. Dommage local | Une copie absente, illisible, transplantée ou portant un MAC incorrect laisse le rang 2 en RO dégradé si l’autre est authentique. Une racine devenue illisible interdit le montage de cette tête. Le rang 1 reste extractible sur demande avec signalement de l’anomalie. |
| S06. Ambiguïté | L’image entière après F1 est identique à celle d’un succès suivi de la perte de A+B en présence de I. Le lecteur rend « indéterminé » dans les deux historiques. Un FLUSH mensonger peut produire cette même image après reçu. Un retour intégral à l’ancienne image authentique est indétectable sans information supplémentaire. |
| S07. Filiation | Forks authentiques, parent incorrect, trou, identifiant de transaction réutilisé et saturation ont été exercés. I permet de retrouver un ancêtre engagé par un descendant. La disparition de ses trois enregistrements impose le refus. |
| S08. Vues | Le catalogue expérimental conserve contenu, identité, date, droits, source, destination, snapshot et clone aux coupures de publication. Après divergence du clone et retrait de l’origine, les deux vues retenues se rouvrent. Un identifiant de transaction ayant perdu sa réponse désigne une seule opération. |
| S09. Confidentialité | Le codec de paires NovaFS vérifie les quatre combinaisons brut/LZ4 et clair/scellé. Par géométrie et architecture, le banc a compté 12 demandes de nonce, 10 nonces distincts et deux échecs injectés. La branche indépendante et la restauration emploient chacune un nouveau domaine et une nouvelle maîtresse de contenu. Le partage immuable ne demande aucun tirage. Une clé erronée, un domaine verrouillé, un mauvais tag, une longueur altérée ou une substitution interdisent la lecture. [Contrôles Rust](../examples/pc1-controles.rs) |
| S10. Compatibilité | Moteur distribué en RO et RW, sauvetage des deux slots et outils historiques ont refusé étendue entière, préfixe, genèse partielle et fausse signature. Le lecteur PC1 a refusé l’ancien P9 et la géométrie du journal historique. En complément, quatre démarrages NovaOS v0.15.0 ont refusé les deux GPT privées (`Geometry`) ou les deux fausses étiquettes P9 (`Device`). Chaque disque a conservé son SHA-256, avec zéro OUT et zéro FLUSH. [Essais GPT](../../novaOS/tests/publication_compatibilite.py) |
| S11. Écrivain | Dans le modèle à propriétaire unique, des commandes retardées doivent être drainées. Une intention stabilisée empêche la signature d’un second corps. L’exclusivité entre copies indépendantes d’un fichier ou entre deux Mac reste hors du banc. |

Les images P9 servant de références ont été ouvertes en lecture. Le contrôle final a retrouvé les mêmes SHA-256 pour 160 images et fixtures historiques et pour sept archives ou binaires, dont l’installateur distribué.

### R01 à R03 et résultats de relecture

Les 116 essais du moteur comprennent 96 essais PC1 et 20 témoins d’E/S chronométrés. Chaque ligne repart d’un disque neuf de 64 Mio (secteur 512) ou de 268 Mio (secteur 4 096). L’étendue PC1 fait 256 pages. Avant RW, la sonde vérifie toute la genèse, attend le FLUSH d’activation puis vérifie à nouveau. Après F1 elle relit I et la racine avant de produire les tags de décision. Après F2 elle relit A et B et remet le reçu. La ligne série remet le rang et l’identifiant du corps. L’oracle les associe aux identités FS/volume/lignée et à l’identifiant de transaction fixés au §8. Le transport de reçus multivolumes reste hors du banc. Le pilote soumet un secteur par OUT. Les 24 arrêts en cours d’E/S ont utilisé les points d’arrêt `write_aio` ou `flush_to_disk` du backend fichier, avec contrôle des soumissions et des complétions. Les autres points incluent un arrêt entre deux secteurs de A, après F2 avant le reçu, et immédiatement après sa remise.

| Secteur et image | PC1 / témoin | SIGKILL / arrêt ordonné | Rang 1 sain / indéterminé | Rang 2 dégradé / sain | Reçus PC1 retrouvés |
| --- | ---: | ---: | ---: | ---: | ---: |
| 512, brut | 24 / 5 | 19 / 10 | 6 / 6 | 2 / 10 | 6 / 6 |
| 512, QCOW2 | 24 / 5 | 19 / 10 | 6 / 6 | 2 / 10 | 6 / 6 |
| 4 096, brut | 24 / 5 | 19 / 10 | 6 / 5 | 3 / 10 | 6 / 6 |
| 4 096, QCOW2 | 24 / 5 | 19 / 10 | 6 / 5 | 3 / 10 | 6 / 6 |

Les colonnes de rang comptent uniquement les essais PC1. Après le premier secteur de A, 512 octets laissent manquer son MAC. En 4 096, une seule OUT contient toute la décision A. La génération 2 est alors récupérable en RO dégradé, avant F2 et avant tout reçu.

- **R01, moteur et application.** 76 processus ont reçu SIGKILL et 40 se sont terminés sur demande après fermeture du pilote. La comparaison porte sur tout le Mio de PC1, la marque et les zéros du reste du disque virtuel. Dans 80 cas, une deuxième VM a relu tout le Mio sur VirtIO sans OUT ni FLUSH. En complément, l’application UTM a réellement importé et démarré le profil QCOW2/512, fourni sa console PTY et remis le reçu de rang 2. Après `utmctl stop --force`, l’export relu contient le SHA-256 `748be82d059cb5c698bde7a9ac84390ef27cbb06a9a2006cb302d7f4911fa5d2`. Les autres couples format/secteur ont été exercés sur son moteur. Le Mac de travail est resté en fonctionnement. [Banc de l’application](../../novaOS/tests/publication_utm_app.py)
- **R02, perte d’alimentation.** Non exécuté. L’utilisateur a confirmé l’absence de banc dédié dont l’alimentation puisse être interrompue indépendamment et les images relues après remise sous tension. Le redémarrage de l’hôte et la perte de son alimentation comptent chacun zéro essai.
- **R03, relecture indépendante.** Les deux architectures ont relu en RO les 116 acquisitions du moteur. Le lecteur Python contrôle les champs, les MAC et la chaîne. Un oracle Rust indépendant utilise HKDF/HMAC et les identifiants des deux corps figés au §8. Tous les diagnostics concordent. Les 116 acquisitions donnent sept contenus d’étendue distincts. L’export de l’application a aussi été vérifié sur les deux architectures. Les dommages unitaires injectés après acquisition de chaque reçu proviennent de copies en mémoire. Aucun dommage spontané n’a été observé dans ces images. [Oracle Rust indépendant](../lab/pc1/vecteur.rs)

### Coût observé

Médianes en microsecondes, cinq transactions sans pause par mode et par profil. L’horloge est le compteur architectural AArch64 sous TCG. La durée transactionnelle va de la première OUT à la production du reçu, après vérification, hors inspection initiale et FLUSH d’activation. Le témoin publie le même objet avec un seul sélecteur et deux FLUSH. Il mesure le coût de ce chemin d’E/S sur le même banc. Le temps complet d’un `cp` P9 en production demanderait une mesure distincte.

| Profil | PC1 | Témoin | F1 PC1 | F2 PC1 | FLUSH d’activation PC1 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 512, brut | 4 502 | 1 442 | 80 | 92 | 110 |
| 512, QCOW2 | 3 950 | 1 691 | 95 | 106 | 121 |
| 4 096, brut | 1 679 | 562 | 111 | 125 | 102 |
| 4 096, QCOW2 | 1 614 | 587 | 98 | 116 | 99 |

Chaque publication réussie a achevé **deux FLUSH de transaction**. L’écart au témoin est de zéro FLUSH par transaction. Le passage initial en RW ajoute **un FLUSH par activation**. PC1 ajoute deux pages d’écriture et quatre pages de relecture pour le vecteur à un seul objet. Ainsi le compteur indique 32 OUT contre 16 en 512 et 4 contre 2 en 4 096. Les temps individuels et les étendues min–max sont conservés avec les résultats. [Mesures individuelles](validation/v0.9.13/publication-conditionnelle-utm.tsv)

### Régressions et arbitrage

Lors de la mise au point, la construction instrumentée de l’auxiliaire Rust a révélé un import manquant des sondes de compression. Cet import a été corrigé dans l’auxiliaire avant la validation complète. Sur chaque architecture, 196 tests standards, 197 instrumentés, 2 304 injections d’ancres et 1 000 000 d’opérations différentielles ont réussi. Le journal expérimental antérieur a aussi rejoué 10 056 coupures et le lecteur P9 a revérifié 142 images UTM historiques par architecture. NovaOS a réussi ses 25 démarrages, 24 groupes bloc, trois contrôles ELF/processus/IPC et quatre essais CLI par architecture. La campagne AArch64 a réussi les 11 scénarios DMA/GICv3 et les 23 scénarios de politique DMA séquentielle. [Résultats de qualification](validation/v0.9.13/publication-conditionnelle-utm.json)

Le maintien du refus de remplacement est recommandé. Une qualification de perte d’alimentation demanderait un contrat précisant le domaine réellement persisté jusqu’au SSD, la couverture des métadonnées d’image, la panne qui laisse une copie et ses objets disponibles, puis un R02 sur banc dédié cohérent avec ce contrat. Les observations actuelles permettent de conserver PC1 comme prototype de laboratoire. Toute intégration dans NovaOS ou évolution du format demande un nouvel arbitrage.

### Reproduire la campagne

Depuis la racine du dépôt, définir `BANC` comme un nouveau répertoire local et `HIST` comme le répertoire de l’audit historique contenant `fixtures`, `systeme-512` et `systeme-4096`. Les commandes de préparation de ces fixtures et des régressions figurent dans le rapport d’ancres. Le banc UTM exige macOS, UTM 5.0.8, Rosetta et les cibles Rust Darwin et AArch64 invité. [Préparation du banc historique](novafs-p9-ancres-dechirees.md#rejouer-la-qualification-sur-le-mac-utm)

~~~sh
mkdir "$BANC"
BANC=$(cd "$BANC" && pwd)
HIST=$(cd "$HIST" && pwd)
(cd novaOS && RUSTC="$(rustup which --toolchain stable rustc)" \
    rustup run stable cargo build --release --locked --offline \
    --target aarch64-unknown-none-softfloat --features pc1-lab --target-dir "$BANC/noyau")
for arch in aarch64 x86_64; do
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run build --offline --bin novafs --example pc1-controles
done
/usr/bin/arch -arm64 /usr/bin/python3 -B novaFS/lab/pc1/simulation.py "$BANC/arm" --ancien "$HIST/fixtures"
/usr/bin/arch -x86_64 /usr/bin/python3 -B novaFS/lab/pc1/simulation.py "$BANC/x86" --ancien "$HIST/fixtures"
/usr/bin/arch -arm64 /usr/bin/python3 -B novaFS/lab/pc1/simulation.py "$BANC/arm-vues.json" --echanger "$BANC/x86"
/usr/bin/arch -x86_64 /usr/bin/python3 -B novaFS/lab/pc1/simulation.py "$BANC/x86-vues.json" --echanger "$BANC/arm"
python3 -B novaOS/tests/publication_conditionnelle.py \
    "$BANC/noyau/aarch64-unknown-none-softfloat/release/novaos" "$BANC/moteur" --paralleles 3
python3 -B novaOS/tests/publication_utm_app.py "$BANC/moteur" "$BANC/application"
python3 -B novaOS/tests/publication_compatibilite.py "$HIST" "$BANC/moteur" "$BANC/gpt"
for arch in arm64 x86_64; do
    /usr/bin/arch "-$arch" /usr/bin/python3 -B novaFS/lab/pc1/simulation.py \
        "$BANC/relecture-$arch.json" --croiser "$BANC/moteur/resultats.json"
done
~~~
