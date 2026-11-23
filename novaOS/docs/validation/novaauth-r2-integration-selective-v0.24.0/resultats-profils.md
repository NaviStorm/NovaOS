# Résultats observés — raccord administratif sans délégation G ouverte

Code `1d5a678a5c8451c01606b102d997e76681eceee2`, arbre `e18173ad333aa467552122cfd4d863006a6f79b2`. Tous les essais ci-dessous sont instrumentés, dans le profil QEMU privé. Aucun essai ordinaire ni aucune recette Q1–Q4 complète.

| Profil | ACK non admis | Réponse ACK perdue | Réponse publication perdue | Échecs sur candidat final |
|---|---|---|---|---|
| x86_64/P9-512 | [Réussi](r2-commandes-x86_64-512-retrait-selectif-vide-ack-absent-02/invite/resultat.json) | [Réussi](r2-commandes-x86_64-512-retrait-selectif-vide-ack-perdu-02/invite/resultat.json) | [Réussi](r2-commandes-x86_64-512-retrait-selectif-vide-publication-perdue-02/invite/resultat.json) | 0/3 |
| x86_64/P9-4096 | [Réussi](r2-commandes-x86_64-4096-retrait-selectif-vide-ack-absent-02/invite/resultat.json) | [Réussi](r2-commandes-x86_64-4096-retrait-selectif-vide-ack-perdu-02/invite/resultat.json) | [Réussi](r2-commandes-x86_64-4096-retrait-selectif-vide-publication-perdue-02/invite/resultat.json) | 0/3 |
| aarch64/P9-512 | [Réussi](r2-commandes-aarch64-512-retrait-selectif-vide-ack-absent-02/invite/resultat.json) | [Réussi](r2-commandes-aarch64-512-retrait-selectif-vide-ack-perdu-02/invite/resultat.json) | [Réussi](r2-commandes-aarch64-512-retrait-selectif-vide-publication-perdue-02/invite/resultat.json) | 0/3 |
| aarch64/P9-4096 | [Réussi](r2-commandes-aarch64-4096-retrait-selectif-vide-ack-absent-02/invite/resultat.json) | [Réussi](r2-commandes-aarch64-4096-retrait-selectif-vide-ack-perdu-02/invite/resultat.json) | [Réussi](r2-commandes-aarch64-4096-retrait-selectif-vide-publication-perdue-02/invite/resultat.json) | 0/3 |

ACK non admis : statuts ELF/CLI `0,20,20`, statut administratif 22 aux deux dernières commandes ; autorité RETRAIT_EN_COURS. Réponses perdues : `0,20,0`, statut administratif 22 à CONFIRM puis COMPLETE à OUTCOME. Les trois commandes exécutent le même ELF dans des processus distincts, PID 21/22/23 dans ces essais. Aucun statut CLI 20 ne signifie une réussite de mutation.

## Ressources observées sur le candidat final

| Profil | Pages registre avant/après | Pages signataire avant/après | Racines max observées | Connexions ACK absent/perdu/publication perdue |
|---|---|---|---|---|
| x86_64/P9-512 | 149/149 | 69/69 | 4 | [79, 91, 87] |
| x86_64/P9-4096 | 149/149 | 69/69 | 4 | [79, 91, 86] |
| aarch64/P9-512 | 146/146 | 61/61 | 4 | [79, 90, 86] |
| aarch64/P9-4096 | 146/146 | 61/61 | 4 | [79, 90, 86] |

Le registre conserve 4 racines et 7 handles ; le signataire 4 racines et 6 handles. Les piles mesurées sont de 64 pages. Les compteurs temporaires reviennent à zéro au drainage final. La cinquième demande de racine est refusée pour les 12 essais. Les maxima par PID sont des échantillons, pas une garantie de pic physique exact.

## Historique de cette campagne, échecs conservés

| Profil | Échecs / essais instrumentés, toutes révisions |
|---|---|
| aarch64/P9-4096 | 0/6 |
| aarch64/P9-512 | 1/7 |
| x86_64/P9-4096 | 0/6 |
| x86_64/P9-512 | 3/9 |

Total : 4/28 échecs, dont 2 exécutions de la même mauvaise ISO, 1 défaut du collecteur AArch64 et 1 saturation du relais lors de la réconciliation répétée. Ce décompte mélange volontairement les révisions pour conserver l’historique ; il ne mesure pas la fiabilité du candidat final (0/12 sur les trois chemins ciblés).

Les quatre parcours nominaux antérieurs sont conservés et applicables selon l’analyse du rapport. Ils ne sont pas revendiqués comme nouvellement exécutés sur le binaire final. Les autres résultats antérieurs affectés sont remplacés dans la décision de qualification par les rejeux ciblés, mais jamais effacés.

## Paramètres et horloges

QEMU : QEMU emulator version 11.1.3 ; 2 vCPU ; 256 Mio ; accélération tcg ; réseau none. Les commandes exactes et les SHA initiaux/finals P9 sont dans chaque résultat. Chaque essai indépendant part d’une copie neuve de la genèse du profil. Aucun redémarrage de récupération n’est exécuté dans cette campagne.

Le délai TLS reste de 10 000 ms invités. Les temps TLS visibles, horodatés dans l’invité, ne sont pas des durées hôtes. Le quota de traces ne garantit pas un relevé temporel de chaque échange.
