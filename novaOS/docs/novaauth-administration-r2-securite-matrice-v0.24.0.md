# NovaAuth v0.24.0 — Matrice des refus R2

Branche `codex/novaauth-administration-0240-phase1`, départ `33d87d8470b533eecf117054a6095bab2a8fd1a2`. Actualisation : [provisionnement et duplex](novaauth-administration-r2-provisionnement-duplex-v0.24.0.md). Une couverture partielle ne qualifie pas toute la ligne. Les succès hôtes ne valent pas exécution QEMU.

## Chemins à exercer

| ID | Préconditions et tentative | Point de contrôle, résultat attendu | Couverture acquise et travail invité restant |
| --- | --- | --- | --- |
| S1 | Bob authentifié, sans portée administrative ; mutation de sa propre appartenance | NovaAuth `DENIED`, aucun privilège ajouté | Requête NXADMIN1 directe refusée par le registre dans les quatre profils du témoin duplex ; commandes de mutation native encore à qualifier |
| S2 | Bob tente d'ouvrir/écrire les trois objets d'identité | VFS/NovaFS, refus natif avant écriture | Port triplet privé existant ; témoin Bob réel à exécuter |
| S3 | Bob présente session, ticket ou capacité d'Alice | Table locale et liaison instance/session, `DENIED`/`REVOKED`, aucun enfant | Routes hôtes et format des sorties testés ; substitution invitée à exécuter |
| S4 | Bob envoie NXACMD01 directement | NovaAuth contrôle la portée et l'identité à chaque demande ; refus | Préparation d'ajout de membre refusée par le registre, `DENIED=5`, quatre profils ; confirmation et consultation de mutation encore à qualifier |
| S5 | Substitution d'identité, session ou cible | Métadonnées noyau et contexte NovaAuth authentifié ; aucun contexte fourni par argv | Annuaire Alice refusé, injection NXACTX01 et rejeu refusés, métadonnées privées refusées, quatre profils ; référence exacte d'une capacité Alice encore à exercer |
| S6 | Ticket expiré, révoqué ou consommé | Trois causes distinctes, aucune création ; cas révoqué avant échéance obligatoire | Admissions historiques réutilisables après contrôle de portée ; refus R2 en deux sessions restant |
| S7 | Mutations concurrentes de révocation/dernier administrateur | Transaction faisant autorité, dernier administrateur conservé ; drainage requis | Plans natifs/ordre durable hôtes ; parcours invité restant |
| S8 | Réponse de mutation perdue | UNCERTAIN puis consultation par identifiant ; une seule publication | Publication/réconciliation native et autorité hôtes ; raccord CLI à terminer |
| S9 | Désactivation puis réactivation de Bob | Nouvelle authentification sous époque neuve et droits actuels ; anciens objets définitivement révoqués | Tests d'époques hôtes réussis ; effecteurs et commandes invités à terminer |
| P1 | Alice consulte puis administre légitimement | Succès et effets exacts ; opérations suivantes encore possibles | Consultation native après fermeture Bob réussie sur quatre profils ; mutations R2 invitées restantes |
| I1 | Installation explicitement destinée à Bob par le provisionneur | Signature et catalogue attestés, installation distincte, Bob non administrateur | Moteur P9 hôte puis authentification, chargement, admission distante et lancement Bob réels dans les quatre profils duplex |
| I2 | Paquet d'Alice, faux signataire, signature altérée, destinataire substitué | Refus sans changement des privilèges ni création d'enfant | Tests moteur hôtes réussis ; refus invité des noms `alice-seulement` et `paquet-non-autorise` suivis dans la matrice d'installation distincte ; pas de faux port invité d'installation |

## Format des preuves

Chaque ligne exécutée conserve : commit et diff exact éventuel (diagnostic seulement), empreintes ISO/noyau/services/P9 initial, profil, préconditions, requête, statut natif attendu/observé, décision d'admission, PID, horloges et échéances, effets persistants attendus/observés, compteurs avant/pic/après et fichiers SHA-256.

Les journaux de refus et reçus légitimes peuvent changer. Comparer les identités, époques, appartenances, politiques et installations, pas seulement une empreinte globale du disque. Les données Alice/Bob sont synthétiques. Les sorties brutes sont exclusivement conservées par le collecteur du banc et ne sont jamais déléguées aux sessions.

## Limites actuelles

La fermeture duplex réussit sur quatre profils au commit `4481e46` ; les neuf contrôles ABI et les quatre ELF historiques sont rattachés à ce point. Les recettes d'installation utilisent les sources invitées `d995e0e` et un banc hôte distinct, consignés ensemble dans chaque preuve. Le timeout de la tentative supplémentaire de lancements concurrents sur x86_64/P9-4096 reste ouvert ; les parcours séquentiels ne le remplacent pas. S2, S3, S6 à S9 et les mutations légitimes restent incomplets dans l'invité. Conserver séparément les réserves de 301 cadres AArch64 instrumentés et de reproductibilité inter-environnements. Aucun tag, fusion ni activation ordinaire ; utilitaires en pause.
