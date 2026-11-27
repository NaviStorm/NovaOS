# NovaOS v0.25.0 — Phase 4B — Décision durable proposée

Le catalogue hors ligne peut recalculer une sélection à partir d’une politique, d’un inventaire et d’objets immuables. La réussite de ce calcul prouve seulement l’éligibilité selon ces entrées. Un instantané ancien mais intégralement restauré peut présenter les mêmes signatures valides que lors de sa première installation.

## Point de décision

Proposer pour un mandat ultérieur une tête authentifiée unique par domaine de distribution. Elle désignerait la génération, la tête précédente, l’empreinte du catalogue complet, l’époque des clés et des révocations, le plancher de sécurité et l’identifiant de mutation. L’autorité accepterait une mutation uniquement sur comparaison de la tête attendue. La consultation par identifiant distinguerait décision confirmée, refus certain et issue inconnue. Deux auteurs partis de la même tête ne pourraient publier tous les deux.

Le provisionneur déposerait sur P9 les paquets et le nouvel index comme objets distincts et immuables. Il attendrait F1, la publication de l’ancre et F2 avant de soumettre la décision externe. Une interruption avant décision laisse des candidats orphelins. Après décision, un acquittement perdu exige de relire l’autorité. Si les objets de la tête confirmée deviennent indisponibles, le système doit retenir le diagnostic et suspendre les admissions concernées. La collecte des révisions anciennes doit tenir compte des baux et des procédures de retrait encore vivantes.

Un deuxième fichier sur le même volume ne permet pas de détecter un retour conjoint à une sauvegarde ancienne. Une autorité indépendante de ce domaine de restauration permettrait de déceler le recul. Son indisponibilité empêcherait de nouvelles admissions sensibles à la fraîcheur. Son propre modèle de restauration, ses clés, son quorum éventuel et les règles d’amorçage du disque hébergeant le catalogue relèvent d’un arbitrage distinct. L’emploi d’un TPM demanderait une preuve de liaison entre compteur, décision et contenu, ainsi qu’une politique de remplacement de machine.

## Épreuves restant à mandater

- T03 durable sur disque jetable, coupure avant et après chacune des barrières et avant et après la décision externe. Relire indépendamment le volume et l’autorité.
- Concurrence de deux installateurs, perte de réponse, tête de révocation périmée, rollback autorisé et rollback sous le plancher. Attendre au plus une publication pour une même tête.
- Restauration du disque seul, de l’autorité seule puis des deux. Consigner les cas détectables et les hypothèses externes nécessaires.
- Pour 4C, fournir des instantanés synthétiques complets et révisés par bus, les preuves de rattachement parent/groupe, une définition de la fraîcheur des inventaires et le budget d’admission réellement disponible. L’identifiant et la révision utilisés dans le plan ne certifient pas le confinement de la machine.
