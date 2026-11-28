# Vérificateur VT-d multi-unités 5U

Bibliothèque Rust `no_std` avec `alloc`, sans dépendance Cargo externe. Elle prend des données figées en mémoire. Aucun accès PCI, MMIO, VT-d, USB ou disque. Son interface retourne des faits de topologie ou un refus, sans capacité d’activation. Le diagnostic de laboratoire 5R demeure un exécutable distinct. [Décision MSI et sources](../../docs/diagnostic-uefi-5u.md)

## Entrées et résultat

1. `Inventaire::decouvrir` prend la liste des tables DMAR et la liste complète des fonctions PCI du dossier. Une DMAR valide peut contenir plusieurs DRHD. Le décodeur partagé 5I borne les enregistrements et les chemins. La couche 5U contrôle l’en-tête et la somme ACPI, valide les ponts et parcourt chaque chemin de portée depuis son bus de départ. Une portée de sous-hiérarchie inclut le pont et ses descendants du même segment. Un endpoint inconnu, un pont manquant, deux portées concurrentes ou une deuxième unité par défaut entraînent un refus. Pour chaque segment le défaut vient après ses unités explicites et couvre les fonctions restantes. Une portée IOAPIC/HPET ne donne pas de couverture DMA à une fonction PCI. [Décodeur réutilisé](../diagnostic-5e/topologie5i.rs) · [Résolution 5U](lib.rs)
2. Chaque fonction déclare son titulaire canonique. Un alias connu doit résoudre vers la même unité que son titulaire et, dans chaque cliché, vers le même domaine. Deux titulaires distincts doivent avoir des couples unité/DID distincts. Le même numéro de domaine peut donc être utilisé sur deux unités. Le résolveur refuse une chaîne d’alias, un cycle, un titulaire absent ou une attribution inter-unités. Un RID alias qui n’apparaît pas dans la liste espérée ne peut pas être découvert par ce modèle. Le fournisseur doit établir l’exhaustivité des demandeurs et des alias, y compris les chemins entre pairs, avant tout usage d’admission.
3. `Inventaire::comparer` prend séparément les rôles xHCI, Ethernet et SATA, les pages protégées, les baux attendus et les deux clichés. Chaque cliché doit contenir exactement les unités, les contextes et les mappings attendus, une traduction active et une racine distincte par unité dans la RAM protégée. Les baux sont exprimés en IOVA, adresse physique, longueur et sens vus du périphérique. Le vérificateur refuse les débordements, les recouvrements de prêts, les prêts aux registres ou aux pages protégées et les droits supplémentaires. Une RMRR doit être couverte intégralement par un bail d’identité du titulaire concerné. Une réservation firmware ne peut ainsi devenir un nouveau droit implicite.
4. La comparaison exige des racines, contextes et permissions stables. Elle préserve le titulaire et la génération SATA, le registre de commande PCI, la plage MMIO dédiée et les valeurs des compteurs de lecture, écriture et FLUSH fournis par le témoin. Un périphérique réseau ou USB ne peut emprunter un cadre du stockage, même en lecture. Les classes PCI servent à confirmer les trois rôles désignés dans le dossier. [Vérification des baux et du SATA](permissions.rs)

| Borne de ce profil expérimental | Valeur ou traitement |
|---|---|
| Table | Une DMAR de révision 1, de 48 octets à 16 Kio. Adresse hôte de 32 à 64 bits. |
| Enregistrements DRHD et RMRR réunis | 32, jusqu’à 32 unités. Registres DRHD de 4 Kio. |
| Portées de périphériques | 128 au total, jusqu’à 16 couples périphérique/fonction par chemin. |
| Fonctions, alias compris | 64. La couverture d’une portée occupe un masque de 64 bits. Les réservations conservent leur plage et ce masque. |
| Politique | 128 plages protégées et 256 baux maximum. Pages de 4 Kio, domaines à permissions explicites. |
| Hors profil | Structure inconnue, ATSR, ATS actif, traduction directe, alias non attribué, portée irrésolue ou réservation partagée entre titulaires. Le refus conserve l’incertitude. |

Les tableaux de tailles variables sont empruntés ou alloués avec réservation faillible après contrôle des limites. Une déclaration de RAM protégée doit venir d’une autorité indépendante des mappings observés. Les clichés normalisés décrivent les feuilles de traduction et l’état SATA que leur producteur a pu observer. La bibliothèque ne marche pas dans les tables physiques, ne mesure pas leur antériorité et n’authentifie aucun émetteur. Un changement suivi d’un retour à l’état initial peut échapper aux deux clichés. La clôture des DMA en vol, la provenance des preuves, la continuité pendant LoadImage et la mesure indépendante des E/S restent des exigences externes. L’attribution du panneau GOP et les décisions de Secure Boot ne font pas partie de cette interface.

## Reproduire les contrôles

Depuis la racine du worktree, sur le Mac ARM64 du laboratoire avec Rosetta et la chaîne rustup déjà installée, exécuter la commande suivante. Le script définit les temporaires du projet, vérifie le format des trois fichiers Rust du composant, compile et exécute chaque binaire de test sous l’architecture indiquée, construit trois bibliothèques cibles hors ligne et contrôle l’ABI. Chaque série reçoit un répertoire de compilation privé. Cargo reçoit explicitement le rustc du toolchain qui fournit les cibles.

```sh
sh novaOS/boot/diagnostic-5u/qualifier.sh
```

La base qualifiée est `188535dcf92bac7eb4dfae01b6b2de1e43606ceb` avec les sources 5U du présent commit. Les 18 tests spécifiques et les quatre tests partagés de 5I ont réussi sur Darwin ARM64 natif et x86_64 sous Rosetta. Les deux répartitions synthétiques comprennent un contrôleur USB isolé sur une unité explicite et un autre cas où seul le graphique relève de l’unité explicite. Les limites de 32 unités et 64 fonctions ont aussi été exercées dans un même dossier. Les compilations pour x86_64 UEFI, x86_64 nu et AArch64 nu produisent des bibliothèques. Aucun EFI n’a été amorcé. [Tests](tests.rs) · [Relevé des deux architectures](../../docs/validation/diagnostic-uefi-5u/hotes.txt) · [Empreinte du relevé](../../docs/validation/diagnostic-uefi-5u/SHA256SUMS)
