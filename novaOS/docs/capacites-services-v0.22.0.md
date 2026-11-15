# Capacités des services · NovaOS v0.22.0, phase 3

## Décision et périmètre

La phase 2 de journalisation est intégrée dans `main` au commit `7e705d37989bf451b486d1fa6c97e00e4da6e751`. Son arbre `7d5fde69bd88be9dd39942f8628017fbb7685a4d` est celui du candidat `8b0131918c28bfdac536489b03c4dfab9c667a0f`. Les neuf contrôles ABI et cinq essais du journal par architecture ont réussi sur la fusion. Le développement de phase 3 part de cette fusion sur `codex/capacites-services-0220-phase3`. [Configuration persistante](services-configuration-v0.22.0.md) · [Journalisation](journalisation-v0.22.0.md)

Le catalogue embarqué associe chaque image installée à son identité, ses dépendances et son plafond. Pour toute nouvelle incarnation, `C_effectives = C_catalogue ∩ C_config`. À la première utilisation, la configuration autorise le plafond. Tout bit extérieur provoque un refus. Une mutation acquittée prend effet à la création suivante du service, y compris lors d’une relance sans redémarrage du système. Un service encore actif conserve les capacités et la génération de sa propre incarnation.

| Service | Plafond stable | Vérification de disponibilité |
|---|---|---|
| 1 · horloge ; 2 · contrôle des temporaires | `private-tmp` (1) | Le service ouvre sa racine privée. Le service 2 exerce la délégation aux travailleurs. |
| 9 · informations de stockage | `private-tmp` (1) | Le gestionnaire exige une réponse native `NVFS` sur le canal de l’incarnation avant `READY`. |
| 10 · cache | `cache-backend` (2) | Dépend obligatoirement de 9. Le courtier vérifie le rôle et le bit à chaque opération. |
| 16 · journal | `journal-store` (8) | Liaison privée au magasin du journal. Le retrait empêche le raccordement et entraîne un échec supervisé. |
| 11 · acteur du banc cache | `private-tmp` (1) et `cache-client` (4) | Le profil fermé peut retirer le droit facultatif au prochain lancement pour exercer le courtier réel. Cette sonde garde son paramètre en mémoire et le catalogue installé conserve ses cinq services. |

Les processus de bloc, de stockage P9 et le lanceur de `nsh` démarrent indépendamment de ces cinq services. Un service qui échoue en phase de démarrage faute de droit passe à `FAILED`. Un dépendant dont le fournisseur a échoué pour cette cause demeure en `WAITING`, avec son fournisseur bloquant. Une restauration administrée et un nouveau lancement rétablissent le service. [Politique de supervision](../user/services/policy.rs) · [Droits de lancement](../abi/service_caps.rs)

## Administration

```text
service capabilities 9
service capabilities restrict 9 private-tmp
service capabilities 9
service restart 9
service status 10
service capabilities restore 9 PRIVATE_TMP
service start 9
service info 9
```

La consultation affiche le plafond, les droits enregistrés, les droits de l’instance active, les deux générations et le changement en attente pour cette instance. L’absence d’instance est explicite. `service status` conserve les générations de la politique de démarrage automatique appliquée au boot. Les noms en minuscules avec tirets et les noms symboliques comme `JOURNAL_STORE` sont acceptés. `service --help` décrit la syntaxe.

La lecture emprunte le port de consultation. Les mutations exigent le canal privé de maintenance, y compris pour restaurer un droit. Le client présente la génération qu’il vient de lire. Les deux contrôles de génération de NovaFS, à la préparation et à la publication, préviennent une mise à jour perdue. Un succès suit l’acquittement durable. Un résultat `UNCERTAIN` (22) exige une nouvelle lecture sur le port de consultation. Le client ne réexécute aucune mutation automatiquement. Les autres résultats exploitables incluent argument invalide (3), accès refusé (5), configuration corrompue (21) et conflit de génération (24). [Client nsh](../../components/shell/services.rs) · [Contrôle du gestionnaire](../user/services/control.rs)

L’autorité attestée pour les événements de restriction, restauration ou refus est le canal privé de maintenance reçu par le gestionnaire. Les événements contiennent le sujet de service et, selon leur code, la génération et le bit demandé ou le masque appliqué. Les messages de démarrage et d’arrêt lient la tentative à l’identité complète et au canal de l’instance. Les droits des utilisateurs individuels relèvent d’une future autorité. La file bornée du journal comptabilise les pertes pendant son indisponibilité et laisse fonctionner la supervision. [Format du journal](../user/services/journal_wire.rs)

## Persistance et récupération

Le contenu `NVSVCC01` et son enveloppe `NSVCCFG1` restent ceux du magasin de configuration existant. La lecture accepte quatre déclarations historiques avec leurs masques exacts et ajoute les valeurs du catalogue pour le service 16. Une simple bascule d’autostart préexistante d’un des quatre services conserve cet encodage jusqu’à l’enrôlement. La première mutation de capacités, ou de la politique du service 16, inscrit les cinq entrées. Le magasin enrôlé refuse une publication à quatre entrées, même après restauration des droits.

La décision durable se trouve dans un objet privé distinct des deux slots CoW. Un deuxième objet privé marque l’enrôlement et lie la décision au volume FS/LV. La décision contient sa génération et les deux empreintes SHA-256 séparées par domaine, couvrant le volume, la génération, la longueur et le contenu des slots. NovaFS est l’unique écrivain. Les noms internes sont masqués et refusés dans l’espace des clients. Le service publie d’abord le marqueur initial et la décision, puis le slot préparé. Chaque publication passe par les barrières P9 F1/F2. L’acquittement de la nouvelle politique suit la dernière publication.

Une interruption entre décision et slot peut rendre la configuration de sécurité invérifiable. Le gestionnaire inhibe alors le démarrage automatique des services concernés, refuse aussi `service start`, expose l’erreur et conserve le chemin minimal P9. Une référence d’autorité absente ou divergente, un slot corrompu ou une incohérence d’enrôlement empêche le repli vers une génération antérieure. Seule une récupération explicitement historique, avant enrôlement et avec les plafonds exacts, peut encore utiliser la précédente politique d’autostart. [Lecteur et écrivain du magasin](../user/novafs/files/config_store.rs) · [Validation des déclarations](../user/services/settings.rs)

La référence et les slots occupent le même support physique. Un retour cohérent de **tous** les objets, du marqueur et de leurs métadonnées à une ancienne image valide produit des octets indistinguables d’un historique légitime. SHA-256 ne constitue ni une horloge monotone externe ni une signature contre un hôte capable de réécrire le volume entier. Une garantie de fraîcheur face à ce modèle nécessiterait une autorité de confiance située hors du domaine restauré. Un refus après incohérence peut nécessiter une intervention administrative hors ligne. [Limite de récupération sur P9](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

## Application au noyau et compatibilité

La trame existante `SERVICE_PROCESS/START` garde ses 16 mots. Les mots 5 et 6 à zéro conservent le lancement historique au plafond. La version 1 au mot 5 présente le masque demandé au mot 6. Le noyau valide les mots réservés, le profil, la fonction du service et son plafond, puis renvoie le masque attribué au mot 9 pour cette variante. Le gestionnaire vérifie la réponse et conserve le masque et la génération avec le droit d’observation et les 32 octets d’identité. Aucun PID fourni par un client ne peut autoriser un lancement. L’application de la politique persistante repose sur le gestionnaire présent dans l’image système de confiance.

Le noyau attribue les canaux du journal et le bail temporaire selon le masque. Le courtier du cache vérifie le bit, le rôle, la session et la qualité de principal. L’enfant ordinaire reste dépourvu du droit du parent. La délégation explicite de temporaire exige un bail valide dans l’incarnation courante. La destruction retire les dérivations. Ni le registre des 48 sélecteurs de l’ABI native v1 ni les formats NovaFS P8/P9 ne changent. Les sources affichent 0.20.0, le SDK `nova-system` 0.2.0, NovaFS 0.9.13 P9. [Chemin START](../kernel/src/sched/process/ipc/service.rs) · [Courtier de cache](../kernel/src/sched/process/cache.rs)

## Qualification du candidat

Les ELF des noyaux testés sur QEMU 11.1.3 ont les SHA-256 suivants.

| Architecture | SHA-256 du noyau |
|---|---|
| AArch64 | `fa14b1bf933099149c9a0c7b31290ff7bf0adc1cf27124043bc424b682e651a9` |
| x86_64 | `38ad282de5111ed4eb51fb20de851fcff17bc1b75b891cfca48018cd8d35aafb` |

| Banc | Conditions et résultats |
|---|---|
| Restrictions réelles et concurrence | 12 cellules fonctionnelles, AArch64 et x86_64 × 1, 2, 4 vCPU × 512 et 4 096 octets. Six cellules supplémentaires de course. Total de 56 démarrages dans cette matrice. Les 56 ISO ont été relues pour vérifier l’ELF exécuté. |
| Génération 12 → 13 | Douze arrêts forcés de QEMU après acquittement du retrait de `private-tmp` à 9. Après relance, 9 échoue sous sa politique restreinte et 10 attend 9. Huit démarrages sur copies corrompues ont exercé la perte d’intégrité du slot ou de l’autorité dans les deux architectures et géométries. Lancement automatique inhibé, `service start 9` refusé, diagnostic explicite et lecture ordinaire de P9 réussie. |
| Autorités et générations concurrentes | Ports lecture et exploitation refusés pour la mutation. Sur chacun des six profils SMP, un seul CAS sur deux aboutit. La restauration est admise, son canal est révoqué avant l’acquittement et la génération 14 est relue sans réémission. |
| Volume personnalisé à quatre déclarations | 2 cellules, 4 démarrages. Sur ARM64/2/4096 et x86_64/4/512, les services 9 et 10 ont un budget de relance personnalisé. La première mutation retire `JOURNAL_STORE` de 16. Seul 16 redémarre. 9 et 10 répondent `NVFS` et `CACH` avec leurs identités d’origine. Les quatre déclarations conservent leurs 552 octets. Après restauration et redémarrage, l’événement durable est relu une fois sur son BootID d’origine. |
| Volumes archivés de la phase 2 | Deux images de la livraison phase 2 ont été identifiées dans ses archives par SHA-256, copiées puis démarrées sur les noyaux de phase 3. Sur AArch64/512 et x86_64/4096 à deux vCPU, la première consultation conserve les deux enregistrements historiques de 568 octets. Le retrait de 16 puis sa restauration publient les générations 3 et 4 à 704 octets chacune. Les deux images originales ont été relues et leurs empreintes sont intactes. |
| Délégation, admission du noyau et droit facultatif | Sur les deux architectures, le service 2 privé du bail teste un enfant ordinaire sans héritage et le refus natif de `TMP_WORKER`. Le service 10 privé de `cache-backend` échoue au démarrage. L’acteur de qualification 11 exerce les vrais appels de cache et garde son temporaire après retrait de `cache-client`, sur 6 cellules. Le noyau distingue un doublon de lancement, un masque hors plafond et une version ou des mots réservés invalides. Une autorité de l’ancienne incarnation reste révoquée après restauration. |
| P9 avec périphérique simulé | Rust Darwin ARM64 natif et x86_64 sous Rosetta, secteurs 512/4096. 168 injections ciblées de coupure ou d’erreur de FLUSH sur l’enrôlement et le changement suivant. Les quatre objets ont été corrompus ou supprimés individuellement. Le banc a présenté une ancienne génération et une substitution à génération identique avec CRC recalculé. Toute décision incohérente bloque la lecture de sécurité, le repli et la mutation. |
| Régressions | 9 contrôles ABI et 48 sélecteurs. Par architecture, 5 tests de déclarations, 13 de politique, 4 de transport et 1 de publication du journal. Quatre démarrages de régression fonctionnelle du journal par architecture. Les 16 démarrages historiques supplémentaires couvrent la perte du gestionnaire, les trois corruptions de configuration antérieure à l’enrôlement et les quatre sessions P8. La lecture seule P8 conserve la génération 43 et la modification confirmée est retrouvée en génération 44. |

Le bilan recense **96 démarrages QEMU qualifiés** sur les deux noyaux indiqués, dont 56 pour la matrice principale et ses courses. Les essais exploratoires et la première version de l’oracle de séquences sont conservés séparément et exclus de ce total. [Détail des cellules, pics mesurés et SHA-256 des rapports](validation/capacites-services-v0.22.0.json)

Les compteurs ci-dessous proviennent du noyau invité. Les pics incluent l’ensemble du système pendant le scénario.

| Scénario | AArch64, cadres de 4 Kio | x86_64, cadres de 4 Kio |
|---|---:|---:|
| Restriction puis restauration, session de restauration | 1 038–1 043 | 1 044–1 051 |
| Configuration de sécurité invérifiable | 642 | 648 |
| Course CAS et perte d’acquittement | 873–877 | 871–882 |
| Acteur de qualification, 1/2/4 vCPU | 1 160 / 1 156 / 1 156 | 1 164 / 1 153 / 1 153 |

La table des processus réserve 19 360 octets, soit 256 de plus pour les 32 masques de 64 bits. Le courtier cache atteint 1 696 octets et revient à zéro dans les six essais. Après drainage, les compteurs de processus, objets IPC, pages utilisateur, piles et allocations du tas retrouvent leurs valeurs initiales. Les tables de pages et cadres DMA conservés sont comptabilisés séparément. Ces observations portent sur les scénarios exercés. La consommation de macOS et le maximum réellement occupé dans la pile de chaque ELF n’ont pas été mesurés.

## Reproduction

Utiliser Rust stable et les répertoires de travail durables prescrits par `AGENTS.md`. Définir `NOVA_KERNEL_ELF` sur le noyau de l’architecture testée. Donner au banc un répertoire de preuves neuf, situé hors des temporaires. Pour créer l’oracle hôte et exercer une cellule depuis la racine du dépôt sur macOS ARM64, avec `PREUVES` défini sur ce répertoire :

```sh
make -C novaOS ARCH=aarch64 kernel
novaFS/lab/run build --offline
export NOVA_KERNEL_ELF="$PWD/novaOS/target/aarch64-unknown-none-softfloat/release/novaos"
lib="$PWD/novaFS/target/aarch64-apple-darwin/release"
rustup run stable rustc --edition=2024 --target aarch64-apple-darwin -C opt-level=2 \
    -L "dependency=$lib/deps" --extern "novafs=$lib/libnovafs.rlib" \
    novaOS/tests/services_configuration_fixture.rs -o "$PREUVES/oracle"
python3 -B novaOS/tests/services_capacites.py aarch64 2 4096 "$PREUVES/arm-2-4096" "$PREUVES/oracle" --corruptions
python3 -B novaOS/tests/services_capacites_journal.py aarch64 2 4096 "$PREUVES/arm-journal" "$PREUVES/oracle"
python3 -B novaOS/tests/services_capacites_delegation.py aarch64 4096 "$PREUVES/arm-delegation"
sh novaOS/tests/services_config_stockage.sh aarch64
python3 -B novaOS/tests/abi_registre.py
```

Pour x86_64, construire avec `ARCH=x86_64`, sélectionner `novaOS/target/x86_64-unknown-none/release/novaos` et passer `x86_64` aux bancs. Le même oracle Darwin ARM64 relit les deux types d’images invitées. Un arrêt forcé de QEMU coupe la VM sur un hôte alimenté. Les erreurs de FLUSH du banc hôte proviennent d’un périphérique simulé. Aucune campagne de coupure électrique physique, de restauration d’un volume hostile complet ni de distribution UTM n’a été exécutée pour cette phase.

Le premier banc dédié au service 16 comparait le numéro de séquence de la vue privée avec celui de la vue globale. Ces deux compteurs ont chacun leur espace. Le banc corrigé capture les deux vues avant l’arrêt et compare l’événement dans la même vue après redémarrage. Les traces de la première tentative figurent également dans les archives.

## Conservation des preuves et suite

Les résultats détaillés se trouvent dans `OS-worktrees/preuves-capacites-services-0220-phase3/`. Les 642 fichiers ont une copie distincte sous `OS-worktrees/archives-capacites-services-0220-phase3/`. Chaque original et chaque membre décompressé de l’archive ont été relus et comparés au manifeste. Le conteneur `preuves-capacites-services-0220-phase3.tar.gz` mesure 312 210 907 octets et porte le SHA-256 `2452e903452c90b1237d190e7090e4ae378f9c405c6a55ab88d575498889d2f9`. Le SHA-256 du manifeste `SHA256SUMS` est `f03c1ac884ca72eabccf96056a593185cf277a0e78e34e7f55986617ddbbe035`. Les deux exemplaires sont sur le même Mac.

La qualification couvre l’administration des services par les canaux existants sur les deux architectures. Une revue MOA/MOE peut désormais statuer sur l’intégration de cette phase avec le modèle de confiance documenté. La restauration intégrale du support avec protection externe contre le rejeu et la recette du futur serveur stable restent ouvertes.
