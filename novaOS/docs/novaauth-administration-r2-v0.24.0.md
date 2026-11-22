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

## Raccords restant à réaliser

1. Transporter le sujet et l'époque immuables depuis la session attestée jusqu'au ticket et à l'admission distante, sans les confondre avec la révision d'installation.
2. Relier préparation, confirmation et consultation d'issue à la même session administrative et aux capacités courantes ; publier le triplet par NovaFS seulement après les contrôles et reçus prévus.
3. Acquitter effectivement les révocations des vérificateurs, tickets et effecteurs VFS. Un retrait de groupe doit préserver les origines de droits indépendantes.
4. Réaliser les commandes natives et la documentation ARC-026, puis la recette de réactivation sans consultation intermédiaire des anciens objets, avant leur expiration.
5. Qualifier les quatre profils QEMU et les régressions applicables, sous verrou commun, sans confondre les résultats hôtes avec ceux de l'invité.

Les réserves historiques de 301 cadres AArch64 instrumentés et de reproductibilité entre environnements sont conservées. Les utilitaires parallèles restent en pause.
