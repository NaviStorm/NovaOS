# NovaAuth v0.24.0 — Qualification globale du candidat aa489ca

Bilan du 21 novembre 2026. **24 scénarios manquants réussis sur les quatre profils QEMU ; huit scénarios précédemment réussis restent valides. Verdict : apte à soumettre à une fusion contrôlée des sources qualifiées QEMU, avec la réserve mémoire instrumentée AArch64 et la limite de reproductibilité inter-contextes conservées.** Le présent mandat ne fusionne rien et ne clôture pas NovaAuth v0.24.0.

## Références et identité du candidat

- Candidat testé : `aa489ca4eb61dfa05749c36191389559419c0934`.
- Arbre : `557b8f2d748b15a96d51d87c2417d4adb03b1f2f`.
- Branche : `codex/novaauth-autorite-0240`.
- `origin/main` relevé avant la campagne : `a3efdbb70d652f2687e7bbf31e27872fbe05c3a1` ; aucune intégration dans cette branche.
- Binaires exacts conservés de l'export EXEC `eb4c4cc089d9abd9f3ec45b4c121858b0b461c85`. L'export de `aa489ca` contient les mêmes **625 fichiers Rust** octet par octet. Les changements intervenus concernent le banc hôte et les documents. Les six empreintes ISO/ELF sont dans `identite-artefacts.json`, l'identité de l'export dans `equivalence-export-aa489ca.json`.

Aucune source fonctionnelle ni attente de test n'est modifiée dans ce mandat. Le commit portant ce rapport ne change que les documents. Le worktree principal, ses modifications locales, les branches parallèles, les anciennes preuves et le candidat de fusion local `b08ba80` sont conservés. Aucun tag ni activation ordinaire.

## Matrice exécutée

La [matrice détaillée](validation/novaauth-qualification-globale-aa489ca.json) associe chaque résultat à sa commande, son fichier de preuve et son SHA-256. Les oracles natifs existants sont conservés ; chaque scénario indépendant utilise des copies P9 neuves et sa propre autorité de laboratoire. Les deux VM d'un parcours partagent une seule autorité indépendante, jamais deux copies de son état. QEMU 11.1.3 ; paramètres et images initiales figurent dans les fichiers `*-commande.json` et les journaux.

| Scénario | AArch64/P9-512 | AArch64/P9-4096 | x86_64/P9-512 | x86_64/P9-4096 |
|---|---|---|---|---|
| Ancienne VM vivante, ticket non consommé | Réussi | Réussi | Réussi | Réussi |
| Deux consommations réellement concurrentes | Réussi | Réussi | Réussi | Réussi |
| Réponse terminale perdue | Réussi | Réussi | Réussi | Réussi |
| Course : admission durable avant retrait | Réussi | Réussi | Réussi | Réussi |
| Course : retrait avant admission | Réussi | Réussi | Réussi | Réussi |
| Autorité injoignable par A avant admission | Réussi | Réussi | Réussi | Réussi |
| Réponse d'admission perdue | Acquis réutilisé | Acquis réutilisé | Acquis réutilisé | Acquis réutilisé |
| Admission acquittée, création retardée | Acquis réutilisé | Acquis réutilisé | Acquis réutilisé | Acquis réutilisé |

Les 24 nouveaux parcours réussissent à leur première tentative. Les huit acquis ont les mêmes binaires invités et le même banc fonctionnel ; leurs empreintes ont été relues. Ils ne sont pas comptés comme de nouvelles exécutions. La couverture consolidée est **32/32 scénarios à deux VM**, et non une qualification générale de toutes les fonctionnalités de NovaAuth ou de la distribution.

## Révocation, concurrence et opérations incertaines

Dans le scénario nominal, A reste vivante et suspendue avec son ticket valide non consommé. B est refusée pendant `RETRAIT_EN_COURS`. Après drainage durable, `RETIREE` précède l'activation de B. A réveillée reçoit `REVOKED=10`, avec le compteur monotone de refus strictement antérieur à l'échéance du ticket et aucun enfant. Ces valeurs sont des **ticks noyau** ; les chronologies `NXAUTH` sont séparément en millisecondes. Le refus ne repose pas sur l'expiration. B lance ensuite son propre ELF avec le statut 0.

Les deux threads du témoin présentent le même ticket et reçoivent exactement un succès 0 et un refus `REFUSED=12`. Un seul enfant et un seul droit de collecte sont observés. Chacun conserve les trois refus des anciennes formes `SPAWN` depuis un thread secondaire. Les rejeux par B et après redémarrage de A retournent `DENIED=5`, sans enfant ; ce dernier refus dans une nouvelle table locale est distinct de celui d'une ancienne incarnation encore vivante.

Les courses conservent l'ordre durable admission/retrait/issue. Une admission antérieure doit avoir un enfant identifié et collecté, ou une annulation irréversible attestée, avant `RETIREE`. Les pertes de réponse sont rapprochées par identifiant ; aucune réponse manquante ni durée écoulée ne vaut annulation. Les fichiers `ordre_durable`, `A_achevee_avant_RETIREE`, `annulation_A` et `operations_RPC_A` donnent les issues réellement observées. Aucune annulation n'est déduite d'un simple timeout. Dans les quatre courses « retrait premier », les événements observés sont `RETRAIT_EN_COURS → cancel-before-admission → cancelled → RETIREE → activation B`, avec enfant nul. Les admissions antérieures observées aboutissent à un enfant identifié ; ces passages ne sont pas présentés comme une démonstration d’annulation après une admission déjà acquittée.

L'autorité indisponible provoque un refus avant expiration et sans admission ni enfant. Le shell ne transforme pas cette panne en permission : la commande suivante de consultation est elle-même refusée ; le résultat de sécurité est le statut natif, sans inventer un `$?` non observable. Une seule autorité durable est conservée : le transport de A est coupé, tandis que B peut encore joindre cette même autorité. Ce témoin exerce une indisponibilité pour A, pas une extinction simultanée de l’autorité pour tous ses clients.

## Anti-rollback, VFS et récupération 2-sur-2

Les quatre recettes VFS antérieures restent réutilisées : Alice lit effectivement le témoin avant révocation ; la génération révoquée est confirmée ; tous les disques locaux sont restaurés ; le VFS invité refuse l'ancien droit. Après approbation et récupération 2-sur-2, une nouvelle authentification permet la lecture du dossier toujours autorisé, refuse le dossier révoqué sans octet privé, puis permet une nouvelle opération autorisée. Les contenus, statuts et décisions invités sont conservés dans les preuves référencées, pas remplacés par des contrôles hôtes.

Leur profil VFS ne passe pas par l'export EXEC dont la cadence et les traces ont changé ; le diff depuis leur candidat est archivé dans `reutilisation-verifiee.json`. Les 24 parcours EXEC vérifient leur préparation avec récupération 2-sur-2 et l'autorité commune, mais ne sont pas présentés comme 24 nouvelles recettes VFS complètes.

Les quatre campagnes de dix connexions, avec défi expiré puis neuf authentifications explicites fraîches, restent acquises sur les mêmes binaires. Le défi de 1,5 s, le bail de 60 s, le délai TLS de 10 s, le quota de 16 traces et les quatre racines IPC restent inchangés. Les 106 tests hôtes précédemment réussis ne sont pas réexécutés sans modification pertinente.

## Ressources et compatibilité

Chaque nouveau parcours conserve les relevés avant, pendant et après : racines, handles, endpoints, nœuds, attentes, processus, threads, droits de collecte, cadres IPC et cadres libres. `nsh` revient à sa référence après usage ; registre et signataire restent dans la limite de quatre racines. Les pics par acteur et le refus de la cinquième racine figurent dans les traces. Après redémarrage refusé, la preuve de restitution est la trace noyau de famille drainée, et non un message TLS attendu d'un signataire déjà retiré.

Les sondes natives relèvent les valeurs suivantes sur les 24 parcours (maxima échantillonnés, pas profilage continu). Les 48 restitutions de référence de A et B sont vérifiées. `pages` et `pile` restent les compteurs natifs séparés ; ils ne sont pas assimilés à toute la mémoire physique du service.

| Acteur | Pic racines | Pic handles | Pages AArch64 / x86_64 | Pile, maximum | Threads, maximum |
|---|---:|---:|---|---:|---:|
| nsh | 3 | 11 | 83 / 88 | 72 | 3 |
| Registre, service 22 | 4 | 8 | 125 / 132 | 64 | 1 |
| Signataire, service 23 | 4 | 6 | 57 / 65 | 64 | 1 |

`ressources-consolidees.json` conserve aussi tous les relevés globaux avant/pendant/après ; la matrice versionnée reprend ces mesures. Les compteurs globaux ne sont pas additionnés comme s’ils appartenaient à chaque acteur.

Le pool IPC peut conserver un cadre au premier usage, conformément à l'oracle historique. Cela n'est pas présenté comme une restitution physique. Les six cycles mémoire prolongés déjà qualifiés sont réutilisés : plateau des deuxième et troisième lancements, puis trois cycles supplémentaires ; les 32 cadres retenus par l'allocateur NovaFS restent réutilisables, sans augmentation de budget. Aucune croissance prolongée nouvelle n'est prétendue mesurée ici.

Le premier contrôle ABI a été arrêté après huit réussites et une erreur de compilation des en-têtes C++. Le `clang++` LLVM Homebrew 23.1.6 sélectionnait ses en-têtes libc++ hôtes pour les cibles bare-metal. Une sonde ne contenant aucun en-tête NovaOS reproduit les mêmes erreurs sur les deux cibles ; Apple Clang 21 compile cette même sonde. Le seul environnement de la suite ABI est ensuite fixé sur `/usr/bin/clang` et `/usr/bin/clang++`, avec le même Python et les neuf assertions inchangées. L’échec, les versions et les quatre sondes minimales restent dans `abi-export-aa489ca.log` et `diagnostic-abi-compilateur.json`. Aucun essai QEMU n’était engagé à cet arrêt. La configuration globale du Mac n’est pas modifiée.

Les **neuf contrôles ABI** réussissent sur l'export réellement testé. Les **48 sélecteurs ABI v1**, leurs contrats historiques et NovaFS P9 sont conservés. Les quatre applications ELF v0.20.0 ci-dessous sont exécutées sans recompilation de l'application, avec les oracles historiques et les noyaux QEMU construits depuis l'export courant :

| ELF historique | SHA-256 de l'application | Résultat |
|---|---|---|
| aarch64 / sdk-example | `db2d14ec8a1f8d1ac22acc331ee4d3d90336a90adc8d50ca057c3129e129c3a8` | Réussi |
| aarch64 / threads-test | `fa1640df7589a2700f172acc43197c2904e6e2f833fb748048bc15984575cbd9` | Réussi |
| x86_64 / sdk-example | `1ad5ebf8b6a0d80b10d2df826500f39052fdc222fac418640b408b9657d78c6e` | Réussi |
| x86_64 / threads-test | `6073a78fb2ffa5c716e75ac942932728faa218d614c3dedf6f30c9f067f6b30c` | Réussi |

Le premier noyau ordinaire AArch64 avait une empreinte complète différente de la référence historique (`71bd543a58ab995622c95beb2e5bcd5c92f3c7b65b523207aca62a5fa33c30ef` au lieu de `1caedffc2e9507c41e055c9c1242ddfd2a3cace9d73f149268c686e3f43bbf19`). L’analyse conserve les fichiers intacts : les quatre segments chargés, leurs adresses et leur contenu sont identiques ; seules les tables non allouées de 50 symboles locaux portent des suffixes LLVM numériques différents. Ce premier écart et l’arrêt du contrôle restent archivés.

Pour établir une comparaison stricte, la base `a3efdbb` et le candidat `aa489ca` sont reconstruits successivement avec les mêmes outils et dans les mêmes chemins, en déplaçant les copies de construction précédentes sans les supprimer. Les **empreintes complètes base/candidat sont identiques** pour chacune des deux architectures :

- AArch64 : `f32262b56dda238a58b7988fd0a508db742778cb89125bb582c81795f9a2e054`.
- x86_64 : `5620d088036a628ed22d358b0ec9fbeeb59747c93dcdce2e48d8f9fa0a9e1105`.

La reconstruction de la base historique dans ce nouveau contexte diffère également de son ancien fichier, notamment dans `.rodata` qui contient les ELF incorporés. La comparaison appariée base/candidat est, elle, intégralement identique. Les comparaisons `ordinaire-comparaison-*.json` conservent les échecs de rapprochement aux anciens fichiers ; elles ne les transforment pas en égalités. Le contrôle démontre la préservation du noyau ordinaire entre les deux révisions dans des conditions identiques, pas la reproduction de tous les anciens octets depuis un autre contexte. Aucun binaire n’est normalisé, dépouillé de ses symboles ou modifié pour rendre les empreintes égales. La reproductibilité bit à bit hors d’un contexte de construction identique n’est pas revendiquée ; la cause précise de ces variations inter-contextes reste non attribuée et n’est pas présentée comme corrigée. L’avertissement hôte `rust-objcopy` / `libLLVM.dylib` est également conservé : il figure déjà dans les journaux historiques. Ce contrôle n’est pas une nouvelle qualification UTM ou matérielle.

## Verrou commun et durées

Le verrou commun est `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/.qualification-utilitaires-exclusive.lock`. Chaque groupe l'acquiert par création atomique, vérifie l'absence de QEMU/hyperfine incompatible, puis libère uniquement son propre jeton dans `finally`. Aucun verrou tiers n'est supprimé ou forcé. Campagnes dont les créneaux occupés ont été respectés : **BASE32, BASE64, COMM, EXPAND, FOLD, JOIN, NL, OD, PASTE, UNEXPAND**. Les propriétaires et leurs transitions sont archivés ; aucune qualification NovaAuth n'a été lancée pendant leurs créneaux.

Toutes les heures ci-dessous sont UTC (heure de Paris : UTC+1 le 21 novembre 2026).

| Groupe | Acquisition UTC | Libération UTC | Attente, secondes |
|---|---|---|---|
| nominal | 2026-11-21T16:02:06.032625+00:00 | 2026-11-21T16:04:37.161125+00:00 | 100.04 |
| concurrent | 2026-11-21T16:37:37.849309+00:00 | 2026-11-21T16:40:39.601280+00:00 | 1951.07 |
| perte-issue | 2026-11-21T16:54:59.940047+00:00 | 2026-11-21T16:58:03.335694+00:00 | 860.30 |
| course-admission | Non acquis | Attente reprise : 2026-11-21T17:38:50.341059+00:00 | 2446.96 |
| course-admission | Non acquis | Attente reprise : 2026-11-21T18:48:43.363699+00:00 | 4151.31 |
| course-admission | Non acquis | Attente reprise : 2026-11-21T19:02:58.881142+00:00 | 813.62 |
| course-admission | 2026-11-21T19:17:21.465875+00:00 | 2026-11-21T19:20:19.237496+00:00 | 862.53 |
| course-retrait | 2026-11-21T19:20:23.735131+00:00 | 2026-11-21T19:22:59.603683+00:00 | 4.47 |
| indisponible | 2026-11-21T19:40:44.257105+00:00 | 2026-11-21T19:43:14.793682+00:00 | 1064.62 |
| nonregression | 2026-11-21T20:48:51.434311+00:00 | 2026-11-21T20:48:53.976715+00:00 | 3923.78 |
| nonregression | 2026-11-21T20:53:21.404356+00:00 | 2026-11-21T20:53:37.405506+00:00 | 146.69 |
| nonregression | 2026-11-21T21:23:25.352620+00:00 | 2026-11-21T21:23:42.137547+00:00 | 1497.00 |
| nonregression | 2026-11-21T21:27:03.738770+00:00 | 2026-11-21T21:29:39.620628+00:00 | 0.00 |

- Attente cumulée du verrou : **17822.39 s**, intégralement exclue du budget d'essais.
- Exécution mesurée conservatrice : **1191.30 s**, sous le budget de 5 400 s (90 min). Ce total mesure chaque processus de banc de son lancement à sa fin, inclut les trois arrêts contrôlés et les constructions des noyaux témoins ; les compilations ordinaires représentent 70.90 s de ce total.
- Diagnostic hors banc : **696.87 s** pour le consolidateur, le coordinateur, la sélection du compilateur et l’analyse des métadonnées ELF hôtes. Le consolidateur incluait un contrat statique sans chronologie ; son filtrage est corrigé. L’attente périodique du coordinateur manquait les fenêtres de libération face aux coordinateurs événementiels ; elle utilise désormais la même notification kqueue que le pilote commun BASE32. La relecture du budget avant chaque tentative de mkdir (médiane mesurée 0,287 ms) est déplacée avant l’attente ; le contrôle frais avant chaque essai reste obligatoire. La part exacte de ces asymétries dans l’attente totale n’est pas quantifiée. Aucun test, statut natif ni oracle n'a changé.
- Reprise du coordinateur inactif, hors banc : **41.69 s** ; aucun essai interrompu.
- Préparation, lecture documentaire et archivage sont des opérations hors banc, distinctes des diagnostics de défaut. Le diagnostic du consolidateur a eu lieu pendant une attente de verrou : ces deux durées se chevauchent et ne doivent pas être additionnées pour reconstruire la durée murale totale.

Les attentes longues ont été signalées sans déclencher de campagne concurrente. Les trois premières attentes du groupe course-admission ont été reprises au point sûr : propriétaire externe confirmé, aucun enfant et aucune acquisition. Leurs interruptions volontaires ne constituent ni une tentative QEMU ni un échec de qualification ; toute cette attente reste comptabilisée. L’observation périodique de dix secondes, puis d’une seconde, est remplacée par la notification commune de libération ; aucune priorité, aucun forçage ni modification du verrou commun. Aucune attente n'est comptée comme échec ou succès de qualification. La matrice contient uniquement des scénarios réellement exécutés ou explicitement réutilisés.

## Preuves et réserves

- Archive privée : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/qualification-globale-auth-aa489ca-privee.tar.gz` ; 319 fichiers, 320 membres décompressés et vérifiés.
  SHA-256 archive : `1c5e0d784cdbd49ac40afc3388efc5918cea34cfa4c10aade4cd592873b954cf`. SHA-256 manifeste : `ae51b0d1c7ffb02420aac69efd8e5ac1f7b57f30cb81a6fcfc3b475bbf1f2078`.
- Archive des preuves : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/qualification-globale-auth-aa489ca-preuves.tar.gz` ; 1231 fichiers, 1232 membres décompressés et vérifiés.
  SHA-256 archive : `f0bd9571e0b961f3965070bf8f3c769520dc2600ecf06cbf4b0b9cd90569996b`. SHA-256 manifeste : `52589a1c783488d93c34191ffab8c7e7071930724d57d3ed125acf35e0e64387`.

Le [reçu des archives](validation/novaauth-qualification-globale-archives.json) indique les tailles et vérifications. Chaque membre a été décompressé, relu et comparé à l'original. Les pilotes de coordination, commandes, versions, états d'autorité, traces natives et comparaisons sont archivés. Les secrets fictifs et binaires exacts restent dans l'archive privée locale, protégée ; aucune clé n'est committée ou publiée. Les 673 et 1 774 preuves antérieures sont revérifiées ; les 1 800 preuves initiales restent préservées dans leur archive historique.

La réserve instrumentée AArch64 de **301 cadres reste ouverte**, avec plafond de **2 048 cadres inchangé**. Les six cycles NovaFS ne lèvent pas cette réserve distincte. Le coût CPU du polling plus fréquent et l'absence d'expiration sous charge arbitraire ne sont pas démontrés. Les résultats finis ne constituent pas une preuve universelle d'absence d'intermittence. La vérification des en-têtes C/C++ porte sur Apple Clang 21 ; la configuration libc++ bare-metal de LLVM Homebrew qui a échoué n’est pas déclarée qualifiée.

La protection anti-rollback est qualifiée dans les profils QEMU testés avec l'autorité indépendante et les deux détenteurs conservés hors du périmètre restauré. Ni l'autorité, ni les gardiens, ni le signataire de laboratoire ne sont déclarés solutions de production. UTM et matériel physique ne sont pas testés ici ; l'image distribuée UTM reste v0.17.0.

**Verdict : apte à fusionner les sources qualifiées QEMU, sous nouvel arbitrage MOA/MOE et maintien explicite des réserves.** Une éventuelle évolution de `main` devra être contrôlée et l'arbre réellement intégré qualifié selon ses différences. Aucune fusion, aucun tag et aucune clôture de NovaAuth v0.24.0 n'ont lieu dans cette livraison.
