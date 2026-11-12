# NovaOS v0.16.3 — Terminal et nsh

## Utilisation

| Action | Comportement |
|---|---|
| Tab en fin de mot | Complète le préfixe commun des commandes, des alias ou des noms du répertoire sélectionné. Un nom de répertoire se termine par `/`. Les espaces et caractères spéciaux sont protégés selon la citation en cours. Une citation ouverte est fermée sur un fichier unique. |
| Deux Tab | Présente les candidats triés lorsque le préfixe commun est épuisé. Un préfixe sans candidat conserve sa saisie. Au milieu d’un mot, Tab laisse le suffixe et le curseur à leur place. Les flèches et les effacements continuent de fonctionner. |
| `history` | Affiche les entrées numérotées du terminal, dont la commande courante. La profondeur configurée vaut 64 par défaut. L’appel depuis un script imbriqué est refusé. |
| `clear`, `help` | Efface l’écran courant et ramène le curseur à l’origine, ou affiche le catalogue des commandes de production. |
| `ps` | Affiche PID, rôle, état vivant/terminé et mémoire privée en Kio de la session. |
| `top` | Actualise le même échantillon dans l’écran alternatif. Quitter avec `q`, Échap ou Ctrl-C. |

Le shell exécute une commande externe à la fois et en attend la fin. À l’invite, la liste contient normalement seulement `nsh`. Une sonde du parent vérifie l’observation d’un enfant avant sa collecte. L’ELF enfant vérifie que le même appel lui est refusé. Le compteur de mémoire additionne les pages privées de code/données/tas et de pile de l’espace utilisateur. Le protocole ne fournit ni charge CPU, ni mémoire globale, ni informations sur les services d’autres sessions. [ABI native](abi.md#observation-de-la-session-depuis-v0163)

La complétion lit jusqu’à 128 entrées du répertoire autorisé. Les noms cachés exigent un préfixe commençant par un point. Le parcours réemploie les deux tampons de lecture des noms et consulte le type des seuls candidats. La syntaxe de script reste analysée par nsh au moment de valider la ligne. L’ambiguïté, un refus de parcours ou la limite de ressources laissent le texte éditable.

## Causes des défauts

**P2.** HT était décodé comme une touche invalide. Le drapeau d’erreur restait actif après avoir effacé et corrigé le nom. HT est désormais un événement d’édition distinct. Entrée exécute le tampon visible. Les contrôles de séquences invalides restent actifs.

**P3.** Chaque nouvelle invite était écrite à la colonne laissée par la commande. Le terminal série mémorise si la dernière sortie de texte se termine par LF. Au début de la lecture interactive, il termine la ligne ouverte. Le terminal garde séparément l’état de la ligne principale pendant l’affichage de vi ou de top. À la fermeture de top, l’invite revient au curseur restauré, sans ligne vide supplémentaire. Les quatre utilitaires continuent de produire leurs propres octets. Le test relit indépendamment le contenu P9 après arrêt de la VM.

## Installation dans UTM

Sur un Mac Apple Silicon avec UTM 5.0.8 et `qemu-img`, exécuter ces commandes depuis la racine du dépôt. Elles créent une VM et un disque vierge de 64 Mio dans un nouveau répertoire. [Archive AArch64](../../distribution/images/novaos-0.16.3-utm-aarch64.tar.gz)

```sh
NOVA_TRAVAIL="$(mktemp -d "$HOME/Documents/NovaOS-nsh-0.16.3.XXXXXXXX")"
NOVA_DIST="$NOVA_TRAVAIL/novaos-0.16.3-utm-aarch64"
NOVA_VM="$NOVA_TRAVAIL/machine/NovaOS-installation-AArch64.utm"
tar -xzf distribution/images/novaos-0.16.3-utm-aarch64.tar.gz -C "$NOVA_TRAVAIL"
(cd "$NOVA_DIST" && shasum -a 256 -c SHA256SUMS) &&
    python3 novaOS/scripts/paquet_installation.py creer "$NOVA_DIST" "$NOVA_TRAVAIL/machine"
```

1. Ouvrir la VM dans UTM. Conserver le port série en mode **Terminal**. L’installateur doit afficher un disque vierge de 67 108 864 octets, série `NOVA-CIBLE-000000001`, SID 56. Pour ce seul disque, saisir `AUTORISER NOVA-CIBLE-000000001 67108864`.
2. Attendre `INSTALLATION bilan Ok(())`, arrêter la VM et retirer son raccourci de la bibliothèque en conservant ses fichiers.
3. Exécuter `python3 novaOS/scripts/paquet_installation.py systeme "$NOVA_VM"`. Rouvrir la VM et la démarrer. Le disque système présente l’invite `nsh>`. [Construction depuis les sources](cli-v0.15.1.md#image-et-installation-dans-utm)


## Qualification du 12 novembre 2026

Les invités QEMU 11.1.3 ont exécuté le noyau et les ELF de leur architecture. Les tests du moteur NovaFS ont tourné sur Darwin ARM64 natif et x86_64 sous Rosetta (indicateur de traduction égal à 1). Les observations UTM concernent l’application 5.0.8 et son moteur QEMU 10.0.12. [Matrice et mesures](validation/nsh-v0.16.3/resultats.json) · [Audit antérieur](validation/nsh-audit-v0.16.3.md)

| Banc | Profils et résultat |
|---|---|
| Saisie de nsh | AArch64 et x86_64, secteurs 512 et 4 096. Exécution après Tab, correction d’un mot, déplacement au milieu de la ligne, chemin cité ou échappé, liste sur deux Tab, historique, effacement et aide. Deux rafraîchissements de `top`, sortie par `q` et retour à l’invite. |
| Sortie sans LF | Sur les quatre profils, `cat`, `head`, `tail` et `more` placent l’invite sur la ligne suivante. Après arrêt, le lecteur hôte a vérifié les identifiants, la génération et l’égalité octet par octet du fichier sans LF avec la fixture d’origine. |
| Régressions système | Par architecture, 25 démarrages, 24 groupes bloc, trois bancs ELF/cycle de vie/IPC et quatre essais CLI. Les sondes exercent le droit du parent et le refus de l’ELF enfant. Espaces de noms, deux clients, chemins longs et coupures rejoués sur les deux géométries. |
| Shell et scripts | Par architecture et par secteur, 113 commandes du banc historique et 31 du banc de scripts. Reprise et oracle Bash sur les onze sorties communes. |
| vi | Par architecture et par secteur, 14 ELF collectés et 13 fichiers relus. Modes normal, insertion et commande, déplacements, suppression, modification, annulation/rétablissement, `:w`, `:q`, `:q!` et `:wq`. Les refus de remplacement, de concurrence, de révocation et d’allocation conservent les protections existantes. |
| Écran de vi | Par architecture, 20 frappes puis 20 lignes, défilement et retour au shell. Le banc vérifie le curseur et les cellules modifiées en 48×12, 36×8, 1×1, puis 80×24. Après retour à 80×24, il contrôle le texte en haut, le statut en ligne 23 et la commande indépendante en ligne 24. |
| Stockage et DMA | Sur AArch64, 11 scénarios GICv3 et 23 scénarios de DMA séquentielle. Installation GPT en secteurs de 4 096 octets, démarrage autonome, relecture après redémarrage et zéro écriture sur le support d’installation. |
| NovaFS v0.9.13 P9 | Par architecture, 196 tests standards, 197 instrumentés, six ignorés dans chacune de ces passes, 2 304 injections sur les ancres et 1 000 000 d’opérations différentielles. Les 360 refus de montage des injections sont attendus. |

La contre-vérification reprend les bancs `options.py` et `mutations.py` de l’audit antérieur avec les mêmes données et la bannière attendue 0.16.3. Chaque profil couvre 84 contrôles d’options, de clavier et de limites, puis 21 opérations sur des images séparées et relues par le moteur hôte. T10 rejoue exactement la séquence fautive : `cat xyz`, Tab, trois retours arrière, `alpha.txt`, Entrée. La sortie est `alpha A` et le statut vaut zéro. [Bancs et préparation des fixtures](validation/nsh-audit-v0.16.3.md#reproduire-laudit)

Le contrôle de démarrage compare une capture du bandeau de version. La comparaison initiale avec les empreintes v0.16.2 a échoué sur les deux architectures. Les nouveaux témoins correspondent à la modification du dernier chiffre du bandeau. Le banc des 25 démarrages a ensuite réussi sur chaque architecture. Le banc GIC a également nécessité d’inclure la version 0.16.3 dans son contrôle de bannière.

### Treize cas auparavant non testés

| Cas | Profil | Injection et observation |
|---|---|---|
| NT01 × 4 | Deux architectures × deux secteurs | Pendant l’expansion de `ls incident/**/*.txt`, le shell lance un deuxième ELF pour créer une entrée entre deux réponses READDIR. La reprise avec la génération précédente reçoit un refus. Une nouvelle commande voit l’entrée créée. |
| NT02 × 4 | Idem | Après une première entrée, une vraie réservation de 512 Mio dans la collecte de `ls manque/**/*.txt` échoue. Le diagnostic « mémoire insuffisante » est suivi d’une nouvelle commande réussie. |
| NT03 × 4 | Idem | Après la première tranche de 704 octets de `cat /lignes`, le parent révoque la capacité de l’enfant. Le conducteur observe la première tranche, le refus à la lecture suivante et le statut d’échec, sans les lignes suivantes. La fixture multioctet exerce la même frontière que le fichier `/aux/grand` du premier audit. |
| NT04 | UTM graphique, AArch64, QCOW2/4 096 | Dans la fenêtre Terminal, complétion de `pwd` et d’un nom de fichier, sortie tronquée suivie d’une invite distincte, historique, PID et mémoire, entrée dans `top` puis retour au shell. [Session](validation/nsh-v0.16.3/utm-4096.jpg) · [Écran de top](validation/nsh-v0.16.3/top-4096.jpg) |

NT01 à NT03 nécessitent des points de rendez-vous dans une copie instrumentée. Les contrôles NTx s’exécutent sur des disques jetables. Le patch n’entre pas dans l’installateur distribué. Pour les reproduire depuis la révision livrée, dans un shell sur le Mac de développement équipé des outils du projet :

```sh
NOVA_LAB="$(mktemp -d)"
git clone --local "$PWD" "$NOVA_LAB/OS"
(
    set -e
    cd "$NOVA_LAB/OS"
    git apply --unidiff-zero novaOS/tests/fixtures/nsh-incidents.patch
    NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
    for arch in aarch64 x86_64; do
        make -C novaOS ARCH="$arch" kernel
        NOVA_ARCH="$arch" python3 -B novaOS/tests/nsh_incidents.py
    done
)
```

Sur l’image livrée, l’application UTM en profil QCOW2/512 a installé un disque vierge, démarré deux fois sans le support et relu des fichiers édités de 66 560 et 66 562 octets. Le banc utilise le PTY de l’application pour comparer les sorties. La qualification graphique 4 096 utilise un disque converti en QCOW2 après l’installation avec le moteur UTM. [Banc UTM](../tests/nsh_utm.py)

Dans la fenêtre UTM 4 096, vi a ouvert `/persistant`. Vingt caractères insérés sont restés sur la première ligne, avec la barre d’état fixe. La touche `u` a restauré le texte initial et Ctrl+R a rétabli le groupe. Après `:q!`, la commande `cat /persistant` a relu le texte initial `NovaOS`. [Capture après insertion](validation/nsh-v0.16.3/vi-4096.jpg)

### Mémoire et limites

| Mesure | AArch64 | x86_64 |
|---|---:|---:|
| Pic observé des scripts v0.16.2 | 440 Kio | 452 Kio |
| Pic observé des scripts v0.16.3 | 452 Kio | 468 Kio |
| Hausse sur le même banc | +12 Kio | +16 Kio |
| Pic du banc vi courant, deux géométries | 376 Kio | 376 Kio |
| Cadres retenus à la fin du banc CLI P9 | 3 | 12 |

Les pics proviennent d’échantillons de pages privées de 4 Kio, pile comprise. Le pic du banc vi courant est identique au témoin v0.16.2. Le nombre de cadres retenus rejoint la référence CLI et le banc retrouve le même nombre d’octets occupés dans l’allocateur Buddy au début et à la fin. Les grands volumes font l’objet d’une mesure distincte. La complétion donne accès aux noms visibles dans le répertoire consulté. Un répertoire au-delà de 128 entrées ne fournit pas de complétion partielle. L’historique reste en mémoire pendant la session. La sortie de `ps` et de `top` concerne le shell et l’enfant direct dont il détient encore le droit d’attente. Aucun état d’un autre service n’est visible.

Sur le banc des grands fichiers P9 en secteurs de 4 096 octets, le lecteur hôte a vérifié les originaux et les quatre nouvelles destinations à 8 Kio, 64 Kio, 1 Mio et 4 Mio. Les mesures de vi couvrent le lancement, l’ouverture et la sauvegarde après modification :

| Taille | Pages privées de 4 Kio à ces trois points, sur les deux architectures |
|---|---|
| 8 Kio | 78 → 94 → 94 |
| 64 Kio | 78 → 126 → 158 |
| 1 Mio | 78 → 366 → 926 |
| 4 Mio | 78 → 1 134 → 3 230 |

Les quatre profils mémoire reproduisent les mesures de v0.16.2. L’ouverture de 4 Mio demeure lente dans ces invités QEMU : 682 s sur AArch64 et 577 s sur x86_64. Plusieurs bancs tournaient en parallèle sur le Mac. Ces durées de bout en bout décrivent cette exécution. [Mesures historiques de vi](vi-v0.16.2.md)

Pour rejouer les bancs du terminal et de l’éditeur sur les deux invités :

```sh
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" nsh-terminal-test nsh-test nsh-scripts-test \
        vi-test vi-ecran-test vi-memoire-test vi-volumes-test || exit 1
done
```
