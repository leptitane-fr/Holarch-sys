# Proposer un pilote

1. **Choisir une demande** dans `demandes/<clé>/` : lire `fiche.txt`
   (l'appareil) et `demande.txt` (ce qui est permis).
2. **Lire le guide** : `guide/09-pilotes.md`, au moins le § 3 (le
   contrat : poignées, `DeviceResources`, service, évènements), le § 5
   (la méthode par étapes), le § 7 (les règles de sûreté) et le § 8
   (les pièges). Pour un appareil HID sur I2C : § 9.4.
3. **Partir du modèle** : copier `sdk/programs/modele` en
   `pilotes/<nom>/` ; adapter `Cargo.toml` et le **manifeste** (macro
   `pilote!` dans `src/main.rs`) : `nom`, `appareil`, `niveau`,
   `ressources`, `évènements`, `écran-seul`, `description`.
4. **Compiler** avec `sdk/` (Rust 1.98.1 exactement) : le pilote doit
   compiler sans avertissement.
5. **Ajouter `ORIGINE.txt`** :

   ```
   licence = <identifiant SPDX, ex. MIT, GPL-2.0-only>
   auteur = <qui, humain ou IA et son éditeur>
   savoir = <d'où vient la connaissance de l'appareil : norme, fiche
             technique, pilote Linux (fichier et version)…>
   ```

6. **Proposer** par une demande de fusion (pull request). Pas de
   binaire, pas de script qui s'exécute à la construction (`build.rs`
   refusé), pas de dépendance hors de `sdk/`.

Commencez au niveau **lecture** (étape 1 du guide) : Aiwos accorde
l'écriture ensuite, par un second accord à l'écran. Un pilote qui écrit
alors qu'il n'a que la lecture est arrêté par le processeur.

Tout texte d'un pilote (description, réponses) est présenté par Aiwos
**comme le texte de son auteur**, jamais comme une vérité.
