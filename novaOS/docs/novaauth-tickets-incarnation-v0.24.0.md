# NovaAuth v0.24.0 — Tickets ELF et retrait d’incarnation

Audit du 18 novembre 2026, base `96b4f67cdbaab8048074a6c1a218434801734f67`.
Branche `codex/novaauth-autorite-0240`. Le scénario deux VM et le rejeu après
redémarrage sont approuvés. Le présent constat ne prétend pas les avoir exécutés.
[Empreintes des sources examinées](validation/tickets-incarnation-audit-v0.24.0.json).

## Frontière observée

`exec::Ticket` vit dans `Process.exec`, avec propriétaire, émetteur, session,
installation attestée, époque, empreinte, échéance et état. Le numéro local
n’est ni un objet persistant ni une preuve transférable entre VM.

Dans le profil de lancement, `consume` appelle `session_relay::expire`, vérifie
`owns_session` et `validate`, puis prépare et publie l’enfant. Ces vérifications
utilisent l’état local. `owns_session` consulte la route et son échéance ;
`validate` contrôle les capacités locales, l’instance et l’échéance du ticket.
Les invalidations explicites passent notamment par `end_session`, `retired`
et `registry_fence`. Aucun de ces contrôles ne consulte à lui seul la tête de
l’autorité indépendante lors de la consommation. Une décision durable prise
par l’autorité ne modifie pas magiquement l’état d’une VM suspendue ou isolée.

`NXLDISUE` borne le ticket à 5 000 ms et à l’échéance du bail. Augmenter cette
borne ou compter son expiration comme révocation serait contraire au mandat.
Le banc devra relever les horloges de A avant suspension et après réveil et
la décision distante dans son propre ordre durable ; il ne doit pas supposer
que pause QMP et horloge de sécurité ont la même sémantique sur les deux ISA.

Le profil de récupération remplace le client de lancement par les clients VFS
Alice/Bob et n’émet aucun EXEC_TICKET. Le banc d’expiration du lanceur conserve
un vrai ticket mais ne démontre pas la reprise 2-sur-2 avec une autorité commune.
Juxtaposer deux résultats ne constitue pas le nouveau témoin demandé.

## Pourquoi un raccord d’admission est nécessaire

Si A est suspendue avec un ticket valide et ne reçoit aucune information nouvelle,
son état local peut être identique dans deux histoires : l’autorité a retiré
sa route, ou elle ne l’a pas retirée. Les seules vérifications locales décrites
ne distinguent pas ces histoires. C’est un constat architectural, pas un succès
ou un échec QEMU observé. Aucun lancement après retrait n’est déclaré reproduit.

Une consultation fraîche dans le shell seulement serait contournable par un
appel direct au syscall. Une consultation avant le syscall laisse également
une course entre la décision distante et la publication locale de l’enfant.
Ne pas ajouter au banc une révocation locale injectée par l’hôte pour faire
réussir artificiellement le refus de A.

## Raccord ciblé proposé pour arbitrage

1. Dans le seul profil QEMU, distinguer réservation du ticket et admission de
   sa consommation. L’émission préalable n’autorise pas encore le lancement.
2. Exiger dans le chemin effectif de consommation une décision durable à usage
   unique de l’autorité, liée au serveur, domaine, incarnation, instance, session,
   installation/empreinte, ticket et identifiant d’opération. La demande passe
   par le registre et le signataire isolés ; NovaFS reste transporteur opaque.
3. Sérialiser cette admission avec le retrait de l’incarnation. Une demande
   admise après le retrait est refusée avant création d’un processus. L’autorité
   indisponible entraîne un refus sûr. Ne pas garder le verrou du noyau pendant
   une attente de transport ; réserver atomiquement le ticket pour éviter deux
   enfants concurrents, puis vérifier le résultat au retour.
4. Réconcilier une réponse perdue par identifiant. Une issue UNCERTAIN ne doit
   jamais relancer automatiquement l’enfant ni acquitter une collecte inexistante.
5. Réutiliser les canaux privés autorisés ; établir avant raccord le budget
   de racines, feuilles et handles par phase, y compris échec. Le plafond reste
   quatre, les sélecteurs restent 48 et les formes historiques de SPAWN restent
   inchangées. Le noyau ordinaire n’est pas modifié.

Cette proposition ajoute une frontière d’admission au chemin de consommation,
pas seulement des sondes. Elle n’est pas implémentée sans arbitrage explicite.

## Décision temporelle à arrêter

Deux garanties ne doivent pas être confondues :

- **Admission durable** : une consommation admise avant le retrait peut encore
  achever sa publication locale ; toute consommation admise après est refusée.
  Cela correspond à l’ordre durable déjà utilisé pour les mutations NovaAuth,
  mais doit être accepté explicitement pour le lancement.
- **Absence absolue de création après retrait durable** : le retrait doit aussi
  attendre une barrière de drainage des admissions antérieures, ou une mise
  hors capacité d’exécution attestée de A. Si A reste isolée ou suspendue et que
  cette barrière n’est pas prouvée, la reprise B doit rester bloquée. Une simple
  requête réseau fraîche ne suffit pas à offrir cette seconde garantie.

Le mandat formule la seconde garantie. Ne pas la réduire silencieusement à la
première. Il faut arrêter le traitement des consommations déjà admises mais
pas encore publiées avant d’implémenter le raccord.

## Recette préparée, non exécutée

A et B utilisent une autorité unique, jamais copiée. A émet un ticket réel et
non consommé sous session authentifiée et installation attestée, puis le banc
archive son contexte public et les compteurs, prend l’instantané et suspend A.
B restaure une copie des disques, obtient l’approbation indépendante et récupère
le triplet actuel avec les deux parts. L’ordre durable atteste le retrait de A
avant toute admission de B.

Après réveil de A, relever maintenant/échéance AVANT SPAWN. Un ticket déjà expiré
ne qualifie pas la révocation ; conserver ce résultat et corriger seulement le
protocole temporel du banc, jamais le TTL du produit. Exiger le refus natif lié
à l’incarnation retirée, zéro publication d’enfant, puis drainage. B présente la
référence historique avant émission d’un nouveau ticket ; aucune identité de A
ne doit être recréée. Un nouveau ticket de B permet le vrai lancement et une
seule collecte. Conserver séparément les tests de concurrence, rejeu et expiration.

Redémarrer ensuite A sur ses anciens disques : le refus des références anciennes
est un autre témoin, pas la preuve de révocation d’une instance vivante.
Exécuter les quatre profils, tracer les admissions durables, les transitions
locales et les compteurs ; conserver les 48 sélecteurs, les quatre ELF historiques
et les précédentes recettes VFS/2-sur-2. Toute modification supplémentaire de
sécurité exige son propre arbitrage.

## État

Aucun nouveau contrôle EXEC_TICKET n’est déclaré qualifié. Les 40 démarrages VFS
après intégration et complément, les preuves antérieures et la réserve mémoire
AArch64 de 301 cadres restent conservés. La fusion locale demeure non publiée ;
aucun tag et aucune activation ordinaire ne sont autorisés par ce document.
