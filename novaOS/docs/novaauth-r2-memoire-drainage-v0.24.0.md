# NovaAuth v0.24.0 — Mémoire et drainage sélectif R2

État du 23 novembre 2026. Branche `codex/novaauth-administration-0240-phase1`.
Code mémoire/Q2 : `e0b246cee517067670fc6d17f7893e2e6269578f`.
Code du témoin concurrent : `6e589b45f920505d76c552a716e84409fe1d40a1`.
Arbre : `5750e97997fd6fe40062a3394cda266201600dc8`.

**Qualification partielle.** La représentation compacte et trois témoins ciblés
sont validés sur les quatre profils. Les tickets EXEC réellement issus de G,
les autres courses Q3 et la reprise froide d'un retrait incomplet restent ouverts.
Ce rapport ne clôture ni la révocation sélective ni R2.

## Représentation mémoire et acquittements

L'autorisation MOA/MOE de compaction est appliquée. Les messages privés restent
longs de **128 octets**. Les réservés sont vérifiés avant stockage ; la
reconstruction restitue exactement les octets utiles et les zéros canoniques.
Un réservé non nul est refusé, jamais tronqué. Les états réels mesurés sont :

| État | Octets sur ARM64 et x86_64 |
|---|---:|
| Sous-total hérité, inchangé | 3 008 |
| Option du retrait sélectif | 304 |
| Reçu terminal conservé | 224 |
| Admission VFS individuelle | 114 |
| File/contexte NovaFS compact | 224 |
| Objets des leases VFS | 96 |
| Références du shell | 24 |
| **Sous-total statique additionnel** | **3 994 / 4 096** |

Le gain est de **88 octets** par rapport aux 4 082 antérieurs. La marge est
102 octets. Les compilations invitées des deux architectures vérifient les
assertions sur les types réels ; les mesures Darwin ARM64 et x86_64/Rosetta
restent identifiées comme tests natifs, distincts des essais QEMU.
Ce budget n'inclut ni les piles TLS ni le tas P9 ; leurs mesures invitées sont
présentées séparément. Aucun budget n'est augmenté.

La classe 1 garde l'identité NovaFS ; les classes 2 et 4 partagent l'identité
commune du vérificateur. Elles conservent **trois demandes EFFECT**, trois classes,
trois enregistrements durables et trois bits d'acquittement. Un ACK classe 4 ne
valide pas la classe 2. Les preuves restent liées à l'opération et à l'instance.
Les tests exercent l'ordre 4,1,2, le doublon, la substitution d'instance/classe,
l'opération étrangère, une autre clé signataire et la reprise d'un masque partiel.
La fermeture EXEC qualifiée vérifie les tickets indépendants : elle ne constitue
pas encore une preuve de révocation d'un ticket réellement issu de G.

## Q2 : issues individuelles et drainage

Le défaut traité était la perte de l'issue locale après une réponse ambiguë :
les octets de requête étaient consommés sans conserver le fait nécessaire à la
réconciliation. La file conserve désormais le résultat et son fait corrélé dans
les mêmes buffers compacts. `NXMGRES1` consulte ce fait ; `NXMGREL1` libère l'état
de façon idempotente et ne constitue jamais une preuve de réussite.

Le registre garde l'empreinte de l'admission originale avant son émission et
vérifie la signature, le défi et cette empreinte lors de QUERY. Une phase indique
si aucun GO n'a été envoyé, si l'effet VFS est possible, ou si FINISH a déjà été
tenté. Une issue inconnue reste `UNCERTAIN (22)`. La réconciliation ne réémet ni
ADMIT ni l'E/S ; le premier FINISH ne peut être envoyé qu'avec un fait établi.
Après un FINISH déjà tenté, seule la consultation de son issue est permise.

Deux témoins invités sont qualifiés :

- **FINISH durable, réponse perdue** : retour UNCERTAIN puis consultation de la
  même opération ; une seule E/S et un seul FINISH, Direct/H disponibles ensuite.
- **Admission G en vol** : l'E/S réelle précède le retrait, son premier FINISH est
  retardé. L'ordre durable vérifié est **ADMIT G < RESERVE < FINISH G < PUBLISH**,
  puis trois ACK et RETIREE. L'issue est réconciliée avant fermeture et publication.

Cela ne qualifie pas encore l'annulation d'un PREPARE bloqué ni tous les réveils.
La perte d'ADMIT avant GO reste à exercer : la cancellation distante ne suffit
pas à prouver la restitution de la requête déjà en file dans NovaFS. Ce chemin
ne doit pas être déclaré complet sur la seule réussite du FINISH perdu.

## Q3 : deux confirmations de la même intention

Deux ELF `admin-sonde` distincts sont exécutés par le vrai `nsh` et synchronisés
par un pipe. Les deux PREPARE sont en vol, puis chaque ELF émet une seule
confirmation. L'intention est déterministe pour la même requête, session et image :
les deux identifiants préparés doivent être identiques et non nuls.

Résultat exact : **un succès 0 et un DENIED 5**, une publication, trois ACK,
RETIREE. Le second appel ne retrouve plus la préparation consommée. Deux PID
(21 et 22 dans les traces), deux statuts d'ELF 0, stdout **192 octets** contrôlés,
stderr vide et statut du pipe 0. Ces statuts d'ELF désignent la réussite de la
sonde ; le refus administratif natif 5 figure séparément dans son résultat.

Le premier essai Q3 attendait à tort un conflit/révocation : il est conservé en
échec. Le nouvel oracle impose l'identité de l'intention, le reçu canonique du
succès, les zéros du refus et exactement les statuts [0,5]. Il refuse deux succès,
deux refus, une autre opération, une troncature ou un statut différent.
Il ne s'agit **pas** d'une qualification de deux mutations distinctes ni de la
course entre retrait et nouvelle admission G.

## Matrice et applicabilité

La [matrice détaillée](validation/novaauth-r2-memoire-drainage-v0.24.0/resultats.json)
contient chaque commande, statut natif, décision VFS, admission durable,
acquittement, mesure et SHA-256 de journal.

| Périmètre | x86_64/512 | x86_64/4096 | AArch64/512 | AArch64/4096 |
|---|---|---|---|---|
| Q1 VFS Direct/G/H réel, réexercé | Réussi | Réussi | Réussi | Réussi |
| Q2 FINISH perdu, candidat compact | Réussi | Réussi | Réussi | Réussi |
| Q2 admission G en vol, drainage | Réussi | Réussi | Réussi | Réussi |
| Q2 annulation PREPARE / autres réveils | Non qualifié | Non qualifié | Non qualifié | Non qualifié |
| Q3 double confirmation de même intention | Réussi | Réussi | Réussi | Réussi |
| Q3 retrait contre nouvelle admission | Non exécuté | Non exécuté | Non exécuté | Non exécuté |
| EXEC réellement dépendant de G | Non raccordé | Non raccordé | Non raccordé | Non raccordé |
| Q4 PUBLISH/ACK perdus, ACK absent, froid après RETIREE | Historique conservé | Historique conservé | Historique conservé | Historique conservé |
| Q4 froid d'une opération incomplète | Bloqué | Bloqué | Bloqué | Bloqué |

Les **12 recettes finales** réussissent. La campagne conserve **20 essais :
16 réussis, 4 échoués**. Les quatre premières réussites FINISH perdu utilisaient
la représentation précédente ; elles sont conservées mais remplacées par les
quatre essais `q2-finish-10` pour qualifier la représentation compacte.

Sources précises :

- Q2 drainage : `60045b41f01e12ffbcf3823039e9d5be29404f05` ; les changements vers
  `8cc96199fd1293ab7294e38d48d04d3e4cdafbf0` portent uniquement sur les tests et
  assertions de taille, diff intégral archivé.
- Q2 FINISH perdu final et Q3 : noyaux/services compilés depuis `8cc96199...`,
  de même arbre que `e0b246c`. Leurs sources compilées sont inchangées dans
  `e63d3f237ae932558f67adcee42a41b2144735e7`, de même arbre que `6e589b4`.
- La nouvelle sonde Q3 et le banc viennent de `e63d3f2...`. Le paquet signé de
  la sonde est installé dans la fixture ; il n'est pas confondu avec le noyau.
- Chaque ISO `novaauth-sessions`, noyau, registre, shell, sonde et fixture possède
  son empreinte effectivement relue. Les ISO/ELF privés restent conservés hors
  des preuves publiées, car ils incorporent des secrets de laboratoire.

Entre les constructions 04 et 07, les SHA de `nsh` sont identiques ; ceux des
noyaux et registres diffèrent. Les tailles de sections texte sont identiques mais
leurs octets et tables de symboles diffèrent. L'applicabilité de Q2 drainage repose
sur le diff de sources limité aux tests/assertions, **pas sur une identité binaire**.
La réserve de reproductibilité entre environnements n'est pas levée.

Les preuves Q4 historiques de `cddbaaf` sont conservées dans leur périmètre daté,
sans les présenter comme de nouveaux essais sur cet arbre. Les 52 parcours
historiques, les quatre ELF historiques et les 36 scénarios de sécurité ne sont
pas relancés : aucune nouvelle qualification globale de ces suites n'est revendiquée.

## Tests hôtes, natifs et compatibilité

- Autorité sélective : 25 tests ; profil/collecteur/oracle Q3 : 9 tests réussis.
- Effecteurs/file compacte : **22 tests par architecture native**.
- Coordinateur/disposition mémoire : **15 tests par architecture native**.
- Réponses signées de récupération : **8 tests par architecture native**.
- Deux constructions invitées réussies ; **9/9 contrôles ABI** sur les sources,
  l'export expérimental, puis l'arbre final. Les attentes historiques ne changent pas.
- Les sources du noyau ordinaire, de l'ABI et du moteur NovaFS ne sont pas modifiées.
  Les adaptations de service sont appliquées uniquement à l'export QEMU privé.

Les tests de coordinateur et d'inventaire EXEC utilisent des doubles explicités ;
ils ne constituent pas des faits d'exécution de tickets G dans l'invité.
Les erreurs de préparation des harnais sont archivées : export déjà transformé,
versions `rustc`/`rustdoc` différentes, clé publique du harnais différente de son
signataire fictif. La correction de ce dernier point ne modifie aucune clé QEMU.

## Mesures invitées

Chaque essai final termine avec zéro endpoint, nœud, espace, thread et cadre IPC
temporaire. Racines/handles/attentes des services reviennent à leur référence,
avec au maximum **quatre racines par acteur** ; la cinquième est refusée.
Les états persistants et caches résidents sont distingués des ressources temporaires.

| Compteurs observés dans Q3 | x86_64 (deux géométries) | AArch64 (deux géométries) |
|---|---:|---:|
| Pic résident global, cadres | 3 044 | 2 992 |
| Pic retenu global, cadres | 3 044 | 3 461 |
| Minimum de cadres libres observé | 59 207 | 58 690 |
| Registre, pages avant/après | 156 / 156 | 152 / 152 |
| Signataire, pages avant/après | 69 / 69 | 61 / 61 |
| Pile registre et signataire, pages chacun | 64 | 64 |
| Racines / handles registre avant/après | 4 / 7 | 4 / 7 |
| Racines / handles signataire avant/après | 4 / 6 | 4 / 6 |

Ce sont les extrema des points instrumentés, pas une mesure continue de toutes
les allocations. Les autres recettes ont leurs propres valeurs dans la matrice.
Les échanges TLS Q3 horodatés durent 2 872 à 3 460 ms sur l'horloge invitée,
sous la limite inchangée de 10 s. Aucun horodatage hôte n'est soustrait à une
échéance invitée. La réserve historique de **301 cadres instrumentés AArch64**
reste séparément ouverte.

## Échecs conservés et corrections

1. Q2 FINISH-01 : contrôle privé de stockage non raccordé au nouveau message de
   consultation ; refus DENIED, raccord du dispatch privé sans droit supplémentaire.
2. Q2 drain-03 : le collecteur ne reconnaissait pas la phase `en-vol` ; parseur
   corrélé à la phase et à la révision complète, test hôte ajouté.
3. Q2 drain-04 : marqueur RETIREE supprimé par le quota DEBUG existant. Suppression
   d'une trace redondante de tête uniquement dans l'export sélectif ; signature,
   fraîcheur, quota et oracle terminal inchangés.
4. Q3 double-08 : oracle ne décrivant pas la consommation d'une même préparation ;
   diagnostic et nouveau témoin explicitement borné à cette sémantique.

## Limites bloquantes

1. **EXEC G** : les installations résolues sont personnelles. Le prototype conservé
   ajoute une restriction de lecture du paquet, qui ne démontre pas une autorisation
   d'exécution réellement issue de G. Il n'est pas publié comme code actif.
   Ses +24 octets par ticket et admission ont été mesurés sur les deux architectures,
   mais la cardinalité totale et la provenance effective restent à démontrer avant
   intégration sous 4 096 octets. `Ticket.scope` reste un scope de fichiers.
   En outre, le code publié exige encore une égalité entre la tête globale
   conservée dans le ticket et la tête fraîche. Une mutation G peut donc faire
   annuler aussi un ancien ticket personnel : sa conservation en mémoire ne
   prouve pas sa consommation ultérieure. Ce diagnostic statique est consigné
   dans [le contrôle de tête](validation/novaauth-r2-memoire-drainage-v0.24.0/diagnostic-tickets-independants.json).
   La revalidation ciblée proposée reste non intégrée et non qualifiée.
2. **Q2** : annulation avant GO, PREPARE bloqué et réveils restent à qualifier avec
   restitution de la file locale, en plus de l'issue durable indépendante.
3. **Q3** : nouvelle admission simultanée au retrait, mutations distinctes et
   retrait ultérieur restent à exercer sans réactiver les anciennes barrières.
4. **Q4 non terminal** : `guard_activation` interdit la nouvelle incarnation
   tant que l'issue d'une ancienne admission manque. Une réservation signée ou un
   redémarrage ne prouve ni annulation ni collecte. L'ancien effecteur ne peut être
   remplacé silencieusement dans l'ACK. Le système reste UNCERTAIN et inactif ;
   toute procédure supplémentaire de neutralisation probante doit faire l'objet
   d'un contrat et d'un arbitrage avant implémentation.

Les modifications locales du prototype sont préservées, ainsi que leur diff et
leurs nouveaux fichiers dans l'archive. Elles sont exclues des commits de code validé.
Aucune fusion, aucun tag, aucune activation ordinaire. Les utilitaires restent en pause.

## Durées et preuves archivées

Arrêté des mesures : 2026-11-23T17:32:50.745087+00:00. Début : 2026-11-23T16:01:08+00:00.
Attente du verrou : **0.007357 s** ; exécution effective QEMU :
**626.882 s** ; développement, diagnostic, compilations, tests
natifs et documentation hors banc : **4875.856 s**.
Les 20 acquisitions/libérations sont horodatées individuellement dans la matrice.
Aucun conflit de planification observé et aucun verrou forcé. La préparation
finale de publication suit cet arrêté, dans le budget de 120 minutes.

Archive durable : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/finalisation-selective-cddbaaf-20261123.tar.gz`.
**1111 fichiers de preuve**, plus le manifeste, relus intégralement
dans l'archive. SHA-256 : `0cd8f95d16fce1039c713d171ca6d76dfca183d80f42094db10385d8b63a9964`.
Manifeste : `bf7c49e3ded4160b08bacf809344698e3e87804d8a9a561575b516c53754800d`.

[Métadonnées de l’archive](validation/novaauth-r2-memoire-drainage-v0.24.0/archive.json) · [Manifeste](validation/novaauth-r2-memoire-drainage-v0.24.0/MANIFEST.sha256).

Les neuf instantanés de sources sont conservés dans un pack Git vérifié,
avec la base publiée requise. Les 72 artefacts privés ont été relus et leurs
SHA comparés à ceux des essais. Aucune clé connue ni marqueur PEM privé n’a été
trouvé dans les preuves ; ce contrôle borné ne prouve pas l’absence de tout secret
inconnu. L’archive historique de 2 019 preuves a été relue et reste inchangée.
