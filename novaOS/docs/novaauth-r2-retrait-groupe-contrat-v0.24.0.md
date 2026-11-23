# NovaAuth R2 — Contrat proposé de retrait sélectif de groupe

2026-11-23 — branche `codex/novaauth-administration-0240-phase1`.
Référence auditée : `82349ed9cc35a7b1bd601642cb8e0738ed1ab919`.
**Conception soumise à arbitrage, sans implémentation de ce raccord.**
La décision MOA/MOE de cette session autorise ce document et la poursuite des quatre autres fonctions R2. Aucun résultat ci-dessous ne vaut qualification QEMU.

## 1. Écart établi dans les sources

- `src/lib.rs::State::apply(Change::RemoveMember)` sait retirer une appartenance dans le triplet transactionnel. Sous `politiques-lab`, il retire aussi sa révision d'admission. Ce plan seul ne révoque pas les capacités déjà déléguées.
- `src/policy.rs` distingue `Origin::Direct` et `Origin::Group(Id)`, avec révisions de règle, d'appartenance et d'installation dans `Permission`.
- L'ancien effecteur `user/novafs/files/policy_directory.rs` vérifie ces origines et révoque les leases invalidées. Il ouvre cependant le registre avec l'autorité **locale**. Il ne doit pas être réutilisé tel quel dans le profil à registre isolé et autorité indépendante.
- `src/recuperation_view.rs::View::folder` ne fournit actuellement que les dossiers directement autorisés au titulaire. Il n'exporte pas de décision par origine de groupe aux effecteurs isolés.
- `service/src/bin/common/administration_mutations.rs` refuse encore `RemoveMember`. `administration_channel.rs` et `administration_retirement.rs` savent clôturer l'époque d'un utilisateur entier. Employer cette clôture pour un groupe supprimerait les droits indépendants et contreviendrait au contrat R2.

Chemins relatifs à `novaOS/user/novaauth/`, sauf le chemin NovaFS explicitement indiqué. L'extension concerne donc la décision privée par origine, sa propagation et son reçu terminal. Ni le nom du groupe ni le changement de génération globale ne constituent une révocation sélective.

## 2. Identités et provenance

L'origine à retirer est le tuple **(serveur, domaine, sujet stable, groupe stable, révision d'appartenance)**. Les décisions individuelles restent liées à l'installation attestée, au propriétaire du dossier, à l'objet, aux droits, aux révisions de règle et d'image et à la session authentifiée. Le registre calcule ces champs depuis l'état frais ; le CLI ne les atteste pas.

Une capacité administrative de gestion des groupes n'accorde aucun droit de lecture de données privées. La portée NXADMIN1 reste vérifiée à PREPARE, CONFIRM et OUTCOME. Aucun endpoint public supplémentaire, aucune délégation de MANAGE ou de droit d'arrêt n'est proposé.

Les droits de lecture directe et d'écriture par G restent représentés séparément. Une sous-délégation conserve exactement la provenance de son parent. Un handle ne doit pas fusionner des origines en une autorité indifférenciée : l'opération sélectionne une permission précise. Si plusieurs permissions indépendantes autorisent une opération, celles encore valides restent utilisables, sans réactiver le lease retiré.

Un nouvel ajout ultérieur au même groupe reçoit une nouvelle révision d'appartenance. Il ne réactive ni l'ancienne provenance, ni ses handles, tickets ou sous-délégations.

## 3. Transaction et points d'effet

1. **PREPARED** : intention sans effet, liée au principal, à la session, à l'instance, à la tête et à l'opération. CONFIRM revalide tous ces éléments et le rôle administratif. Le plan compare la tête et l'appartenance exacte dans la transaction faisant autorité.
2. **BLOCAGE_LOCAL** : le registre bloque les nouvelles décisions pour cette provenance et adresse aux effecteurs une fermeture préparatoire liée à l'opération. Les autres origines restent actives. Aucun succès terminal n'est rendu ici. Une interruption conserve une opération à réconcilier ; aucun rétablissement implicite des anciennes capacités.
3. **RETRAIT_EN_COURS** : l'autorité a durablement ordonné le retrait et la nouvelle tête ; toute nouvelle admission portant l'ancienne provenance est refusée. NovaFS reste le seul écrivain et publie atomiquement le triplet complet. L'acquittement de l'autorité et la publication P9 sont deux preuves distinctes ; aucune des deux ne remplace l'autre.
4. **DRAINAGE** : chaque effecteur ferme les capacités de cette origine, réveille les attentes concernées et établit l'issue des opérations précédemment admises. Les barrières existantes d'incarnation, de session et d'admission ELF restent cumulatives.
5. **RETIREE** : la tête fraîche, la publication transactionnelle et les acquittements de tous les effecteurs attendus sont démontrés et enregistrés durablement. Seul cet état autorise la réponse administrative COMPLETE.

L'ordre distant est celui des admissions durables, pas l'heure d'émission d'un message. Entre BLOCAGE_LOCAL et l'admission durable du retrait, une demande déjà transmise peut être admise par l'autorité : elle doit entrer dans le drainage. Cette fenêtre est explicite, sans promesse de révocation instantanée à distance.

Un échec de préparation n'autorise aucune mutation. Un échec après un blocage local ou une soumission peut laisser une issue UNCERTAIN ; OUTCOME consulte l'identifiant original. Une demande de restauration des droits est une nouvelle opération explicitement autorisée, pas un rollback du retrait.

## 4. Effecteurs et opérations déjà admises

- **NovaFS/VFS** : fermer les handles et sous-délégations dont l'origine correspond au tuple ; vérifier la provenance à chaque admission, y compris pour les handles ouverts avant la mutation. Réveiller les lectures/écritures en attente de cette origine avec le statut natif approprié. Ne pas invalider les handles directs ou issus de H.
- **Vérificateur et lanceur QEMU** : empêcher la création de nouvelles délégations et l'utilisation d'un ticket reposant effectivement sur l'origine retirée. Un ticket d'une installation autorisée indépendamment reste utilisable. Le retrait d'appartenance ne ferme pas automatiquement la session entière.
- **IPC** : utiliser les références générationnelles et les descendants existants. Le courtier ne décide pas des origines et n'accède pas aux contenus. Révoquer les feuilles concernées via leurs propriétaires effectifs.
- **Mutations de stockage engagées** : conserver l'identifiant, la position d'admission et l'issue durable. Une mutation déjà publiée reste traçable ; elle n'est pas annulée rétroactivement. Une annulation doit être irréversible et attestée. Une attente expirée, un effecteur injoignable ou une réponse perdue n'est jamais une preuve d'annulation.

Le registre conserve la liste authentifiée des effecteurs attendus, liée à leurs **instances**. Un masque vide n'est acceptable que si l'absence de délégation de cette origine est elle-même démontrée dans l'état d'admission contrôlé. Le redémarrage d'un effecteur ne permet pas de fabriquer l'acquittement de son ancienne instance. Tant qu'une issue reste indémontrée, le retrait reste EN_COURS et la réponse demeure UNCERTAIN.

## 5. Reçus persistants et reprise

Proposition additive au protocole privé, à valider avant code : une catégorie distincte « retrait d'appartenance », jamais assimilée au type historique UPDATE sans retrait ni au retrait d'époque utilisateur.

Corps canonique proposé de 248 octets, puis signature Ed25519 de 64 octets sous un domaine de signature distinct `NovaAuth/QEMU/membership-retired/v1` :

| Offset | Taille | Champ |
|---:|---:|---|
| 0 | 8 | Version/magic dédiée |
| 8 | 16 | Serveur |
| 24 | 16 | Domaine |
| 40 | 32 | Incarnation du registre |
| 72 | 16 | Identifiant d'opération |
| 88 | 16 | Sujet stable |
| 104 | 16 | Groupe stable |
| 120 | 8 | Révision d'appartenance retirée, u64 BE |
| 128 | 8 | Génération de mutation, u64 BE |
| 136 | 32 | Tête précédente |
| 168 | 32 | Tête publiée |
| 200 | 8 | Position durable, u64 BE |
| 208 | 1 | État : EN_COURS ou RETIREE |
| 209 | 1 | Masque des classes d’effecteurs attendues |
| 210 | 1 | Masque des classes entièrement acquittées |
| 211 | 5 | Réservés nuls |
| 216 | 32 | Défi frais de consultation |
| 248 | 64 | Signature sur domaine et corps complet |

Bits proposés des deux masques : 1 = VFS/stockage, 2 = vérificateur/délégations, 4 = admission/création ELF ; autres bits refusés. Un bit satisfait signifie que **toutes** les instances attendues de cette classe ont acquitté. Le masque satisfait doit être inclus dans le masque attendu ; RETIREE exige leur égalité et les preuves de publication. Les valeurs proposées pour l’état sont 1 = RETRAIT_EN_COURS et 2 = RETIREE ; toute autre valeur est refusée.

L'historique persistant conserve séparément les instances et acquittements détaillés : le masque compact ne remplace pas cette liste. La requête authentifiée conserve également le principal administratif et son époque, comme les mutations R2 existantes. La réponse signée au défi prouve une consultation actuelle ; une ancienne réponse signée ne suffit pas à autoriser un accès. Les identifiants de retrait ne sont jamais réutilisés.

312 octets applicatifs restent sous la capacité des pages hors P9 ; ils ne sont **pas** des métadonnées de liaison. La borne de 256 octets des métadonnées et leur représentation native actuelle restent inchangées. Fragments, version, taille exacte et réservés sont vérifiés ; aucun tronquage.

Après redémarrage : récupérer une tête indépendante fraîche, relire le triplet, retrouver l'opération et les acquittements par identifiant, réconcilier les publications déjà engagées. Ne jamais rejouer automatiquement CONFIRM. Une tête P9 ancienne, un reçu altéré, une autorité inaccessible ou un historique incomplet impose un refus sûr. Une réauthentification ne recrée aucune ancienne capacité ; elle utilise les appartenances actuelles.

## 6. Protection administrative et budget proposé

Le rôle administratif actuel est un attribut contrôlé de l'utilisateur ; appartenir à un groupe nommé « administrateurs » ne le confère pas. Conserver le refus historique de désactiver un administrateur et l'invariant transactionnel du dernier administrateur. Aucun retrait de rôle administratif, suppression de compte ou suppression de groupe n'est introduit. Si un futur rôle dépend d'un groupe, son retrait exigera une vérification du dernier administrateur dans la même transaction et un arbitrage distinct.

Réutiliser les conversations privées existantes et sérialiser la mutation en cours. Aucun nouveau processus, aucune nouvelle racine, aucun nouveau handle permanent. Budget statique hérité des deux routes :

| Acteur | Plafond pendant admission/retrait/reprise |
|---|---:|
| NovaFS | 4 racines |
| Vérificateur NovaAuth | 4 |
| Registre isolé | 4 |
| Signataire serveur 23 | 4 |
| Chaque nsh | au plus 4, authentification et admission comprises |
| Chaque signataire de session et courtier | au plus 4 |
| Chaque enfant administratif | au plus 4 |

Ce tableau est une **contrainte de conception**, pas une mesure de ce raccord. Les mesures R2 antérieures ne valident pas les nouveaux états. Les enveloppes de contrôle passent sur les feuilles déjà réservées, jamais sur les flux utilisateur. Aucun canal temporaire ne peut emprunter le quota d'un autre acteur.

Proposition d'état supplémentaire borné : une opération de retrait à la fois, corps 248 octets, deux buffers IPC de 128 octets, références des effecteurs de la famille bornée à 32 processus (identité 32 + identifiant local 8 + état 8, soit 1 536 octets), compteurs/empreintes et padding à chiffrer au layout natif. Cible de contrôle ≤ 4 Kio, **pas une hausse accordée du budget mémoire**. Les références actives sont inspectées par lots bornés sous les protections existantes ; ne pas allouer une copie de toutes les tables de capacités. Si le layout réel ou les quotas ne tiennent pas, suspendre avant raccord.

## 7. Statuts et qualification à réaliser

| Situation | Résultat attendu |
|---|---|
| Bob sans portée administrative / provenance substituée | DENIED (5), aucune mutation |
| Ancienne capacité de G après effet local | REVOKED (10), zéro accès via cette origine |
| Tête ou appartenance modifiée concurremment | CONFLICT (24), aucune seconde mutation |
| Retrait non encore démontré / réponse perdue | UNCERTAIN (22), identifiant conservé |
| Reçu terminal et publication vérifiés | OK (0), COMPLETE |
| Saturation d'une borne | LIMIT, sans fuite ; pas de succès implicite |

Les chiffres sont ceux des chemins natifs concernés ; conserver leur distinction avec les statuts CLI (notamment UNCERTAIN → 20). Ne pas convertir toute indisponibilité en REVOKED : documenter le premier statut réel. Le contrôle d'une provenance retirée doit précéder une nouvelle admission, et non être remplacé par l'expiration d'un bail.

Sur chacun des quatre profils AArch64/x86_64 × P9/512/4096 :

1. Alice lit directement, écrit par G et dispose aussi d'un droit indépendant par H ; ouvrir handles et sous-délégations avant retrait. Retirer seulement G avec Bob non administrateur comme témoin négatif. Exiger lecture directe et droits H inchangés, écriture G refusée avant expiration.
2. Retirer G pendant une écriture en attente, puis pendant une mutation déjà admise. Archiver réveil, issue exacte, reçus et drainage. Couvrir une issue UNCERTAIN sans la répéter.
3. Faire concourir deux retraits, retrait/ajout et admission/retrait. Vérifier tête, révision d'appartenance, absence de double mutation et conservation du dernier administrateur.
4. Perdre chaque acquittement et couper avant/après publication P9 et admission durable. Redémarrer registre/NovaFS et restaurer les disques locaux ; conserver l'autorité indépendante. Reprendre par identifiant, sans réactivation de G.
5. Une nouvelle authentification conserve les droits directs/H, refuse G. Un nouvel ajout explicite à G ne valide pas les anciens handles.
6. Mesurer racines, handles, endpoints, processus et cadres avant/pendant/après. La cinquième racine doit rendre LIMIT sans allocation résiduelle. Saturation et effecteur inaccessible ne doivent pas terminer artificiellement le retrait.
7. Rejouer uniquement les contrôles R1/R2, ABI et ELF réellement affectés. Verrou QEMU obligatoire ; temps d'attente séparé de l'exécution. Conserver la réserve AArch64 de 301 cadres et la reproductibilité entre environnements.

## 8. Contrat des acquittements et fermeture des courses

Un effecteur ne reçoit pas un identifiant de groupe librement choisi par l'application. Le registre produit la demande sur sa conversation authentifiée, avec l'identifiant d'opération, le tuple d'origine complet, la génération et la position durable. La réponse réutilise ces valeurs et l'identité d'instance vérifiée sur le canal. Une réponse issue d'un autre canal ou d'une instance redémarrée ne satisfait pas l'acquittement attendu.

L'effecteur installe d'abord une barrière d'admission pour cette origine, puis prend l'inventaire de ses opérations et délégations antérieures sous la même synchronisation que leur admission. Une admission qui précède la barrière entre nécessairement dans cet inventaire ; une admission qui la suit est refusée. Il ne peut donc pas déclarer une liste vide pendant qu'un autre thread ajoute une opération admise. L'enregistrement d'un nouvel effecteur exige la tête et les origines actuelles, et ne peut obtenir l'ancienne origine pendant le retrait.

Les opérations sont suivies individuellement : identifiant, origine, instance, position d'admission et état (en attente, publiée/achevée, annulée irréversiblement ou UNCERTAIN). Le reçu terminal atteste ces issues ; un simple compteur à zéro ou une absence de réponse ne suffit pas. Pour un lancement, l'issue « achevée » identifie l'enfant effectivement créé selon la barrière EXEC existante ; elle ne prétend pas que cet enfant a terminé son calcul. Ses délégations issues de G doivent cependant être révoquées. Aucun droit nouveau d'arrêt du processus n'est proposé.

Les octets déjà livrés avant l'effet de révocation ne peuvent pas être retirés de la mémoire du destinataire. Après l'acquittement terminal, aucune nouvelle lecture, écriture ou création ne peut être autorisée par G. Entre la fermeture locale du registre et l'acquittement des autres effecteurs, le client reçoit un état intermédiaire, jamais la promesse d'un retrait global achevé. Une capacité directe ou issue de H reste utilisable indépendamment.

La demande d'acquittement est une consultation/confirmation idempotente du **même** retrait, pas une répétition de la mutation d'appartenance. Après perte de réponse, la reprise interroge l'opération durable et ses effecteurs. Le retrait ne devient pas RETIREE à l'échéance d'un timeout. Les attentes IPC respectent la limite existante de 15 000 ticks par appel ; une nouvelle attente bornée recontrôle l'état et les révocations. Un effecteur durablement injoignable laisse le retrait incomplet ; toute nouvelle procédure de neutralisation exige un arbitrage.

La protection du dernier administrateur est évaluée sur la tête courante dans la transaction de mutation, pas seulement à PREPARE. Deux changements concurrents ne peuvent chacun se baser sur l'autre administrateur encore présent : une seule publication peut comparer avec succès la même tête, puis le perdant revalide l'invariant sur la tête suivante. Ce contrat n'autorise aucune suppression définitive de compte, de groupe ou de données.

## 9. Arbitrage demandé

Valider la provenance portée jusqu'aux effecteurs isolés, les états et points d'effet ci-dessus, la catégorie et le reçu privés additifs, puis le budget natif mesuré avant raccord fonctionnel. Aucun de ces changements n'est implémenté par ce document. La CLI continue de refuser le retrait actuellement non démontré. Les quatre autres fonctionnalités R2 poursuivent leur qualification indépendamment.
