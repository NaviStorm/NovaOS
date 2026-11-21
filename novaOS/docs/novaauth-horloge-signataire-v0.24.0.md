# NovaAuth v0.24.0 — Horloge du signataire QEMU

## Décision et périmètre

Recette du 17 novembre 2026 sur la branche `codex/novaauth-autorite-0240`, issue de `1e9fec9313f85409ee6ec8e2492a220804d50dbb`. Avant correction, chacun des quatre invités renvoyait `DENIED` au service 23 dès sa première lecture de l'horloge. Le filtre du profil autorisait le vérificateur de sessions pour cette opération. Le signataire ne pouvait donc pas établir son échéance avant d'ouvrir TLS. Voir les quatre séries dans `diagnostic-horloge/` de l'archive.

L'export explicite `profile.py --horloge-signataire` donne au signataire supervisé la lecture de `SERVICE_PROCESS(4, 0, 0)` (sélecteur `0x1007b`, opération `NOW`). Le noyau déduit le demandeur du contexte d'exécution. Il exige le gestionnaire courant comme propriétaire du service, l'identifiant de service 23, un espace d'adressage présent et une instance encore vivante. Ni le PID ni le numéro de service ne proviennent d'un argument fourni par le client. Le sélecteur et le contrôle historique du gestionnaire continuent à traiter les autres appels selon leurs droits préexistants.

| Appel depuis le même signataire | Résultat exigé et observé |
| --- | --- |
| Deux lectures `NOW, 0, 0` | Valeur en millisecondes de démarrage, seconde valeur supérieure ou égale à la première |
| `START`, `POLL`, `KILL`, chacun avec trame de 128 octets | `DENIED` pour chaque appel |
| `DRAIN, 0, 0` | `DENIED` |
| `NOW, adresse, 0`, `NOW, 0, 128`, `NOW, adresse, 128` | `DENIED` pour chaque appel |
| Opération inconnue avec trame de 128 octets | `DENIED` |

## Échéance et ressources

Le registre invité (service 22) et le signataire (service 23) sont deux processus supervisés. La négociation TLS 1.3 authentifie les deux extrémités par certificat épinglé et preuve de possession. Le signataire transmet les 32 octets de l'identité d'instance sur ce canal et exige leur restitution exacte. Le relais transporte les enregistrements sur deux pages volatiles hors P9. Le contrôle des artefacts compte une occurrence de la graine de laboratoire dans le signataire et zéro dans chaque autre ELF ou binaire utilisateur inspecté, dont NovaFS et le courtier. L'ISO de qualification contient le signataire et reste dans l'archive locale à accès restreint.

Une seule échéance absolue couvre ouverture, négociation, émission, réponse et vérification. Chaque attente IPC utilise le temps restant. Le témoin lent fournit un octet toutes les 650 ms pour entretenir la progression du transport au-delà de l'échéance. Le signataire constate `Io(TimedOut)`, ferme le transport et le registre reçoit un refus. La mesure finale inclut le drainage. Les dix secondes bornent l'acceptation de l'échange.

| QEMU, P9 | Échange accepté, ms | Essai expiré, ms jusqu'à la fin du nettoyage | Résultat du registre, nominal / expiré |
| --- | ---: | ---: | --- |
| AArch64, 512 | 504 | 10 152 | confirmé / refusé |
| AArch64, 4 096 | 744 | 10 168 | confirmé / refusé |
| x86_64, 512 | 564 | 10 156 | confirmé / refusé |
| x86_64, 4 096 | 932 | 10 184 | confirmé / refusé |

Deux vCPU, QEMU 11.1.3. Pour les huit essais, le signataire conserve quatre racines, six handles et zéro attente aux points de mesure. Pages d'image du signataire avant et après : 47 sur AArch64, 54 sur x86_64. La pile compte 64 pages. Chaque fermeture vérifie la réponse du port et son retour à l'état fermé. Les cadres IPC globaux incluent le démarrage des autres services. Les journaux les donnent séparément des compteurs du signataire.

Le banc IPC indépendant conserve sa limite de 2 048 cadres retenus au repos. L'invité instrumenté AArch64 en retient **2 349**, dont zéro cadre IPC actif, et échoue au seuil. L'invité instrumenté x86_64 en retient 1 918 et réussit. Le banc ordinaire réussit sur les deux architectures, avec respectivement 1 761 et 1 373 cadres retenus. La réserve AArch64 reste ouverte. Les compteurs de repos seuls ne permettent pas d'attribuer l'excédent à une fuite du signataire.

## Compatibilité et preuves

Les neuf contrôles ABI réussissent sur l'arbre ordinaire et sur l'export QEMU. Le registre contient 48 sélecteurs. Les quatre témoins ELF historiques disposent de journaux et d'empreintes dans l'archive. Les noyaux ordinaires reconstruits sur la base et sur les sources modifiées sont identiques octet par octet pour chaque cible. Leurs SHA-256 sont `6f6db625d222f010596e06339dc4c0aa5c9a6ffb89dfbac061a9d987a0994e25` (AArch64) et `411ea88e406c7e0e0fea1cb61a29269927d40704295dc7b40df3084f5287d29c` (x86_64). Le profil conserve le format disque P9.

L'archive versionnée contient 78 pièces et leur manifeste interne. Elle inclut les sorties série, les métadonnées TLS, les traces NBD, les trois instants monotones de chaque parcours, les empreintes des images initiales, les essais avant correction et la réserve IPC. SHA-256 de l'archive : `3d4771068b8667eacaa3ce6e448fe0b45dc305ce1d8a8ee65f33ffce84176f48`. Les 427 fichiers de l'archive locale complète sont inventoriés dans `OS-worktrees/preuves-autorite-invite-1e9fec9/MANIFESTE-LOCAL.sha256` (SHA-256 `ca6f491e3e5b369c6eaf12d9685af1556a6ca541a330bc1d0c0de1867b829248`). [Preuves de qualification](validation/horloge-signataire-v0.24.0.tar.gz)

### Reproduction

Exporter le profil depuis le commit à qualifier dans un répertoire neuf. Utiliser les certificats et clés jetables de laboratoire, le répertoire temporaire du projet et un adaptateur natif compilé. Le serveur hôte emploie les réglages TLS de Python/OpenSSL et les images de genèse du banc de lancement ELF. Le banc copie l'image initiale avant chaque essai. La commande suivante utilise des variables de chemin définies par l'opérateur.

```sh
python3 novaOS/user/novaauth/service/profile.py "$EXPORT" --horloge-signataire
# Après compilation du profil novaauth-sessions-lab et création de son ISO :
python3 novaOS/user/novaauth/autorite_qemu/horloge_qemu.py \
  "$ARCH" "$SECTEUR" "$MODE" "$ISO" "$GENESE" "$CERTIFICATS" "$SORTIE"
```

Utiliser `aarch64` et `x86_64`, les deux secteurs, puis `nominal` et `lent` avec une sortie neuve. Renseigner `NOVA_ADAPTATEUR` et `NOVAOS_TMPDIR`. Les preuves conservent le SHA-256 de chaque ISO exécutée.

## Limites d'intégration

Le test TLS confirme une identité d'instance sur le transport invité. Le composant externe de reprise dispose séparément de sept tests hôte, dont perte de réponse, redémarrage et admission concurrente. Il reste à raccorder les décisions de ce composant à la transaction des trois objets d'identité et au contrôle effectif des ouvertures dans le VFS invité. La recette décisive reste à exécuter sur les quatre profils : révoquer un droit avec reçu durable, restaurer tous les disques locaux, puis vérifier le refus d'ouverture avec l'ancien reçu, y compris lorsque l'autorité est injoignable.

## Capacité future de lecture monotone

Une évolution ordinaire pourrait accorder à un service identifié un droit explicite de lecture du temps écoulé. Le contrat devra fixer l'origine par démarrage, la résolution, la gestion des débordements, les destinataires et les règles de délégation et de révocation. Il devra aussi examiner le risque d'observation temporelle entre services. Cette capacité serait indépendante de l'administration des processus, du temps civil et de l'autorité qui garantit la fraîcheur des droits. Son introduction relève d'un arbitrage ABI et de sécurité distinct.
