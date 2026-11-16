# Admission de sort dans nsh

Sur la base `ea2d0c3`, le catalogue documentaire rend l’aide, la version et le manuel. Sur les quatre profils de cette branche, `sort /documents/exemple.txt` rend 127. `cat /documents/exemple.txt | sort -n` et `sort /documents/exemple.txt | head -n 1` rendent 3 à l’analyse de la commande. Ces statuts précèdent la création d’un enfant. [Transcriptions par architecture](results/qemu)

Le récepteur `6b94f2f6349add9fc6ac0bc8276ab1b8747a5e36` de la branche `uniq` installe les programmes 17 (uniq) et 18 (sort). Dans sa grammaire, `sort donnees | uniq -c` admet un seul fichier P9 en lecture et le port producteur. Le banc substitue exclusivement les ELF sort de la présente livraison dans une copie isolée de ce catalogue. Les résultats propres à cette intégration figurent sous `pipe` et `pipe-256k`. Le rôle lecteur de sort et l’association avec head restent à admettre. [Catalogue récepteur](https://github.com/NaviStorm/NovaOS/commit/6b94f2f6349add9fc6ac0bc8276ab1b8747a5e36)

**Arbitrages demandés**

- Admettre le lancement direct de sort et les deux paires avec cat et head, avec la délégation du handle P9 sélectionné et du seul port utile. Le lecteur utilise `config(2)`, le producteur `config(1)`. Réutiliser l’identité 18 déjà éprouvée sur le récepteur. La sélection des chemins et des droits appartient au lanceur. Le tmp appartient à l’instance.
- Décider de la prise en charge des listes de chemins, de la publication conditionnelle de `-o`, du filtre `--compress-program`, de la source d’aléa et des travailleurs avec un contrat explicite. L’ELF refuse ces cas aujourd’hui.
- Qualifier le démarrage du service sur le grand volume. Le fichier de 320 Mio a été intégralement relu sur l’hôte après remontage P9. Dans chacun des quatre invités de 256 Mio, le service de fichiers rend 26 (E/S) et le client 8 avant l’invite. Le résultat borne le profil testé. La taille maximale que P9 peut monter dans cette configuration reste à déterminer.
- Sur le chemin admis, il reste à exercer le retrait explicite du droit P9 pendant une lecture, deux instances de sort simultanées dans le même invité, et la fermeture anticipée de head. Le banc récepteur a pu exercer le volume plein, la panne pendant la fusion et la récupération des lots après arrêt forcé. Les observations et leurs profils figurent dans [le bilan](RECETTE.md).

Une commande future `sort -o DEST SOURCE` doit acquérir la délégation nécessaire à la publication et au traitement des collisions d’identité. La suppression suivie d’une recréation compromettrait la protection de la destination. [Contrat du temporaire privé](../../novaOS/docs/contrat-tmp-prive-v0.21.0.md)
