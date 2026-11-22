# NovaAuth v0.24.0 — Administration R2

Point de passage du 22 novembre 2026, depuis R1 `bc6ec3e6749dc80565e7a3e7e572cb4afb11384d`, sur `codex/novaauth-administration-0240-phase1`. R2 n'est pas globalement qualifié. Aucun raccord ordinaire, fusion ni tag.

## Invariant de réactivation

La réactivation conserve l'identité stable et les données du compte. Elle permet exclusivement une **nouvelle authentification**, évaluée contre ses droits courants. Elle ne réactive aucune ancienne session, capacité, sous-délégation ni aucun ticket. L'époque clôturée reste définitivement `RETIREE`, même après succès d'une nouvelle authentification. Une tête plus récente n'est pas, à elle seule, une autorisation de remplacer l'époque d'un ancien ticket.

Le reçu terminal de désactivation et les trois acquittements réels (vérificateurs, lanceur, VFS), ainsi que l'issue de chaque lancement admis, restent nécessaires avant réactivation. Une réponse perdue, un délai ou une panne ne vaut pas acquittement. Les issues `UNCERTAIN` sont recherchées par identifiant, sans répéter une mutation.

Aucune suppression de compte, groupe ou données n'est exposée. Le refus existant de désactiver tout administrateur reste en vigueur ; aucun groupe nommé « admin » ne crée de privilège.

## Admission native liée à l'époque

Le modèle d'autorité R2 existant porte déjà un `SubjectAdmission`. Son codec Rust natif est maintenant disponible derrière `novaos-recovery/administration-lab`. La route Python n'accepte cette variante qu'avec `administration=True` explicite. Aucun banc invité ni profil existant ne l'active implicitement.

| Octets | Contenu | Encodage |
| --- | --- | --- |
| 0–8 | `NAEXAD01` | ASCII |
| 8–256 | Champs historiques : serveur, domaine, incarnation, génération, tête, instance titulaire, session, installation, empreinte ELF, ticket, opération | Même disposition que `NAEXEC01` |
| 256–272 | Identité stable du sujet authentifié | 16 octets non nuls |
| 272–280 | Époque d'authentification | u64 big-endian, strictement positive et inférieure à 2^63 |

Les champs de base restent validés par le décodeur historique. `NAEXEC01` reste strictement de 256 octets ; ni son interprétation ni ses attentes de tests ne changent. Le sujet et l'époque doivent provenir du chemin de confiance session/lanceur ; le codec ne les authentifie pas à lui seul.

Le reçu d'admission signe la position durable et les **280 octets complets**. Le reçu terminal engage leur empreinte complète et l'identité de l'enfant créé, ou une annulation démontrée. La consultation engage également un défi frais. Une signature ancienne pour une autre époque ou session ne peut pas être réutilisée. Une admission en attente reste non terminale : elle ne libère pas le drainage.

## Étape A exécutée

- Autorité Python : **26/26** tests réussis, incluant désactivation, drainage, réactivation, refus de l'ancienne époque, réponse perdue, concurrence et contrats historiques.
- Rust : **7/7** tests sur Darwin ARM64 et **7/7** sur x86_64 sous Rosetta, dont trois tests historiques inchangés. Un vecteur public émis par l'autorité Python est vérifié nativement, avec contrôle de signature et de corrélation.
- Compilation `no_std` réussie sur `aarch64-unknown-none-softfloat` et `x86_64-unknown-none`.
- Une nouvelle assertion attendait à tort la génération 9 après deux publications depuis 6 ; la génération correcte est 8. La trace négative est conservée. Aucun seuil ou contrôle historique n'a été affaibli.
- **Aucun essai QEMU R2 exécuté à ce point** : ni qualification des commandes ni preuve de révocation des handles invités. Aucun verrou QEMU acquis ; attente et exécution QEMU = 0.

Preuves et quatre exécutables de tests hôtes exacts : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/`. Manifeste de ce point : `SHA256-etape-a.txt`, **14 fichiers relus et vérifiés**, SHA-256 `bd46a8a23b59d82ef473549691ac1d24fa7567e7b22f33388ad76588f2fecc9d`.

## Raccord de compilation R2

Le profil explicite `--administration-r2` ajoute `NXLDUSR1` sur la route privée déjà authentifiée du vérificateur. Le noyau vérifie le nonce de sélection, la session, le sujet et la tête, et fige l'époque dans le ticket, y compris pour le lecteur préparé d'un pipe. Ni l'ELF ni NovaFS ne fournissent cette époque. Une réponse de portée administrative est acceptée seulement pour la même époque. Les métadonnées privées du registre passent de 29 à 32 mots ; aucun handle, racine IPC, sélecteur ou droit public n'est ajouté. Le champ `Binding.epoch` demeure la révision d'installation.

Le signataire signe les 280 octets, après validation de la taille et du couple serveur/domaine/incarnation. Les anciennes variantes restent sélectionnées hors du profil R2. Les profils ordinaires ne sont pas modifiés.

Les exports QEMU compilent pour les deux architectures : x86_64 en 24,34 s et AArch64 en 22,71 s. Les neuf contrôles ABI réussissent. **Il s'agit de compilations et de contrôles statiques, pas d'une qualification QEMU de R2.** Les journaux conservent l'avertissement de `rust-objcopy` : sa bibliothèque LLVM est absente, et l'étape facultative de réduction du binaire échoue ; les compilations se terminent néanmoins au statut 0. La configuration du Mac n'a pas été modifiée.

Les deux noyaux exacts sont conservés dans une archive privée, car ils embarquent des secrets de laboratoire. Les sept preuves complémentaires figurent dans `SHA256-raccord-admissions-01.txt` (empreinte `3999639f0d7935d2c41ac28ebd6ad2d1438d4ffa6ef0858299907483402b8bb8`). Les preuves R1 et le manifeste de l'étape A sont conservés sans modification.

## Publication administrative R2, raccord encore incomplet

Les plans administratifs réutilisent les mêmes `State/apply`, triplet et écrivain NovaFS. Le registre dérive le sujet, l'époque et le type de changement du plan autorisé ; l'ELF ne les choisit pas. Le signataire QEMU R2 dispose d'une route privée additionnelle sur le canal existant :

| Message privé | Corps signé hors préfixe | Effet |
| --- | --- | --- |
| 50 — publication | 209 octets : état 112, opération 16, nouvelle empreinte 32, principal 16, époque principale 8, sujet 16, époque cible 8, type 1 | Compare/publie la tête et les métadonnées dans la transaction existante |
| 51 — acquittement des effecteurs | 129 octets : état 112, opération 16, masque 1 | Acquittement durable, distinct du reçu terminal |
| 52 — consultation | 160 octets : état 112, opération 16, défi frais 32 | Lecture pure des acquittements et du reçu terminal éventuel |

Les identités sont des octets exacts et les compteurs réseau sont big-endian. Le signataire vérifie chaque taille et le serveur/domaine/incarnation ; la signature couvre le préfixe de séparation puis tout le corps. Le reçu de publication confirme la tête atomique historique : il ne vaut ni clôture de session ni reçu terminal `RETIREE`. La vérification de ce dernier reste nécessaire avant réactivation. Aucun nouveau sélecteur, processus ou racine IPC n'est ajouté. Les messages n'embarquent ni secret de facteur ni triplet clair.

Après un acquittement d'effecteur ou une preuve terminale de lancement, l'autorité peut progresser vers `RETIREE` **seulement** si les trois effecteurs et toutes les admissions sont déjà démontrés. Une consultation ne fait progresser aucune mutation. Le test R2 ajouté au commit 03b61 a été précisé pour exiger cette progression après une annulation irréversible, puis vérifier que le même retrait n'est pas exécuté deux fois ; les assertions historiques de `test_admission_exec` sont inchangées.

Le chemin `Pending` réutilise la préparation, la relecture P9, les reçus et la réconciliation par identifiant. Il refuse de soumettre deux fois une mutation. **Les commandes R1 continuent toutefois à refuser les mutations : leur préparation/confirmation et les acquittements réels des effecteurs ne sont pas encore raccordés.** Un succès hôte ne vaut pas un acquittement invité.

Résultats de cette étape : autorité Python **31/31** ; codecs natifs **10/10** sur Darwin ARM64 et **10/10** sous Rosetta x86_64 ; registre **8/8** par architecture avec secteurs simulés 512/4096 ; sessions **7/7** sur Darwin ARM64 ; ABI **9/9**. Les deux exports QEMU compilent. Le nouveau témoin de persistance retire un groupe pendant la désactivation, réactive avec le reçu terminal puis remonte P9 : identité stable, nouvelle époque, appartenance toujours absente et ancien grant refusé. Il ne remplace pas une authentification dans l'invité.

Archive : `SHA256-mutations-03.txt`, 33 fichiers, SHA-256 `419199ef7bf636f44b85a92a0c21d27357e4a6fb0cad4a964bde8e5946def1d3`. Les neuf exécutables de tests exacts sont sauvegardés ; les noyaux contenant les secrets de laboratoire restent dans l'archive privée. L'échec initial de préparation de l'export (chemin d'import Python absent avec `runpy`) est conservé et corrigé en utilisant le point d'entrée standard du profil. Aucun changement de configuration du Mac.

Aucun essai QEMU R2 ni nouvelle qualification des quatre ELF historiques exécuté à cette étape. Leur compatibilité R2, les ressources en exécution et les quatre profils restent à vérifier. Les délais et budgets sont inchangés.

## Raccords restant à réaliser

1. Terminer le raccord du banc à la variante d'admission R2 et à l'enrôlement des époques dans l'autorité ; exercer les refus dans l'invité.
2. Relier préparation, confirmation et consultation d'issue à la même session administrative et aux capacités courantes ; publier le triplet par NovaFS seulement après les contrôles et reçus prévus.
3. Acquitter effectivement les révocations des vérificateurs, tickets et effecteurs VFS. Un retrait de groupe doit préserver les origines de droits indépendantes.
4. Réaliser les commandes natives et la documentation ARC-026, puis la recette de réactivation sans consultation intermédiaire des anciens objets, avant leur expiration.
5. Qualifier les quatre profils QEMU et les régressions applicables, sous verrou commun, sans confondre les résultats hôtes avec ceux de l'invité.

Les réserves historiques de 301 cadres AArch64 instrumentés et de reproductibilité entre environnements sont conservées. Les utilitaires parallèles restent en pause.
