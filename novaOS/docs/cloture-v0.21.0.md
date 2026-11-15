# NovaOS v0.21.0 — Clôture officielle des sources qualifiées

**Décision MOA/MOE du 15 novembre 2026.** NovaOS v0.21.0 est officiellement clôturée pour ses sources qualifiées sur x86_64 et AArch64. La décision couvre les identités et le cycle de vie des instances, le temporaire privé, la supervision des services et le cache persistant dans les conditions de la matrice. Les huit reports et les quatorze qualifications avec limites conservent leur statut.

| Référence de clôture | Objet |
|---|---|
| Sources qualifiées | `4569a6876720f6b4efa901db707b670e415abeeb` |
| Rapport d’audit approuvé | `afb4477623021ac3402ad9e605e9d0e546844a5f`, enfant direct des sources qualifiées |
| Tag annoté de livraison des sources | `v0.21.0`, sur le commit qui intègre le rapport et enregistre la décision officielle |

La comparaison avec l’arbre source porte exclusivement sur le rapport, ses annexes, le relevé de traçabilité et les documents de feuille de route et d’acceptation. Les objets Git du code, des interfaces générées, du moteur NovaFS et des paquets distribués sont identiques.

## Fusion de la phase 4

| Référence | Valeur vérifiée |
|---|---|
| Premier parent, précédent `origin/main` | `80a3699c76af0db9ccdc1a85964fe15576c32439` |
| Second parent, candidat publié sur `codex/cache-0210-phase4` | `4a3d3079bad8a8ef074601df5afdf9c6d5faacfb` |
| Fusion publiée sur `origin/main` | `4569a6876720f6b4efa901db707b670e415abeeb` |
| Arbre Git du candidat et de la fusion | `ea3fab4c63f669b3325ecf3c7af0aa5e21f68a7b` |
| Commits propres à la phase 4 | `1ea146d1a4451cac5ef9698f00f3efb0894053c9`, `2a5d775ab88625ba8619b229002802fd13607362`, puis le candidat ci-dessus |

Les deux références distantes et la filiation ont été vérifiées avant fusion. Le contrôle de génération et les neuf groupes ABI ont réussi sur l’arbre fusionné avant le push. Une lecture de la référence distante a confirmé le commit publié. La branche de clôture part de cette fusion. [Relevé des contrôles et empreintes](cloture-0210/controles.json)

## Matrice des exigences

Les annexes identifient, pour chaque critère, le contrat, le code, le test, son architecture, l’archive et son SHA-256. Un critère qualifié avec limites satisfait le contrat dans les conditions indiquées sur sa ligne. « Non validé » désigne ici un critère exigible sans preuve suffisante. Les huit thèmes reportés ont une échéance documentée hors de ce jalon. Les versions distribuées et les domaines matériels non exercés figurent dans une matrice de disponibilité distincte.

| Domaine | Validés | Validés avec limites | Reportés | Non validés bloquants |
|---|---:|---:|---:|---:|
| Identités et cycle de vie | 5 | 4 | 0 | 0 |
| Temporaire privé et protocole NEXT/COMPLETE | 6 | 4 | 0 | 0 |
| Supervision des services | 10 | 1 | 0 | 0 |
| Cache persistant | 8 | 4 | 0 | 0 |
| Compatibilité et récupération NovaFS | 7 | 1 | 0 | 0 |
| Quatre anciens ELF et archive SDK | 5 | 0 | 0 | 0 |
| Registre ABI et interfaces générées | 3 | 0 | 0 | 0 |
| Autres engagements de feuille de route | 0 | 0 | 8 | 0 |
| **Total de 66 critères** | **44** | **14** | **8** | **0** |

[Matrice identités et tmp](cloture-0210/identites-temporaire.md) · [Matrice services, cache, stockage et binaires](cloture-0210/services-cache.md) · [ABI et disponibilité dans l’image installable](cloture-0210/abi-distribution.md) · [Matrice des reports et sources des décisions](cloture-0210/perimetre.md)

## Fonctionnement qualifié

| Phase | Résultat observable |
|---|---|
| Identités et cycle de vie | Le noyau lie l’incarnation au BootID, au numéro local et à la génération. Sur Cortex-A72 sans RNDR, la graine du relais UEFI/QEMU permet au nouveau travailleur de démarrer. Après chacun des deux parcours, redémarrage normal et arrêt forcé suivi d’une relance, le droit valide refuse l’identité précédente et accepte l’identité courante. La sonde sans source qualifiée obtient un refus explicite. |
| Temporaire privé | Le courtier accorde une vue privée sur P9, partagée entre threads et déléguée explicitement au travailleur. La fin de l’instance retire les droits. La récupération vérifie l’appartenance avant purge et garde en quarantaine un orphelin incertain. Si NovaFS disparaît pendant une opération, la réponse dépend de l’état du ticket et de l’acquittement disponible. Une écriture `UNCERTAIN` ne fait l’objet d’aucun rejeu automatique. |
| Services | Le gestionnaire en espace utilisateur ordonne les dépendances. Après l’annonce du service d’information de stockage, il vérifie une vraie réponse `INFO`. Une ancienne annonce `READY` est rejetée. Après relance, l’identité et les canaux sont neufs. Le démarrage minimal du pilote, de la pile bloc et du stockage précède l’attribution des temporaires aux services qui en ont besoin. |
| Cache avec utilisateurs et installations **provisionnés par le banc** | Deux instances autorisées partagent des données avec des capacités distinctes. Les paires étrangères sont isolées. Les essais couvrent persistance, quotas, révocation, purge avec écriture en vol, recréation immédiate, reprise et restauration avec attestation. Les nouvelles capacités désignent la nouvelle génération, dans laquelle l’ancienne écriture est absente. |
| Cache en **session ordinaire sans utilisateur attesté** | Le client lancé par `nsh` fonctionne, crée un fichier dans son temporaire P9 et reçoit `DENIED` à l’ouverture du cache. Cet essai existe sur les deux ISA et les deux géométries. L’accès de véritables comptes attend l’autorité d’identité, la preuve de continuité de l’installation et une qualification de bout en bout. |

## Étendue des essais, mémoire et compatibilité

- Les preuves invitées couvrent QEMU AArch64 et x86_64. Les matrices SMP concernées comportent 1, 2 et 4 vCPU, avec secteurs de 512 et 4 096 octets pour le stockage. Les tests hôtes du moteur utilisent Darwin ARM64 natif et x86_64 sous Rosetta. Chaque annexe indique sur quelle révision une sonde historique a tourné et quels contrôles ont ensuite été effectués sur le candidat.
- La phase 3 conserve 52 démarrages de supervision et 44 de régression. La phase 4 conserve 104 démarrages cache, dont 12 interruptions de VM lors d’une purge. Le laboratoire couvre 200 interruptions après FLUSH et 64 erreurs de FLUSH sur les deux architectures hôtes et les deux tailles de secteur. La clôture réutilise ces journaux et les relectures des disques. Elle ajoute le contrôle ABI sur la fusion. [Services](services-phase3-v0.21.0.md) · [Cache](cache-prive-phase4-v0.21.0.md)
- **Mémoire.** Dans le parcours complet de supervision, le pic supplémentaire est de 1 208–1 212 cadres sur ARM et 1 205–1 216 sur x86. Pour la matrice cache, les pics système vont de 941 à 1 339 cadres sur ARM et de 939 à 1 337 sur x86. Un cadre vaut 4 Kio. Les tables dynamiques du courtier cache atteignent 1 344–1 696 octets sur les deux ISA et reviennent à zéro après drainage. Les compteurs de plateforme DMA et de tables de pages sont suivis séparément. Une interruption forcée empêche la mesure de fin de session. [Mesures de supervision](services-phase3-v0.21.0.md) · [Mesures par cellule cache](validation/cache-phase4-v0.21.0.json)
- **ABI et binaires.** Le registre natif v1 couvre 48 sélecteurs. Les 47 entrées présentes avant la phase 4 sont identiques, et `cache::CONTROL` ajoute le numéro `0x1007c`. Les quatre ELF v0.20.0 archivés ont donné les résultats prévus sur les noyaux de sonde issus des sources qualifiées. L’archive distribuée de `nova-system` 0.2.0 a été relue. [Détail par ELF](cloture-0210/services-cache.md) · [Contrôles Rust/C/C++ et dispatch](cloture-0210/abi-distribution.md)
- **Stockage.** Le moteur NovaFS reste en v0.9.13. Les formats P8 et P9 sont inchangés. Le mode public historique P8 conserve la génération en lecture. Le lanceur privé refuse P8 avant écriture. Les données publiques sur P9 ont été comparées indépendamment après les parcours. Les refus avant exécution, les résultats connus et les écritures incertaines sont distingués. Les garanties de publication s’appliquent à l’unité transactionnelle effectivement testée. [Matrice stockage](cloture-0210/services-cache.md) · [Correction NEXT/COMPLETE](integration-abi-tmp-v0.21.0.md)

## Distribution et limites de décision

Le chantier source v0.21.0 correspond au SHA de fusion ci-dessus. Son noyau affiche **NovaOS v0.20.0**, le SDK livré porte **0.2.0**, NovaFS porte **0.9.13**. Le paquet UTM distribué contient **NovaOS v0.17.0**, construit le 14 novembre 2026 depuis `b76c863c9302bf6ead53fac3b80b26df0513fe0d`. On peut y exercer l’installation, le démarrage autonome, `nsh`, `vi` et NovaFS P9. Les quatre nouvelles fonctions v0.21.0 sont absentes de ce support. [Provenance, contrôles des octets et correspondance fonction par fonction](cloture-0210/abi-distribution.md)

La qualification laisse ouverts le matériel physique, UTM graphique pour ces nouvelles fonctions, la perte électrique de l’hôte, le remplacement à chaud du gestionnaire, les véritables comptes et le recul intégral d’un disque ou d’une VM. Une graine rejouée ou une restauration intégrale peuvent reproduire un état ancien cohérent. Une décision d’accès après restauration dépend d’une autorité de continuité de confiance. Aucun de ces domaines n’a reçu un verdict positif sur la base d’une simulation. [Périmètre approuvé](cloture-0210/perimetre.md)

Les archives versionnées rendent les observations citées contrôlables. L’archive physique sœur conserve les images, les traces et les séries de phase 4, avec 4 159 copies comparées aux originaux. Le relevé précise quels historiques possèdent encore des données locales à conserver. [Inventaire et conservation des preuves](cloture-0210/worktrees.md)

Les huit thèmes reportés sont les comptes et NovaAuth, les ACL persistantes, la commande `permissions`, le réseau, SSH, les compilateurs natifs, la distribution et les mises à jour, puis NovaUI après la stable serveur. Les critères de v0.30.0 restent à exercer sur leur propre candidat. L’ouverture d’un prochain jalon relève d’une décision distincte. [Feuille de route](reconciliation-v0300/proposition.md) · [Acceptation de la stable serveur](reconciliation-v0300/acceptation-serveur.md)
