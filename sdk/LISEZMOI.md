# Le nécessaire pour écrire un pilote de Holarch

Servi par Holarch lui-même (`dev sdk`, puis `dev fichier <chemin>`) : chaque
fichier vient du noyau signé, avec sa taille et son empreinte SHA-256. Le
pont (outil `aiwos_sdk`) les vérifie avant de rien écrire.

## Ce qu'il contient

| Chemin | Rôle |
|---|---|
| `crates/abi` | `holarch-abi` : le contrat avec le noyau (appels système, structures, `VERSION`) |
| `programs/rt` | `holarch-rt` : la bibliothèque des programmes (appels, journal, `entry!`) |
| `programs/pilote` | `holarch-pilote` : la bibliothèque des pilotes (registres, service, DMA, contrôleur I2C) |
| `programs/modele` | le squelette d'un pilote (famille A, étape 1), à recopier |
| `programs/user.ld` | la disposition en mémoire (0x400000, un segment par droit : W^X) |
| `Cargo.toml` | l'espace de travail (tout le dossier) et les options de compilation |
| `construire.sh`, `construire.ps1` | la commande de compilation |
| `rust-toolchain.toml` | Rust 1.98.1 (version fixée : le même ELF octet pour octet), cible `x86_64-unknown-none` |
| `symboles.py` | nomme la fonction d'une adresse dans un ELF (rapports de panne), sans addr2line ni nm |

## Compiler

Rust 1.98.1 ; rustup installe cette version et la cible au premier essai. Rien d'autre à
télécharger : tout est ici.

    bash construire.sh          (Linux, macOS, Git Bash)
    pwsh construire.ps1         (Windows ; ou powershell -ExecutionPolicy Bypass -File construire.ps1)

L'ELF : `target/programs/x86_64-unknown-none/release/modele`. Le chemin du
dossier n'y entre pas : avec le même Rust (`rustc --version`), deux
dossiers donnent le même ELF, octet pour octet.

## Un nouveau pilote

1. Copier `programs/modele` en `programs/<nom>` ; dans son `Cargo.toml`,
   remplacer `modele` par `<nom>` (paquet et `[[bin]]`) ; dans
   `src/main.rs`, le manifeste (macro `pilote!`) : `nom = <nom>`, puis
   `appareil`, `niveau`, `ressources`, `description`.
2. Ajouter `"programs/<nom>"` à `members` dans le `Cargo.toml` du dossier.
3. `bash construire.sh` : l'ELF est
   `target/programs/x86_64-unknown-none/release/<nom>`.

La méthode (étapes 0 à 6), le contrat exact et les règles : `dev guide`
(outil `aiwos_guide`). Ce que Holarch vérifie et refuse : `dev sûreté`.

## Quand un programme tombe

Holarch garde un rapport de chaque faute ou panique (`pannes`, puis
`pannes <n>`) : l'instruction en cause, l'adresse visée, les adresses de
retour trouvées dans la pile, les dernières lignes du programme au
journal. Les programmes sont liés à une adresse fixe : ces adresses sont
celles de l'ELF compilé ici.

    python symboles.py target/programs/x86_64-unknown-none/release/<nom> 0x4002f8 0x4001a2

nomme la fonction de chaque adresse (et son décalage) ; sans adresse, il
liste toutes les fonctions. Une panique donne en plus son message et sa
ligne dans le source.

## Charger un pilote (IA6)

`aiwos_pilote(<ELF>)` : le pont le signe (clé des pilotes du PC), Holarch
vérifie signature, ELF, manifeste, ABI, appareil, ressources et niveau,
puis demande l'accord **à l'écran** : l'utilisateur appuie sur Entrée,
dans les **deux minutes**. `pilotes` montre la demande en attente, puis
le pilote en marche ; une demande expirée ou refusée y reste signalée
(« dernière demande ») : il suffit de renvoyer. Accepté, il tourne en
mémoire : `pilote <nom> <verbe>`, `pilote <nom> relance`, `pilote <nom>
arrête`. Une faute : relancé 3 fois au plus, puis laissé arrêté.

Pas encore : interruption, DMA, plus d'une BAR, appareils ACPI ; garder
un pilote sur le disque (IA6e).
