
preuves/mesures-mac.json — tr (nova-cli) — nova-cli 0.20.0 / tr (GNU coreutils) 9.12
Nova SHA-256 30c43ada0b43d245ab93a3456df3b81695df8859e3cbd9c4afbd47f195222e24
GNU SHA-256  075bd855ba66facf7309fcc0eb6f06ca98195e58c3fbe949b3cd65fe1568c77f
| Charge | Arguments tr | Entrée (octets) | Sortie (octets) | Nova (ms) | GNU (ms) | Nova/GNU | Plafond 1,10 | SHA-256 sortie |
|---|---|---:|---:|---:|---:|---:|---|---|
| 1 M lignes | `a-z A-Z` | 8000000 | 8000000 | 4.9838 | 8.1615 | 0.61064 | respecté | `5996560a8b4d066f5bf4c2a45ee8dae92a6ad89bf28d85ee80ebc11efe92f47e` |
| 1 M lignes | `-d X` | 8000000 | 7000000 | 10.9261 | 8.2556 | 1.32348 | dépassé | `8dd5e047c82bf085f7651111cf48dbfea63d29238ddc9f7bd579f64e5a6d165d` |
| 1 M lignes | `-s aX` | 8000000 | 8000000 | 9.7907 | 46.7362 | 0.20949 | respecté | `82eb6a1e6f03f6f51bf8d271ea5e3e36b5c622621f53a3fc7226da987f1add79` |
| 1 M lignes | `-ds X a` | 8000000 | 7000000 | 11.3610 | 28.7787 | 0.39477 | respecté | `8dd5e047c82bf085f7651111cf48dbfea63d29238ddc9f7bd579f64e5a6d165d` |
| 10 M lignes | `a-z A-Z` | 80000000 | 80000000 | 24.4627 | 56.1350 | 0.43578 | respecté | `9cbb042b843fc29e70a9149f8f7a60823b8a0ddb386c3f44ff6b7161c9e8c889` |
| 10 M lignes | `-d X` | 80000000 | 40000000 | 49.2082 | 66.4348 | 0.74070 | respecté | `eede3991b65f92db8cdcde0b27519da3883e3902b54ec3405014acb2e0c023ce` |
| 10 M lignes | `-s aX` | 80000000 | 30000000 | 48.3352 | 427.8673 | 0.11297 | respecté | `819d0dea151623cf586df0fa20f464691ca212b923653b1158c13b3294b077f9` |
| 10 M lignes | `-ds X a` | 80000000 | 20000000 | 39.8235 | 257.5057 | 0.15465 | respecté | `dcdcfef582ffe48eac454404ad1f4bd71c8577d6d630be3a0b74cdfbc330d795` |
| 65 M lignes | `a-z A-Z` | 521048576 | 521048576 | 139.0190 | 346.6521 | 0.40103 | respecté | `50de09cf94795b951054fbd341dbe3b2a5ff6d6f4706013d698acb042d823e6c` |
| 65 M lignes | `-d X` | 521048576 | 261048576 | 306.0534 | 409.8508 | 0.74674 | respecté | `8d0634397aa88dc3554dddc74ac973701476fa471c20b79968d42eed2ec05684` |
| 65 M lignes | `-s aX` | 521048576 | 325000000 | 338.2585 | 5397.9470 | 0.06266 | respecté | `3eef4016e54c45f0f53513b3f7e96094d135d0b2bbed437eb84a272c175c122e` |
| 65 M lignes | `-ds X a` | 521048576 | 130000000 | 234.6778 | 1644.4741 | 0.14271 | respecté | `ff23fce618b9d97524bb1e2fc3a4a167cf1e5f0db8198d8b60a12a88e16c3915` |
| 32 Mio binaire | `a-z A-Z` | 33554432 | 33554432 | 12.3985 | 25.0707 | 0.49454 | respecté | `1e137f270fa2c8f441739b9b3f93dd638188674e37af555a9d9a11c4b2b8a695` |
| 32 Mio binaire | `-d X` | 33554432 | 33423360 | 36.8405 | 28.1818 | 1.30724 | dépassé | `5d287989a93abd09ca4b3ce1a1aa413e45daff68b0e01377c6c1f92f4edfbb2d` |
| 32 Mio binaire | `-s aX` | 33554432 | 33554432 | 32.1142 | 26.6590 | 1.20463 | dépassé | `e09320c5b00b34bb704802136c599a95b3996332ba84d7c7f21112b6231b6bd0` |
| 32 Mio binaire | `-ds X a` | 33554432 | 33423360 | 34.7852 | 35.2295 | 0.98739 | respecté | `5d287989a93abd09ca4b3ce1a1aa413e45daff68b0e01377c6c1f92f4edfbb2d` |

3 dépassement(s) sur 16 charges.

preuves/recette-courte-2026-11-17/mesures.json — tr (nova-cli) — nova-cli 0.20.0 / tr (GNU coreutils) 9.12
Nova SHA-256 30c43ada0b43d245ab93a3456df3b81695df8859e3cbd9c4afbd47f195222e24
GNU SHA-256  075bd855ba66facf7309fcc0eb6f06ca98195e58c3fbe949b3cd65fe1568c77f
| Charge | Arguments tr | Entrée (octets) | Sortie (octets) | Nova (ms) | GNU (ms) | Nova/GNU | Plafond 1,10 | SHA-256 sortie |
|---|---|---:|---:|---:|---:|---:|---|---|
| 100 000 lignes journal | `a-z A-Z` | 5200000 | 5200000 | 3.5008 | 5.1830 | 0.67544 | respecté | `3352506f36948bbef85cd9d5f85a1e34d0e63d1522351affa4e0a6423cb3e947` |
| 4 096 lignes répétées | `-d X` | 9965568 | 1576960 | 6.0775 | 10.0520 | 0.60460 | respecté | `9d6d2dd68f1ae271f93e2f1f09435bae3679d4e1f83cd2f614aa3664f055c52c` |
| 4 096 lignes répétées | `-s aX` | 9965568 | 12289 | 6.6115 | 7.9157 | 0.83525 | respecté | `6188c598d99eb3c891e0f7b296c224ed834bf7afc9f52d5512eda86db798e0e0` |
| 4 096 lignes répétées | `-ds X a` | 9965568 | 8192 | 5.8482 | 11.5443 | 0.50659 | respecté | `b16aa539084042802c278f51593235f4856ce1eea58b6a23485c795ac58d17d2` |
| 1 048 576 octets | `a-z A-Z` | 1048576 | 1048576 | 2.5447 | 2.6860 | 0.94737 | respecté | `0ac253f625925f48b65855bc4f322ceced9f1899fb903bfd69109d14b1894fd6` |
| 1 048 576 octets | `-d X` | 1048576 | 1044480 | 3.1069 | 2.7980 | 1.11041 | dépassé | `6e658cf01cfb214455154b71da369fa43f50e39e06cc9b9e72df94c665203b33` |
| 1 048 576 octets | `-s aX` | 1048576 | 1048576 | 2.9092 | 2.7816 | 1.04587 | respecté | `fbbab289f7f94b25736c58be46a994c441fd02552cc6022352e3d86d2fab7c83` |
| 1 048 576 octets | `-ds X a` | 1048576 | 1044480 | 3.0170 | 3.0300 | 0.99572 | respecté | `6e658cf01cfb214455154b71da369fa43f50e39e06cc9b9e72df94c665203b33` |

1 dépassement(s) sur 8 charges.
