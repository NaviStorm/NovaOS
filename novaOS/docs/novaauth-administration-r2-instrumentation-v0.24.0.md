# NovaAuth R2 — Instrumentation ciblée et unique essai

Base 4a85328b9d13ca7f59933d3a20c3605a47e676b0.
Sondes 7a432256d2e9dbeab3dc09f6c6d0e24738309d05 ; candidat compilé/testé
53ed5427045fdc5e2e4dbdc762f2c891837ca5a2.
Profil exclusivement QEMU, x86_64/P9-4096.

## Changements
Le générateur ticket-vivant applique une instrumentation additionnelle dans
l'export, sans changement aux sources ordinaires du noyau ni à l'ABI.
Pas de modification de droits, baux, délais, statuts retournés ou oracles.
Les sondes n'accordent pas NOW à Bob : l'horodatage est produit côté noyau sur
son DEBUG_WRITE existant, avec ses contrôles et son quota inchangés.

R2TIME fournit pid et ticks invités. Le PID est corrélable aux routes, instances
et sessions de l'observateur existant. Les messages ne contiennent ni clé,
signature, contenu de ticket, argument privé ou donnée utilisateur.
Étapes Bob :
- 1 : résultat de receipt inattendu, statut natif ou résultat positif ;
- 2 : attente config(1) inattendue, statut ou kind du message ;
- 3 : 300 itérations terminées sans révocation (k=2, n=0 : borne, pas succès) ;
- 4 : SPAWN ne renvoie pas REVOKED, statut original.
Étapes receipt : 11 erreur SEND, 12 erreur RECEIVE, 13 forme de réponse,
14 statut embarqué inattendu. Aucune sonde par itération réussie.
Étapes vérificateur : 20 entrée Confirm, 21 avant échange de publication,
22 retour réussi. Les erreurs de confirmation gardent les traces existantes.

R2LOAD marque réception de la taille ELF, début/fin de scellement ; son intervalle
inclut transfert et attente, pas seulement le CPU du chargeur. Les transitions
d'admission distante et la publication de l'enfant reçoivent des ticks.
Les mesures ne confondent pas ces phases avec les heures hôtes.

L'ordonnancement reste inchangé pour isoler la cause ; aucune préparation
exigeant une authentification n'est déplacée. Aucun préchargement supplémentaire
ou rendez-vous de confirmation n'a été développé dans cette étape.

## Validation avant essai
47 tests hôtes réussis avant export puis 47 sur le candidat final (2,874 s).
Application des sondes à l'export vérifiée par ancrages uniques.
Première construction : échec E0433, alloc absent du vérificateur.
Correction : messages de sonde fixes sans allocation dans ce service.
Seconde construction : réussite en 21,709 s. Le journal initial est conservé.
Avertissement historique rust-objcopy/libLLVM également conservé.
ISO novaauth-sessions et identité SHA-256 du noyau embarqué vérifiées.

## Unique essai : échec en amont de Bob
Premier statut de sortie natif : **12, signataire PID 8**, après le dialogue de
récupération. Il ne s'agit pas de la sortie 12 du témoin Bob étudiée auparavant.
Chronologie monotone invitée :
- début TLS 852 ms ;
- échéance absolue 10 852 ms ;
- fin 11 040 ms, soit 188 ms après échéance.

Les deux remises sont observées, mais aucune activation n'est confirmée.
Le registre annonce « fermeture dialogue non confirmee », puis quarantaine.
Le pont recovery_bridge termine par map_err(|_| rt::REFUSED) : le sous-statut
TLS exact est perdu par ce chemin existant. La trace démontre le dépassement
et le refus sûr ; elle ne permet pas de prouver quel appel TLS a échoué en
premier. Ne pas attribuer automatiquement cette variation à la charge hôte
ni aux sondes ajoutées, non encore atteintes.

Aucun lancement de sessions Alice/Bob, ticket Bob, révocation, REVOKED avant
expiration, reçu RETIREE ou mesure de chargement/admission/confirmation R2.
Tous ces critères sont NON EXÉCUTÉS. Aucune relance ni correction après l'échec.

## Ressources et temps
Signataire : avant/après [4 racines, 6 handles, 0 attente, 66 pages, 64 pile,
31 cadres IPC globaux] dans les traces de mesure.
Registre : avant [4,7,0,145,64,31], après [4,6,0,145,64,26].
Ces points ne démontrent pas le drainage final R2 ; le banc le laisse négatif.
Pic observé : quatre racines maximum ; pas d'élargissement du quota.
Le relais hôte est drainé, aucun autre QEMU lancé.

Acquisition 2026-11-22T15:25:14.283277+00:00, libération 2026-11-22T15:26:55.250249+00:00 UTC.
Attente 0.000465 s ; exécution 100.919 s.
Travail actif sous 30 minutes ; diagnostic/développement non chronométré
séparément à la seconde. Aucune soustraction d'horloges hôte/invité.

## Preuves et suite soumise
Archive /Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/instrumentation-4a85328 : 37 fichiers vérifiés SHA-256.
Manifeste 509fc9ff8673689b085dd35124ec85c425cdc026713d8d0bdf2f8f1b980b6aa4.
Artefacts privés /Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/artefacts-prives-administration-0240-r2/instrumentation-4a85328 : 20 fichiers vérifiés ; manifeste 4238afedde033ff6915e31cb38b3cc22e8bfaed46addb69de3e6b274eed667c0.
Les secrets/ELF contenant les clés du laboratoire ne sont pas publiés.

Le refus Bob reste ouvert : ses nouvelles sondes sont compilées mais non
exercées. Avant tout nouvel essai, arbitrer la reprise et, si nécessaire,
une sonde bornée de l'erreur TLS sous-jacente, sans prolonger le délai de 10 s.
Aucune réserve levée prématurément. Réserves historiques inchangées.
Publication R2 seulement ; aucune fusion, aucun tag, utilitaires en pause.
