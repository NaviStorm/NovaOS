# Périmètre approuvé du jalon v0.21.0

## Origine des quatre phases

À la fusion du cache, les quatre décisions de développement ont les points d’intégration suivants sur la première lignée de `main`. Le dossier de clôture rattache les observations de chaque phase au code, au test, au profil et aux archives correspondants. [Matrice de qualification](../cloture-v0.21.0.md)

| Engagement de v0.21.0 | Fusion dans `main` | Contrat et limite du mandat |
|---|---|---|
| Identités et cycle de vie | `a6df93fb8687c68893680707f8763471d9f8a2b3` | Rattachement, génération, révocation, restitution et distinction entre démarrages sur les sources qualifiées. Sur Cortex-A72, la recette exige le rejet de l’ancienne identité et le succès de la nouvelle avec le même droit après redémarrage normal et forcé. [Instances](../instances-phase1-v0.21.0.md) · [BootID](../bootid-phase1-v0.21.0.md) |
| Temporaire privé par instance | `bcdfe4ccfd98c9e4a58f5cead3ce666e917ce33c` | P9 pour le nouveau lanceur, révocation, nettoyage et quarantaine. La compatibilité P8 conserve le banc de lecture historique et l’absence de migration automatique. [Contrat](../contrat-tmp-prive-v0.21.0.md) · [Résultats](../tmp-prive-phase2-v0.21.0.md) |
| Services système | `80a3699c76af0db9ccdc1a85964fe15576c32439` | Service réel, ordre des dépendances, READY de l’incarnation courante, renouvellement des capacités et arrêt ordonné. Le gestionnaire conserve un journal volatil et sa perte entraîne le retrait des familles. [Supervision](../services-phase3-v0.21.0.md) |
| Cache persistant | `4569a6876720f6b4efa901db707b670e415abeeb` | Identité d’installation stable et utilisateur attesté. Sur P9, le profil de banc provisionne cette identité. La session ordinaire sans attestation obtient son `tmp` et reçoit un refus pour le cache. [Cache privé](../cache-prive-phase4-v0.21.0.md) |

La feuille de route antérieure laissait v0.21.0 en attente d’affectation après l’intégration des threads au jalon Rust (ARC-021). Les mandats successifs ont fixé le périmètre ci-dessus. La préparation de `permissions` en phase 1 portait sur les identifiants, les trois sous-commandes et les autorités nécessaires. Le contrat publié en garde la trace. [Registre des décisions](../reconciliation-v0300/decisions.md) · [Contrat d’instances](../reconciliation-v0300/contrat-instances-services.md)

## Huit thèmes futurs demandés au contrôle de la feuille de route

L’état **Reporté** situe le livrable à l’échéance approuvée. Les sept premiers thèmes restent exigés pour la première stable serveur, le huitième vient après. Les numéros de lots intermédiaires relèvent de la proposition ARC-019. Chaque ligne compte une fois dans le bilan des reports. La préparation de contrats ou un essai avec une autorité de banc ne vaut pas réalisation de la fonctionnalité future.

| Thème | Statut | Échéance et engagement restant | Preuve documentaire |
|---|---|---|---|
| Comptes et NovaAuth | Reporté | v0.30.0, lot v0.24.0 proposé. Enrôlement, sessions identifiées, révocation et rétablissement, à vérifier notamment en S01–S02. | D (ARC-014/017), E, P, A |
| ACL persistantes | Reporté | v0.30.0, lot v0.24.1 proposé. Lier droits et version de politique aux objets, qualifier leur fraîcheur et leur retrait, y compris après restauration (S03, S16). | D (ARC-017), E, P, A |
| Commande native `permissions` | Reporté | v0.30.0, sans version intermédiaire approuvée. `list`, `grant`, `revoke` sous autorité explicite. La recette S03 doit comparer les droits exposés aux opérations permises et refusées. | I, T, A |
| Réseau | Reporté | v0.30.0, lots v0.25.0 et v0.26.0 proposés. Double pile, transports, configuration et filtrage sur un pair indépendant (S05, S06, S08). | D (ARC-014), E, P, A |
| SSH | Reporté | v0.30.0, lot v0.26.1 proposé. Serveur et client SSH, transferts SFTP, contrôle des identités et refus hors délégation (S09). | D (ARC-020), P, A |
| Compilateurs Rust/C/C++ natifs | Reporté | v0.30.0, sans numéro intermédiaire attribué. Compiler, lier et exécuter dans `nsh` en trois langages sur chacune des deux ISA. Le SDK croisé a une preuve distincte. | D (ARC-022), P, A |
| Distribution et mises à jour | Reporté | v0.30.0, lot v0.28.0 proposé. Distribution serveur signée, activation atomique, retour contrôlé et rétablissement des droits (S01, S12–S16). | D (ARC-014), E, P, A |
| NovaUI | Reporté | Après v0.30.0 par décision explicite, sans numéro attribué. Le serveur texte conserve NovaTUI. | D (ARC-015), P |

Les rapports v0.21.0 ne revendiquent ni un essai graphique UTM, ni une coupure électrique physique, ni un remplacement à chaud du gestionnaire. Les critères d’origine portent sur les deux invités QEMU, le modèle de défaillance injecté et la reprise documentée. Une restauration intégrale sans attestation externe de continuité reste ambiguë. Les limites de distribution, d’attestation et de reprise sont à conserver dans toute décision de clôture. [BootID](../bootid-phase1-v0.21.0.md) · [Services](../services-phase3-v0.21.0.md) · [Cache](../cache-prive-phase4-v0.21.0.md)

## Sources de la comparaison

Les SHA-256 ci-dessous identifient le contenu relu des documents dans Git. Les références D, E, I, P et A désignent leur état dans la fusion `4569a6876720f6b4efa901db707b670e415abeeb`, avant modification documentaire. T désigne la version préparatoire du chapitre S03, S07 au commit `622d8d97e547434d62b65da00827324cf9405507`. Ces pièces documentent des décisions et des critères. Les journaux d’exécution figurent dans les matrices par phase.

| Référence | Document dans `novaOS/docs/reconciliation-v0300/` | SHA-256 |
|---|---|---|
| D | [decisions.md](../reconciliation-v0300/decisions.md) | `dc2c421bb00629fbcff08127d12750545aa11a2f52eb8eb78f93ebdc45c43781` |
| E | [exigences-serveur.md](../reconciliation-v0300/exigences-serveur.md) | `30cc0b439d57e33ff1d7f35f19ed01fd9cc5ade97675316fd1644f8eb81c032c` |
| I | [contrat-instances-services.md](../reconciliation-v0300/contrat-instances-services.md) | `0c53629f16593f66b5ca04f921730a39311058bda348992e4973b7f090bb1aaf` |
| P | [proposition.md](../reconciliation-v0300/proposition.md) | `ea084c2c8a0a54b0033f825698bb3b843d80e93eb13f2313a41457d8146efb6e` |
| A | [acceptation-serveur.md](../reconciliation-v0300/acceptation-serveur.md) | `b973a683c35e35df0bc9e95e374aecfe01c6ca24a3721d4374334ea83d57b1d5` |
| T | [acceptation-serveur.md à 622d8d9](https://github.com/NaviStorm/NovaOS/blob/622d8d97e547434d62b65da00827324cf9405507/novaOS/docs/reconciliation-v0300/acceptation-serveur.md) | `db1ef2ced846abe3cd4ae606081054e39dfc679067a7def0e09e46b6d13d4e70` |
