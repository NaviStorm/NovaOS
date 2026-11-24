# NovaAuth v0.24.0 — Ancien ticket Direct et état de la reprise froide

25 novembre 2026, heure de Paris. Base publiée : `07cef0854c8916c623497b7d986fc2685639a828`.
Branche : `codex/novaauth-administration-0240-phase1`.

La recette E1 vérifie la consommation, dans l'invité, du ticket Direct de Bob après le retrait de G **à Bob**. Alice confirme le retrait dans une session séparée. L'oracle exige la même référence, une inscription antérieure à la publication, les trois acquittements durables, `RETIREE` avant l'admission et exactement un enfant. Il vérifie le statut natif 0 et compare les deux flux de Bob aux oracles du banc, avec 64 octets attendus sur stdout et stderr vide. Il exige aussi la restitution des deux sessions et le drainage global.

Le code invité E1 est `b67f25d8ea30b8ce6d6955020ecc407a29b8367a`. Le générateur de la fixture compacte est `d20b8637ed44cfd2563d0a5a00c802cfa8a24cc3`. Leur diff ne porte que sur l'exemple de provisionnement hôte. Les compilations repartent d'un export Git propre. L'ISO contient le profil `novaauth-sessions` vérifié et le noyau correspondant à son empreinte.

## Cause de l'expiration initiale et corrections

- `8fc2deb` présentait le ticket 1 092 ms après l'échéance. Consulter de nouveau chaque classe déjà confirmée rallongeait la confirmation. Le coordinateur de `a5ce707` lit le masque signé avant les envois, envoie trois acquittements distincts et consulte le terminal signé sur la tête fraîche. Après perte, il relit le même identifiant avant toute suite. Aucune classe envoyée sans décision authentifiée ne peut être réémise. Après cette optimisation, un essai x86_64/P9-512 a présenté le ticket à 84 ms de l'échéance et a expiré pendant l'admission.
- `a9039fb` place le rendez-vous après la réserve et l'inspection des effecteurs. Après remise à Bob, le vérificateur vérifie à nouveau la session et la capacité administrative d'Alice, puis soumet sa confirmation d'origine une fois. Un essai a créé l'enfant à temps. L'oracle s'est arrêté sur une variable locale non initialisée, corrigée dans `96c2539`. Une préparation suivante a échoué avant QEMU par import hôte manquant. Les deux essais x86_64 de la reprise suivante ont réussi avec la fixture complète.
- AArch64/P9-512 a encore expiré pendant l'admission avec 256 ms de marge à la présentation, puis 372 ms après la synchronisation de `b67f25d` sur l'offre native acquittée. La fixture compacte `d20b863` retire du scénario les règles VFS de démonstration, les installations auxiliaires de Bob et les portées administratives d'Alice autres que celle de `usermod`. Le triplet passe de 12 120 à 7 572 octets sur AArch64/P9-512. Les échecs de la fixture complète restent conservés. Les délais de session, de ticket et TLS sont les mêmes.
- Les certificats éphémères du laboratoire ont été renouvelés après expiration selon le générateur existant. Les essais emploient deux vCPU, 256 Mio, TCG et aucun réseau invité. La validation compare uniquement les événements d'une même horloge monotone.

## Matrice consolidée Q1–Q4

Périmètre des preuves historiques : chaque succès reste attaché à son code, à son profil et à sa recette. Les [résultats d'intégration](novaauth-r2-revalidation-exec-integration-v0.24.0.md), la [campagne de droits réels](novaauth-r2-droits-reels-v0.24.0.md) et le [rapport de reprise réservée](novaauth-r2-revalidation-exec-etat-v0.24.0.md) donnent les révisions et empreintes par profil.

| Garantie | x86_64/512 | x86_64/4096 | AArch64/512 | AArch64/4096 | Périmètre établi |
|---|---|---|---|---|---|
| Q1 VFS Direct/G/H | Historique réussi | Historique réussi | Historique réussi | Historique réussi | Droits VFS de `nsh`, retrait G, conservation Direct/H |
| Q2 VFS | Historique réussi | Historique réussi | Historique réussi | Historique réussi | Admission G en vol, perte FINISH et annulation avant GO. Annulation PREPARE complète et drainage EXEC G restent ouverts |
| Q3 VFS | Historique réussi | Historique réussi | Historique réussi | Historique réussi | Deux confirmations de la même intention et E/S G en vol. La course avec une admission EXEC G dans l'invité reste ouverte |
| Q4 réponse ACK durable perdue | Réussi | Réussi | Réussi | Réussi | Une publication, trois ACK distincts, consultation après `UNCERTAIN` |
| Q4 ACK jamais admis | Réussi | Historique réussi | Historique réussi | Réussi | `RETRAIT_EN_COURS`, aucun ACK inventé |
| Q4 reprise après `RETIREE` | Historique réussi | Historique réussi | Historique réussi | Historique réussi | Consultation de l'historique terminal, sans publication répétée |
| Q4 reprise froide après `RESERVE` | Refus sûr historique | Refus sûr historique | Refus sûr historique | Refus sûr historique | Réservation préservée, zéro nouvelle publication et zéro ACK, B en quarantaine avec statut 5 |
| EXEC ancien Direct E1 | Réussi | Réussi | Réussi | Réussi | Fixture compacte, retrait au titulaire, création effective avant expiration |
| EXEC ancien H et G E2/E3 | Non exécuté | Non exécuté | Non exécuté | Non exécuté | Chemin de provenance et de délégation à l'ELF encore absent |

Les quatre essais de réponse ACK perdue et les deux essais d’ACK absent utilisent le code invité et le banc `d20b8637ed44cfd2563d0a5a00c802cfa8a24cc3`. Pour la perte, `CONFIRM` retourne 20 (natif 22), puis `OUTCOME` retourne 0. Pour l’absence, les deux consultations restent au statut 20 (natif 22). Dans les deux cas, une seule publication durable est observée. La ligne Q4 froide établit la sûreté du refus. La restauration de disponibilité après disparition des effecteurs n'est pas acquise.

## Raccordements et limites

- **EXEC G/H.** Le chargeur isolé [transmet une portée VFS vide](../user/novaauth/service/exec_package_read.rs). Le [registre sélectif](../user/novaauth/service/src/bin/common/selective_exec_guest.rs) ne lie que le rôle Direct sans délégation, et le [chemin historique des fichiers enfant](../user/novafs/files/exec_files_lab.rs) consulte le registre local et une règle Direct. La permission attestée fraîche doit lier le dossier et les droits réellement transmis à l'enfant à sa session, son image scellée et son descripteur. Il reste à connecter cette décision aux deux canaux fichier déjà prévus, à la fermeture des seuls tickets et baux de G et à l'inventaire des lecteurs préparés. Un READ du paquet ne constitue pas cette preuve.
- **Course EXEC et retrait.** L'autorité vérifie le descripteur et la barrière de groupe dans une transaction. Les entrelacements hôtes et les courses VFS invitées existent. Une qualification EXEC G doit encore montrer l'issue de chaque admission antérieure puis le refus natif des nouvelles admissions sur G, avec les PID et leurs marges d'expiration. L'ordre E1 place volontairement l'admission Direct après le reçu durable.
- **Reprise froide incomplète.** Le codec authentifie la réservation et l'historique. La consultation d'une réservation non publiée sur sa tête précédente exacte donne une issue non terminale. En cas de divergence, le refus reste explicite. Le registre du profil publié n'a pas encore de consultation `PENDING` avant activation. La [barrière de drainage](../user/novaauth/autorite_qemu/admission_exec.py) bloque B si la réserve de A reste ouverte. Elle interdit aussi la publication d'une réserve par A après passage de sa route en retrait. Aucun effecteur redémarré ne peut acquitter au nom du précédent. Le [contrat §8](novaauth-r2-revalidation-exec-contrat-v0.24.0.md#8-reprise-et-anti-rollback) demande un arbitrage distinct pour ce cas. L'arbitrage doit définir le sort de la réservation non publiée, les preuves irréversibles de neutralisation et la récupération des issues individuelles avant de permettre une activation. Une perte de liaison ou l'arrêt de la VM ne donne aucun de ces faits à l'autorité.
- **Réserves.** Le sous-total de contrôle additionnel mesuré reste 4 026/4 096 octets sur les deux cibles natives, marge 70. Le périmètre de cette mesure exclut les piles TLS et le cache P9. Les mesures invitées donnent les pics observés et les instantanés avant et après. La réserve historique de 301 cadres instrumentés AArch64 et la reproductibilité interenvironnements restent ouvertes, avec l'avertissement `rust-objcopy` pour `libLLVM.dylib` lors de ces compilations.

Les neuf contrôles ABI réussissent. Les 48 sélecteurs sont identiques. Les quatre ELF historiques restent sur les preuves de leurs profils historiques : le diff de cette campagne ne modifie ni leur ABI, ni leur chargeur, ni le noyau ordinaire. Le nouveau rendez-vous est injecté uniquement dans la copie QEMU Direct. Le prototype conservé est exclu des exports et du commit.


## Marges et ressources des quatre témoins E1

| Profil | PID enfant | Marge à la création | TLS initial | Pages signataire / registre | Pic cadres IPC |
|---|---:|---:|---:|---:|---:|
| x86_64/P9-512 | 23 | 1744 ms | 1980 ms | 73 / 163 | 74 |
| x86_64/P9-4096 | 23 | 1808 ms | 2068 ms | 73 / 163 | 74 |
| aarch64/P9-512 | 23 | 596 ms | 2468 ms | 65 / 160 | 74 |
| aarch64/P9-4096 | 23 | 1976 ms | 1980 ms | 65 / 160 | 74 |

Sur les quatre profils, le même ticket est inscrit en position 13, la mutation publiée en 14,
le retrait terminal en 18, l'admission en 19 et l'issue de création en 20. Ces positions sont
propres à chaque autorité de test. Le registre revient à sept handles et le signataire à six.
Le plafond de quatre racines par acteur et le refus d'une cinquième sont observés.
Le bilan final indique zéro endpoint, nœud, espace et cadre IPC.
Les durées TLS du tableau correspondent aux échanges horodatés.

## Contrôles natifs et archives

- 52 contrôles hôtes de l’autorité, de la concurrence, du rendez-vous et de la preuve froide réussis.
- 17 contrôles de coordinateur et de layout passent par architecture sur ARM64 Darwin et x86_64 Rosetta. Les neuf contrôles natifs de reçu, dont la réservation non publiée, réussissent également par architecture. Le dernier correctif `0a472637c26a44df5c8848d8b40ef6992bffd146` porte sur cette lecture et ses tests. Les essais QEMU ci-dessus précèdent ce correctif, qui intervient uniquement sur le verdict d'un reçu `RESERVED` associé à sa tête précédente exacte. Les états `DRAINING` et `RETIRED` exercés restent soumis aux mêmes comparaisons.
- 17 QEMU démarrés, dont 12 réussites et 5 échecs conservés. Un arrêt d'import hôte avant démarrage est également conservé. Attente totale du verrou 0.007999 s. Temps hôte des bancs ayant démarré QEMU 606.243 s. Chaque acquisition/libération est consignée.
- 2 293 fichiers des trois archives historiques relus contre les manifestes. L'archive de cette campagne contient 218 preuves sélectionnées. Les disques, clés et binaires restent dans les répertoires privés. Les empreintes d'artefacts et les sorties témoins sont conservées. La recherche des clés de laboratoire connues ne trouve aucune occurrence.

Archive SHA-256 `7cf57698dcf575b5f5d6ac4cc1e097a134b1e6d3a97ba950c880a1712f5f500c`.
Manifeste SHA-256 `462f03110a742ecffe1bf3f334d0a07558a40a97ed26c5ae25b2d3da3a6dd061`.

[Matrice détaillée et chronologies](validation/novaauth-r2-ancien-direct-v0.24.0/resultats.json) ·
[Archive et empreintes](validation/novaauth-r2-ancien-direct-v0.24.0/archive.json) ·
[Manifeste des preuves](validation/novaauth-r2-ancien-direct-v0.24.0/MANIFEST.sha256).
