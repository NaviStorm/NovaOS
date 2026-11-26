# Critères de performance du premier port Ethernet v1

Référence fonctionnelle et de performance initiale `dc498394108210f0479189a30c3e5d91de601c78`. Critères fixés avant modification du chemin de données.

- Cellule de décision par architecture : lots de huit trames de 1 514 octets au pair écho, aller-retour complet jusqu'à restitution. Débit utile TX en bits de trame Ethernet par seconde. Le débit duplex additionne les deux sens. Le temps inclut le traitement des reçus.
- Objectif simultané sur AArch64 et x86_64 : rapport des médianes de débit entre démarrages ≥ 2, p99 par lot et nombre d'appels IPC invités par trame TX strictement inférieurs à la référence instrumentée. Présenter les résultats par démarrage. Aucun changement de seuil après mesure.
- Matrice : lots de 1, 4, 8, tailles 60 et 1 514 octets. Viser trois démarrages indépendants par architecture et par variante avec au moins 1 024 lots par cellule par démarrage. Calculer le p99 par rang supérieur. Distinguer TX seul, RX seul, duplex et aller-retour écho. Une mesure absente reste ouverte.
- Apparier les firmwares, modèles QEMU, accélération, CPU, topologie, pair et contrôle des octets. Documenter la géométrie de publication du témoin historique et toute impossibilité de comparaison directe.
- Vérifier propriété, tickets/reçus, huit crédits, isolement, permissions DMA, filtres MAC et EtherType, quatre racines IPC, 48 sélecteurs et continuité du stockage. Conserver les archives de phase 3. Budget de 120 minutes de travail actif, hors attente du verrou partagé.
