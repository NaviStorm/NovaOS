# Admission du lecteur et retrait du producteur

**16 novembre 2026.** Branche `codex/lancement-generique-elf`, code `6efe536e5ad16260b7e430e2cdb3b6fcb5d1b39c`. Le banc exerce le lanceur interne, deux processus utilisateur et la porte publique PROCESS_WAIT sous QEMU 11.1.3. L’exécution de CUT depuis le shell, le service émetteur de tickets et l’installation à chaud ont une recette distincte. [Recette de bout en bout](lancement-generique-utilitaires-recette.md)

## Décision et durée de vie

Au démarrage du producteur le noyau attache une extrémité SEND, enregistre dans le parent la génération du contrôleur et le PID complet du nouvel enfant, et remet son droit WAIT. La référence de retrait reste dans le noyau. Le constructeur accepte un producteur sans droit de lancement de descendants. La publication du lecteur consomme la référence sous la garde de l’ordonnanceur. Un refus local définitif ferme le pipe et réveille les attentes. Si le producteur identifié est encore vivant, le noyau retire l’instance et ses droits. Le timer permet de quitter le calcul en espace utilisateur même sans nouvel appel système. Le décès du parent rend impossible toute admission ultérieure du lecteur et provoque le nettoyage.

La course est tranchée sous la même garde que la sortie normale. Le résultat interne distingue une sortie déjà acquise d’un retrait forcé. Le format de PROCESS_WAIT v1 conserve son couple historique type et code. Une sortie volontaire et un retrait peuvent tous deux porter le nombre 125. Le banc vérifie séparément l’origine. La restitution de cette provenance au shell générique par le service d’admission reste à raccorder. Le constructeur du lecteur ne peut recevoir que des erreurs d’allocation ou de construction locales. Une réponse de transport perdue exige une consultation de l’issue de publication.

## Mesures invitées

Un tick vaut 4 ms (250 Hz). Pour le refus d’un ELF tronqué, l’intervalle commence juste avant l’admission du lecteur et se termine après la collecte par le parent, y compris son deuxième WAIT de contrôle. Le processus témoin de même image est resté actif dans les six exécutions.

| QEMU | vCPU | Refus ELF jusqu’à collecte | Cas de course, ordre observé |
|---|---:|---:|---|
| AArch64 | 1 | 3 ticks | retrait |
| AArch64 | 2 | 2 ticks | retrait |
| AArch64 | 4 | 2 ticks | retrait |
| x86_64 | 1 | 3 ticks | retrait |
| x86_64 | 2 | 2 ticks | retrait |
| x86_64 | 4 | 2 ticks | retrait |

Les deux ordres sont également imposés séparément. Le banc recueille une seule fois le statut de sortie, remplit la file de huit trames, bloque le neuvième envoi, retire le parent en attente de lecteur et présente une décision tardive après recyclage du slot. Il contrôle les slots de processus, les pages IPC et l’audit du tas avant et après. Un IPC_REVOKE isolé laisse le producteur en calcul. Le seuil de test est de 250 ticks. Les mesures portent sur des vCPU disponibles, avec la charge du banc. La récupération physique d’un tmp P9 en présence d’une transaction tierce et la durée sur matériel physique restent à qualifier.

## Régressions

- Les deux architectures ont passé `elf-test`, `ipc-test`, `ipc-races`, `lifecycle-test` et `nsh-scripts-test`. Les scripts ont parcouru 31 commandes par géométrie, avec redémarrage.
- Les deux pipes historiques ont passé 40 observations pour `cat | wc -l` et 41 commandes pour `cat | head -n 1`, pour chacune des quatre combinaisons architecture/P9-512 ou P9-4096. Le premier passage P9/512 du banc wc a précédé les derniers ajustements des sondes. Les ISO propres à chaque exécution sont conservées.
- Neuf contrôles du registre ABI réussis. Les 48 sélecteurs et les sources du format NovaFS P9 sont inchangés dans ce commit.
- Le banc ELF utilise aussi une image temporaire, efface la source avant exécution et vérifie la restitution après pénurie de cadres. L’image installée possède sa table d’en-têtes dans un cadre privé du noyau.

## Reproduction et conservation

Depuis le worktree, construire le noyau avec `make -C novaOS ARCH=aarch64 kernel`. Exécuter `python3 -B novaOS/tests/admission_pipe.py aarch64 2 DOSSIER_NEUF`. Remplacer l’architecture ou le nombre de vCPU pour les autres cellules. Les sorties définitives sont dans `retrait-v3`. Les passages intermédiaires sont conservés sous `retrait-v1` et `retrait-v2`.

Le [manifeste SHA-256 des 178 fichiers](validation/lancement-generique-retrait.sha256) est relatif à `OS-worktrees/preuves-lancement-generique-elf/`. La seconde copie durable est `OS-worktrees/archives-lancement-generique-elf/retrait-6efe536.tar.gz`, 116 212 759 octets, SHA-256 `f078e254ce5f6252e6eb2b522e6bcf747299dcf574cde3942526229de4f85a26`. Chaque membre décompressé a été relu et comparé au fichier d’origine.

## Qualification avec deux instances du même ELF scellé

Le commit `c1561cc6cc9a6bfd0740317ec70a5bfbf99a8964` utilise le même ELF autonome pour le producteur et pour le processus indépendant. Tous deux calculent en espace utilisateur. Le parent peut constater que le compteur du témoin avance après la collecte du producteur. La requête est identifiée dans la table du parent par la génération du contrôleur. Le noyau conserve la décision jusqu’à acquittement explicite. Une consultation après perte de réponse retrouve le même PID et le droit WAIT originel, puis le PID seul si WAIT a déjà été consommé. Le refus garde le statut du lecteur et l’indicateur de retrait du producteur. Les reçus sont exercés au niveau du lanceur interne.

Le premier passage ELF a laissé cinq cadres de 4 Kio dans le cache du tas sur chaque architecture. Les blocs vivants et les octets occupés retrouvaient leur valeur initiale, tandis que le tas mappé passait de 12 à 32 Kio. La copie de préparation passe désormais par des pages privées du superviseur, rendues après chargement ou abandon. La fenêtre virtuelle de 8 Mio accepte des images jusqu’à 4 Mio. Seules les pages demandées occupent des cadres. Une absence de plage contiguë ou de cadres rend une erreur de ressources. Les deux invités ont exercé la saturation, la réutilisation après libération et l’échec partiel avec 0, 1, 4 et 7 cadres disponibles. L’assertion de restitution compare directement les compteurs avant/après, cache compris.

Les mesures suivantes proviennent des six ISO du dernier passage, dans `producteur-elf-corrige/final-*`. Un tick invité vaut 4 ms. La mesure comprend la collecte par PROCESS_WAIT.

| Architecture | vCPU | ELF du lecteur refusé | Sortie naturelle déjà acquise | Course sortie/retrait | Contrôleur révoqué | Lecteur admis |
|---|---:|---:|---:|---:|---:|---:|
| AArch64 | 1 | 3 | 2 | 3 | 7 | 10 |
| AArch64 | 2 | 2 | 0 | 0 | 2 | 1 |
| AArch64 | 4 | 1 | 0 | 0 | 2 | 2 |
| x86_64 | 1 | 4 | 2 | 3 | 7 | 10 |
| x86_64 | 2 | 2 | 0 | 0 | 2 | 0 |
| x86_64 | 4 | 2 | 0 | 0 | 2 | 1 |

Un zéro signifie que le compteur de ticks n’a pas changé entre les deux lectures. Les seuils portent sur un invité qui reçoit du temps CPU. La collecte de l’ELF en calcul, le réveil du neuvième envoi sur le pipe plein, le décès du parent et le recyclage de slot ont laissé zéro processus et zéro page IPC dans les six invités. Une sortie volontaire avec le code 125 porte l’indicateur « forcé » à faux. Le refus définitif avec le producteur en vie donne le même code et l’indicateur vrai.

Les quatre anciens ELF de v0.20.0 ont été exécutés sous QEMU après vérification de leurs empreintes. Les neuf contrôles du registre ABI v1 passent. Les 48 sélecteurs et les fichiers du moteur NovaFS sont identiques à la base `425e4e7cc5c03e246efb64d3638376906344bf47`. Les régressions IPC, courses IPC, instances, cycle de vie, scripts et les deux pipes historiques ont un passage complet par configuration applicable. Les régressions des deux pipes exécutent respectivement 40 et 41 contrôles pour chaque couple architecture/géométrie P9.

Deux passages de `pipe_head.py` sur AArch64 ont échoué au contrôle du texte série. Sur P9/4096, une trace du bloc coupe le marqueur `[pipe head=0]` alors que les deux processus ont été collectés avec le statut 0. Sur P9/512, la trace du bloc s’intercale au milieu de l’invite `nsh> ` après retour de la commande. Les traces et les passages complets suivants sont conservés. L’entrelacement du diagnostic avec la console reste une réserve.

Le [manifeste complémentaire](validation/lancement-generique-scellement.sha256) couvre 399 pièces. L’archive durable `OS-worktrees/archives-lancement-generique-elf/scellement-retrait-c1561cc.tar.gz` mesure 236 187 242 octets, SHA-256 `5e36268aeed309d6cbce7b004e069d910a5dede405f7093706d0850ec4d64527`. Chaque membre décompressé a été relu. Les journaux des échecs, les ISO, les noyaux et les ELF archivés permettent de retrouver les conditions de chaque passage.

## Limites du raccord public

Le prochain raccord doit conserver la table d’autorité de la session et le `tmp` privé de l’instance. Le retrait physique dans P9 peut être différé lorsqu’une autre écriture détient déjà la transaction de stockage. Le délai mesuré ci-dessus couvre le processus, ses canaux et la mémoire du noyau. Le choix d’une garantie de délai pour les blocs P9 sous contention reste soumis à arbitrage.

La porte SPAWN historique rejette un appel émis par un thread secondaire. La consommation concurrente du ticket public doit être qualifiée avec la règle d’autorité retenue pour ce raccord. Le constructeur interne, le reçu et le chargeur scellé sont disponibles à ce stade. L’exécution des quatre formes CUT, l’ouverture du TSV par le PID du producteur et l’installation à chaud du second ELF via le service public restent à démontrer.

## Régressions du shell et des fichiers

Les bancs `nsh.py` et `cli_p9.py` ont terminé leurs huit parcours sur les deux architectures et les deux tailles de secteur. Le premier exerce 113 commandes dans chaque configuration. Le second compare les opérations et les dates avec un oracle hôte, puis contrôle le volume après remontage. Le [manifeste des 68 pièces](validation/lancement-generique-cli.sha256) comprend les images jetables, les journaux, les exécutables des oracles et les résultats individuels. La copie durable `OS-worktrees/archives-lancement-generique-elf/cli-c1561cc.tar.gz` mesure 1 226 214 octets. Son SHA-256 est `88d143248c1dbbe5e3945bb296abd39a99021bb4a4a1e3b8b94a70152661ab02`. Tous les fichiers décompressés ont été comparés aux originaux.
