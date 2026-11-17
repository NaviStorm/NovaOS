# NovaOS v0.24.0 — Phase A — Diagnostic des pipes historiques

La base instrumentée est `752d2597689f7ce0a46a1f8a7a7502519c4f9cd0`. Le banc `flux_qemu.py historiques` a exécuté les deux commandes dans chaque shell authentifié, sur quatre copies indépendantes de P9 et de l’autorité de laboratoire. Le retour global du banc vaut 1 pour chaque profil. Le retour du pipeline dans le shell vaut 0 pour chacune des huit commandes.

| Invité | Secteur | wc -l pour deux LF | head -n 1 | LIMIT / demandes de cinquième racine |
|---|---:|---|---|---:|
| AArch64 | 512 | 0 | première ligne absente | 2/2 |
| AArch64 | 4096 | 0 | première ligne absente | 2/2 |
| x86_64 | 512 | 0 | première ligne absente | 2/2 |
| x86_64 | 4096 | 0 | première ligne absente | 2/2 |

| Acteur | Avant le pipe | Pic dans le pipe défaillant | Après restitution, shell ouvert |
|---|---:|---:|---:|
| nsh | 2 | 4 | 2 |
| NovaAuth | 4 | 4 | 4 |
| Courtier | 4 | 4 | 4 |
| NovaFS | 4 | 4 | 4 |
| Producteur | processus absent | 0 | 0 |
| Lecteur | processus absent | 0 | 0 |

Le contrôleur du pipe porte le shell à trois racines, le premier canal VFS à quatre. Le second canal VFS reçoit `LIMIT=6`. À l’entrée et au retour de cette création, le noyau compte 43 endpoints, 84 nœuds de capacités et 10 entrées locales pour nsh. Le nettoyage retire les deux racines transitoires. Le producteur et le lecteur ont des PID distincts. Le statut individuel de chaque enfant et la disponibilité de NovaAuth pendant la seconde admission restent à mesurer sur le correctif. Le temps d’exécution de ces images instrumentées n’est pas une mesure de performance.

Le chemin historique dérive deux handles de la table du parent avant de publier l’enfant. Les canaux VFS du lanceur générique peuvent déjà appartenir à l’enfant. Leur raccordement dépend d’une attestation et d’un protocole de démarrage différents. [Sélection et délégation](../../components/shell/capabilities.rs) · [Admission et dérivation](../kernel/src/sched/process/ipc/spawn.rs) · [Droits directionnels du lanceur générique](../kernel/src/sched/process/ipc/session_display_lab.rs)

## Budget proposé pour le raccord privé de qualification

| Phase du pipe historique | nsh | NovaAuth | Courtier | NovaFS | Producteur | Lecteur |
|---|---:|---:|---:|---:|---:|---:|
| Session authentifiée | 2 | 4 | 4 | 4 | — | — |
| Contrôle ouvert, sélection en attente | 3 | 4 | 4 | 4 | — | — |
| Première puis seconde racine de fichier | 3 | 4 | 4 | 4 | 1 puis 2 | — |
| Admission du lecteur et transfert | 3 | 4 | 4 | 4 | 0 à 2 selon sa durée de vie | 0 |
| Échec, retrait ou fin et fermeture du contrôleur | 2 | 4 | 4 | 4 | 0 | 0 |

Ces chiffres sont des bornes de conception pour les deux exécutables historiques. Les mesures doivent couvrir le pic et le solde des ressources après chaque échec, la requête réelle à NovaAuth pendant l’admission et les sorties et statuts de chaque processus. Le courtier reste étranger aux données. Une sélection doit désigner l’objet admis, le producteur, le contrôleur générationnel, l’instance du shell et la session courante. La disparition d’un acteur, l’expiration ou la révocation retire la réservation et les droits dérivés.

## Reproduction et preuves

Utiliser le profil `profile.py --lancement` au commit indiqué, puis appliquer `sondes.diff` dans l’export jetable. Les sondes observent les créations, destructions et admissions. Construire avec `novaauth-sessions-lab`, la clé de distribution publique et les mêmes entrées de qualification que les redirections. Les sources, le profil et le banc sont identiques pour les deux géométries de chaque architecture. `mesures.json` identifie les PID, les compteurs et la version de QEMU par profil. Les répertoires `avant-*` contiennent les disques, les résultats négatifs et les journaux série.

Archive durable : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/preuves-lancement-generique-elf/diagnostic-racines-bnr7zs2w`. Les 27 fichiers du manifeste ont été relus après sa création. Les 802 fichiers du manifeste de la campagne précédente ont également été relus et comparés. Les onze parcours ELF et redirections appartiennent à cette campagne initiale. [Manifeste du diagnostic](validation/diagnostic-racines-ipc-0240.sha256) · [Qualification initiale](lancement-generique-flux-redirections.md)
