# Client ELF pour les installations de laboratoire

Le paquet produit les ELF autonomes de laboratoire `cat`, `prefixe-temoin` et `temoin`. Le paquet frère `nova-cut` emploie le même client pour son adaptateur natif. Le moteur de sélection de CUT et son parseur proviennent de la révision `008d62b82d5d7c7ffb601bb15a0bdee09af1b6c1`.

## Bloc de démarrage

Le chargeur prépare la page DATA avant la première instruction. Les huit mots de l’en-tête sont en little-endian.

| Octets | Contenu |
|---|---|
| 0–7 | Signature ASCII `NXEXEC01` |
| 8–15 | Clé locale de lecture stdin |
| 16–23 | Clé locale d’écriture stdout |
| 24–31 | Clé locale d’écriture stderr |
| 32–39 et 40–47 | Canaux locaux de demande et de réponse du service de fichiers, tous deux présents ou absents |
| 48–55 | Longueur encodée des arguments, de 1 à 2 560 octets |
| 56–63 | Bits de présentation pour le traitement UTF-8, la documentation française et les options de compatibilité de CUT, respectivement bits 0, 1 et 2 |
| À partir de 64 | Nombre d’arguments sur un octet, puis longueur little-endian sur deux octets et octets UTF-8 de chaque argument. Maximum de 32 arguments. |

Le résolveur de confiance doit sélectionner la langue selon la politique BCP 47/IANA de NovaOS. Le client refuse les bits inconnus, un canal fichier isolé et les arguments tronqués. Une clé absente vaut zéro. Une valeur dans DATA n’accorde aucun droit à elle seule.

L’ELF interprète ses options avant d’appeler `open_at(0, path, READABLE)`. Le serveur de fichiers doit authentifier les canaux privés, contrôler le cwd et la racine de la session ainsi que les droits sur l’objet. Le consommateur en pipe lit stdin jusqu’à EOF. Les écritures transmettent les octets bruts sur le canal stdout borné. Une erreur après admission d’un préfixe termine l’opération.

Le témoin émet `T:` suivi des octets reçus. Il est construit hors de l’image de démarrage. Les trois ELF de chaque ISA ont été compilés séparément. La démonstration depuis `nsh` et l’installation à chaud dans le même invité restent à qualifier.

## Témoin de l’installation en cours de session

Sans argument, `temoin` écrit les douze octets `41 09 42 09 43 0a 44 09 45 09 46 0a` sur stdout. Avec des arguments, il restitue chacun sous forme de longueur u16 little-endian suivie des octets UTF-8. Pour `temoin '' 'a b' -x`, l’oracle est `00 00 03 00 61 20 62 02 00 2d 78`. Dans les deux modes il utilise uniquement stdout. La construction de cet ELF est indépendante de l’image de démarrage.

## Conversation administrative expérimentale NXADMIN1

La feature explicite `administration-qemu` ajoute un contrat distinct, réservé
au profil de qualification : en-tête de 80 octets, arguments à l'offset 80,
canal administratif à l'offset 64, mot réservé nul à l'offset 72. Les deux
champs de canaux fichiers restent nuls. Le décodeur NXEXEC01 refuse ce bloc ;
les anciens en-têtes de 64 octets et leur sémantique sont conservés.

`admin-sonde` exerce des consultations avec une feuille privée liée à
l'installation attestée, l'instance et la session. NovaAuth contrôle à chaque
requête les capacités effectives ; la présence de cette feuille ne suffit pas.
Les mutations administratives ne sont pas disponibles dans ce raccord R1.
Un échange interrompu ferme la conversation et rend `UNCERTAIN`, sans répétition
automatique. Voir le [contrat et les limites de qualification R1](../../novaOS/docs/novaauth-administration-r1-qualification-v0.24.0.md).


## Façades R2 en cours de raccordement

La même feature expérimentale construit six ELF indépendants : `whoami`, `id`,
`groups`, `useradd`, `usermod` et `groupadd`. Leur parseur et leur client NXADMIN1
sont testés sur l'hôte ; leur compilation sur les deux cibles nues ne constitue
pas encore une qualification des mutations dans l'invité.

Chaque ELF expose `--help`, `--version` et l'argument `man`, localement et sans
requête administrative. L'entrée du catalogue `man` du shell reste à raccorder.
Les notices et diagnostics utilisent le bit de langue fourni par le chargeur
(fr en français, sinon anglais). Les identifiants, empreintes et états de sortie
sont des champs de protocole stables. stdout et stderr conservent leurs octets.

Une mutation s'effectue en deux invocations : arguments métier avec `--prepare`,
puis les mêmes arguments avec `--confirm EMPREINTE --request ID`. La seconde
invocation revérifie la préparation avant de confirmer une seule fois. Le
registre devra rendre cette préparation idempotente pour la même session,
tête et action, et rejeter tout changement de contexte. Une perte de réponse
renvoie `UNCERTAIN` et l'identifiant connu ; `--outcome ID` consulte seulement
l'issue. Ce client n'accorde aucun droit, ne supprime aucun utilisateur, groupe
ou fichier et n'ajoute aucun privilège administratif.

Le format de réponse de confirmation/issue est privé à R2 : statut natif u64 LE
aux octets 0–7, identifiant aux octets 8–23, génération u64 LE aux octets 24–31,
état complet égal à 1 à l'octet 32, reste nul. Un état incomplet ne devient pas
un succès CLI. Les réponses de consultation partagent une empreinte de tête
aux octets 80–111 ; une énumération mélangeant deux têtes est refusée.

Le raccord serveur de ces mutations, les effecteurs de révocation et les deux
sessions doivent encore être qualifiés sous QEMU avant de déclarer R2 achevée.
