# NovaAuth R2 — Profil ISO corrigé, témoin arrêté

22 novembre 2026. Branche R2. Base e27d26672139f54d09cda8a04480833e248d13ed.
Contrôle préalable ajouté au commit f2a8101d67adc920a5f4f63fcaa070819d51b8fe.
Binaires invités inchangés issus de 00521f5b8314949727af8df7d07af4563fa06a9c.

## Profil et résultat
ISO assemblée avec novaauth-sessions. Le contrôle extrait limine.conf, exige
exactement cette unique cmdline et compare le SHA-256 du noyau embarqué avec
l'artefact du banc. Contrôle réussi avant le démarrage ; preuve controle-iso.json.

Un seul essai x86_64/P9-4096 : échec, sans répétition.
Le registre récupère son triplet et le lanceur annonce sa disponibilité.
Bob PID 11 annonce son attente puis sort avec le statut natif 3.
Cause déterminée dans les sources : le raccord introduit en 00521f5 appelle
receive_for(config(17),25000). IPC_RECEIVE refuse timeout > 15000 dans
novaOS/kernel/src/sched/process/ipc/traffic.rs. Ce BAD_ARGUMENT précède
l'authentification de Bob. Il ne s'agit pas d'une expiration de son bail.
Alice lance usermod PID 21 : --disable bob --prepare retourne statut shell 3,
stderr « Administrative failure: native=5 status=3 ». Le détail interne de
ce DENIED ultérieur n'est pas établi par cette trace.

Ticket Bob valide, révocation, REVOKED avant expiration, absence de création
après révocation et reçu RETIREE : NON EXÉCUTÉS. Aucune qualification positive
de drainage R2 n'est revendiquée. Aucun changement NovaAuth ni délai effectué.
La correction de cette attente exige une étape distincte ; aucune relance ici.

## Durées et preuves
Verrou acquis 2026-11-22T15:02:18.938955+00:00, libéré 2026-11-22T15:02:53.390398+00:00 (UTC).
Attente 0.000376 s ; exécution 34.407 s.
Préparation/diagnostic/archivage à cette mesure :
117.535 s.
Total conservateur 151.943 s, sous 20 minutes ; publication suit.
Aucune durée invitée soustraite à une horloge hôte.

Archive durable : /Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/profil-iso-e27d266
37 fichiers relus et SHA-256 vérifiés. Empreinte du manifeste :
9737248c7617cb960a33059e36b96b180806f7450b20ebe3d82b2d44bcc0377a.
Artefacts privés préservés dans artefacts-prives-administration-0240-r2/profil-iso-e27d266.
Le manifeste conserve également la commande QEMU et les traces originales.
La coordination référence 00521f5 pour les sources invitées et le banc
fonctionnel, inchangés ; le nouveau contrôle préalable est identifié ci-dessus.

Aucun AArch64, autre scénario R2, fusion, tag ou activation ordinaire.
Utilitaires en pause. Réserves historiques inchangées.
