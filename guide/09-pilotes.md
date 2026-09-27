# 09 — Écrire un pilote pour Aiwos

Ce guide dit comment un pilote d'Aiwos est bâti, et comment en écrire un
nouveau vite et sans casse. Il est tiré des pilotes qui existent et de
leur histoire (clavier, USB, Wi-Fi, eMMC, pavé tactile, graphique, son :
voir le journal).

**À qui il s'adresse.** À toute instance de travail, humaine ou IA de
codage, **y compris sans accès au dépôt** : toutes les valeurs utiles
(numéros d'appels système, ordre des poignées, formats des messages,
limites) sont écrites ici. Il est aussi le texte que sert la commande
`dev guide` d'Aiwos (feuille de route : [10 — Aiwos interrogeable par les
IA](10-aiwos-interrogeable.md), étape IA2) ; il doit donc rester
**autonome** : ne pas renvoyer à un fichier du dépôt pour une information
indispensable.

**La source de vérité**, c'est Aiwos en marche : ce qu'il répond
(`aide`, `matériel …`, les réponses des pilotes) l'emporte sur ce texte
en cas de désaccord. Le signaler alors, pour corriger le guide.

Sommaire :

1. Le modèle en une page
2. Les quatre familles de pilotes
3. Le contrat d'un programme pilote
4. Le câblage hors du pilote (aujourd'hui)
5. La méthode, étape par étape
6. Le cycle d'essai sur la machine
7. Les règles de sûreté
8. Les pièges déjà rencontrés
9. Modèles à recopier
10. Pour une IA sans le dépôt

---

## 1. Le modèle en une page

```
                 ┌───────────────────────────────────────────────┐
   noyau         │ découvre (PCI, tables ACPI), prépare (D0,     │  anneau 0
   (anneau 0)    │ « bus master », MSI), fabrique des OBJETS :   │
                 │ registres (mémoire), interruption, droit DMA, │
                 │ ports ; les confie à racine, et à lui seul    │
                 └──────────────────────┬────────────────────────┘
                                        │ messages de démarrage
                 ┌──────────────────────▼────────────────────────┐
   racine        │ seul distributeur : lance chaque pilote avec  │  anneau 3
                 │ SES poignées, garde son canal de SERVICE,     │
                 │ relaie les commandes (écran, console          │
                 │ distante), filtre ce qui est permis à         │
                 │ distance, demande « résumé » et « arrête »    │
                 └───────┬──────────────────────────────┬────────┘
                         │ service (texte ⇄ texte)       │
                 ┌───────▼────────┐   évènements   ┌─────▼──────┐
   pilote        │ programme isolé├───────────────►│ shell,     │
                 │ no_std, sans   │ (structures    │ pile réseau│
                 │ allocation     │  de l'ABI)     │ …          │
                 └───────┬────────┘                └────────────┘
                         │ lectures/écritures volatiles, DMA
                     [ le périphérique ]
```

Quatre idées suffisent :

- **Un pilote est un programme ordinaire**, en anneau 3, isolé : il ne
  voit que les poignées reçues à sa naissance. Il ne peut ni toucher un
  autre périphérique, ni lire la mémoire d'un autre programme. S'il fait
  une faute, **lui seul** est arrêté (« « X » arrêté : … Le reste
  continue. » au journal), et Aiwos garde un rapport de panne (`pannes`,
  § 3.9).
- **Le noyau ne sait rien des protocoles.** Il donne des *accès* : une
  zone de registres, une interruption, le droit de créer de la mémoire
  DMA, une plage de ports. Tout le savoir sur l'appareil est dans le
  pilote.
- **racine est le chef d'orchestre** : c'est lui qui lance le pilote, et
  c'est par lui que passent toutes les commandes (`son bip` tapé à
  l'écran ou envoyé par la console distante devient la demande `bip` sur
  le canal de service du pilote `son`).
- **Un pilote parle en texte à racine** (demandes et réponses lisibles
  par un humain), et **en structures binaires** à ceux qui consomment ses
  données (le shell reçoit des `TouchEvent`, la pile réseau des trames).

## 2. Les quatre familles de pilotes

| Famille | Ce que le noyau confie | Exemples | Particularités |
|---|---|---|---|
| **A. Périphérique PCI à registres mémoire** | la zone de registres d'une BAR, son interruption (MSI ou MSI-X, sinon aucune : interrogation), le droit DMA ; parfois d'autres zones | `usb` (xHCI 00:14.0), `wifi` (RTL8822CE 01:00.0, BAR 2), `disque` (eMMC 00:1a.0), `graphique` (00:02.0), `son` (00:1f.3 : HDA en BAR 0, DSP en BAR 4) | Le cas le plus fréquent. Le noyau a déjà sorti l'appareil de veille (D0), autorisé ses accès mémoire et réservé son interruption. |
| **B. Appareil derrière un contrôleur de bus, décrit par ACPI** | les registres du **contrôleur** (I2C, SPI…), l'adresse de l'appareil et la vitesse du bus (tables ACPI), la ligne d'interruption **de l'appareil** (IO-APIC) | `pavé` (Elan sur I2C0, 0x15, ligne 35) ; la puce du casque (RT5682 sur I2C4, 0x1a) dans `son` | Le pilote contient le pilote du contrôleur de bus (Intel LPSS, cœur DesignWare) et celui de l'appareil. Rien dans le PCI ne dit ce qui est branché sur le bus : seules les tables ACPI le disent (`matériel acpi`). |
| **C. Appareil USB** | rien de plus : il passe par le contrôleur USB | souris HID, clé USB, partage de connexion (RNDIS) | Aujourd'hui, ce sont des **modules du programme `usb`** (pas des programmes à part). La forme des rapports HID se lit dans l'appareil (descripteur de rapports) ; l'**étude** (`matériel étude …`) enregistre ce qu'il envoie pendant qu'on essaie chaque bouton. |
| **D. Ports d'entrée-sortie historiques** | une plage de ports (lecture, ou lecture et écriture), une interruption ISA | `clavier` (0x60 à 0x64, IRQ 1), `énergie` (Chrome EC, 0x900 à 0x9FF en lecture) | Rare sur une machine récente. |

Au-dessus des pilotes, des **services** (la pile `réseau`, le système de
fichiers dans `disque`, le shell) consomment ce que les pilotes
produisent ; ils suivent le même contrat de programme (§ 3) sans toucher
au matériel.

**Les pilotes qui existent** (poignées de départ, verbes de service,
sorties) :

| Programme | Fam. | Poignées de départ | Verbes (en plus de `état`, `résumé`) | Sorties |
|---|---|---|---|---|
| `usb` | A, C | 1 registres, 2 interruption, 3 DMA, 4 service, 5 journal, 6 vers `réseau`, 7 souris → shell | `liste`, `clé …`, `étude …`, `arrête` | trames `net::FRAME`, `MouseEvent` |
| `wifi` | A | 1 registres (BAR 2), 2, 3, 4, 5, 6 vers `réseau-wifi` | `allume`, `scan`, `associe`, `connecte` (écran seul), `déconnecte`, `éteins`, `témoin`, `reprends` | trames `net::FRAME` |
| `disque` | A | 1 registres, 3 DMA, 4 service, 5 journal | `débit`, `liste`, `fichier`, `fichiers …`, `installe` (racine seul) | — |
| `graphique` | A | 1 registres et GGTT, 2, 3, 4, 5, 6 canal du curseur (depuis le shell) | `luminosité`, `ddb`, `reg`, `ggtt`, `curseur essai`, `blitter`, `arrête` | — |
| `son` | A, B | 1 HDA, 2, 3, 4, 5, 6 DSP (BAR 4), 7 page GPIO, 8 contrôleur I2C du casque | `démarre`, `bip`, `joue`, `gamme`, `volume`, `casque …`, `reg`, `dsp`, `vidage`, `arrête` | — |
| `pavé` | B | 1 registres du contrôleur I2C, 2 son interruption, 3 service, 4 journal, 5 interruption du pavé, 6 évènements → shell | `essai`, `init`, `doigts [s]` | `TouchEvent` |
| `clavier` | D | 1 interruption, 2 ports, 3 évènements → shell, 4 réglages, 5 journal | — | `KeyEvent` |
| `énergie` | D | 1 ports (lecture), 2 service, 3 journal, 4 tuile | `ligne`, `zéro` | la tuile Batterie |

## 3. Le contrat d'un programme pilote

### 3.1 Compiler

- Rust **1.98.1** (fixé par `rust-toolchain.toml`), cible `x86_64-unknown-none` ; `#![no_std]`,
  `#![no_main]`, pas de bibliothèque d'allocation.
- Options : `RUSTFLAGS="-C relocation-model=static -C
  link-arg=-T<chemin>/user.ld"`, `cargo build --release --target
  x86_64-unknown-none`. Profil : `panic = "abort"`, `lto = true`,
  `codegen-units = 1`, `opt-level = "s"`.
- Dépendances : la bibliothèque des programmes **`aiwos-rt`** (qui
  dépend de `aiwos-abi`, le contrat avec le noyau), et la bibliothèque
  des pilotes **`aiwos-pilote`** (§ 9.1), qui ne dépend que d'elle. Le script d'édition
  des liens `user.ld` place le programme à **0x400000**, un segment par
  droit : code `R-X`, constantes `R--`, données `RW-` (W^X : aucune page
  à la fois modifiable et exécutable).
- Point d'entrée : `aiwos_rt::entry!(main);`. Une panique termine le
  programme (`process_abort`) : son message et sa ligne dans le source
  vont au journal, et un rapport de panne est gardé (§ 3.9). Écrire
  quand même au journal **avant** ce qui peut échouer : le rapport dit
  où, le journal dit dans quel état.
- Résultat : un exécutable ELF (18 Kio pour `clavier`, 41 pour `pavé`,
  180 pour `disque` ; `son` et `wifi` dépassent 500 Kio à cause du
  micrologiciel et des tables qu'ils embarquent). Le squelette du § 9.1
  fait 19 Kio ; il ne demande que `aiwos-abi`, `aiwos-rt`,
  `aiwos-pilote` et `user.ld`.
- **Le nécessaire**, servi par Aiwos lui-même (`dev sdk`, `dev fichier
  <chemin>` ; l'outil `aiwos_sdk` du pont les rassemble et vérifie leurs
  empreintes) : ces quatre pièces, le squelette `modele`, un espace de
  travail qui contient tout, et `construire.sh` / `construire.ps1` (les
  options ci-dessus, et `--remap-path-prefix`). Avec le même Rust, deux
  dossiers donnent le même ELF, octet pour octet.

### 3.2 La mémoire d'un programme

- **Pile de 64 Kio**, avec une page de garde : un débordement est une
  faute (le programme s'arrête). Un `Text<4096>` sur la pile va ; une
  dizaine de tampons de 4 Kio dans des fonctions imbriquées, non.
- **Pas d'allocateur** : des tableaux de taille fixe (`[T; N]`), des
  textes de taille fixe (`Text<N>`). Pour une grande zone :
  `memory_create(taille)` puis `memory_map(poignée)` (projetée à partir
  de 0x1000_0000_0000), rendue par `memory_unmap(adresse)`.
- Une zone de registres se projette de la même façon :
  `memory_map(MMIO)` rend son adresse ; le noyau l'a créée **non mise en
  cache** (la mémoire d'une image d'écran : en *write-combining*).

### 3.3 Les poignées de départ

Un programme reçoit ses poignées sous les numéros **1, 2, 3…**, dans
l'ordre choisi par racine. **Convention** pour un pilote (à suivre pour
tout nouveau pilote) :

| N° | Poignée | Droits | Remarque |
|---|---|---|---|
| 1 | registres (objet mémoire de périphérique) | projeter, lire, écrire | taille : `DeviceResources::mmio_size` |
| 2 | interruption du périphérique | attendre, acquitter | signal `INTERRUPT` |
| 3 | droit DMA (famille A) ; interruption **de l'appareil** (famille B, qui ne fait pas de DMA) | écrire ; attendre, acquitter | pour `memory_create_dma` ; le noyau donne toujours une 3e poignée pour un périphérique PCI |
| 4 | canal de **service** (racine ⇄ pilote) | lire, écrire, attendre | le premier message est `DeviceResources` (§ 3.4) |
| 5 | **journal** | écrire | `log!(JOURNAL, …)` |
| 6+ | propres au pilote | — | canal d'évènements, autres zones de registres (BAR 4, page GPIO…), interruption de l'appareil |

**Un pilote chargé à chaud (IA6, `aiwos_pilote`)** reçoit exactement :
1 les registres de la BAR demandée ; 2 l'interruption, si le manifeste
dit `ressources = bar 0 ; interruption` (famille B : la ligne **de
l'appareil** ACPI, sur niveau ; sinon un canal muet) ; 3 un canal muet
(pas de DMA avant IA7) ; 4 le service (`DeviceResources` en premier
message, avec `i2c_address`, `i2c_speed_hz`, `i2c_interrupt` pour un
appareil `acpi <_HID>`) ; 5 le journal ; 6 le canal des évènements, si
le manifeste dit `évènements = toucher` (des `TouchEvent` vers le
shell). **La ligne d'une famille B se masque d'elle-même à chaque
déclenchement** et ne se rouvre qu'à `interrupt_ack` : lire d'abord ce
que l'appareil a à dire (ce qui relâche la ligne), puis acquitter.

Exception historique : `pavé` a mis service et journal en 3 et 4, et
l'interruption du pavé en 5 ; `disque` reçoit l'interruption en 2 sans
s'en servir. Tout pilote commence par une table de ses poignées en
constantes, et le dit en tête de son fichier :

```rust
const MMIO: Handle = Handle(1);
const IRQ: Handle = Handle(2);
const DMA: Handle = Handle(3);
const SERVICE: Handle = Handle(4);
const JOURNAL: Handle = Handle(5);
```

### 3.4 Le premier message : `DeviceResources`

racine écrit d'abord, sur le canal de service, la description que le
noyau a faite du périphérique (56 octets, `#[repr(C)]`) :

| Décalage | Champ | Type | Sens |
|---|---|---|---|
| 0 | `mmio_size` | u64 | taille de la zone de registres (poignée 1) |
| 8 | `vendor_id` | u16 | fabricant PCI |
| 10 | `device_id` | u16 | modèle PCI |
| 12 | `kind` | u32 | genre (`device_kind` : 0 USB, 1 Wi-Fi, 2 eMMC, 3 pavé, 4 graphique, 5 audio) |
| 16 | `i2c_address` | u32 | famille B : adresse de l'appareil sur le bus (ACPI), 0 sinon |
| 20 | `i2c_speed_hz` | u32 | famille B : vitesse du bus (ACPI) |
| 24 | `i2c_interrupt` | u32 | famille B : ligne IO-APIC (GSI) de l'appareil, 0 sans |
| 28 | `extra` | [u32; 6] | propre au genre (graphique : GGC, BDSM, image du firmware, fenêtre BAR 2 ; audio : taille de la BAR 4, classe) |

Le lire avant toute chose (squelette au § 9.1).

### 3.5 Les appels système

Instruction `syscall` : numéro dans `rax`, arguments dans `rdi`, `rsi`,
`rdx`, `r10`, `r8`, `r9` ; résultat dans `rax`, **négatif = erreur**.
`aiwos-rt` les enveloppe (colonne de droite).

| N° | Appel | Rôle | Dans `aiwos-rt` |
|---|---|---|---|
| 0 | `journal_write` | une ligne au journal | `journal_write`, `log!` |
| 1 | `handle_close` | fermer une poignée | `handle_close` |
| 2 | `handle_duplicate` | copie, **jamais plus de droits** | `handle_duplicate` |
| 3 | `channel_create` | deux bouts d'un canal | `channel_create` |
| 4 | `channel_write` | envoyer (octets + poignées) ; le 6e argument est le numéro d'appel à rendre (0 : message ordinaire) | `channel_write`, `channel_reply` |
| 5 | `channel_read` | lire sans attendre (`ShouldWait` si vide) | `channel_read` |
| 6 | `channel_call` | demande + attente de **sa** réponse, avec échéance | `channel_call`, `channel_call_with` |
| 7 | `wait_one` | attendre un signal sur un objet | `wait_one` |
| 8 | `memory_create` | mémoire ordinaire (256 Mio au plus) | `memory_create` |
| 9 | `memory_map` | projeter une mémoire | `memory_map` |
| 10 | `process_create` | lancer un programme (racine) | `process_create` |
| 11 | `process_exit` | finir | `process_exit` |
| 12 | `clock_ns` | ns depuis le démarrage | `clock_ns`, `deadline_in_ms` |
| 13 | `sleep_ns` | dormir | `sleep_ms` |
| 14 | `ioport_read` | lire un port (famille D) | `ioport_read` |
| 15 | `ioport_write` | écrire un port | `ioport_write` |
| 16 | `interrupt_ack` | effacer le signal INTERRUPT (**avant** de traiter) | `interrupt_ack` |
| 17 | `memory_create_dma` | mémoire DMA : contiguë, sous 4 Gio | `memory_create_dma` |
| 18 | `memory_physical` | son adresse physique | `memory_physical` |
| 19 | `wait_many` | attendre sur 16 objets au plus | `wait_many`, `wait_item` |
| 20 | `system_read` | état du système (racine seulement) | `system_read` |
| 21 | `system_relaunch` | relance signée (racine) | `system_relaunch` |
| 22 | `tile_set` | remplir une tuile de l'écran | `tile_set` |
| 23 | `system_set` | réglage du noyau (racine) | `system_set` |
| 24 | `clock_unix` | heure UTC (s depuis 1970) | `clock_unix` |
| 25 | `system_power` | redémarrer, éteindre (racine) | `system_power` |
| 26 | `memory_unmap` | retirer une projection | `memory_unmap` |
| 27 | `process_abort` | finir sur une panique : message au journal, rapport de panne | `process_abort` (le gestionnaire de panique d'`aiwos-rt`) |
| 25 | `system_power` | redémarrer, éteindre (racine) | `system_power` |
| 26 | `memory_unmap` | retirer une projection | `memory_unmap` |

**Erreurs** : 1 `BadHandle`, 2 `AccessDenied`, 3 `InvalidArgs`,
4 `ShouldWait`, 5 `PeerClosed`, 6 `NoMemory`, 7 `TimedOut`,
8 `BufferTooSmall`, 9 `BadSyscall`, 10 `WrongType`.
**Droits** : `DUPLICATE` 1, `TRANSFER` 2, `READ` 4, `WRITE` 8, `MAP` 16,
`WAIT` 32. **Signaux** : `READABLE` 1 (canal), `PEER_CLOSED` 2 (canal),
`TERMINATED` 4 (processus), `INTERRUPT` 8 (interruption).
**Limites** : message de 64 Kio et 64 poignées au plus ; `wait_many` :
16 objets ; échéance « jamais » : `FOREVER` (`u64::MAX`).

Attention : `wait_one` et `wait_many` rendent `Err(TimedOut)` quand
l'échéance passe. Avec une échéance, ne jamais écrire
`if … .is_err() { return; }`.

### 3.6 La boucle principale

Un pilote dort tant que rien n'arrive : `wait_many` sur le canal de
service, son interruption, et ses autres canaux. Au réveil, il traite ce
qui est prêt, puis se rendort. Jamais d'attente active de longue durée,
jamais de boucle sans échéance (squelette complet au § 9.1).

### 3.7 Le protocole de service

- **Demande** : un texte UTF-8 (les mots de la commande après le nom du
  pilote : `son reg 0x14` → `reg 0x14`). Vide ou `état` : l'état.
- **Réponse** : un texte UTF-8 de **4 096 octets au plus**, envoyé par
  `channel_reply(SERVICE, r.call, …)` avec le **numéro d'appel** reçu
  (`Received::call`) : sans lui, racine ne reconnaît pas la réponse. Une
  réponse trop tardive est jetée (« réponse tardive jetée » au journal).
- **Délais** : racine attend de 0,5 à 30 s selon la demande. Une
  demande longue (plusieurs secondes) **bloque racine**, et donc les
  petites questions de l'interface : répondre vite, et continuer le
  travail en arrière-plan (comme `son` depuis S4).
- **Verbes normalisés**, à fournir par tout pilote :

| Verbe | Réponse | Qui le demande |
|---|---|---|
| `état` (ou vide) | l'état lisible : identité, registres clés, compteurs | un humain, une IA |
| `résumé` | 1re ligne : `niveau<TAB>état court` (niveau 0 neutre, 1 bon, 2 attention, 3 alerte) ; lignes suivantes facultatives (détail) | le panneau Matériel |
| `arrête` | couper DMA et interruptions du périphérique, le laisser sûr | racine, avant une relance, un arrêt, un redémarrage |
| `reg <adresse>` | la valeur d'un registre (lecture seule) | mise au point |
| `essai`, `init` | un essai sans risque ; la mise en route | mise au point, étapes 1 et 2 |

- Un verbe qui **écrit sur le matériel** ou montre un secret doit être
  signalé : racine le refuse à la console distante (`« … » : réservé à
  l'écran d'Aiwos.`).

### 3.8 Les évènements

- Un pilote qui produit des données en continu les envoie sur un **canal
  à part**, que racine crée et dont il donne l'autre bout au
  consommateur (le shell pour les entrées).
- Format : une **structure `#[repr(C)]` de l'ABI**, envoyée telle quelle
  (octets de la structure). Existantes : `KeyEvent` (8 octets : genre,
  caractère), `MouseEvent` (8 : dx, dy, molette, inclinaison, boutons),
  `TouchEvent` (48 : taille de la surface, boutons, nombre de doigts,
  5 × `TouchPoint` de 8 octets aux emplacements stables), et les
  messages réseau (`net::FRAME` 0, `LINK_UP` 1 + adresse, `LINK_DOWN`
  2). Une nouvelle sorte de donnée = une nouvelle structure dans l'ABI.
- **`TouchEvent` d'un écran tactile** (`évènements = toucher`) : `width`
  et `height` = l'étendue logique de l'écran (le maximum des
  coordonnées) ; pour chaque doigt posé, `present = 1`, `x`, `y` dans
  cette étendue (origine en haut à gauche) ; `count` = doigts posés ;
  `buttons` = 0. Le shell met la flèche **sous** le premier doigt posé,
  et poser un doigt fait un clic gauche. Envoyer aussi l'évènement du
  doigt levé (aucun doigt posé), sinon le shell croit le doigt resté.
- **Ne jamais attendre le consommateur** : si le canal est plein,
  compter l'évènement comme perdu et continuer. Compter les envoyés et
  les perdus, et les montrer dans `état` et `résumé`.

### 3.9 Le journal

- `log!(JOURNAL, "format", args…)` : une ligne de **160 octets au
  plus**, préfixée par le nom du programme. Le noyau garde les **1 024
  dernières lignes**, **numérotées** ; des lignes identiques de suite
  sont regroupées (« (×k) »). Les lire : `journal` (les plus récentes),
  `journal depuis <n>`, `journal <programme> [n]`, `journal <programme>
  depuis <n>` ; outil `aiwos_journal` du pont (les lignes nouvelles
  depuis son dernier appel). Forme : `dev formats`.
- Le journal peut être lu à distance : **aucune donnée personnelle**
  (nom du réseau de la maison, mot de passe, contenu de fichiers). Une
  ligne qui commence par `local: ` ne va que sur le port série.
- **Quand le pilote tombe** (faute du processeur, ou panique) : le noyau
  garde un **rapport de panne** (`pannes` : la liste, 16 gardés ; `pannes
  <n>` : un rapport). Il dit la faute, l'**instruction** en cause et
  l'**adresse visée**, où elles tombent (son code, ses données, sa pile,
  juste sous sa pile : débordement, une mémoire projetée et à quel
  décalage, l'adresse 0…), les **appels probables** (adresses de retour
  trouvées dans sa pile), le message d'une panique et sa ligne dans le
  source, et ses 8 dernières lignes au journal. Le programme est lié à
  une adresse fixe : ces adresses sont celles de **son** ELF ;
  `python symboles.py <elf> <adresses>` (dans le nécessaire) nomme les
  fonctions. Ni registres ni contenu de pile dans le rapport : ils
  peuvent contenir des secrets ; ils vont au port série seulement.
- **Mesures** : `programmes` (temps de processeur, part d'un cœur,
  mémoire, registres, poignées, messages, appels système, interruptions
  de chaque programme en marche ; `programmes tsv` en colonnes).
- Écrire au journal : le démarrage du pilote (et en combien de temps),
  chaque changement d'état, chaque erreur avec sa valeur brute (« registre
  0x14 = 0xffffffff »), les mesures d'une étape. Pas une ligne par
  évènement répétitif : un compteur.

### 3.10 Registres, attentes, DMA, interruptions

- **Registres** : `read_volatile` / `write_volatile` sur l'adresse
  projetée, **bornés** par la taille de la zone. `0xFFFFFFFF` en lecture
  veut dire, presque toujours, « appareil absent, éteint ou en veille ».
- **Attentes** : toujours avec une échéance (`clock_ns` +
  `spin_loop` pour des microsecondes, `sleep_ms` au-delà), et un message
  au journal quand l'échéance passe (« la puce ne répond pas en 50 ms,
  0x0C = … »).
- **DMA** : `memory_create_dma(DMA, taille)` (pages contiguës, sous
  4 Gio), `memory_physical` pour l'adresse à donner au périphérique,
  `memory_map` pour y lire et écrire. **Il n'y a pas encore d'IOMMU** :
  un périphérique mal programmé peut écrire n'importe où en mémoire. Un
  pilote DMA est donc de confiance : vérifier chaque adresse donnée au
  périphérique, et implémenter `arrête`. Ce qui empêche vraiment un
  périphérique de toucher la mémoire, c'est le bit « bus master » de
  son espace PCI, que seul le noyau règle : sans lui, ni DMA ni MSI
  (c'est le cas des niveaux « lecture » et « écriture » d'un pilote venu
  de l'extérieur, § 7).
- **Interruptions** : le noyau a choisi MSI, sinon MSI-X, sinon rien
  (le pilote interroge alors ses registres). Pour une famille B,
  l'interruption de l'appareil vient d'une ligne de l'IO-APIC (front ou
  niveau, actif haut ou bas, d'après ACPI). Toujours `interrupt_ack`
  **avant** de lire la cause dans les registres, pour ne rien manquer ;
  une ligne à niveau restée active ne refait pas de front : vider ce qui
  attend.

## 4. Le câblage hors du pilote (aujourd'hui)

Tant que la feuille de route IA5 et IA6 n'est pas faite, un nouveau
pilote est **embarqué dans le noyau**, et il faut le câbler à la main.
Les premiers pas de `son` (S1), `pavé` (P2) et `graphique` (G1) ont
chacun touché 11 à 14 fichiers. La liste, dans l'ordre :

| # | Où | Quoi |
|---|---|---|
| 1 | `crates/abi/src/lib.rs` | un `device_kind` ; un rang `IMAGE_<NOM>` dans `boot_handles` (et `COUNT` + 1) ; au besoin une structure d'évènement ou des champs de `DeviceResources` |
| 2 | `kernel/src/main.rs` | un numéro d'interruption (`<NOM>_VECTOR`, libres après 0x38) ; une fonction `prepare_<nom>` qui trouve l'appareil (identifiants PCI, ou tables ACPI par `kernel/src/acpi.rs`) et appelle `prepare_device(appareil, bar, vecteur, genre, nom)` ; l'ajouter à la liste `controllers` |
| 3 | `kernel/src/acpi.rs` (famille B) | une fonction de recherche sur le modèle de `touchpad()` / `headset_codec()` (`i2c_device` par identifiant `_HID`) |
| 4 | `kernel/src/user/mod.rs` | `static <NOM>: &[u8] = include_bytes!(…)` et `image(<NOM>)?` à sa place dans la liste de `start_root` |
| 5 | `programs/Cargo.toml` | le programme dans `members` ; `programs/<nom>/Cargo.toml` (copie de celui de `son`) |
| 6 | `programs/racine/src/main.rs` | le champ dans `Resources` ; son cas dans `receive_resources` ; le lancement dans `start` (canal de service, premier message `DeviceResources`, poignées **dans l'ordre du § 3.3**) ; le champ dans `Services` ; la branche de `handle()` qui relaie `<nom> …` (délai, verbes refusés à distance) ; `arrête` dans les deux listes d'arrêt (redémarrage et relance) |
| 7 | `programs/racine/src/materiel.rs` | la ligne du panneau (`driven(…)`) et la fiche (`sheet`) |
| 8 | `crates/commandes/src/lib.rs` | **l'entrée de la commande** dans la table : syntaxe, sens, détail des demandes, ce qui est permis à distance (règles mot pour mot, la plus précise l'emporte), attente ; ajouter ses cas aux essais (`cargo test -p aiwos-commandes`) |

Depuis IA0, la table des commandes donne l'aide de l'écran et celle de
la console distante (`aide`, `aide <commande>`), le filtre de la console
distante (racine garde ses refus en seconde ligne), les attentes du
shell, de `distant` et du pont (`distant` annonce `ATTENDS <n> <ms>`),
et le relais du shell vers racine : ni le shell, ni `distant`, ni le pont
ne sont plus à toucher. IA6 supprimera le reste de ce câblage.

Un appareil **USB** (famille C) se câble, lui, dans le programme `usb`
(un module à côté de `hid.rs`, `stockage.rs`, `net.rs`), plus un canal
vers son consommateur (étapes 6 à 9).

## 5. La méthode, étape par étape

Chaque pilote d'Aiwos a été écrit en **étapes courtes**, chacune
compilée, envoyée, essayée sur la machine et validée par l'utilisateur
avant la suivante (P1 à P6 pour le pavé, G1 à G4 pour le graphique, S1 à
S5 et H1 à H3 pour le son). Presque toutes ont marché **au premier
essai** : c'est la méthode qui l'a permis. La garder.

Règle d'or : **d'abord lire, ensuite écrire ; d'abord sur demande,
ensuite au démarrage.**

### Étape 0 — Recenser (sans écrire de code)

**But** : la fiche d'identité complète de l'appareil (modèle au § 9.2).

À demander à Aiwos :

| Commande | Ce qu'elle apprend |
|---|---|
| `matériel panneau` | les appareils, leur pilote (« — » : aucun), leur état ; la **clé** de chacun (`sd`, `tactile`, `usb:6`…) |
| `matériel fiche <clé>` | ce qu'Aiwos sait d'un appareil ; clés stables aussi : `pci:00:14.5`, `acpi:\_SB.PCI0.I2C2.H05D`, `usb:6` |
| `matériel tout [tsv]` | tous les périphériques PCI : bus:appareil.fonction, fabricant:modèle, genre |
| `matériel pci <bb:dd.f>` | la fiche PCI : identifiants, classe, sous-système, commande et état, **zones (BAR) mesurées au démarrage** (adresse, taille, genre), broche et ligne, capacités décodées (énergie D0-D3, MSI, MSI-X et sa table, PCIe et son lien), capacités étendues ; l'appareil ACPI qui le décrit |
| `matériel pci <bb:dd.f> config` | l'espace de configuration en hexadécimal (4 Kio avec la table MCFG ; numéro de série masqué) |
| `matériel acpi` | les appareils des tables ACPI sur un bus série ou une broche GPIO : `_HID`, `_CID`, bus et adresse, vitesse, interruption (ligne, front ou niveau), broches GPIO |
| `matériel acpi appareils` | tous les appareils ACPI, en colonnes : chemin, table, `_HID`, `_CID`, `_UID`, `_ADR`, `_DDN`, `_STA`, `_CRS` |
| `matériel acpi <chemin\|nom\|_HID>` | la fiche d'un appareil ACPI : toutes ses valeurs fixes (`_DSD`, `_PRx`…), `_CRS` décodé, ses méthodes et leurs arguments, son parent et son `_ADR`, ce qu'il contient (sources d'alimentation…) |
| `matériel acpi tables`, outil `aiwos_export(dossier)` | la liste des tables ; toutes écrites sur le PC (`dsdt.dat`, `ssdt1.dat`…), vérifiées, pour `iasl -d` : ce que **calculent** les méthodes (un `_CRS` calculé, l'alimentation `_PS0`/`_ON`, les broches GPIO de réinitialisation) ; Aiwos, lui, ne les exécute pas |
| `matériel machine` | SMBIOS (fabricant, modèle, carte, micrologiciel ; jamais de numéro de série), processeur (CPUID : famille, modèle, fonctions), mémoire |
| `usb` | les appareils USB : port, identifiants, classes des interfaces |
| `usb descripteurs <port>` | tout ce que l'appareil dit de lui-même : fiche, configurations, interfaces, voies, descripteurs de classe (HID, UVC, son), descripteur de rapports HID, textes ; en octets et décodé, en lecture seule |
| `matériel étude début usb:<port>`, puis `matériel étude lis` | (HID, stockage) les descripteurs, et ce que l'appareil envoie pendant qu'on essaie chacune de ses commandes ; se lance au clic « Étudier » de la fiche, l'utilisateur suit les étapes à l'écran |

Les longues réponses arrivent par pages (première ligne « page n/N » ;
« <demande> page <n> » pour la suite ; l'outil `aiwos_commande` les
recolle seul) ; les colonnes sont décrites par `dev formats`.

Puis chercher la documentation, dans cet ordre :

1. **La norme publique** quand il y en a une (xHCI, SDHCI, HDA, HID sur
   I2C, UVC, spécification ACPI) : elle suffit souvent.
2. **La fiche technique** du fabricant.
3. **Le pilote Linux**, **lu comme une documentation** : on en tire
   l'ordre des opérations, les registres, les pièges (souvent commentés).
   Chaque fonction d'Aiwos **cite la fonction Linux** dont elle vient
   (« `rtw_mac_power_on` »). Le code sous GPL **ne se recopie pas** : il
   se réécrit d'après ce qu'il fait. Les tables sous licence BSD
   (Realtek) peuvent être reprises, en le notant dans les licences.
4. **coreboot** (le micrologiciel de la machine) : réglages propres à la
   carte (broches GPIO, horloges), et ce que le micrologiciel a déjà
   fait (un contrôleur trouvé « déjà réglé »).
5. Un **micrologiciel tiers** (Wi-Fi, DSP audio) ne s'embarque qu'avec
   l'**accord de l'utilisateur** et sa licence ; on préfère un
   micrologiciel libre (choix de l'utilisateur pour le son : SOF, jamais
   de micrologiciel signé Intel).

**Livrable** : la fiche d'identité, et un **plan en étapes** (1 à 6
ci-dessous, avec leurs critères de réussite), présentés à l'utilisateur,
qui choisit.

### Étape 1 — Faire connaissance (lecture seule)

**But** : prouver qu'on parle au bon appareil, sans rien changer.

- Le pilote projette les registres, lit l'identité (version, signature,
  capacités), l'état laissé par le micrologiciel, et le rend par `état`.
- `reg <adresse>` pour tout registre ; `résumé` (niveau 0 ou 1).
- **Aucune écriture** sur le matériel. Aucune au démarrage non plus.

**Réussite** : les valeurs lues ont un sens (identifiants attendus,
version conforme à la norme, pas de `0xFFFFFFFF`), et le reste d'Aiwos
n'a pas bougé. Exemples : `son` S1 (HDA 1.0, 6 sorties, DSP à 2 cœurs),
`graphique` G1 (1920 × 1080 à 60,003 Hz), `pavé` P2 (descripteur lu en
1 ms).

### Étape 2 — La mise en route, sur demande

**But** : sortir l'appareil de sa réinitialisation, l'alimenter, régler
ses horloges, charger son micrologiciel s'il en a un.

- Un verbe (`init`, `démarre`, `allume`), **jamais au démarrage** à ce
  stade.
- Chaque attente bornée, chaque échec dit au journal avec les registres
  en cause.
- Mesurer la durée (« prêt en 84 ms »).

**Réussite** : l'appareil annonce qu'il est prêt (bit d'état, version du
micrologiciel, réponse à une première commande) ; on sait le remettre au
repos (`arrête`).

### Étape 3 — Parler son protocole, par interrogation

**But** : les échanges utiles (commandes, lectures de données), sans
interruption : le pilote lit l'état à intervalles réguliers.

- Un verbe d'essai qui fait un échange complet et le résume
  (`pavé doigts 5` : ce que le pavé a vu pendant 5 s ; `son bip`).
- Compter les échanges et les erreurs.

**Réussite** : des données justes et vérifiables par l'utilisateur (un
bip entendu, deux doigts vus, un fichier relu à l'identique), zéro
erreur sur un grand nombre d'échanges (1 600 lectures du pavé).

### Étape 4 — Les interruptions

**But** : remplacer l'interrogation par l'attente de l'interruption.

- `wait_many` sur l'interruption ; `interrupt_ack` d'abord ; traiter
  **tout** ce qui attend.
- Compter les signaux et les données reçues.

**Réussite** : autant de données que de signaux, aucune perdue ni en
double (pavé P4 : 975 signaux, 975 rapports) ; processeur au repos
quand l'appareil se tait.

### Étape 5 — Brancher au système

**But** : que l'appareil serve à quelque chose.

- Les évènements vers leur consommateur (§ 3.8) ; au besoin une nouvelle
  structure dans l'ABI.
- `résumé` juste pour le panneau Matériel ; la fiche de l'appareil.
- Les réglages à garder (luminosité, volume) : racine les écrit dans
  `/système/réglages.txt` quand la réponse du pilote l'annonce.
- `arrête` complet ; ce qu'il faut transmettre à la version suivante
  lors d'une relance (le « témoin », 256 octets au total : Wi-Fi).
- La mise en route passe **au démarrage** du pilote.

**Réussite** : l'utilisateur s'en sert (la flèche bouge, le son sort)
et dit que « ça marche » ; une relance (mise à jour) le laisse en état.

### Étape 6 — Durcir

**But** : la vie réelle.

- Débrancher, rebrancher, relancer, éteindre, rallumer ; une erreur
  en plein échange (le contrôleur I2C resté occupé après une relance :
  corrigé en abandonnant la transaction, puis en réinitialisant le
  contrôleur).
- L'énergie : éteindre ce qui ne sert pas (le DSP audio s'éteint une
  minute après le dernier son).
- Les mesures (débit, latence, pertes) au journal et dans `état`.
- Les **dettes** connues, écrites.

**Réussite** : les essais de l'utilisateur, relances comprises ; les
mesures dans le journal du projet.

## 6. Le cycle d'essai sur la machine

1. **Compiler** : `tools/build.sh` (tout), puis `tools/maj.sh
   --noyau-seul` (paquet signé du noyau seul) ou `tools/maj.sh` (noyau et
   programme EFI, pour l'écrire sur le disque). La clé de signature est
   sur le PC de l'utilisateur, hors du dépôt.
2. **Demander l'accord** de l'utilisateur **avant chaque envoi**, puis
   `aiwos_mise_a_jour`.
3. L'utilisateur tape `mise-à-jour` puis Entrée **sur l'écran d'Aiwos**.
   Aiwos se relance **en mémoire** (paquet `--noyau-seul`) ou écrit la
   version sur le disque puis se relance. La console distante revient
   seule en 10 à 20 s : attendre, vérifier `aiwos_etat`, puis renvoyer
   la commande (une commande envoyée avant « session reprise » se perd).
4. **Essayer** : les verbes du pilote, `aiwos_journal`, `système`,
   `programmes`, `matériel panneau` ; s'il tombe, `pannes`. Pour ce qui
   se voit ou s'entend, demander à l'utilisateur.
5. **Pour un changement risqué au démarrage** : d'abord en mémoire
   (l'arrêt ramène la version du disque), le disque ensuite. Après une
   version qui plante, installer la correction **deux fois** (une
   installation range l'ancienne en `ancien.efi`, l'entrée « Aiwos
   (précédent) » du menu).
6. **Valider** : une étape = un commit (« …, P3 : … (à valider) »), puis
   une entrée datée du journal du projet : ce qui est fait, un tableau
   « Sujet | Choix | Raison », « Validé sur le Chromebook » avec les
   mesures, les dettes (modèle au § 9.3).

## 7. Les règles de sûreté

Non négociables : elles viennent de l'utilisateur ou d'incidents.

0. **Le fil rouge : rien n'est appliqué sans qu'Aiwos l'ait validé**,
   de bout en bout. Valider, c'est : **vérifier** (qui envoie,
   signature, forme, cohérence avec le matériel), **borner** (les plus
   petits droits, imposés par le matériel : registres projetés en
   lecture seule, « bus master » coupé, plus tard l'IOMMU), **faire
   accepter** à l'écran ce qu'Aiwos a calculé, **tracer** et pouvoir
   reprendre. Un pilote venu de l'extérieur commence au niveau
   « lecture » (une écriture est une faute : le pilote est arrêté), monte
   à « écriture » sur un nouvel accord, et n'aura le DMA qu'avec l'IOMMU.
   Détail : docs/10, § 3.
1. **Le mot de passe Wi-Fi ne passe jamais par la console distante** ; ne
   jamais le demander.
2. **Écrire sur le matériel ou sur le disque se décide devant l'écran** :
   à distance, un pilote n'offre que la lecture et les essais sans
   risque. racine fait respecter la règle ; le pilote la documente.
3. **Rien ne s'installe sans l'accord à l'écran** (mises à jour,
   demain les pilotes chargés à chaud).
4. **Ne jamais écrire sur la clé USB de l'utilisateur** (elle porte la
   sauvegarde du micrologiciel d'origine) sans son accord.
5. **Pas de donnée personnelle** dans le journal ni dans le dépôt, qui
   peut devenir public.
6. **Un pilote en cours d'écriture ne touche au matériel que sur
   demande**, jamais au démarrage, tant que l'étape n'est pas validée.
7. **Aucune attente sans échéance, aucune boucle sans fin.**
8. **`arrête` pour tout pilote qui fait du DMA** : une relance avec un DMA
   en marche écrirait dans la mémoire du noyau suivant.
9. **Licences** : pas de code GPL recopié ; tout élément tiers (tables,
   micrologiciel, police) inscrit dans `docs/licences.md`.
10. **Mesurer avant d'affirmer** : un relevé isolé ne vaut rien (la
    batterie retarde d'une minute) ; dire ce qui a été vu, avec les
    valeurs.

## 8. Les pièges déjà rencontrés

| Piège | Symptôme | Leçon |
|---|---|---|
| Liste de poignées vide : adresse factice 4 | « argument invalide », programme à 100 % d'un cœur | corrigé dans le noyau ; lire les erreurs, ne pas les ignorer |
| Réponse arrivée après l'échéance | toutes les réponses suivantes décalées d'un cran | numéros d'appel (`channel_reply` avec `r.call`) |
| `wait_many` avec échéance → `Err(TimedOut)` | le pilote s'arrêtait à la première attente échue (G3 : écran figé) | distinguer `TimedOut` des vraies erreurs |
| Puce RT5682 muette | accuse réception mais rend des zéros | écrire 0xFFFF ← 1 (mode I2C) puis attendre 15 ms, comme Linux : lire **tout** le chemin de sonde de Linux |
| Sonnette du DSP (HIPCIDR) | « invalid IPC header » | le bit 30 veut dire « message compact » : ne recopier dans la sonnette que le bit 31 ; relire le code du micrologiciel, pas seulement le pilote |
| Type de message du volume | micrologiciel qui s'éteint, mémoire lue à 0xFFFFFFFF | 0x4 = gestion d'énergie ; le bon était 0x5 : vérifier chaque constante à sa source |
| Bits des broches GPIO inversés | l'amplificateur ne s'allumait pas | vérifier les broches voisines avant d'écrire, et relire après |
| Ordre USB | descripteurs HID lus avant la configuration : refus | configurer d'abord, demander ensuite |
| Relance en plein échange I2C | pavé mort après la relance | `arrête` abandonne la transaction ; au démarrage, reprendre un contrôleur trouvé occupé |
| Mesurer une BAR | l'image de l'écran disparaît un instant | ne pas mesurer la BAR d'un appareil que le micrologiciel fait tourner (écran) |
| Longue demande au pilote | l'interface ne répond plus 3 s | répondre vite, travailler en arrière-plan |
| Écran tactile HID sur I2C muet | descripteur lisible, mais aucune interruption au toucher | l'appareil attend **SET_POWER ON** puis **RESET** (§ 9.4), comme Linux : aucune interruption avant |
| Scripts en heredoc (Git Bash) | `\n` changés en vrais retours à la ligne dans le code | écrire les scripts dans un fichier, jamais en heredoc |

## 9. Modèles à recopier

### 9.1 Le squelette d'un pilote (famille A, étape 1)

C'est `programs/modele`, compilé à chaque construction d'Aiwos (jamais
embarqué) : ce texte en est la copie exacte. Il s'appuie sur la
bibliothèque des pilotes, **`aiwos-pilote`** (`programs/pilote`), qui ne
fait que ce que décrit le § 3 :

| Élément | Rôle |
|---|---|
| `Mmio::map(poignée, taille)` | projette une zone de registres ; `r8/r16/r32`, `w8/w16/w32` bornés et alignés (hors zone : lecture à 1, écriture ignorée), `modify32`, `wait32(adresse, masque, valeur, ms)` à échéance |
| `resources(service)` | attend et lit le premier message du service, `DeviceResources` (§ 3.4) |
| `respond(service, tampon, \|demande, réponse\| …)` | lit une demande, écrit la réponse (`Reply` : 4 Kio), la renvoie avec son numéro d'appel ; rend `Served::Closed` si racine a fermé le canal |
| `summary(réponse, niveau, format_args!(…))` | la réponse à `résumé` : `niveau<TAB>état` |
| `Dma::new(droit, taille)` | une mémoire DMA : `address` (pour le pilote), `physical` (pour le périphérique) |
| `parse_hex(mot)` | une adresse tapée (« 0x70180 » ou « 70180 ») |
| `i2c::Controller::new(registres, adresse, vitesse)` | le contrôleur I2C d'Intel (LPSS, DesignWare), `transfer(écrire, lire)` : familles B (pavé, casque, écran tactile) |

`programs/<nom>/Cargo.toml` :

```toml
[package]
name = "modele"
version.workspace = true
edition.workspace = true

[[bin]]
name = "modele"
test = false
bench = false

[dependencies]
aiwos-rt = { path = "../rt" }
aiwos-pilote = { path = "../pilote" }
```

`programs/<nom>/src/main.rs` :

```rust
//! « modele » : le squelette d'un pilote d'Aiwos (famille A : un
//! périphérique PCI à registres mémoire), à recopier pour en commencer un.
//! Étape 1 du guide, « faire connaissance » : il lit, il n'écrit rien sur
//! le matériel. Compilé à chaque construction, jamais embarqué : il ne
//! peut pas se périmer. Contrat et méthode : docs/09-pilotes.md.
//!
//! Poignées de départ (convention du guide, § 3.3) : 1 registres,
//! 2 interruption, 3 droit DMA, 4 canal de service, 5 journal.

#![no_std]
#![no_main]

use core::fmt::Write;

use aiwos_pilote::{Dma, Mmio, Reply, Served, parse_hex, respond, summary};
use aiwos_rt::{self as rt, DeviceResources, FOREVER, Handle, log, signals};

const MMIO: Handle = Handle(1);
const IRQ: Handle = Handle(2);
const DMA: Handle = Handle(3);
const SERVICE: Handle = Handle(4);
const JOURNAL: Handle = Handle(5);

struct Driver {
    regs: Mmio,
    info: DeviceResources,
    interrupts: u32,
    /// Une mémoire DMA, prise quand il en faudra une (étape 2 et
    /// suivantes).
    #[allow(dead_code)]
    dma: Option<Dma>,
}

impl Driver {
    /// Une demande de racine (les mots après le nom de la commande).
    fn handle(&mut self, request: &str, out: &mut Reply) {
        let mut words = request.split(' ');
        match words.next().unwrap_or("") {
            // Pour le panneau Matériel : « niveau\tétat ».
            "résumé" => summary(out, 1, format_args!("lecture seule · {} interruptions", self.interrupts)),
            "reg" => match words.next().and_then(parse_hex) {
                Some(offset) => {
                    let _ = write!(out, "{offset:#06x} = {:#010x}", self.regs.r32(offset));
                }
                None => {
                    let _ = write!(out, "reg <adresse en hexadécimal>");
                }
            },
            // Avant une relance ou un arrêt : plus de DMA ni d'interruption.
            "arrête" => {
                let _ = write!(out, "arrêté");
            }
            _ => self.describe(out),
        }
    }

    /// « état » : identité, taille des registres, un premier registre.
    fn describe(&self, out: &mut Reply) {
        let _ = writeln!(
            out,
            "{:04x}:{:04x}, registres : {} Kio",
            self.info.vendor_id,
            self.info.device_id,
            self.regs.size() / 1024
        );
        let _ = write!(out, "registre 0x0 : {:#010x}", self.regs.r32(0));
    }

    fn on_interrupt(&mut self) {
        // D'abord effacer le signal, puis lire la cause dans les registres.
        let _ = rt::interrupt_ack(IRQ);
        self.interrupts += 1;
    }
}

fn main() {
    let Some(info) = aiwos_pilote::resources(SERVICE) else {
        log!(JOURNAL, "pas de description du périphérique");
        return;
    };
    let regs = match Mmio::map(MMIO, info.mmio_size) {
        Ok(regs) => regs,
        Err(e) => {
            log!(JOURNAL, "registres inaccessibles : {e:?}");
            return;
        }
    };
    let mut driver = Driver { regs, info, interrupts: 0, dma: None };
    // Exemple, pour plus tard : `Dma::new(DMA, 4096)`.
    let _ = DMA;
    log!(JOURNAL, "prêt ({:04x}:{:04x}), lecture seule", info.vendor_id, info.device_id);

    let mut request = [0u8; 256];
    loop {
        let mut items = [
            rt::wait_item(SERVICE, signals::READABLE | signals::PEER_CLOSED),
            rt::wait_item(IRQ, signals::INTERRUPT),
        ];
        if rt::wait_many(&mut items, FOREVER).is_err() {
            return;
        }
        if items[1].observed & signals::INTERRUPT != 0 {
            driver.on_interrupt();
        }
        if items[0].observed & signals::READABLE != 0 {
            if respond(SERVICE, &mut request, |text, out| driver.handle(text, out)) == Served::Closed {
                return;
            }
        } else if items[0].observed & signals::PEER_CLOSED != 0 {
            return;
        }
    }
}

rt::entry!(main);
```

Pour une famille B, remplacer la poignée 3 par l'interruption de
l'appareil, et parler au bus par `aiwos_pilote::i2c`.

### 9.2 La fiche d'identité d'un appareil (étape 0)

```text
Appareil        : <nom lisible>                clé Aiwos : <clé du panneau>
Famille         : A | B | C | D
Identité        : PCI <bb:dd.f> <fabricant:modèle> classe <cc ss pi>
                  ou ACPI <chemin> _HID <…> _CID <…> sur <bus> <contrôleur>, adresse <…>, <kHz>
                  ou USB port <n> <idVendor:idProduct>, interfaces <classe/sous-classe/protocole>
Ressources      : BAR <n> (<taille>), interruption <MSI|MSI-X|ligne n, front/niveau, actif haut/bas>,
                  DMA oui/non, autres zones (GPIO, second contrôleur…)
État laissé     : ce que le micrologiciel a déjà fait (allumé, réglé, en veille D3…)
Documentation   : norme <…> ; fiche technique <…> ; pilote Linux <fichier> (fonctions : …) ;
                  coreboot <fichier> ; licences
Micrologiciel   : aucun | <nom, origine, licence, accord de l'utilisateur>
Consommateur    : shell (évènements <structure>) | pile réseau | système de fichiers | …
Risques         : ce qui peut mal tourner (DMA, alimentation, bus partagé, appareil déjà utilisé)
Étapes          : 1 … 6, chacune avec son critère de réussite mesurable
```

### 9.3 L'entrée du journal d'une étape

```markdown
## JJ/MM/AAAA — <Appareil>, <X>n : <ce que fait l'étape>, validée

**Ce qui est fait** : <en deux ou trois phrases>.

| Sujet | Choix | Raison |
|---|---|---|
| … | … | … |

**Validé sur le Chromebook** (<relance en mémoire | disque>, <premier
essai | après N corrections>) : <mesures, ce que l'utilisateur a vu ou
entendu>.

**Dettes** : <ce qui reste, connu>.

**Suite, <X>n+1** : <l'étape suivante>.
```

Message de commit : `<Appareil>, <X>n : <ce qui change> (à valider)`, en
français, suivi de la ligne `Co-Authored-By` ; le journal suit dans un
commit à part (« Journal : <X>n validée »).

### 9.4 Un appareil HID sur I2C (écran tactile, pavé)

La norme « HID over I2C » de Microsoft (appareils ACPI `PNP0C50`,
comme `GDIX0000`) ; tous les registres de l'appareil ont 16 bits,
envoyés **poids faible d'abord** avant une lecture (`transfer(&[lo, hi],
&mut lu)`).

1. **Le descripteur HID** : 30 octets au registre donné par ACPI
   (`_DSM`, presque toujours **0x0001**). Champs, en u16 petit-boutiste
   depuis l'octet 0 : longueur (30), version (0x0100), longueur du
   descripteur de rapport, **registre du descripteur de rapport**,
   **registre d'entrée**, longueur max d'un rapport d'entrée, registre
   de sortie, longueur max de sortie, **registre de commande**, registre
   de données, puis fabricant, produit, version.
2. **Allumer** : écrire au registre de commande `[cmd_lo, cmd_hi, 0x00,
   0x08]` (SET_POWER, état « marche »). Puis **réinitialiser** :
   `[cmd_lo, cmd_hi, 0x00, 0x01]` (RESET). L'appareil tire alors sa
   ligne une fois : lire 2 octets (0x00 0x00), c'est la fin du reset.
   **Sans ces deux commandes, aucune interruption.**
3. **Les rapports d'entrée** : à chaque interruption, lire **sans
   registre** (`transfer(&[], &mut lu)`) la longueur max d'entrée ; les
   2 premiers octets donnent la longueur réelle (longueur 0 ou 2 : rien),
   puis l'identifiant du rapport, puis ses champs.
4. **Le format des champs** est dans le **descripteur de rapport**
   (registre du point 1, sa longueur aussi) : la grammaire HID (Usage
   Page 0x0D « Digitizer », Usage 0x04 « Touch Screen », collections
   « Finger » 0x22, Tip Switch 0x42, Contact Identifier 0x51, X/Y de la
   page 0x01, Logical Maximum…). Le lire et le décoder, ou, à défaut,
   relever des rapports en touchant les coins et le vérifier ainsi : le
   montrer dans `état` pour qu'on puisse le contrôler.
5. **Arrêter** proprement (verbe `arrête`) : SET_POWER « veille »
   `[cmd_lo, cmd_hi, 0x01, 0x08]`.

## 10. Pour une IA sans le dépôt

**Aujourd'hui** (depuis IA4, le 26/09), par la console sûre seule
(chiffrée, authentifiée dans les deux sens), une IA peut faire l'étape 0
en entier (`matériel panneau`, `matériel fiche`, `matériel pci`,
`matériel acpi`, les tables ACPI par `aiwos_export`, `matériel machine`,
`usb descripteurs`, l'étude d'un appareil USB), lire ce guide et la
feuille de route (`dev`, outil `aiwos_guide`), rassembler le nécessaire
et **compiler** (`dev sdk`, outil `aiwos_sdk`, puis `construire.sh`).
Elle ne peut **ni charger** son pilote (il est embarqué dans le noyau,
qui se signe avec une clé qu'elle n'a pas), **ni lire les registres**
d'un appareil sans pilote.

**Demain** (feuille de route [10](10-aiwos-interrogeable.md)) : Aiwos
décrit son matériel en détail, prête une **sonde** pour lire les
registres d'un appareil (après accord à l'écran, registres en lecture
seule), et accepte un **pilote chargé à chaud** : signé, vérifié par
Aiwos, borné à son niveau
par le matériel, accepté à l'écran, en mémoire d'abord, sans toucher au
noyau. La méthode du § 5 ne change pas, elle suit les niveaux : étape 1
au niveau « lecture », étapes 2 à 5 au niveau « écriture » (le DMA après
l'IOMMU). Seul le § 4 disparaît.
