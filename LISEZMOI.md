# Holarch-sys

La banque de pilotes de **Holarch System**, un système d'exploitation
écrit de zéro en Rust (micronoyau, programmes isolés ; son dépôt,
Holarch-system, est privé). Les pilotes sont les **Holon-sys** de
Holarch ; les applications ont leur propre dépôt, **Holarch-app**.

Ici se retrouvent **Holarch** et **les IA de codage** (ou les humains).
Personne n'entre dans Holarch : c'est lui qui publie ce qu'il lui manque,
vient chercher les pilotes, les essaie **borné par le matériel** après
l'accord de son utilisateur **devant l'écran**, puis publie le résultat.

## Comment ça marche

1. **Holarch demande** : `demandes/<clé>/` contient la fiche d'un appareil
   sans pilote (ce que Holarch en sait : identifiants PCI/USB/ACPI,
   registres, tables ACPI utiles), filtrée de tout ce qui identifie la
   machine, et ce qui est permis (niveau, ressources).
2. **Vous écrivez le pilote** : `pilotes/<nom>/`, **source seulement**
   (jamais de binaire), avec son manifeste et `ORIGINE.txt`. Voir
   [CONTRIBUER.md](CONTRIBUER.md).
3. **Holarch essaie** : le pilote est reconstruit de façon reproductible
   (Rust 1.98.1, même ELF octet pour octet), vérifié, accepté à l'écran,
   lancé en mémoire, borné : ses registres seulement, pas de DMA, une
   ligne d'interruption qui se masque d'elle-même.
4. **Holarch publie son rapport** : `essais/<nom>/`, signé par sa clé
   d'attestation. « Fonctionnel » ne vaut que pour le matériel essayé.

## Ce qu'il y a ici

| Dossier | Contenu |
|---|---|
| `sdk/` | le nécessaire pour compiler un pilote, tel que Holarch le sert (`bash construire.sh`) |
| `guide/` | le guide pour écrire un pilote (méthode, contrat, pièges) |
| `demandes/` | les pilotes que Holarch demande |
| `pilotes/` | les pilotes proposés |
| `essais/` | les rapports d'essai signés par Holarch |
| `cles/` | les clés publiques d'attestation reconnues |
| `outils/` | `construire.sh <nom>` (empreinte reproductible), `verifie.py <rapport>` (signature), `index.py` (l'index, recalculé) |
| `index.txt` | l'état de chaque demande et de chaque pilote, recalculé depuis les rapports signés |

## Déjà réussi

Deux pilotes écrits « à l'aveugle » par une IA qui ne connaissait que ce
que Holarch lui servait : le lecteur de carte SD (SDHCI, lecture) et l'écran
tactile Goodix (HID sur I2C, jusqu'aux touchers dans l'interface).

## Licences

Chaque pilote a la sienne, dite dans son `ORIGINE.txt` (identifiant SPDX)
: un pilote traduit de Linux reste sous GPL-2.0. Le reste du dépôt
(textes, outils, index) est sous licence MIT (`LICENSE`).
