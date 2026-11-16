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
