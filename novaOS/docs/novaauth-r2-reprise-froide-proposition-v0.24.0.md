# NovaAuth v0.24.0 — Contrat proposé pour la reprise froide d’un retrait R2

25 novembre 2026. Référence `07cceee5d6cd83255f7a246834c804fc2d69e271`.
Document soumis à arbitrage MOA/MOE. Les mécanismes proposés restent à implémenter et à qualifier après arbitrage.

## Situation actuelle

L’autorité indépendante conserve la réservation signée, l’identifiant de mutation, les positions durables, les issues d’admission et les acquittements par classe et instance. NovaFS reste l’unique écrivain P9. Un reçu terminal se consulte sans rejouer la mutation. Le [contrat EXEC](novaauth-r2-revalidation-exec-contrat-v0.24.0.md#8-reprise-et-anti-rollback) maintient un retrait incomplet quand un ancien effecteur disparaît. La [campagne Q4](novaauth-r2-ancien-direct-v0.24.0.md#matrice-consolidée-q1q4) a démontré la quarantaine de la nouvelle incarnation après une coupure suivant la réservation, avec zéro publication et zéro acquittement ajouté.

La [barrière EXEC](../user/novaauth/autorite_qemu/admission_exec.py) interdit l’activation de B avant le drainage établi de A. La [publication](../user/novaauth/autorite_qemu/reprise.py) exige aussi une route active. Une réservation encore non publiée se trouve donc bloquée dès que la route A est mise en retrait et que ses effecteurs ne répondent plus. Augmenter un délai ou relancer la confirmation ne résout aucun de ces deux prérequis.

## Autorité et vue de reprise

L’administrateur indépendant authentifie une **demande de consultation et de reprise**. Celle-ci désigne le serveur, le domaine, l’incarnation A, l’instance de récupération B, l’identifiant original du retrait, les deux têtes complètes et un défi frais consommé une seule fois. L’autorité tient un seul dossier durable par retrait. Le dossier n’est ni une ouverture de session ni un accord de publication ou de création. La prise en charge du dossier est sérialisée avec les transitions d’A et les publications sur la tête. Une autre instance peut consulter son propre résultat selon les droits approuvés. Le remplacement du récupérateur exige une nouvelle décision explicite et le retrait durable de l’ancienne route de reprise.

Avant activation, B n’a accès qu’à la vue de reprise, distincte du registre actif. L’autorité répond à un défi frais par l’inventaire signé des réservations, y compris un inventaire vide, l’historique du retrait désigné, les deux têtes, sa publication éventuelle, les trois classes d’effecteurs et les issues individuelles. Les réponses sont liées entre elles par le même identifiant et la même position de journal ou par un ancrage monotone vérifiable. Une page manquante, un conflit de position ou une signature non vérifiable impose `UNCERTAIN`. La consultation ne réécrit ni l’auteur ni l’inventaire original. Le détail est accessible à l’instance de récupération autorisée, avec vérification séparée du droit de consultation.

## Sort d’une réservation non terminale

| Fait durable observé | Conduite proposée |
|---|---|
| `RESERVEE`, aucune mutation enregistrée, P9 exactement sur la tête précédente | Conserver la barrière de G. Après preuve de clôture des anciennes voies et résolution des opérations antérieures, autoriser explicitement l’exécution restante de **la même intention**. Recalculer le triplet déterministe depuis la tête précédente et vérifier son empreinte complète contre la réservation. L’autorité compare la tête et l’absence de publication, puis admet au plus une fois le même identifiant. |
| `RESERVEE`, candidat P9 présent mais publication indépendante absente ou issue locale indéterminée | Mettre le candidat en quarantaine. Établir la tête précédente et le sort de la préparation par les preuves transactionnelles P9 disponibles. Si la comparaison demeure incertaine, rester bloqué. Une ancre lisible ou un candidat préparé n’autorise aucune promotion. |
| `RETRAIT_EN_COURS`, mutation présente et P9 égal à la tête durable | Reprendre le drainage et les consultations du même retrait. Conserver la publication existante. |
| `RETRAIT_EN_COURS`, mutation présente et P9 ancien | Refuser toute activation. Restaurer le triplet exact correspondant à la tête durable depuis la sauvegarde chiffrée hors serveur approuvée, via la récupération 2-sur-2 et relecture P9. Cette réparation de stockage reste liée à la mutation déjà décidée et à son reçu. Si le triplet exact manque, maintenir le refus. |
| `RETIREE`, reçu, tête et publication cohérents | Restituer le terminal d’origine à la consultation. Poursuivre l’activation seulement si les autres conditions de drainage de l’incarnation sont satisfaites. |
| Têtes contradictoires, autorité inaccessible, état ou preuve inconnus | Maintenir `UNCERTAIN`, la barrière de G et la quarantaine. |

La procédure recommandée permet une reprise explicite du même identifiant uniquement dans le premier cas. Elle demande une **autorisation privée de publication de reprise**, subordonnée à la réservation et à la preuve de neutralisation. La route ordinaire de A demeure retirée. La procédure n’autorise ni annulation implicite de la réservation, ni nouvelle intention de retrait, ni réactivation de G. Un abandon administratif, si la MOA/MOE souhaite l’envisager, demande un contrat distinct définissant le devenir de la barrière. Il est exclu de ce projet.

## Preuves concernant les effecteurs

L’autorité fige l’inventaire et la borne des admissions antérieures dans la transaction qui ferme la route A. Les inhibitions locales ou l’arrêt de A restent des observations jusqu’à la preuve d’une clôture empêchant une reprise de cette incarnation. La proposition distingue un **acquittement original** d’une **preuve de substitution de classe**.

- **Classe 1, VFS et NovaFS.** Établir la clôture irréversible de la route et de ses baux, le sort de chaque lecture, écriture et PREPARE admis, puis la publication atomique du triplet et sa relecture. Un résultat d’écriture indécidable interdit la confirmation. La seule concordance de la tête du registre ne démontre pas le sort des E/S utilisateur.
- **Classe 2, vérificateur et délégations.** Établir que les routes, sessions et capacités déléguées de A ne peuvent plus recevoir d’admission, y compris après restauration d’une ancienne mémoire ou d’anciens disques. La nouvelle instance n’hérite d’aucun handle ni ticket.
- **Classe 4, EXEC.** Pour chaque admission durable antérieure à la clôture, conserver soit l’issue de création avec l’identité complète de l’enfant, soit la preuve d’annulation irréversible avant création. La clôture de la route seule établit l’interdiction des nouvelles admissions, sans établir le sort d’une création déjà admise. Les portes Dormant et les lecteurs préparés doivent être couverts par la clôture effective de leurs routes.

Une preuve de substitution éventuelle doit être signée par un effecteur ou superviseur indépendant explicitement autorisé pour cette classe. Elle est liée à l’ancienne instance complète, à la borne de clôture, à l’identifiant du retrait et à chaque issue concernée. Son type, son auteur et son lien aux preuves originales restent distincts dans le journal. Le nouveau service ne signe jamais au nom de l’ancienne instance. Trois classes exigent trois justifications. Une réponse QMP, l’observation d’un PID disparu, un délai, une coupure réseau, un compteur vide ou une VM redémarrée ne fournissent seuls aucun fait terminal. Les preuves acceptables de neutralisation et les moyens de rendre compte d’une issue sans témoin durable restent à décider par la MOA/MOE. Sans tels moyens, l’autorité peut uniquement réutiliser les preuves déjà durables et garder le reste incomplet.

## Linéarisation et pertes de réponse

1. Fermer les nouvelles admissions de A dans l’ordre durable, figer l’inventaire et consigner `RETRAIT_EN_COURS` pour la route.
2. Attacher à l’opération existante le dossier de reprise. Chaque autorisation et chaque fait soumis ont un identifiant distinct non réutilisable, avec référence à l’opération originale.
3. Rechercher chaque décision antérieure par identifiant. Une réponse perdue ne transforme jamais une issue en absence. Après une coupure, rechercher d’abord la même décision. L’autorité conserve le lien vers la publication unique et refuse toute seconde publication.
4. Après les preuves individuelles requises, comparer à nouveau la tête indépendante, la relecture P9 et la complétude des trois classes. Une transaction de l’autorité consigne `RETIREE` pour le retrait. La confirmation adresse un reçu signé et frais sur cet état.
5. Terminer le drainage d’incarnation. B ne devient active qu’après ce deuxième terminal, l’approbation administrative d’incarnation, les contrôles de tête, l’installation des barrières dans B et le refus définitif des anciennes références. Les nouvelles authentifications utilisent les droits courants. Direct/H ne sont pas supprimés par le retrait de G. En cas de perte durable de preuve, la disponibilité de B reste suspendue.

La publication P9 et le registre de l’autorité ont des points d’effet distincts. Préparer une image P9 ou lire un reçu historique ne vaut pas adoption. Tout état intermédiaire récupérable doit rester identifiable sans repartir d’un état local ancien. La disponibilité ne peut être promise si une opération attendue n’a aucune issue démontrable.

## Bornes et qualification proposées

Réutiliser les conversations privées, 128 octets par message IPC et la fragmentation authentifiée existante. Stocker l’inventaire et les preuves complètes auprès de l’autorité, les consulter par lots bornés et vérifier leur complétude avant adoption. Ne pas copier les tables entières dans l’invité. Le budget reste 4 096 octets statiques et quatre racines IPC par acteur, à remesurer sur les deux cibles avant raccord fonctionnel. Aucun seuil historique, sélecteur ABI, octet d’identité, garantie P9 ou délai de sécurité ne change.

Sur les quatre profils QEMU, tester les coupures après réserve, préparation et scellement P9, publication distante, relecture, chacun des trois acquittements, terminal et activation. Exercer l’autorité indisponible, la restauration des disques et d’un état mémoire A, chaque preuve absente ou substituée, une E/S ou une création dont l’issue reste inconnue, deux reprises concurrentes et une réponse perdue à chaque transition. Vérifier l’unicité de la mutation, l’absence d’ouverture par G, la disponibilité des opérations légitimes après reprise, les statuts natifs, la restitution des ressources et les preuves SHA-256. La réserve AArch64/P9-512 avec fixture complète reste applicable à la qualification EXEC.

**Arbitrages demandés.** Valider ou modifier la vue privée de reprise, l’autorisation de poursuivre une réservation avec le même identifiant et la politique de preuves par classe pour une ancienne instance disparue. Préciser l’autorité capable d’attester une clôture irréversible et les faits locaux dont la preuve peut survivre à la coupure. Tant que ces points ne sont pas validés, le comportement qualifié de quarantaine reste la référence.
