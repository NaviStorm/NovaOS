# Objectifs consignés avant la grande recette

Le pilote Darwin ARM64 de 256 Mio (trois passages, cache non purgé) donne une médiane de 0,236 s pour le candidat et 0,929 s pour GNU, avec 8 Mio demandés, un travailleur et huit lecteurs par fusion. Sorties équivalentes sur les trois passages.

Pour 160 Gio, retenir un travailleur, 2 Gio par lot, 128 lecteurs, une sortie régulière sur APFS. Objectifs pour Nova : au moins 200 Mio/s, RSS au plus 4 Gio, temporaires au plus 165 Gio si la fusion finale tient en une passe. La marge de débit tient compte du changement de régime du cache. La marge RSS couvre les index, les tampons de lecture et l’allocateur. Les répertoires de lots sont échantillonnés toutes les 50 ms. La génération et la validation sont chronométrées séparément. Un passage par exécutable, successif, et comparaison du SHA-256 sur la totalité des octets.
