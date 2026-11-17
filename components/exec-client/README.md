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
