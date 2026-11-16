# NovaAuth v0.24.0 — Qualification de l’attribution d’un dossier à chaud

Le provisionneur de laboratoire a attribué D à Alice après son authentification, alors que son processus tournait déjà. La révocation a pris effet au retour du VFS dans les quatre configurations QEMU. Le test utilise le service de fichiers de l’invité et un volume P9 jetable. Le code exercé est `719c78083e3d02e2afd26e18f355f716e08fc356` sur la branche `codex/novaauth-0240-phase3`.

## Chaîne d’autorité

Le profil explicite `novaauth-sessions-lab` lance quatre processus isolés. Le vérificateur est le PID 6. Le provisionneur est le PID 7. Les clients de B (administrateur du registre) et de A sont les PID 8 et 9. Les trois derniers exécutent le même ELF de confiance avec des canaux initiaux distincts. Les clés privées restent dans les clients.

Le lanceur présente au provisionneur l’identité stable de A issue du registre et l’identifiant de sa session authentifiée. Le provisionneur approuve une seule attribution sur son canal privé. Le lanceur vérifie le message entier puis le transmet au VFS sur son propre canal d’enrôlement. L’effecteur contrôle le profil, le montage inscriptible, le titulaire actif, l’absence de dossier antérieur et l’identité de la demande. Il confirme la création de la politique sur P9 avant remise du handle. La politique lie le titulaire, le volume et les identifiants du dossier, du registre, de la politique et de la branche protégée. Le banc refuse le rattachement d’un dossier préexistant.

Pour une opération, le client adresse les données au VFS et présente sa session au vérificateur. NovaAuth atteste le canal, le titulaire, la session, la génération du registre par son empreinte et le numéro de requête. Le VFS exige le message correspondant de l’instance et vérifie l’empreinte courante. Il relit la politique, contrôle les objets, les droits et la chaîne de délégation avant d’agir. La tentative de lecture sous `prive` vérifie la protection héritée du parent. Le vérificateur ne possède aucun endpoint de données sur D. Le provisionneur comptabilise les racines de transport avec des droits ramenés à zéro après dérivation. Le plafond ordinaire d’endpoints est conservé.

## Résultats

QEMU 11.1.3, deux vCPU. Chaque durée couvre un démarrage, le scénario complet, les temporisations cryptographiques et l’arrêt. Les pages privées sont relevées au lancement dans l’ordre vérificateur / provisionneur / B / A. Le noyau relève aussi 64 pages de pile par processus. Une page mesure 4 Kio.

| Invité | Secteur P9 | Durée hôte | Pages privées | Relecture P9 indépendante | Redémarrages de contrôle réussis |
| --- | ---: | ---: | --- | --- | ---: |
| AArch64 | 512 | 92,39 s | 19 / 18 / 18 / 18 | 2 enregistrements | 3/3 |
| AArch64 | 4 096 | 71,45 s | 19 / 18 / 18 / 18 | 2 enregistrements | 3/3 |
| x86_64 | 512 | 74,12 s | 21 / 21 / 21 / 21 | 2 enregistrements | 3/3 |
| x86_64 | 4 096 | 62,30 s | 21 / 21 / 21 / 21 | 2 enregistrements | 3/3 |

- Avant attribution, la session de A reçoit un refus. Après attribution, le même processus lit D. B reçoit le numéro réel du handle pendant qu’il est valide et obtient un refus. Une autre session authentifiée de A ne peut pas davantage l’utiliser.
- A dérive un parent, un enfant, et un témoin en lecture seule directement depuis la racine. Le VFS refuse l’écriture par le témoin et l’accès au descendant surprotégé. Le retrait du parent invalide parent et enfant avant l’acquittement. Le témoin reste utilisable jusqu’au retrait distinct de la racine. A reste authentifiée après le retrait.
- Une écriture admise via l’enfant est annulée avant son exécution. La tentative de validation reçoit `REVOKED`. Une deuxième écriture atteint P9, puis le VFS omet sa réponse de données. Le client obtient `UNCERTAIN`, interroge l’issue par identifiant et retrouve une seule occurrence. Une troisième écriture reçoit une confirmation normale. Le banc ne soumet pas une seconde fois la mutation incertaine.
- Après arrêt, le lecteur indépendant ouvre P9 en lecture seule et vérifie les 16 octets exacts, l’absence de l’opération annulée, les identifiants des objets, le titulaire initial, sa tombe et le titulaire distinct créé sous le même nom. Les SHA-256 de P9 et de l’autorité sont identiques avant et après relecture. Les traces contiennent la collecte des quatre processus et l’arrêt des deux vCPU.
- Sur chaque profil, trois redémarrages sur copies vérifient le refus d’un nouvel enrôlement du domaine existant, le refus du volume revenu à son état initial avec autorité actuelle et le refus après perte de l’autorité. Les clés du banc sont éphémères. L’authentification avec une clé persistante après redémarrage reste ouverte.

## Portée et réserves

La révocation est sérialisée dans un effecteur, sur un maximum de douze handles et une préparation pendante. La campagne établit le point de retrait pour cette chaîne et l’absence de double écriture. Le témoin de lecture est une branche sœur sous la même racine. Le transfert de handle par un processus propriétaire reste un scénario ultérieur. La politique enregistrée d’une installation arrêtée, un droit VFS dérivé d’un groupe, deux révocations simultanées à quatre vCPU et les acquittements tardifs après redémarrage de l’effecteur restent à qualifier. Le retrait du facteur et le mandat crontab de laboratoire sont décrits dans le [rapport de qualification des sessions](novaauth-v0.24.0-phase3.md).

L’ACL de propriétaire et le sous-dossier surprotégé exercent la règle de refus du témoin. Le déverrouillage cryptographique ARC-017, les caches et fichiers privés généraux, SSH et crontab en production restent hors de cette démonstration. Les relevés mémoire ne mesurent ni le pic résident ni un bilan de tous les cadres. Le temps de retrait isolé n’a pas été mesuré. La qualification concerne QEMU. Aucune coupure électrique physique ni restauration conjointe du volume et de son autorité logicielle n’a été qualifiée.

Les révisions intermédiaires et leurs échecs figurent dans l’archive complète. Le raccord initial dépassait le budget de racines IPC. Une attente de 20 000 ticks dépassait également la limite native de 15 000. Le banc final répartit les racines entre les acteurs et utilise un délai admissible. Sur 512 octets, un ancien parcours échouait en fin de session administrative. Le banc vérifie désormais B pendant la validité du handle puis réauthentifie B avant les opérations administratives tardives. La durée de session demeure 60 s.

## Compatibilité, reproduction et preuves

Les deux noyaux ordinaires reconstruits sont identiques octet par octet à la référence ordinaire archivée. Les images complètes du vérificateur, du client, du VFS et du gestionnaire utilisés pour ce banc sont présentes dans les noyaux de qualification et absentes des binaires ordinaires correspondants. Les neuf contrôles de registre ABI réussissent. Le diff de `novaOS/abi` et de `novaFS/` est vide depuis la base de ce complément. L’ABI v1 conserve 48 sélecteurs, NovaFS reste en v0.9.13 P9. Les preuves des services, du cycle de vie, des IPC et des deux pipes du profil ordinaire ont été réutilisées après comparaison des octets et des empreintes des résultats précédents. L’intermittence historique du comparateur de pipe x86_64/4096 reste consignée.

Les commandes de construction du [rapport initial](novaauth-v0.24.0-phase3.md#références-et-reproduction) s’appliquent au commit indiqué. À chaque exécution, fournir un répertoire de sortie neuf et la genèse correspondante. Exécuter l’exemple Cargo `relire_dossier` avec ce répertoire et la taille de secteur après fermeture de QEMU. Pour les trois contrôles à froid, passer à `qemu.py` les options `--apres` et `--attendu initialise|recul|autorite`.

L’archive complète se trouve dans `OS-worktrees/archives-novaauth-dossier-0240/qualification-719c780.tar.gz`. Elle contient 306 fichiers de preuve et leur manifeste. Chaque membre a été décompressé et comparé au fichier original par SHA-256. L’extrait Git contient le manifeste complet, les journaux série du candidat, les résultats des seize démarrages et les constats de relecture. [Empreintes des archives](validation/novaauth-dossier-phase3-0240.json) · [Extrait publié](validation/novaauth-dossier-phase3-0240.tar.gz)
