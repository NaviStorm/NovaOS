# Temporaire privé par instance · NovaOS v0.21.0, phase 2

Une instance applicative lancée sur P9 reçoit une racine vide avant sa première instruction utilisateur. Le propriétaire est le triplet `(BootID, InstanceID, génération)` du gestionnaire. Un refus de création termine l’admission. Les services internes de stockage démarrent indépendamment pour pouvoir fournir cette racine. Deux ELF témoins historiques identifiés par leur image embarquée peuvent exécuter les épreuves d’instances et de threads sur le profil sans disque. Ils n’y obtiennent aucune capacité temporaire. Le [contrat du BootID](bootid-phase1-v0.21.0.md) fixe les conditions de confiance par plateforme.

## Accéder à sa racine

Le nom logique `tmp` désigne une vue de répertoire obtenue par la porte privée `CAP`. Une application présente cette capacité à `CALL` avec un tampon de réponse et de demande accessible en lecture et écriture. L’appel transporte une [trame fichier](../abi/files.rs) de 128 octets. La session privée vaut 1 et le handle initial 0 désigne la racine. Par exemple, `OPEN_AT` avec `CREATE | WRITABLE` et le nom relatif `sortie` ouvre un nouveau fichier. `WRITE` prépare ses octets et le succès de `SYNC` confirme leur publication sur le profil de stockage. `CLOSE` ferme le handle.

Les cinq sélecteurs privés vont de `0x10076` à `0x1007a` dans l’ordre `CAP`, `CALL`, `WORKER`, `NEXT`, `COMPLETE`. Le courtier marque ses enveloppes internes `CREATE`, `REMOVE` ou `IO`. Les trames encapsulées réutilisent les opcodes fichier existants. Le registre ABI centralisé de la branche indépendante `codex/abi-registry-v1`, au commit `df88f1cfeaf0940da876ea543584e59215abacd4`, recense les 41 portes antérieures. Inscrire et qualifier les cinq nouveaux sélecteurs dans ce registre lors de la coordination des deux branches.

Le noyau vérifie la capacité locale du processus et que l’instance est encore active. Il inscrit lui-même les 32 octets d’identité dans la demande remise au service. Seul le processus NovaFS admis pour ce montage peut prendre une demande et l’acquitter. Un nom, un OID copié depuis un résultat de `STAT`, une valeur de capacité devinée ou le BootID public ne confère aucun droit. Même `nsh`, lorsqu’il détient séparément une vue de fichiers publique, ne peut énumérer le conteneur des temporaires. [Portes et enveloppe](../abi/tmp.rs) · [Courtier du noyau](../kernel/src/sched/process/tmp.rs)

Les threads utilisent la table de leur processus. Le lancement ordinaire d’un travailleur de la même instance lui laisse sa capacité temporaire vide. Avec le droit préexistant de création, le principal peut demander `tmp::WORKER` en présentant son identité complète. Le noyau rattache le travailleur et lui délègue la même vue. Sa sortie retire ses propres requêtes. Les handles de la vue restent utilisables par l’instance tant que le principal est vivant.

La résolution part de la racine privée ou d’un handle de cette vue. Les chemins absolus, les composants `..` et les caractères de contrôle sont refusés. Les deux extrémités d’un renommage doivent être accessibles dans la même vue. Les portes d’administration, le montage, l’ouverture historique absolue et la délégation d’une session publique sont inaccessibles par cette capacité. Le catalogue P9 de ce profil représente des fichiers ordinaires et des répertoires. [Résolution VFS](../user/novafs/files/namespace.rs) · [Service](../user/novafs/files/temporary.rs)

## Stockage et preuve d’appartenance

Le service reste l’unique écrivain sur le volume NovaFS. La racine réservée porte le nom interne `\x1fNovaOS-tmp-v1`, dont le premier octet est le caractère de contrôle 0x1f. Le VFS public filtre cette entrée et les préparations à la racine. Les droits de fichiers ne donnent aucun droit brut sur le volume. Le sceau `registre` et le marqueur individuel sont des fichiers P9 ordinaires de 104 octets.

| Octets | Valeur contrôlée au retrait |
|---|---|
| 0–7 | Signature de version `NVTMP001` |
| 8–39 | Identifiants de système de fichiers et de volume logique, 16 octets chacun |
| 40–55 | OID du conteneur et du sceau |
| 56–87 | Identité complète de l’instance, ou 32 zéros pour le sceau |
| 88–103 | OID du marqueur et du répertoire de données |

Les OID sont des entiers little-endian. Les deux noms `r<identité>` et `t<identité>` utilisent les 32 octets encodés en hexadécimal. Le service crée le marqueur, publie son contenu, puis crée le répertoire vide avec l’OID attendu. Chaque publication emploie les transactions P9 existantes. La capacité devient utilisable après confirmation de la création. Une collision, un sceau inconnu ou un type inattendu interdit l’adoption de l’objet. Le contrôle de provenance repose sur l’exclusion des autres écrivains et la validation P9. Un auteur capable de réécrire intégralement le volume brut se trouve hors de ce modèle. [Implémentation du registre](../user/novafs/files/tmp_store.rs)

## Retrait, coupure et reprise

La sortie, l’arrêt forcé ou la faute du principal ferme l’autorité avant le nettoyage. Le courtier sérialise la requête déjà remise au service et le retrait. Le service invalide la table de handles et abandonne les fragments privés non synchronisés. Si le chemin public détient une réservation de blocs pour une écriture en cours, le retrait physique attend sa libération. Une annulation ou une expiration avant remise peut être acquittée comme telle. Après remise d’une opération dont l’issue est encore inconnue, le client reçoit `UNCERTAIN`.

La purge contrôle le sceau, les noms et les OID du marqueur et de sa racine. Elle descend par les enfants du catalogue, détache chaque feuille, retire la racine puis le marqueur. Une seconde notification après retrait complet est sans effet. Un parent inconnu, un marqueur incomplet ou un OID substitué demeure inaccessible. La restitution physique peut aussi terminer la collecte P9 d’objets déjà détachés sur ce volume.

Avant la première admission du nouveau démarrage, le service vérifie le registre et reprend le retrait des anciennes incarnations dont il peut établir la provenance. Il conserve en quarantaine une entrée ambiguë ou de l’époque courante. Les préparations non publiées restent cachées. Deux emplacements de préparation et 65 noms au maximum dans le registre bornent la croissance des métadonnées, y compris après des coupures répétées. Deux préparations devenues invérifiables peuvent bloquer les admissions. Les données en quarantaine continuent de consommer de l’espace.

Le service accepte au plus 32 vues actives ou en attente de purge, 32 handles par vue et un transfert préparé de 64 Kio par vue. Le volume conserve ses propres quotas. Le temps de purge dépend du nombre d’entrées et d’extents morts. Pour un grand arbre, l’attente du lanceur peut expirer. Une sauvegarde brute ou un snapshot P9 peut conserver des octets détachés. La restauration d’une VM entière, mémoire comprise, peut restaurer les capacités et le BootID du même instant. La détection d’une telle restauration demande un signal de fraîcheur indépendant.

## Compatibilité P8/P9

| Profil de lancement | Volume P9 | Volume P8 |
|---|---|---|
| Lanceur v0.21.0 (`cli`, `cli-test`) | Création obligatoire du temporaire persistant avant l’entrée dans l’application | Refus explicite de format avant écriture |
| Compatibilité historique (`cli-p8`, `cli-p8-test`) | Refus du format par ce profil | Contrat CLI historique, sans création de temporaire ; une session de lecture préserve la génération |

La sélection du profil est explicite au démarrage. Les bancs construisent les volumes de test à l’avance. L’ABI native v1 et le SDK `nova-system` 0.2.0 conservent leurs interfaces existantes. Les portes privées sont additives. La [qualification de phase 2](tmp-prive-phase2-v0.21.0.md) distingue essais invités, injections du moteur et limites du support.
