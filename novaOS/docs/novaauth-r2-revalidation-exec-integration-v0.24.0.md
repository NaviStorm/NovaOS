# NovaAuth R2 — intégration partielle de la revalidation EXEC

État du 23 novembre 2026. Branche `codex/novaauth-administration-0240-phase1`.
Mandat approuvé à partir de `b02a9ebb5f65a0d9a41f505049816e1298e8130d`, budget de 120 minutes actives hors attente du verrou.

**R2 n'est pas terminé.** Deux périmètres supplémentaires sont qualifiés dans les quatre profils QEMU : annulation VFS avant GO après perte de réponse d'admission, et lancements de tickets Direct neufs via le descripteur sélectif. Ils ne démontrent pas encore la conservation d'un ancien ticket Direct/H après mutation, ni la révocation d'un ticket EXEC réellement dépendant de G.

## Sources et périmètres

| Élément | Référence exacte |
|---|---|
| Base et contrat approuvé | `b02a9ebb5f65a0d9a41f505049816e1298e8130d` |
| Autorité sélective, codec et politique | `7b48262e8735ff4c0615e8e3fba10974ee2a72db` |
| Annulation VFS réelle | `7a0fc187413ba978a8d693477ac2b99399744e88` |
| Code des invités Q2 | `db98a7ec57be94e23e0b9c69d83821147564b156` |
| Banc Q2 retenu | `fc77f314f1b884b767b1a904ef6a0b5e58e9aeae` |
| Code des invités EXEC Direct | `515626408b382ad212a53de8fdee17670e0b4bf4` |
| Banc EXEC Direct retenu | `d6fe83a055c7a35995b220ff2ff013b3e59b8472` |

Les sources compilées ont été comparées à un export propre du commit : 713 fichiers Rust/TOML/linker examinés. Deux écarts initiaux concernaient le modèle non compilé de la pompe (sa copie réellement compilée était déjà identique) et un fichier de tests natifs ; ils sont explicités dans `identite-source-selectif-02.json`. Les tests natifs finaux utilisent l'export propre. Les ISO sont vérifiées par extraction de `limine.conf` et du noyau, avec profil `novaauth-sessions` et empreinte identique du noyau incorporé.

Le prototype initial non qualifié reste hors des commits et hors des exports utilisés. Ses dix fichiers ont été sauvegardés ; neuf restent strictement identiques dans le worktree. `distribution.rs` conserve intégralement son contenu initial et reçoit seulement l'ajout committé du module privé `execution_identity`. Ne pas utiliser le worktree sale comme export de qualification.

## Raccord effectivement réalisé

Le registre construit un descripteur canonique immuable à partir de la session vérifiée, des métadonnées natives et du catalogue attesté. L'autorité indépendante enregistre son identité avant remise du ticket. Cette inscription n'admet aucun processus. Le noyau lie une seule fois son identifiant à la porte d'admission encore dormante.

À la consommation, le registre consulte une tête fraîche, retrouve le descripteur signé, vérifie ses liaisons et les conditions propres à l'installation. L'autorité vérifie dans une seule transaction la tête courante, l'incarnation, l'époque utilisateur et les barrières des dépendances, puis inscrit l'admission unique. Le résultat natif identifie l'enfant ou une annulation irréversible ; une réponse perdue conduit à consulter le résultat.

Le raccord invité actuel couvre exclusivement le rôle Direct sans délégation VFS. Les autres rôles et portées conservent le contrôle historique strict de tête globale. Aucun allègement ne repose sur une déclaration du CLI. L'inscription privée utilise les RPC 62–65, activés explicitement seulement dans le profil expérimental. Les messages IPC restent de 128 octets. Le descripteur contient 584 à 800 octets, dans le transport applicatif borné ; il ne remplace pas les métadonnées de liaison.

Le codec, la politique et l'autorité savent représenter les dépendances Direct/G/H, mais le raccord invité n'invente aucune provenance G/H manquante. Toutes les dépendances effectivement nécessaires doivent être satisfaites ; une provenance G retirée n'est pas remplacée silencieusement par H.

## Correction Q2 : annulation avant GO

Le défaut était une confirmation durable `Cancelled` possible après perte de la réponse d'admission, alors que la requête restait encore dans la file de NovaFS. La correction lie l'annulation locale à la session, au tag et à l'empreinte complète du message en attente. NovaFS neutralise cette requête avant GO et conserve un résultat consultable. Le registre vérifie ce résultat avant la confirmation durable. Aucun timeout ne vaut acquittement, aucune seconde admission ou E/S n'est émise.

Cette recette annule G avant tout octroi. Elle exige donc zéro bail G fermé lors du retrait ultérieur, avec preuve préalable de l'annulation durable ; l'oracle historique Q1 continue d'exiger le retrait d'un bail G réellement ouvert. Les attentes historiques n'ont pas été changées pour accepter un échec.

## Résultats QEMU nouveaux

| Profil | Q2, admission perdue avant GO | EXEC Direct neuf, sans E/S VFS |
|---|---|---|
| x86_64/P9-512 | Réussi, `annulation-05` | Réussi, `direct-03` |
| x86_64/P9-4096 | Réussi, `annulation-01` | Réussi, `direct-01` |
| AArch64/P9-512 | Réussi, `annulation-01` | Réussi, `direct-01` |
| AArch64/P9-4096 | Réussi, `annulation-01` | Réussi, `direct-01` |

Les noms complets, commandes, SHA des artefacts, décisions individuelles, horodatages et acquisitions/libérations du verrou sont dans la matrice jointe.

Q2 : admission de G à la position durable 6, annulation terminale à la position 7, statut natif `CANCELLED (9)`, zéro octet de G. Avant retrait, Direct/H réussissent ; après retrait, G rend `REVOKED (10)` et zéro octet tandis que Direct/H réussissent. Le retrait de génération 31 devient `RETIREE` après les trois acquittements distincts des classes 1, 2 et 4. Les deux admissions H ont chacune une issue durable ; l'admission et l'issue de G ne sont pas répétées. La fermeture des sessions et le drainage global sont démontrés.

EXEC Direct : trois commandes administratives sont exécutées par de vrais ELF. Chacune possède un descripteur durable, une admission et un enfant distinct. Les ordres observés sont inscription/admission/issue 6/7/8, 9/10/11 et 18/19/20. La troisième commande utilise un **nouveau** ticket après mutation. Ce résultat ne qualifie pas l'utilisation après mutation d'un ticket conservé auparavant.

## Matrice Q1–Q4 et EXEC restante

| Garantie | Preuve disponible | Limite restante |
|---|---|---|
| Q1 VFS Direct/G/H réels | Campagnes antérieures conservées ; Direct/H et refus G aussi observés dans Q2 | Ne vaut pas preuve EXEC G/H |
| Q2 annulation de requête en file avant GO | Nouvelle recette réussie sur quatre profils | Les autres annulations PREPARE et les admissions EXEC G en vol restent à raccorder/qualifier |
| Q3 retrait concurrent et admission VFS | Entrelacement antérieur vérifié, mode sans nouveaux descripteurs | Pas de nouvelle recette Q3 invitée avec anciens tickets sélectifs ; concurrence de l'autorité testée sur hôte |
| Q4 retrait réservé interrompu | Refus sûr antérieur, aucune activation ni ACK inventé | Reprise complète de disponibilité après perte de l'ancien effecteur non qualifiée |
| EXEC Direct neuf | Nouvelle recette sur quatre profils | Ancien ticket traversant la mutation non exécuté |
| EXEC G et H, provenances multiples | Tests hôtes et natifs | Raccord aux délégations G/H réelles et preuve QEMU non réalisés |
| Réponses perdues, rejouements et annulation EXEC sélective | Tests de l'autorité et codec natif | Injection de ces pertes sur le raccord invité sélectif non exécutée |

Les 443 fichiers du dossier précédent ont été vérifiés contre leur manifeste, sans modification. Leur applicabilité reste limitée à leurs sources et modes : les essais sans schéma sélectif ne qualifient pas son activation. Les nouvelles fonctions d'autorité sont optionnelles ; les branches historiques gardent leurs contrôles. Aucun cumul de résultats hôtes n'est présenté comme qualification de l'invité.

## Mesures mémoire, IPC et TLS

Le sous-total statique passe de **3 994 à 4 026 octets**, soit **70 octets de marge** sous 4 096. La seule augmentation persistante de la pompe est l'empreinte de 32 octets. Sa taille passe de 296 à 328 octets. Mesure native sur AArch64 Darwin et x86_64 Rosetta ; assertion de borne également compilée sur les deux cibles invitées. Ce sous-total n'inclut ni tout le code ELF ni les piles TLS.

| Profil | Pages signataire/registre, Q2 | Pages signataire/registre, Direct | TLS initial mesuré Q2 / Direct |
|---|---:|---:|---:|
| x86_64/P9-512 | 69 / 156 | 73 / 163 | 2 960 / 2 968 ms |
| x86_64/P9-4096 | 69 / 156 | 73 / 163 | 2 968 / 2 940 ms |
| AArch64/P9-512 | 61 / 153 | 65 / 160 | 3 552 / 3 556 ms |
| AArch64/P9-4096 | 61 / 153 | 65 / 160 | 2 860 / 2 848 ms |

Les valeurs résidentes avant/après sont identiques dans chaque essai. Le profil Direct ajoute quatre pages au signataire et sept au registre par rapport à Q2 ; cette hausse d'empreinte résidente ne doit pas être confondue avec les 32 octets de contrôle statique. Les segments ELF chargeables du registre passent de 152 à 159 pages sur AArch64 et de 155 à 162 sur x86_64 : ils expliquent ses sept pages supplémentaires. Les quatre pages supplémentaires du signataire sont mesurées, sans attribution fine à chaque section dans cette campagne.

Les pics observés sont de quatre racines par acteur, huit handles au registre et six au signataire. Le registre revient à sept handles, le signataire à six, sans attente dans les mesures terminales. Le pic des cadres IPC globaux observés est de 66 ; les instantanés avant/après des services indiquent 31. Les journaux détaillent les PID, handles, pages, piles et threads. Ces pics sont les maxima des points instrumentés, pas une mesure continue de toutes les allocations. Le refus de la cinquième racine est conservé.

Le délai TLS absolu de 10 secondes reste imposé. Seuls les échanges effectivement horodatés sont mesurés ; les horloges hôte et invité ne sont jamais soustraites entre elles. Les réserves historiques de 301 cadres instrumentés AArch64 et de reproductibilité entre environnements restent ouvertes.

## Tests hors invité et compatibilité

- 83 tests hôtes de l'autorité et du banc réussis, dont 18 nouveaux contrôles du descripteur/admission sélective : dépendances, barrières, rejouements, concurrence, signatures, versions, bornes et perte de réponse.
- Codec : 11 tests ; politique : un test couvrant les deux architectures et deux géométries ; porte native : six tests, exécutés pour chaque architecture native. Les résultats finaux utilisent l'export propre.
- Effecteurs et file VFS : 24 tests par architecture sur sources propres non transformées ; le harnais applique lui-même son profil.
- Harnais mémoire/coordinateur : 16 tests par architecture, avec clé publique synthétique correspondant aux reçus du harnais.
- Neuf contrôles ABI réussis ; 48 sélecteurs inchangés. Les quatre ELF historiques ne sont pas rejoués dans cette campagne : aucune nouvelle preuve de leur exécution n'est revendiquée.
- Le module de porte modifié n'est inséré dans le noyau que par le générateur du profil privé. Aucun profil ordinaire n'est activé. Les noyaux ordinaires ne sont pas reconstruits ici ; aucune identité binaire nouvelle n'est prétendue.

## Échecs conservés et corrections

1. Q2 `annulation-01` : genèse préparée avec une ancienne empreinte de nsh ; refus natif 5 avant l'admission. Genèse reconstruite avec l'ELF réellement incorporé.
2. Q2 `annulation-02` : annulation native réussie, puis borne de 128 connexions consommée par l'opération supplémentaire. La recette annule désormais l'opération G initiale ; aucune hausse de quota.
3. Q2 `annulation-03` : absence de bail G après annulation correcte, mais oracle du retrait d'un bail existant. Recette distincte avec preuve durable de non-octroi ; oracle historique conservé.
4. Q2 `annulation-04` : import hôte absent dans la copie isolée ; QEMU non lancé. Copie remplacée par un export complet.
5. Direct `direct-01` x86_64/512 : admissions sélectives réussies, puis saturation à 128 connexions dans la recette combinée VFS. Séparation des périmètres ; recette Direct à 96 connexions, sans hausse de borne.
6. Direct `direct-02` : le banc enrôlait les époques avant l'ancien RPC 40 ou l'admission VFS 57, mais pas avant l'inscription 62. Refus sûr 5 par l'autorité. L'enrôlement existant est maintenant déclenché avant 62 ; aucune autorisation implicite ajoutée au serveur.

Les premiers essais de mesure mémoire ont échoué faute de variable de clé publique, puis avec une clé distincte de celle des reçus synthétiques. Les journaux sont conservés ; la mesure finale utilise la clé attendue. Un premier essai du harnais d’effecteurs sur un export déjà transformé appliquait deux fois les champs du profil ; il a été exécuté ensuite sur l’archive propre non transformée, avec 24 réussites par cible. Les erreurs de préparation/compilation sont archivées et ne sont pas comptées comme essais QEMU réussis.

Un champ descriptif des anciens rapports Q2 indiquait `delegations_G_ouvertes=true` du seul fait de l'activation des E/S. Dans la recette d'annulation, aucune délégation G n'avait été octroyée : les preuves d'annulation 6→7 et la trace `retires=00 conserves=02` font foi. Le champ est corrigé pour les prochains rapports ; les preuves originales restent intactes.

## Suite nécessaire

1. Conserver un ticket Direct puis H avant une mutation de G et démontrer leur consommation après cette mutation, avant expiration, dans l'invité.
2. Raccorder les provenances G/H réellement déléguées, y compris le lecteur dormeur, au même descripteur immuable ; ne pas activer le prototype initial.
3. Qualifier les tickets G révoqués, les dépendances multiples, les annulations PREPARE et les courses d'admission avec leurs issues individuelles.
4. Reconstruire un retrait interrompu depuis l'historique signé en conservant les barrières et sans déduire l'annulation d'une injoignabilité ; qualifier la reprise froide complète.
5. Compléter les pertes de réponse et substitutions du nouveau raccord dans les quatre invités, puis mener l'audit de clôture.

Les branches parallèles, le worktree principal et les utilitaires en pause sont préservés. Aucune fusion, aucun tag, aucune activation ordinaire. L'état publié est intermédiaire ; aucune recommandation de clôture R2.

## Durées, coordination et archives

Début : 2026-11-23T18:24:24Z. Arrêté des mesures : 2026-11-23T20:10:34.217930+00:00.

Attente du verrou : **0.004860 s** ; exécution des bancs : **523.547 s**, dont **523.468 s** pour les treize essais ayant lancé QEMU. Le quatorzième arrêt est un défaut d'import avant lancement. Huit essais QEMU réussissent et cinq échouent, avec traces conservées.

Travail hors fenêtres de banc : **5846.666 s** ; temps actif écoulé : **6370.213 s**. Ce poste regroupe développement, diagnostic, compilations et tests hôtes. Une partie du diagnostic chevauche QEMU : sa durée totale n'est pas mesurée séparément avec exactitude. La publication documentaire suit cet arrêté dans le même budget.

Les quatorze acquisitions et libérations sont consignées ; tous les verrous propres sont libérés. Aucun conflit de planification ni verrou forcé. Les durées monotones invitées restent séparées des durées hôtes.

Archive locale : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/revalidation-exec-b02a9eb-20261123.tar.gz`. **739 fichiers de preuve**, plus le manifeste ; relecture intégrale vérifiée.

SHA-256 archive : `df323cf08078a9bd9e8fe3753c8b89cd3ba1a913b6e8ace609ccc0914ed83dcd`.

SHA-256 manifeste : `e1c32ec7e3c2b31660cba993ead9e25070cfac752b3faae5a870e1a213c2034b`.

[Matrice détaillée](validation/novaauth-r2-revalidation-exec-integration-v0.24.0/resultats-consolides.json) · [Métadonnées de l’archive](validation/novaauth-r2-revalidation-exec-integration-v0.24.0/archive.json) · [Manifeste](validation/novaauth-r2-revalidation-exec-integration-v0.24.0/MANIFEST.sha256).

Recherche bornée des 97 secrets connus de ces essais et des marqueurs PEM : aucune occurrence. Elle ne prouve pas l’absence de tout secret inconnu. Les clés privées, ISO et ELF restent hors de l’archive publique ; leurs empreintes sont conservées.
