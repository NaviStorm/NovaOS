# Qualification des notifications parasites et du délai de complétion

Le 13 novembre 2026, quatre invités QEMU 11.1.3 en TCG à un vCPU ont exercé le pilote IRQ du prototype, sur des disques RAW jetables de 1 Mio ouverts en lecture seule. Chaque invité a relu le motif attendu, retiré le transport et vérifié une lecture exacte par un nouveau propriétaire. L’image entière a ensuite été comparée octet par octet au motif initial. Son SHA-256 est `94ba47c073ace5a030361f638775f28fa577cbf940b0c0f61509a51d903130d2` dans les quatre cas.

## Injections et observations

Sur AArch64, le banc bloque une lecture avec `blkdebug break read_aio`. Un second thread invité attend que le propriétaire soit parqué sur son événement. Il rend le SPI37 pendant en écrivant dans `GICD_ISPENDR1`. QEMU livre l’interruption via le GICv3, les vecteurs EL1 et le gestionnaire habituel. L’accès à l’ISR VirtIO produit un registre sans bit de file. Le banc vérifie après chaque livraison le compteur des parasites, l’état masqué ou démasqué de la ligne, la conservation du ticket et l’absence de réveil. Le retour QMP de `wait_break` établit que le backend a bien suspendu la lecture. [Injection invitée](../../tests/fixtures/io-0166-irq-injection.patch) · [Contrôle QMP](../../tests/irq_defauts.py)

| Architecture et géométrie | Injection | Temps dans la collecte, ms | Résultat |
|---|---|---:|---|
| AArch64 / 512 | 1 SPI puis reprise du backend | 7,376 | 1 parasite, 1 parcage, 1 réveil par la vraie IRQ, lecture exacte |
| AArch64 / 512 | 8 SPI successifs, backend retenu | 1 242,951 | ligne masquée à la huitième assertion, 1 réveil par délai, 1 repli, 1 timeout |
| AArch64 / 4 096 | 1 SPI puis reprise du backend | 1,634 | 1 parasite, 1 parcage, 1 réveil par la vraie IRQ, lecture exacte |
| AArch64 / 4 096 | 8 SPI successifs, backend retenu | 1 242,796 | ligne masquée à la huitième assertion, 1 réveil par délai, 1 repli, 1 timeout |
| x86_64 / 512 | MSI-X masqué avant la lecture | 1 127,592 | `used.idx` publié, MSI-X pendant, 1 réveil par délai, 1 repli, 1 timeout |
| x86_64 / 4 096 | MSI-X masqué avant la lecture | 1 157,015 | `used.idx` publié, MSI-X pendant, 1 réveil par délai, 1 repli, 1 timeout |

Le premier SPI AArch64 laisse le ticket 65 537 en attente et la ligne active. La vraie IRQ après reprise consomme ce ticket et remet à zéro le compteur de notifications sans bit de file. Dans l’essai suivant, les huit SPI laissent le ticket 65 538 intact. Après le timeout, la reprise du backend publie effectivement `used.idx`. La ligne reste masquée, le compteur des IRQ acceptées reste inchangé et la requête reste en vol pour le pilote jusqu’au retrait. Sur x86_64, le bit PBA MSI-X est pendant alors que `used.idx` désigne déjà la réponse. Le pilote renvoie `Timeout` et conserve le ticket. Les anciens cadres sont retirés par le chemin de reset et de purge avant ouverture du nouveau transport.

Le temps affiché commence à l’entrée de la collecte après soumission et se termine à son retour. Les essais ont tourné en parallèle pendant les régressions. Sur AArch64, CNTFRQ vaut 62 500 000 Hz. Sur x86_64, la calibration PIT de l’injecteur a estimé le TSC à 1 252 175 781 Hz en 512 et 1 215 523 437 Hz en 4 096. Le pilote calibre séparément sa propre échéance une fois à l’ouverture du disque sur 64 ticks PIT, soit environ 256 ms nominales. Ces calibrations précèdent l’intervalle mesuré et sont sensibles à l’ordonnancement de l’invité. La progression du vCPU et la livraison périodique du timer sont nécessaires à son réveil. Le repli vérifie le même ticket et conserve le plafond de quatre millions d’itérations et l’échéance totale de deux secondes d’horloge invitée. [Pilote évalué](../../tests/fixtures/io-0166-irq-attente.patch)

Le premier passage de mise au point, avant le formatage final de l’injecteur, a aussi terminé les six cas. En 512 et 4 096, les durées des SPI isolés étaient respectivement de 15,129 et 2,326 ms. Les timeouts AArch64 étaient de 1 239,748 et 1 237,630 ms et les timeouts x86_64 de 1 426,666 et 1 413,677 ms. Les temps des SPI isolés comprennent le délai de reprise QMP.

## Reproduire

Employer une archive IRQ hors sonde et y appliquer `io-0166-irq-injection.patch` avec `patch -p1 -F0`. La fixture remplace la sonde de démarrage `dma-qual` par le scénario du banc. La collecte et les deux contrôles d’échéance du candidat restent ceux de la fixture IRQ. Le banc ajoute des compteurs de diagnostic, une lecture du drapeau de parcage et l’injecteur GIC. La réapplication des deux patches a donné six fichiers identiques octet par octet à l’archive utilisée.

Depuis la racine du dépôt, avec `ARCHIVE` chemin absolu de cette copie et `SORTIE` dossier de travail, exécuter les commandes suivantes.

```sh
make -C "$ARCHIVE/novaOS" ARCH=x86_64 kernel
(cd "$ARCHIVE/novaOS" &&
 RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build \
   --release --locked --target aarch64-unknown-none-softfloat --features gicv3-lab)
for arch in aarch64 x86_64; do
  for secteur in 512 4096; do
    python3 -B novaOS/tests/irq_defauts.py "$ARCHIVE" "$arch" "$secteur" "$SORTIE/$arch-$secteur"
  done
done
```

L’ELF AArch64 mesuré a pour SHA-256 `ca41161c1c8e1d78c76c43410251aa19a8b35ed82f53fffd6106f3e97b5fcb55`, l’ELF x86_64 `3661c1bd3e3b7d0ab7c2bde68cf8dbeef595adeed4a8db0f0d941efc6249bfb5`. Le JSON généré pour chaque invité contient les ticks, la fréquence, les huit compteurs, les empreintes du disque, de l’ELF et du journal série. Il s’agit de qualifications du pilote et de l’acheminement des interruptions dans QEMU direct. La durée d’une sauvegarde complète dans l’application UTM relève de la campagne distincte. [Campagne UTM et QEMU](../es-v0.16.6-irq.md)
