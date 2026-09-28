# Proposer un pilote

> ## Avant de proposer : à vérifier, sans exception
>
> Ces points ont fait échouer les premiers pilotes proposés. Chacun est
> vérifié par Aiwos ou par la construction : un seul manqué, et le
> pilote est refusé.
>
> **1. `Cargo.toml` : chemins `../rt` et `../pilote`, jamais autre chose.**
> Le pilote n'est pas construit dans `pilotes/<nom>/` : `outils/construire.sh`
> le copie dans `sdk/programs/<nom>/`, **à côté** de `sdk/programs/rt/` et
> `sdk/programs/pilote/`. Les chemins sont relatifs à cet endroit-là.
>
> ```toml
> [dependencies]
> aiwos-rt = { path = "../rt" }
> aiwos-pilote = { path = "../pilote" }
> ```
>
> Faux (vu depuis le dépôt, mais Cargo refuse) : `../../sdk/programs/rt`.
>
> **2. Le manifeste : un champ par ligne, `clé = valeur`, toujours.**
> Aucune ligne sans `=`. Plusieurs ressources vont **sur la même ligne**,
> séparées par ` ; `. Modèle complet, à recopier (appareil PCI) :
>
> ```
> nom = mon-pilote
> version = 1
> abi = 1
> appareil = pci 8086:15d7
> niveau = lecture
> ressources = bar 0
> évènements = aucun
> écran-seul = aucun
> description = Ce que fait le pilote, en une phrase
> ```
>
> Faux (Aiwos refuse : « ligne sans « = » ») : `ressources = bar 0`
> puis `interruption` seul sur la ligne suivante.
>
> **3. Ce qu'un pilote chargé reçoit aujourd'hui, et rien de plus.**
>
> | Appareil | Permis dans `ressources` | Refusé au chargement |
> |---|---|---|
> | PCI (`appareil = pci vvvv:dddd`) | **une** BAR : `bar N` | `interruption` (pas encore d'interruption PCI), DMA |
> | ACPI sur I2C (`appareil = acpi <_HID>`) | la BAR du contrôleur I2C, `interruption` (la ligne de l'appareil) | DMA |
> | Ports d'entrée-sortie (0x60, 0x64…), interruption ISA | — | tout : pas encore possible pour un pilote chargé |
>
> La poignée 2 (interruption) reste un canal muet quand l'interruption
> n'est pas accordée : ne pas compter dessus.
>
> **4. Construire avant de proposer, et donner l'empreinte.**
>
> ```sh
> outils/construire.sh <nom>
> ```
>
> Doit se terminer par `<empreinte SHA-256>  <nom>`, **sans aucun
> avertissement** (une constante inutilisée compte). Recopier l'empreinte
> dans le message du commit : elle est recalculée et comparée. Une
> empreinte non obtenue par cette commande est rejetée.
>
> **5. Deux défauts fréquents.**
> - `reg <adresse>` : refuser toute adresse hors de la zone
>   (`adresse + 4 > regs.size()`, ou non alignée sur 4).
> - `arrête` : doit vraiment arrêter (sortir de la boucle après avoir
>   répondu), pas seulement répondre « arrêté ».
>
> **6. Registres : seulement ceux d'une source nommée.** Chaque décalage
> vient d'une source citée dans `ORIGINE.txt` (fichier et version du
> pilote Linux, page de la fiche technique). Pas de registre
> « hypothétique » : si la source ne le donne pas, ne pas le lire.

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
4. **Compiler** avec `outils/construire.sh <nom>` (Rust 1.98.1
   exactement) : sans avertissement, empreinte dans le message du
   commit (voir l'encadré).
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
