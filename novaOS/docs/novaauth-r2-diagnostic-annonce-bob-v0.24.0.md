# NovaAuth R2 — Annonce de Bob au démarrage du banc EXEC

25 novembre 2026. Diagnostic x86_64/P9-512, avant la première commande administrative. L’essai d’origine utilisait le code local `5361ff40c7f6546ddad777d7d5286a370dcd46e5` et l’export instrumenté ancien H. Les trois exécutions ont utilisé la même ISO, le même ELF de registre et le même état P9 initial.

## Diagnostic

| Exécution | Invite nsh | Registre prêt | Annonce de Bob | Issue hôte |
|---|---:|---:|---|---|
| Original, avant correction | ligne 80 | ligne 154 | Aucune dans les 11 100 octets archivés | Recherche immédiate, acteur absent ou ambigu. SIGTERM ligne 155. |
| Attente bornée de l’annonce | ligne 80 | ligne 153 | PID 11, ligne 154 | Le banc attend ensuite une nouvelle invite pendant 30 s et expire. |
| Deux observations depuis le début du démarrage | ligne 80 | ligne 154 | PID 11, ligne 155 | Les deux événements sont observés. Fin volontaire avant commande. |

Dans le premier essai, les routes des shells 5 et 11 étaient installées. Après le marqueur du registre, `Events.actor` inspectait seulement le tampon déjà reçu. Il exigeait exactement un acteur. La trace intégrale contient zéro correspondance et l’exception a conduit le lanceur hôte à arrêter la VM. Le rapport emploie la clé générique `erreur_invite` pour cette exception Python. Aucun statut natif EXEC ne lui est associé. La rupture de tube consignée par le collecteur ne permet pas de conclure à une perte de l’annonce sur la voie série. Le traceur ne permet pas non plus de déterminer si Bob avait atteint son appel d’émission juste avant l’arrêt initial.

Le premier diagnostic corrigé établit que Bob peut émettre après le marqueur du registre. Il révèle également que la première invite de commande est déjà dans le tampon. Le banc cherchait cette invite uniquement après le curseur du registre. L’ordre des deux émissions n’est imposé ni par le lanceur ni par le shell de Bob, qui attend ses routes privées avant de publier son annonce.

Dans le dernier essai, l’invite commence à l’octet 5 935. À la reconnaissance du registre, 11 020 octets ont été reçus et le curseur de commande vaut 11 018. L’annonce complète de Bob arrive dans une lecture ultérieure, à l’octet 11 020. Les deux recherches rendent le curseur à 11 018. L’observateur est fermé explicitement à la fin de la sonde. Son compte rendu ne contient aucune erreur.

## Correction et vérification

Le banc attend l’annonce au moyen de la lecture série bornée existante. Il vérifie l’unicité de l’acteur parmi les annonces reçues, recherche l’invite dans le même démarrage et restaure le curseur même en cas d’exception. Avant de soumettre la première commande, la recette complète place son curseur au bout des octets déjà reçus. L’oracle de commande continue d’exiger le marqueur `DEBUG` corrélé, son PID et le statut. Le délai de l’annonce est un délai d’observation hôte de 20 s. L’attente de l’invite conserve ses 30 s.

Le script de l’essai interrompu vivait hors Git. L’archive conserve son original, la version corrigée intégrale et le module hôte associé. Le patch publié s’applique à l’original d’empreinte `71c51da673235d7ef9f74bddf833a7edf315e86dca1308d74aa378ca8e35b089`. Son application a été vérifiée octet par octet contre la version corrigée. Pour réutiliser cette dernière dans un export, employer aussi le module hôte `administration_events.py` de cette publication. Le démarrage diagnostique s’arrête avant la soumission d’une commande. [Patch du script](validation/novaauth-r2-annonce-bob-20261125/essai-ancien.patch) · [Module hôte](../user/novaauth/autorite_qemu/administration_events.py)

Les 17 tests de la suite hôte d’événements réussissent. Ils exercent l’arrivée tardive, une invite précoce, le rejet d’une ancienne instance, une ligne tronquée, deux acteurs ambigus et la conservation du curseur. Les attentes et la capture se déroulent sur l’hôte. [Tests hôtes](validation/novaauth-r2-annonce-bob-20261125/tests-hotes.log)

| Temps mesuré | Original archivé | Attente de Bob | Deux observations |
|---|---:|---:|---:|
| Attente du verrou, secondes | 0,000256 | 0,000234 | 0,000256 |
| Exécution du banc sous verrou, secondes | 6,251 | 35,970 | 5,890 |
| TLS invité, début → fin, millisecondes monotones | 648 → 3 108 | 592 → 3 108 | 592 → 3 036 |

Les deux essais de ce diagnostic ont acquis et libéré le verrou commun, sans conflit observé. Les heures UTC détaillées et les compteurs par essai figurent dans les résultats. Sur le démarrage final, le registre et le signataire ont chacun un pic de quatre racines. Leurs relevés avant et après récupération TLS sont identiques. Ces mesures précèdent tout lancement G/H. Le dernier relevé statique disponible dans les journaux Darwin des deux architectures est de 4 026/4 096 octets. Le correctif porte sur Python hôte, avec la même image invitée. [Résultats et offsets](validation/novaauth-r2-annonce-bob-20261125/resultats.json)

## État de la branche et portée

Au début, `origin/codex/novaauth-administration-0240-phase1` désignait `beda019dd73432d82b25cac6f6cb95f5b5bf60ed`. Le worktree R2 contenait un commit local supplémentaire, `5361ff4`, sept fichiers suivis modifiés, trois non suivis et un index sans changement. Les dix contenus ont été contrôlés de nouveau par SHA-256. La publication du diagnostic part de la tête distante dans un worktree dédié. Le commit fonctionnel local et les prototypes restent dans le worktree initial. [Inventaire des empreintes](validation/novaauth-r2-annonce-bob-20261125/prototypes.sha256.json)

Seule la synchronisation de démarrage est vérifiée ici. E2, E3, la concurrence EXEC G/H et leur drainage restent à démontrer. Les preuves Direct s’appliquent à la fixture compacte. Les réserves sur la fixture complète AArch64/P9-512, les 301 cadres instrumentés AArch64 et la reproductibilité entre environnements restent ouvertes.

## Reprise froide soumise à arbitrage

Le document intégral du commit `5665ba580772c4d6423b389d452eec3c6048c37e` est disponible dans le dépôt et dans l’archive sous le nom `5665ba5-complement-integral.md`. Les deux copies ont été vérifiées contre le blob Git, SHA-256 `f497e9508b704d7c53ed306bba660855064da3094e5cd4709723bf218b1d184b`. Il soumet à arbitrage les faits à conserver et leur rétention pour les classes VFS, délégations et EXEC. La fenêtre entre effet local et persistance peut laisser une opération d’issue inconnue. La quarantaine reste applicable en l’absence d’une preuve requise. [Complément architectural intégral](novaauth-r2-preuves-avant-coupure-v0.24.0.md)

L’archive contient 37 fichiers et leur manifeste. Chaque entrée a été relue depuis l’archive et comparée à l’original conservé. [Empreinte et emplacement de l’archive](validation/novaauth-r2-annonce-bob-20261125/archive.json) · [Manifeste SHA-256](validation/novaauth-r2-annonce-bob-20261125/archive-fichiers.sha256)

Traces série intégrales publiées avec compression sans perte. [Essai original](validation/novaauth-r2-annonce-bob-20261125/original.serial.gz) · [Première correction](validation/novaauth-r2-annonce-bob-20261125/attente-annonce.serial.gz) · [Diagnostic final](validation/novaauth-r2-annonce-bob-20261125/attente-annonce-et-invite.serial.gz)
