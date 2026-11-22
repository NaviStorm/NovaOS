# NovaAuth v0.24.0 — Matrice des refus R2

Branche `codex/novaauth-administration-0240-phase1`, départ `33d87d8470b533eecf117054a6095bab2a8fd1a2`. Les statuts ci-dessous sont des attentes ; une ligne sans preuve invitée n'est pas qualifiée. Les succès hôtes ne valent pas exécution QEMU.

## Chemins à exercer

| ID | Préconditions et tentative | Point de contrôle, résultat attendu | Couverture acquise et travail invité restant |
| --- | --- | --- | --- |
| S1 | Bob authentifié, sans portée administrative ; mutation de sa propre appartenance | NovaAuth `DENIED`, aucun privilège ajouté | Autorisation métier hôte ; requête NXADMIN1 directe et CLI à exécuter sur quatre profils |
| S2 | Bob tente d'ouvrir/écrire les trois objets d'identité | VFS/NovaFS, refus natif avant écriture | Port triplet privé existant ; témoin Bob réel à exécuter |
| S3 | Bob présente session, ticket ou capacité d'Alice | Table locale et liaison instance/session, `DENIED`/`REVOKED`, aucun enfant | Routes hôtes et format des sorties testés ; substitution invitée à exécuter |
| S4 | Bob envoie NXACMD01 directement | NovaAuth contrôle la portée et l'identité à chaque demande ; refus | Requêtes R1 et contrôles métier existants ; mutations R2 non encore raccordées |
| S5 | Substitution d'identité, session ou cible | Métadonnées noyau et contexte NovaAuth authentifié ; aucun contexte fourni par argv | Contrôles R1, paquet et destinataire hôtes ; exercice duplex restant |
| S6 | Ticket expiré, révoqué ou consommé | Trois causes distinctes, aucune création ; cas révoqué avant échéance obligatoire | Admissions historiques réutilisables après contrôle de portée ; refus R2 en deux sessions restant |
| S7 | Mutations concurrentes de révocation/dernier administrateur | Transaction faisant autorité, dernier administrateur conservé ; drainage requis | Plans natifs/ordre durable hôtes ; parcours invité restant |
| S8 | Réponse de mutation perdue | UNCERTAIN puis consultation par identifiant ; une seule publication | Publication/réconciliation native et autorité hôtes ; raccord CLI à terminer |
| S9 | Désactivation puis réactivation de Bob | Nouvelle authentification sous époque neuve et droits actuels ; anciens objets définitivement révoqués | Tests d'époques hôtes réussis ; effecteurs et commandes invités à terminer |
| P1 | Alice consulte puis administre légitimement | Succès et effets exacts ; opérations suivantes encore possibles | Consultations R1 qualifiées ; mutations R2 invitées restantes |
| I1 | Installation explicitement destinée à Bob par le provisionneur | Signature et catalogue attestés, installation distincte, Bob non administrateur | Test moteur P9 sur deux secteurs/deux manifestes ISA réussi ; démarrage duplex en diagnostic |
| I2 | Paquet d'Alice, faux signataire, signature altérée, destinataire substitué | Refus sans changement des privilèges ni création d'enfant | Tests moteur réussis ; refus aux points de contrôle invités à joindre |

## Format des preuves

Chaque ligne exécutée conserve : commit et diff exact éventuel (diagnostic seulement), empreintes ISO/noyau/services/P9 initial, profil, préconditions, requête, statut natif attendu/observé, décision d'admission, PID, horloges et échéances, effets persistants attendus/observés, compteurs avant/pic/après et fichiers SHA-256.

Les journaux de refus et reçus légitimes peuvent changer. Comparer les identités, époques, appartenances, politiques et installations, pas seulement une empreinte globale du disque. Les données Alice/Bob sont synthétiques. Les sorties brutes sont exclusivement conservées par le collecteur du banc et ne sont jamais déléguées aux sessions.

## Limites actuelles

Les quatre profils doivent encore exécuter ces chemins sur un candidat stabilisé. Les diagnostics intermédiaires ne sont pas des qualifications globales. Les neuf contrôles ABI, les quatre ELF historiques, le budget IPC et la restitution doivent être rattachés au commit effectivement testé. Conserver séparément la réserve instrumentée AArch64 de 301 cadres et la réserve de reproductibilité entre environnements. Aucun tag, fusion ni activation ordinaire ; utilitaires en pause.
