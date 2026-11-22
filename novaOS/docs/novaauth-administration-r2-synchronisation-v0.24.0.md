# NovaAuth R2 — Synchronisation du témoin x86_64/P9-4096

Date : 22 novembre 2026. Branche `codex/novaauth-administration-0240-phase1`.
Base `d237d2ff15bf2d9df0db393c81ea8f8fe12ececd` ; correctif testé
`00521f5b8314949727af8df7d07af4563fa06a9c`.

## Verdict

**Un seul démarrage QEMU, échec de préparation de l'ISO, non probant pour R2.**
Aucun ticket, aucune authentification Alice/Bob et aucune révocation exercés.
Aucune répétition, aucun AArch64 ni autre scénario R2.

## Correction de synchronisation

L'ancien parcours consommait les baux dans les contrôles négatifs et attendait
15 secondes sans acquittement corrélé à la préparation d'Alice. Le profil
`--administration-ticket-vivant` sépare désormais ces contrôles ; le parcours
historique sans ce drapeau est conservé.

Après une préparation `Disable(bob)` acceptée par le registre dans la session
vérifiée d'Alice, le vérificateur envoie un événement privé unique sur sa voie de
réponse préexistante vers Bob. L'événement de 128 octets contient version/étape,
instance cible, instance Alice, identifiant et empreinte de l'intention. Bob
contrôle sa forme, authentifie normalement sa session, compare l'instance obtenue
à la cible, puis demande immédiatement le ticket. L'événement n'accorde aucun
droit. Il ne crée ni racine IPC ni session et ne remplace aucun contrôle serveur.
Le banc corrèle le marqueur à l'intention d'Alice et au PID de Bob, indépendamment
du curseur des commandes. Les opérations nécessitant une session restent après
authentification. Baux, délai TLS, ticket de 5 s, observation de 6 s et quotas
inchangés. L'attente préauthentification est bornée à 25 000 ticks et ne prolonge
aucun droit. Modifications limitées au générateur du profil QEMU et au banc.

Les 41 tests hôtes administratifs réussissent (2.751 s). Construction
x86_64 réussie (22.622 s). Avertissement de toolchain conservé :
`rust-objcopy` ne trouve pas `libLLVM.dylib` lors du retrait des informations de
débogage ; Cargo finit à zéro. Aucune preuve d'identité binaire ordinaire n'est
revendiquée par cette construction expérimentale.

## Blocage exact de l'essai

Erreur d'assemblage commise pendant cette intervention : appel à `iso.sh cli`
au lieu de `iso.sh novaauth-sessions`. Les configurations extraites des ISO sont
archivées. Dans le noyau exporté, `kernel/src/main.rs`, branche
`b"novaauth-sessions"`, seule cette valeur positionne `AUTH_SESSIONS_LAB=true`.
`cli` sélectionne aussi le mode 41, sans activer les acteurs NovaAuth. Le numéro
de mode 41 seul ne constitue donc pas un contrôle suffisant de l'ISO.

La trace montre `NVFS coeur initialise ELF tas prive`, puis PID 3 sortie native
21 (`CORRUPT` dans le protocole fichiers), suivie des sorties 11 des services
bloc/pilote/client. Le banc attend en vain `NAEXEC registre pret...` et finit en
échec. La divergence de profil est démontrée ; le sous-chemin interne exact
produisant 21 n'est pas instrumenté par cette trace. Ce résultat ne démontre ni
une corruption du volume ni un défaut du raccord synchronisé.

Zéro connexion à l'autorité. P9 et ancien disque d'autorité ont conservé leurs
SHA-256. Le drainage global de cette fermeture échouée finit à zéro endpoint,
nœud, espace et cadre IPC. Les pics de racines observés sont au plus quatre,
mais ils ne qualifient pas les acteurs R2 qui n'ont pas démarré.

## Matrice et chronométrage

[Résultats détaillés](validation/novaauth-r2-synchronisation-20261122/matrice.json).
Tous les critères ticket vivant / REVOKED avant expiration / RETIREE / drainage
R2 restent **non exécutés** sur ce candidat.

Verrou acquis `2026-11-22T11:41:06.379550+00:00`, libéré `2026-11-22T11:42:47.329910+00:00` (UTC).
Attente : 0.000355792 s ; essai complet :
100.904993 s. Aucun conflit de campagne observé.
[Durées séparées](validation/novaauth-r2-synchronisation-20261122/temps.json) :
développement/diagnostic/archivage 520.875 s,
construction 22.622 s, tests hôtes 2,751 s ;
total actif conservateur à l'archivage 647.153 s, sous 1 800 s.
Ces temps sont hôtes ; aucun calcul ne mélange les horloges hôte et invité.
Les baux R2 n'ayant pas été émis, aucune chronologie invitée de leur validité
n'est disponible pour cet essai.

## Preuves et prochaine étape soumise

Archive durable : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/synchronisation-d237d2f` ; 37 fichiers vérifiés,
SHA-256 du manifeste `0098a1460c8a38feede7098265108f0f4e6152de91f5c98312e4df380a4a1704`.
[Manifeste archivé](validation/novaauth-r2-synchronisation-20261122/SHA256SUMS-archive).
Artefacts exacts et données privées : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/artefacts-prives-administration-0240-r2/synchronisation-d237d2f` ; 19 fichiers vérifiés,
manifestes privés conservés hors Git (empreinte `c4102a7afd4e86c9b5decbfde00d3337960035dba269b6bce038f01903dcd261`).
Les secrets et les ELF contenant des clés de laboratoire ne sont pas publiés.

Avant un nouvel essai, assembler l'ISO avec `novaauth-sessions`, extraire et
vérifier son `limine.conf`, puis comparer les empreintes de l'ELF embarqué et de
l'ELF désigné au banc. Ce nouvel essai **n'a pas été lancé** : arrêt après l'échec
conformément au mandat. Les acquis antérieurs sont préservés, sans extrapolation.

Réserves historiques conservées : 301 cadres instrumentés AArch64 et
reproductibilité entre environnements. Aucun changement du noyau ordinaire,
des 48 sélecteurs ou du format P9 dans le correctif. Aucune fusion, aucun tag,
aucune activation ordinaire ; utilitaires en pause.
