# NovaOS v0.24.0 — Expiration et réauthentification de la session ELF sous QEMU

Le correctif qualifié porte le SHA `d4be5a38dad01d7b7a91d8480aeb0bd55bdfe881` sur `codex/lancement-generique-elf`. Le même shell, resté actif, a relancé le même ELF après une nouvelle authentification explicite sur les quatre profils. Chaque ancien droit présenté est resté inutilisable. Le bail est de 60 000 ms. La décision de clôture du chantier ELF appartient à la MOA/MOE.

## Origine des deux refus

La base `730bb79c0f58a6f88d75d400a6a54ad384fb3419` avait réussi 51 groupes sur 52 lors de sa première matrice. Sur x86_64/P9-512, l'installation avait abouti et le pipe fonctionnait. Au dernier lancement par nom, le vérificateur refusait l'admission à `NSHEXEND`. Les horodatages hôte de ce premier essai ne donnaient pas l'horloge de la décision NovaAuth. [Qualification initiale](raccord-vfs-limine-qemu-0240.md)

La sonde `582ec65428f99408698ee76a6a50af692bd776c2` horodate l'examen du bail par le vérificateur et la fin de traitement. Dans le témoin instrumenté de l'ancienne séquence, le lancement a réussi trois fois sur trois. Une fois, la vérification suivante, à la sortie du shell, est arrivée après l'échéance. Le témoin sur l'ISO d'origine et le témoin sans la trace de racines ont également lancé l'ELF. Sur la même ISO instrumentée, deux retards contrôlés ont reproduit le refus de `NSHEXEND`, respectivement 624 ms et 920 ms après l'échéance. La décision a rendu 5 et le shell 126. Sous un bail valide, le même paquet, le même nom et les mêmes droits ont abouti. La charge et le coût de l'instrumentation peuvent déplacer la décision de part et d'autre de l'échéance. La contribution de chacun au tout premier incident reste inconnue.

Le refus de se réauthentifier avait une cause distincte. Sur **quatre profils sur quatre**, la table de sessions avait purgé son entrée expirée. Le vérificateur de la route privée conservait néanmoins son identifiant dans `Login.live`. L'ancienne fermeture s'arrêtait sur l'échec de `Sessions::close`, avant l'effacement de `live`. L'ouverture suivante voyait la route occupée. Dans le témoin x86_64/512, l'admission avait réussi à 31 584 ms puis échoué à 96 512 ms pour une échéance à 63 784 ms. La fermeture a rendu 5 et la réouverture 12. Ce défaut a été constaté dans le raccord de qualification du vrai `nsh`. Le vérificateur de cette route n'est pas installé dans le profil ordinaire.

## Transitions et garanties

| Événement | État et effet |
| --- | --- |
| Session ouverte | `live` désigne une seule instance sur la route privée du lanceur. Le relais vérifie son échéance et ses délégations. |
| Échéance, révocation ou fermeture de l'identité active | La route passe en retrait, interdit une nouvelle admission et déconnecte la table. Le retrait du relais ferme les délégations associées. Le vérificateur abandonne l'admission et les préparations volatiles de stockage. Une erreur de nettoyage garde le retrait en attente. |
| Retrait confirmé | Le dernier identifiant clos est mémorisé pour acquitter un doublon. Une fermeture tardive de cet identifiant préserve la nouvelle session. Un autre identifiant étranger est refusé. Une réponse de fermeture perdue laisse le retrait acquis. |
| `session reauthentifier` | Action explicite, isolée au prompt du shell de qualification. Un nouveau défi signé ouvre un identifiant distinct et un nouveau bail sur la même instance attestée. L'admission ordinaire et la continuation de transfert ne prolongent jamais un bail. |
| Mutation antérieure `UNCERTAIN` | L'identifiant de mutation permet une lecture explicite du reçu durable. Aucune mutation en attente n'est relancée par l'ouverture de session. |

Une première révision intermédiaire a mis au jour un second défaut du traitement d'un doublon. Deux demandes de fermeture pouvaient être en vol avant que le client ne lise la première réponse. L'expiration du rendez-vous de cette réponse faisait quitter le vérificateur. La version qualifiée traite la perte de cet acquittement après le retrait et peut servir la seconde demande. Les essais intermédiaires et leurs traces sont conservés. Les essais à forte charge ont aussi exercé le délai distinct de 1 500 ms du défi signé. [Correctif du vérificateur et du shell](../user/novaauth/service/session-expiration.patch) · [Témoin de révocation des droits](../../components/shell/bail_client_lab.rs)

## Décisions dans l'invité

Temps monotones de l'invité en millisecondes. La création correspond à l'échéance moins 60 000 ms. La confirmation d'ouverture intervient après la liaison du relais. Pour chaque ligne, le premier appel est accepté à `NSHEXEND`, le suivant refusé à `NSHEXBG1` avant création de ticket, racine ou processus, et le dernier accepté à `NSHEXEND` sur le nouveau bail.

| QEMU / secteur P9 | Création initiale | Acceptation initiale | Échéance initiale | Refus | Nouvelle création | Nouvelle acceptation | Nouvelle échéance |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| AArch64 / 512 | 3 156 | 28 668 | 63 156 | 103 644 | 106 800 | 138 280 | 166 800 |
| AArch64 / 4 096 | 1 792 | 22 036 | 61 792 | 92 628 | 94 488 | 121 160 | 154 488 |
| x86_64 / 512 | 4 024 | 32 444 | 64 024 | 110 276 | 113 540 | 146 464 | 173 540 |
| x86_64 / 4 096 | 2 924 | 27 340 | 62 924 | 101 984 | 104 672 | 134 092 | 164 672 |

Dans la session initiale de chaque invité, `temoin` est d'abord absent (127). Son installation à chaud réussit (0), puis son lancement par nom rend 0, avec 12 octets sur stdout, `41 09 42 09 43 0a 44 09 45 09 46 0a`, et zéro octet sur stderr. Après expiration, le même nom rend 126. Sous le nouveau bail, le même exécutable reproduit les 12 octets et le statut 0. La comparaison des empreintes du noyau, de `nsh`, des services et du signataire avant et après donne une identité exacte. Les PID des acteurs étaient les mêmes pendant le parcours.

Sur x86_64/512, `nsh` est le PID 5, le vérificateur le PID 6 et le courtier le PID 8. Les ouvertures portent les tags de requête 1 et 273. Les deux témoins acceptés ont les PID 16 et 18. L'ancien handle de lecture a rendu 10 (`REVOKED`), l'ancien ticket non consommé 10 et l'écriture sur l'ancien flux 5 (`DENIED`). Le véritable enfant PID 17, bloqué en lecture, s'est réveillé, a terminé avec 10 et a été collecté sans faute. Les deux fermetures tardives de l'ancienne session ont laissé le nouveau bail utilisable. Le comparateur invité contrôle l'identité complète de l'instance et l'inégalité des identifiants de session. Les tags et PID publiés servent à retrouver ces événements dans la trace.

Dans les quatre essais de perte de réponse, une installation réellement publiée a rendu 22 (`UNCERTAIN`). La nouvelle session a pu interroger sur demande `install --receipt` avec l'identifiant mémorisé. Le journal compte un seul début et une seule décision de publication. Dans les quatre recettes de course qualifiées, le retrait traverse la vraie échéance du noyau, deux fermetures sont acquittées, la commande ordinaire suivante reste refusée, puis un nouveau défi explicite permet de continuer. Lorsqu'un défi a expiré sous charge, son refus figure aussi dans le registre des essais.

## Durées et ressources

Durées hôte d'un parcours instrumenté par profil, en millisecondes. La préparation va du départ du banc au premier prompt. L'authentification va de l'envoi de la première commande à la réception de la confirmation d'ouverture. L'installation inclut le transport du paquet signé et le retour au prompt. La résolution et l'admission vont de l'envoi du nom à la réception de la décision. L'intervalle suivant se termine à l'observation de la sortie du processus. Ces durées de bout en bout incluent l'ordonnancement hôte et l'émission sur le port série.

| QEMU / P9 | Préparation | Authentification | Installation | Résolution et admission | Décision à sortie du processus | Sortie à retour au prompt |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| AArch64 / 512 | 5 982 | 4 836 | 18 909 | 7 873 | 22 | 1 194 |
| AArch64 / 4 096 | 5 985 | 3 330 | 17 842 | 6 599 | 24 | 950 |
| x86_64 / 512 | 2 312 | 4 791 | 15 039 | 8 252 | 23 | 1 183 |
| x86_64 / 4 096 | 2 367 | 3 348 | 14 174 | 6 259 | 24 | 978 |

| QEMU / P9 | Cadres libres, admission → drainage | Cadres retenus, admission → drainage | Pages IPC | Processus | Cadres ELF en préparation au drainage | Racines IPC au dernier retrait |
| --- | --- | --- | --- | --- | ---: | ---: |
| AArch64 / 512 | 60 956 → 60 943 | 2 852 → 2 865 | 18 → 16 | 5 → 5 | 0 | 0 |
| AArch64 / 4 096 | 60 960 → 60 959 | 2 848 → 2 849 | 18 → 16 | 5 → 5 | 0 | 0 |
| x86_64 / 512 | 61 468 → 61 471 | 2 462 → 2 459 | 18 → 16 | 5 → 5 | 0 | 0 |
| x86_64 / 4 096 | 61 472 → 61 471 | 2 458 → 2 459 | 18 → 16 | 5 → 5 | 0 | 0 |

Les compteurs mémoire sont en pages de 4 Kio. Le noyau vérifie à chaque échantillon que tous les cadres utilisés ont une attribution. Le drainage est échantillonné alors que cinq processus existent encore. Le journal confirme ensuite la fin du gestionnaire, le retrait des temporaires, la fermeture des canaux et l'arrêt normal du stockage, du pilote et des deux CPU. À la même étape des deux lancements sur chaque profil, avant la création de la racine de sortie du shell, les tables réelles comptaient **41 endpoints et 82 nœuds IPC dans le noyau, et 8 handles IPC dans `nsh`**, avant comme après le retrait du descendant et la réauthentification. Chaque acteur observé rend sa dernière racine et son pic reste au plus égal à quatre. Dans le témoin de cinquième racine, `nsh` a reçu 6 (`LIMIT`). Le triplet est resté identique avant et après la demande refusée, à 43 endpoints, 84 nœuds et 10 handles locaux dans les quatre profils. Les nombres de cadres libres aux deux instants diffèrent avec la charge des services vivants. La campagne ne mesure pas un compteur de cadres global après l'arrêt du dernier service. Les tests ELF historiques de threads ont retrouvé exactement leurs cadres de départ. Les mesures individuelles par PID, les entrées de bilan RAM et les SHA des traces figurent dans l'annexe. [Mesures et résultats individuels](validation/bail-elf-0240.json)

## Couverture et essais défavorables

| Série et profil | Groupes réussis |
| --- | ---: |
| Sous charge, AArch64 / 512 | 13/13 |
| Sous charge, AArch64 / 4 096 | 13/13 |
| Sous charge, x86_64 / 512 | 51/52 |
| Sous charge, x86_64 / 4 096 | 13/13 |
| Référence, x86_64 / 512, série 1 | 13/13 |
| Référence, x86_64 / 512, série 2 | 13/13 |
| Référence, x86_64 / 512, série 3 | 13/13 |
| Référence, x86_64 / 512, série 4 | 13/13 |

Sous charge, six QEMU étaient actifs lors du relevé sur un Mac16,5 à 16 CPU logiques. L’ouverture du premier bail a échoué dans la quatrième répétition du groupe de sélection x86_64/512. Le défi signé était expiré, avec une échéance de bail encore à zéro. La première course de fermeture x86_64/512 a bien acquitté les deux retraits à 64 460 ms pour une échéance à 64 424 ms. Le défi de réauthentification a ensuite expiré. Le témoin répété seul a réussi. Les seuils de 1 500 ms et 60 000 ms ont été conservés. La campagne de référence a commencé sans autre invité QEMU au relevé, puis avec deux invités simultanés au maximum pour les 52 groupes.

La révision de travail intermédiaire a produit 18/30 essais réussis parmi ceux achevés. La campagne intermédiaire a été interrompue après identification du défaut de double fermeture. L’archive garde les essais achevés et les premiers diagnostics infructueux, dont l’erreur d’horodatage du témoin de création du bail et le contrôle ABI tenté sur un export sans historique Git. Le contrôle ABI a ensuite été exécuté avec l’historique en lecture seule.

Le bilan retenu pour la réserve ELF est **52/52 sur x86_64/512** sur la série de référence, et **13/13** sur chacune des trois autres configurations. Les quatre parcours d’admission, les quatre pertes de réponse et les quatre courses possèdent chacun une trace de réussite.

Les treize groupes couvrent la sélection VFS et ses refus, les invocations de `cut`, les pipes, les auteurs des ouvertures, les arguments, les redirections combinées avec un pipe, les droits, les flux binaires, la syntaxe, les tickets, les redirections vers des cibles neuves et l'installation à chaud. Sur x86_64/512, 52 exécutions correspondent à quatre séries indépendantes de treize groupes. Chaque essai démarre avec une copie neuve de sa genèse P9 et de son autorité indépendante. Les parcours du même démarrage conservent les mêmes supports. La série chaude distingue la recherche infructueuse, l'installation et le lancement par nom sous le bail initial, puis les opérations par chemin et en pipe après réauthentification explicite.

Les pipes historiques produisent `32 0a` pour le fichier de deux lignes avec `cat=0`, `wc=0` et `$?=0`. La première ligne de `cat | head -n 1` est `41 09 42 09 43 0a`. Les statuts sont 0/0 quand toute l'écriture est acquittée et 11/0 quand la fermeture interrompt le producteur. Les recettes vérifient les PID distincts, le statut de pipeline, le flux brut et le traitement de stderr. Le groupe de sélection exerce une requête NovaAuth pendant l'admission du lecteur. [Recettes ELF et pipes](../user/novaauth/service/flux_qemu.py)

Les cinq régressions Rust de possession, de session et d’admission ont aussi réussi sur Darwin ARM64 natif et sur Darwin x86_64 sous Rosetta. Elles couvrent en particulier la substitution de route, le rejeu, la concurrence et la fermeture. Les deux exécutions du registre ont chacune réussi **9/9** contrôles pour **48 sélecteurs ABI v1**. Les quatre ELF v0.20.0 archivés, deux par architecture, ont été contrôlés par SHA-256 et exécutés sans recompilation. Les noyaux des profils historiques ont été reconstruits pour ces essais. Les noyaux ordinaires reconstruits avant et après sont identiques octet par octet sur les deux architectures. Les arbres `novaOS/kernel`, `novaOS/abi` et `novaFS` sont identiques à la référence du mandat. Le passage de relais Limine qualifié est conservé. Le raccord, la commande explicite du shell et les sondes sont cantonnés aux exports QEMU. [Vérificateur du bail](../user/novaauth/service/bail_qemu.py) · [Course de fermeture](../user/novaauth/service/bail_course_qemu.py)

### Reproduction des trois témoins de session

Exporter le commit fonctionnel avec `python3 novaOS/user/novaauth/service/profile.py --bail DESTINATION` dans le répertoire temporaire du projet. Construire l’ISO de qualification avec les mêmes binaires de laboratoire, la trace des racines, des acteurs et des flux. Les SHA de chaque ISO et des composants extraits figurent dans l’annexe. Le banc prend une copie neuve des deux fichiers de la genèse et conserve l'image pendant tout le parcours.

Depuis la racine du worktree, utiliser les scripts de `novaOS/user/novaauth/service/` ci-dessous. `ARCH` vaut `aarch64` ou `x86_64`, `SECTEUR` vaut `512` ou `4096`. Chaque `SORTIE` désigne un dossier neuf distinct. La matrice archivée spécifie la genèse et le paquet exacts de chacun des treize groupes.

```sh
python3 "$SERVICE/bail_qemu.py" "$ARCH" "$SECTEUR" "$ISO" "$GENESE" "$PAQUET" "$SORTIE"
python3 "$SERVICE/bail_qemu.py" --perte-reponse "$ARCH" "$SECTEUR" "$ISO" "$GENESE" "$PAQUET" "$SORTIE_INCERTAIN"
python3 "$SERVICE/bail_course_qemu.py" "$ARCH" "$SECTEUR" "$ISO" "$GENESE" "$SORTIE_COURSE"
```

## Preuves et limites

Les **1 596 fichiers** inventoriés et leur manifeste sont archivés dans `OS-worktrees/archives-lancement-generique-elf/bail-novaauth-d4be5a3.tar.gz`. Les 1 597 membres ont été décompressés et comparés individuellement, par SHA-256, aux fichiers relus dans `OS-worktrees/preuves-lancement-generique-elf/bail-novaauth-n13nye5a`. Le SHA-256 de l’archive est `1e5fe7eece79aa6ac9d433e00e8430b4823719ecb1d523817d6b8621b496bd65`. Celui du manifeste est `78c735321f6cd25b62b8d1c0a27c6e38abac920e48e1b84c72fa964976a6137b`.

Les preuves précédentes du raccord Limine comprennent 1 478 fichiers. Leurs copies ont été relues et vérifiées par SHA-256. Les nouveaux journaux comprennent la trace de toutes les créations et destructions de racines IPC activée sur x86_64/P9-512, les décisions du vérificateur, les traces de sortie et les volumes par essai. La trace des racines ne contient pas tous les octets de tous les messages IPC. Les ISO de laboratoire et les images privées restent dans la copie locale. Le dossier de validation public contient les résultats, les empreintes individuelles et les limites d'interprétation. [Manifeste SHA-256 de cette campagne](validation/bail-elf-0240.sha256)

Le périmètre qualifié couvre QEMU TCG 11.1.3, deux vCPU, deux architectures et les secteurs P9 de 512 ou 4 096 octets. Il reste à qualifier la connexion de production avec un canal et une garde de facteur approuvés, l'autorité durable contre le rollback intégral et les intégrations avec les consommateurs ordinaires de NovaAuth. L'installation, la distribution signée, le courtier et le signataire utilisés ici appartiennent au laboratoire. Les essais UTM, matériel physique, perte d'alimentation réelle et les facteurs de production demeurent ouverts. Les capacités persistantes d'application, les droits effectifs d'instance et les délégations de session demeurent des portées distinctes pour le prochain lot d'administration. [Découpage et conditions de qualification NovaAuth](novaauth-recette-v0.24.0.md) · [Politiques déjà exercées en laboratoire](novaauth-politiques-v0.24.0-phase3.md)
