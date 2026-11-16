# NovaAuth v0.24.0 — Politiques des applications et révocation par origine

## Périmètre exercé

La branche de qualification part de `8bb64c6365c2b7e7aaf9a833a368d1440eac1930`. Le candidat fonctionnel est `8b9b506d3762f6b22ba81a1f3b876ca17857d1d5`. L’export ajoute le raccord aux seuls binaires de laboratoire. Le lanceur, deux signataires Ed25519, le vérificateur, deux instances de l’application et le VFS utilisent des canaux privés dans l’invité. Le provisionneur hors ligne travaille sur deux fichiers jetables distincts, le volume NovaFS P9 et l’autorité de décision.

### Qui possède quel droit ?

| Objet | Identité et durée | Vérification |
| --- | --- | --- |
| Politique persistante | Le titulaire stable, l’installation et l’objet du volume identifient la règle. Elle distingue l’origine directe du groupe désigné par son ID stable. | Le titulaire authentifié peut lire, accorder ou retirer la règle pendant l’arrêt de l’application. La transaction conserve les trois bases du registre existant sous une même décision. |
| Installation | Le provisionneur lie au propriétaire un ID dérivé du domaine, de la génération et de la demande. L’enregistrement contient le SHA-256 de l’ELF. | Le lanceur privé consulte l’autorité. Le noyau mesure le même tableau d’octets immuable qu’il passe au chargeur et transmet la mesure au VFS. Une mise à jour explicitement autorisée change la révision. La suppression conserve une tombe. |
| Capacité effective | Le VFS lie le handle à l’extrémité IPC de l’instance et à la session du titulaire. La racine conserve les révisions de la politique, de l’image et, selon l’origine, de l’admission au groupe. | À chaque opération, l’effecteur vérifie la session attestée, l’autorité du registre, l’ascendance du handle et les restrictions du dossier. Un ancien numéro présenté sur un autre canal échoue. |
| Délégation de session | Les descendants appartiennent à la session qui a obtenu leur racine. | La fermeture retire cette lignée. Un retrait de groupe retire les lignées issues du groupe. Dans les deux cas l’effecteur décide du sort de l’écriture avant son acquittement de retrait. |

Le contenu du fichier de politique attaché au dossier est lié par SHA-256 à la tête du registre. Le VFS vérifie le volume, les objets, le masque de droits et la branche protégée. L’application peut utiliser une installation commune aux deux utilisateurs. Son nom, le nom du compte et le chemin du programme ne participent à aucune décision d’autorisation. Le banc charge les ELF depuis le catalogue embarqué. Sur les quatre volumes, l’oracle a ouvert une copie de l’ELF, l’a retrouvée après renommage avec le même ID et le même condensat. Un démarrage par chemin depuis un installateur réel reste à exercer.

### Scénarios et décisions observables

- **Application arrêtée et deux utilisateurs.** Alice consulte une règle vide, accorde la lecture, la retire, puis la rétablit avant le premier lancement. Le même ELF démarre deux fois. Alice lit D. Bob obtient un refus pour ses deux demandes de racine et lorsqu’il présente le numéro réel du handle d’Alice. La lecture du descendant surprotégé est refusée.
- **Deux origines sur D.** Alice dispose de la lecture directe et de l’écriture via G. Le retrait est effectué avec une écriture en attente sur le petit-enfant du handle de groupe. L’effecteur réveille son client et refuse ensuite l’écriture sur la racine, l’enfant et le petit-enfant. Le handle de lecture directe détenu avant le retrait reste utilisable. Une réadmission au groupe reçoit une nouvelle révision (test du registre).
- **Persistance avec authentification effective.** À chaque démarrage, le vérificateur demande une nouvelle signature aux deux clients isolés. Leurs clés de laboratoire, conservées entre démarrages, sont distinctes. Après le retrait, Alice lit réellement D et ne peut obtenir d’écriture de G. L’oracle hôte ouvre P9 en lecture seule, compare le domaine et l’état autorisé, et retrouve exactement huit octets correspondant à l’unique écriture 35.
- **Fermeture pendant l’écriture.** Si le VFS a accepté une préparation mais ne l’a pas exécutée (36), la fermeture réveille le client avec `CANCELLED`. Si l’écriture est durable et que sa réponse se perd (35), le banc ferme la session alors que le client attend, constate `UNCERTAIN` et retrouve l’effet une seule fois après une nouvelle authentification. Le client ne renvoie aucune mutation. La session de Bob reste utilisable.
- **Installation.** Sur deux copies indépendantes, le provisionneur autorise une mise à jour et réinstalle le même programme. Dans le premier cas l’ancien condensat est refusé, le nouvel ELF s’exécute et lit D. Dans le second, l’ancien ID échoue et le nouvel ID ne donne aucun accès à D. Un remplacement non attesté ou un ID inconnu est refusé. Les signataires et le provisionneur sont réservés à ce banc.
- **Interruption et restauration.** L’hôte tue QEMU au point de décision, après préparation des trois objets, ou immédiatement après la décision durable et avant son acquittement. Au remontage indépendant, la première copie désigne la génération 5 et ne connaît pas la requête 06. La seconde désigne la génération 6, restitue la décision 06 et expose la lecture accordée. La restauration du P9 de génération 5 en conservant l’autorité à 11 refuse l’activation avant authentification. Restaurer ensemble le volume et son autorité logicielle peut réactiver une décision ancienne.
- **Concurrence sur la politique.** Deux demandes utilisent le même état initial. Après publication de la première, l’ancienne autorisation de la seconde produit un conflit et son identifiant reste sans décision. L’oracle exerce cet entrelacement déterministe à l’interface du registre pendant la préparation des deux fixtures d’installation. Deux threads hôtes remettent également leurs demandes au même courtier à écrivain unique, depuis la même génération, avec une barrière de départ et un ordre d’arrivée libre. Un seul masque de droits est publié. Ce test passe sur ARM64 et x86_64 pour les deux tailles de secteur. Le service sérialise les mutations. L’essai SMP du VFS porte sur les processus distincts et l’écriture bloquée.

## Matrice des onze recettes

| N° | Recette | Preuve et limites |
| ---: | --- | --- |
| 1 | Sessions et facteur | Profil historique, trois sessions et deux titulaires. Rejeu, expiration et signature incorrecte refusés. Le profil des politiques ajoute une authentification valide après redémarrage. |
| 2 | Délégation temporaire de D | Attribution à chaud dans le profil antérieur. Contrôle du dossier réel, du masque et de la branche protégée. |
| 3 | Isolation des titulaires et des sessions | Ancien profil pour la seconde session d’Alice. Nouveau profil pour Bob utilisant la même installation et présentant le handle d’Alice. |
| 4 | Fermeture avec écriture en attente | Parcours dédié avec 35 et 36. Refus des anciens handles après reconnexion et maintien de la session de Bob. |
| 5 | Politique de l’application arrêtée et lancement | Consultation, attribution, retrait et lancement vérifiés. Attribution à chaud exercée séparément dans le profil historique. |
| 6 | Révocation pendant l’exécution | L’instance reste active. Après acquittement du retrait de G, chaque nouvelle écriture concernée est refusée et la lecture directe reste utilisable. |
| 7 | Descendants et lignée indépendante | Racine, enfant et petit-enfant vérifiés dans le VFS. Le transfert entre applications indépendantes est différé selon l’arbitrage MOE. |
| 8 | Issue d’une opération engagée | Annulation certaine de 36 et 37. Issue indéterminée de 35 pour le client, puis relecture. Coupures de la publication de politique à deux frontières. |
| 9 | Groupe puis désactivation | Retrait de G sur l’instance active et après démarrage dans le nouveau profil. Désactivation de compte en session dans le profil historique. Réadmission et exclusion de l’installation désactivée dans le registre hôte. |
| 10 | Suppression et réutilisation d’un nom | Facteur et ancienne session refusés dans le profil historique. Le registre refuse l’installation et les droits du compte supprimé lors de la réutilisation du nom. Intégration générale aux fichiers privés et aux caches en attente. |
| 11 | Mandat après déconnexion | Témoin de registre pour le mandat sans délégation de session, positif puis négatif. SSH, planificateur et processus crontab réels restent à intégrer. |

[Contrat des sessions et du facteur](novaauth-v0.24.0-phase3.md) · [Qualification précédente du dossier](novaauth-dossier-v0.24.0-phase3.md)

## Mesures et non-régression

QEMU 11.1.3, deux vCPU, un parcours final par cellule. Les secondes hôte vont du lancement de QEMU à sa terminaison et incluent les délais de perte de réponse injectés. La durée du retrait dans l’invité va de la demande signée à son acquittement et comprend la décision persistante et le retrait par l’effecteur. Elle est mesurée avec l’horloge en millisecondes du gestionnaire. Ces observations individuelles ne constituent pas des percentiles.

| Invité | Secteur | Parcours réussis | Attribution + fermetures, s hôte | Retrait, s hôte | Redémarrage avec réauthentification, s hôte | Retrait acquitté, ms invité |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| AArch64 | 512 | 8/8 | 102,398 | 55,898 | 39,137 | 2 580 |
| AArch64 | 4 096 | 8/8 | 80,192 | 39,239 | 30,300 | 1 344 |
| x86_64 | 512 | 8/8 | 83,061 | 43,094 | 35,046 | 2 368 |
| x86_64 | 4 096 | 8/8 | 69,798 | 29,133 | 22,591 | 1 548 |

Le noyau rapporte au lancement, en pages privées de 4 Kio, **19 / 13 / 2** sur AArch64 et **21 / 15 / 3** sur x86_64 pour le vérificateur, chaque signataire et chaque instance de l’application. Il rapporte séparément 64 pages de pile par processus. Dans les trois parcours avec deux instances, les cinq processus enfants sont collectés et le VFS compte zéro handle, zéro écriture préparée et zéro client en attente après fermeture. Les limites de l’effecteur sont de douze handles et une préparation. Le pic résident et le bilan de chaque cadre physique ne sont pas mesurés.

Les neuf contrôles du registre ABI réussissent pour 48 sélecteurs. Les sources de l’ABI et de NovaFS sont identiques à la base. Les deux noyaux ordinaires reconstruits sont identiques octet par octet à leurs témoins archivés. Les sept images du profil de politiques examinées pour chaque architecture sont présentes dans le noyau de qualification et absentes du noyau ordinaire. Pour les services ordinaires, le cycle de vie des processus, les IPC et les deux pipes, 27 résultats archivés ont été relus avec vérification SHA-256 de la copie locale et du membre décompressé. L’intermittence historique du comparateur de pipe x86_64/4096 reste documentée dans la campagne précédente.

Le profil historique des sessions et du dossier a été recompilé et rejoué sur les quatre combinaisons, avec trois reprises par combinaison. Les oracles historiques relisent les deux écritures attendues et la tombe de l’ancien compte. Sur x86_64/512, deux parcours nominaux successifs ont échoué après le rejet attendu d’une signature altérée. Le processus de stockage a terminé avec le code 5. Le troisième parcours du même ISO a réussi en 69,279 s, puis ses trois reprises ont réussi. Les essais comparatifs avec l’ISO de référence et avec une trace ajoutée au lanceur réussissent également. La cause des deux sorties 5 reste ouverte. Le [diagnostic et le rejeu sur le correctif](novaauth-stabilisation-stockage-v0.24.0-phase3.md) donnent les résultats postérieurs à cette campagne. Une révision intermédiaire du profil de politiques a aussi rencontré un refus cryptographique sur x86_64/512 lors de campagnes concurrentes. Son rejeu et les huit parcours du candidat final sur cette cellule ont réussi. Aucun de ces refus n’a été compté comme réussite.

Les clés Ed25519 logicielles, le provisionneur et l’autorité indépendante sur fichier sont des dispositifs de laboratoire. Restent à qualifier la garde matérielle du facteur, la restauration conjointe des deux supports, l’exécution sur matériel physique et la surprotection cryptographique complète ARC-017. Les essais de coupure arrêtent brutalement le processus QEMU.

## Preuves

L’archive locale privée `OS-worktrees/archives-novaauth-politiques-0240/qualification-8b9b506.tar.gz` contient 746 fichiers et leur manifeste. Chaque membre a été décompressé et comparé à l’original. L’extrait publiable comprend 166 membres, dont les journaux du candidat, les mesures, les résultats des essais intermédiaires et le manifeste. [Empreintes SHA-256](validation/novaauth-politiques-phase3-0240.json) · [Extrait de qualification](validation/novaauth-politiques-phase3-0240.tar.gz)

## Reproduire

1. Exporter la révision avec `novaOS/user/novaauth/service/profile.py --politiques DESTINATION` dans le répertoire temporaire du projet. Fournir deux secrets Ed25519 de laboratoire indépendants de 32 octets via les chemins `NOVA_LAB_ALICE` et `NOVA_LAB_BOB`. Ces fichiers restent privés. Fournir leurs clés publiques hexadécimales au provisionneur.
2. Dans l’export, construire le noyau avec `cargo build --offline --locked --release -p novaos --features novaauth-sessions-lab --target CIBLE` puis l’ISO du profil `novaauth-sessions`. Utiliser `aarch64-unknown-none-softfloat` ou `x86_64-unknown-none`. Construire l’oracle hôte `policy_volume` avec la feature `politiques-lab`.
3. Exécuter `policy_qemu.py ARCH SECTEUR ISO APP_ELF ORACLE GENESE CLE_PUBLIQUE_ALICE CLE_PUBLIQUE_BOB SORTIE`. L’ELF de mise à jour se trouve à côté de celui de l’application. Le répertoire de genèse contient les fichiers vierges de phase 2. Chaque parcours dispose de sa propre copie et d’une sortie neuve.
4. Les résultats individuels, journaux série et oracles figurent dans l’extrait publié, avec le manifeste de l’archive complète. Les binaires de qualification contenant les facteurs privés de laboratoire restent dans l’archive locale à accès restreint.
