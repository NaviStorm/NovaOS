# Killer E2400 — témoin avant extension DMA de phase 5B

Le 27 novembre 2026, les bancs existants ont été rejoués sur la livraison `7c3bb99a2f63326818024ac8e498ec329c6ad345`. La branche de travail `codex/killer-e2400-phase5b` dérive directement de cette révision. Ces résultats fixent le point de comparaison pour l’extension demandée au § 2 du mandat.

| Mesure de référence | ARM64 Darwin natif | x86_64 Darwin sous Rosetta |
|---|---:|---:|
| Tests E2400 historiques | 40/40 | 40/40 |
| Tests du gestionnaire, comprenant les transitions, quotas, allocations et reçus | 28/28 | 28/28 |
| Taille du groupe / registre de 32 groupes | 400 / 12 800 octets | 400 / 12 800 octets |
| Cycles du gestionnaire / variation finale du tas mesuré | 256 / 0 octet | 256 / 0 octet |
| Temps écoulé du cycle complet p50 / p99, en µs | 630,333 / 830,458 | 1 512,417 / 1 549,208 |

Le cycle inclut sélection, contrôle IPC simulé et retrait. Ces temps écoulés ne sont pas le coût CPU isolé d’une opération de registre. Le vecteur historique exerce un groupe de 22 pages DMA. L’essai E2400 vérifie séparément trois refus de réservation à 259 sans attribution. Les valeurs détaillées, le compilateur, l’état de traduction Rosetta et les empreintes des journaux figurent dans le relevé. [Mesures](validation/killer-e2400-phase5b/temoin-5a.json) · [Journaux bruts](validation/killer-e2400-phase5b/temoin-5a.tar.gz)

Un démarrage Q35 TCG sans NIC, sous le verrou partagé, a exercé les six sessions historiques. Les quatre sessions de trafic valident les octets de 288 TX/RX chacune. Les deux pannes valident quatre tickets incertains et quatre annulés par session. Le groupe utilise quatre racines pour le service, zéro pour les deux autres acteurs, deux boîtes et huit pages IPC en PUT/TAKE ou zéro boîte et six pages IPC en DATA. L’audit retrouve 61 505 cadres libres après chaque session. Les 48 sélecteurs et le plafond de huit crédits sont ceux du témoin. Aucun cadre DMA matériel n’a été attribué. [Qualification de référence et commandes de reproduction](killer-e2400-phase5a.md)

La limite de 128 porte sur les pages du groupe dans le gestionnaire simulé. Elle est distincte du masque des membres de l’inventaire PCI. Le reçu synthétique de 4C rassemble une preuve d’arrêt de la source et deux ensembles de pages pour le retrait des traductions et le retour de propriété. La garde de réadmission Q35 s’applique avant la réservation lorsqu’un bail NVMe est actif. Le mandat exige de conserver la distinction entre une preuve du banc et un reçu matériel. [Gestion des groupes](../libs/gestionnaire-pilotes/src/manager.rs) · [Reçus du cycle de vie](../libs/gestionnaire-pilotes/src/lifecycle.rs) · [Périmètre matériel de 4F](gestionnaire-pilotes/phase4f-v0.25.0.md)

L’inventaire MSI et ses commandes de lecture sont préparés pour l’opérateur. Aucun relevé privé de machine n’est joint à ces preuves. [Protocole Windows et limites d’observation](killer-e2400-inventaire-msi-5b.md)

Le registre étendu, le courtier simulé et la comparaison avec ce témoin sont qualifiés dans la livraison 5B. [Rapport de phase 5B](killer-e2400-phase5b.md)
