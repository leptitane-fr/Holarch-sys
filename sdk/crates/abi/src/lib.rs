//! Le contrat entre les programmes et le noyau.
//!
//! Un programme parle au noyau par l'instruction `syscall` : numéro de
//! l'appel dans `rax`, arguments dans `rdi`, `rsi`, `rdx`, `r10`, `r8`,
//! `r9`, résultat dans `rax`. Un résultat négatif est une erreur (voir
//! [`Error`]).
//!
//! Ce fichier est partagé par le noyau et par la bibliothèque des
//! programmes : les deux côtés ne peuvent pas diverger.

#![no_std]

/// La version du contrat, augmentée à chaque changement qui touche un
/// programme déjà compilé (numéro ou arguments d'un appel, forme d'une
/// structure, ordre des poignées de départ). Holarch la donne par `dev`.
pub const VERSION: u32 = 1;

/// Numéros des appels système.
pub mod sys {
    /// `journal_write(journal, texte, longueur)`
    pub const JOURNAL_WRITE: u64 = 0;
    /// `handle_close(poignée)`
    pub const HANDLE_CLOSE: u64 = 1;
    /// `handle_duplicate(poignée, droits) → poignée` : jamais plus de droits.
    pub const HANDLE_DUPLICATE: u64 = 2;
    /// `channel_create(sortie: *mut [u32; 2])`
    pub const CHANNEL_CREATE: u64 = 3;
    /// `channel_write(canal, données, longueur, poignées, nombre, appel)` :
    /// `appel` vaut 0 pour un message ordinaire ; pour répondre à une
    /// demande faite par `channel_call`, c'est son numéro d'appel
    /// (`Received::call`).
    pub const CHANNEL_WRITE: u64 = 4;
    /// `channel_read(canal, tampon, capacité, poignées, capacité, *mut Received)`
    pub const CHANNEL_READ: u64 = 5;
    /// `channel_call(canal, *const CallArgs, échéance)` : envoie la demande
    /// sous un nouveau numéro d'appel, attend la réponse qui porte ce
    /// numéro, la lit. Renvoie la taille reçue. Les messages arrivés sans
    /// ce numéro (réponses tardives à un appel abandonné) sont jetés : un
    /// canal d'appels ne porte que des demandes et leurs réponses.
    pub const CHANNEL_CALL: u64 = 6;
    /// `wait_one(poignée, signaux, échéance) → signaux observés`
    pub const WAIT_ONE: u64 = 7;
    /// `memory_create(taille) → poignée`
    pub const MEMORY_CREATE: u64 = 8;
    /// `memory_map(poignée) → adresse`
    pub const MEMORY_MAP: u64 = 9;
    /// `process_create(image, nom, longueur, poignées, nombre) → poignée`
    /// Les poignées sont transférées au nouveau programme, qui les reçoit
    /// sous les numéros 1, 2, 3… La poignée rendue porte `WAIT` (attendre
    /// sa fin) et `WRITE` (l'arrêter : `process_terminate`).
    pub const PROCESS_CREATE: u64 = 10;
    /// `process_exit(code)`
    pub const PROCESS_EXIT: u64 = 11;
    /// `clock_ns() → nanosecondes depuis le démarrage`
    pub const CLOCK_NS: u64 = 12;
    /// `sleep_ns(durée)`
    pub const SLEEP_NS: u64 = 13;
    /// `ioport_read(ports, port) → octet` : pilotes uniquement.
    pub const IOPORT_READ: u64 = 14;
    /// `ioport_write(ports, port, octet)`
    pub const IOPORT_WRITE: u64 = 15;
    /// `interrupt_ack(interruption)` : efface le signal INTERRUPT. À faire
    /// AVANT de traiter le périphérique, pour ne rien manquer.
    pub const INTERRUPT_ACK: u64 = 16;
    /// `memory_create_dma(dma, taille) → poignée` : mémoire physiquement
    /// contiguë, que les périphériques peuvent lire et écrire directement.
    pub const MEMORY_CREATE_DMA: u64 = 17;
    /// `memory_physical(mémoire) → adresse physique` (mémoire DMA seulement).
    pub const MEMORY_PHYSICAL: u64 = 18;
    /// `wait_many(éléments: *mut [WaitItem], nombre, échéance) → indice` :
    /// attend qu'un des objets présente un de ses signaux ; renseigne les
    /// signaux observés de chacun et renvoie l'indice du premier prêt.
    pub const WAIT_MANY: u64 = 19;
    /// `system_read(système, sujet, tampon, capacité) → octets` : l'état
    /// du système en texte (sujet 0 : mesures et programmes ; 1 : journal ;
    /// 2 : données transmises par le noyau précédent, vides sans relance,
    /// lisibles une seule fois ;
    /// 3 : version du noyau, 8 octets, poids faible d'abord ;
    /// 4 : appareils décrits par les tables ACPI, sur un bus série ou une
    /// broche GPIO ; 5 : les cinq tuiles, une par ligne : nom, niveau,
    /// jauge (« - » sans), valeur, détail, séparés par des tabulations ;
    /// 6 : le journal numéroté, `6 | n << 8` à partir de la ligne n (0 :
    /// les plus récentes), sous une ligne « plus ancienne, plus récente,
    /// démarrage » ; 7 : les mesures des programmes en marche ; 8 : les
    /// rapports de panne, `8 | n << 8` le rapport n (0 : la liste)).
    /// IA4, le matériel, en pages « page n/N » (`sujet | page << 8 | nombre
    /// << 16`, page 0 ou 1 : la première) : 9 : la fiche d'un périphérique
    /// PCI, nombre = bus << 8 | appareil << 3 | fonction ; 10 : son espace
    /// de configuration en hexadécimal ; 11 : les tables ACPI, en colonnes ;
    /// 13 : la machine (SMBIOS, CPUID, mémoire) ; 15 : les appareils ACPI,
    /// en colonnes. 12 (sans page) : les octets bruts d'une table ACPI,
    /// nombre = rang | décalage << 8, précédés de la longueur de la table
    /// (4 octets, poids faible d'abord).
    pub const SYSTEM_READ: u64 = 20;
    /// `system_relaunch(système, mémoire, taille, données, longueur)` :
    /// relance Holarch sur le noyau du paquet de mise à jour signé (`taille`
    /// octets) contenu dans la mémoire, en lui transmettant les données
    /// (256 octets au plus, relues par `system_read` sujet 2). Le noyau
    /// vérifie la signature et la version. Droit d'écriture sur le système
    /// requis. Ne revient qu'en cas d'erreur (paquet refusé).
    pub const SYSTEM_RELAUNCH: u64 = 21;
    /// `tile_set(tuile, données, longueur)` : remplit la tuile de l'écran
    /// que désigne la poignée (droit d'écriture). Données : niveau (1
    /// octet, `tile_level`), jauge (1 octet, 0 à 100, ou 255 : aucune), puis
    /// la valeur principale et le détail en UTF-8, séparés par « \n ».
    pub const TILE_SET: u64 = 22;
    /// `system_set(système, réglage, valeur)` : un réglage du noyau (droit
    /// d'écriture sur le système). Réglage 0 : repos des cœurs (0 : léger,
    /// HLT ; 1 : le plus profond, MWAIT) ; réglage 1 : une indication MWAIT
    /// précise (un sous-état annoncé par CPUID).
    pub const SYSTEM_SET: u64 = 23;
    /// `clock_unix()` : l'heure UTC, en secondes depuis 1970, d'après
    /// l'horloge de la machine (0 si elle est illisible). Sans poignée :
    /// l'heure n'est pas un secret.
    pub const CLOCK_UNIX: u64 = 24;
    /// `system_power(système, action)` : 0 redémarre la machine, 1
    /// l'éteint (droit d'écriture sur le système). Ne revient qu'en cas
    /// d'impossibilité (tables ACPI incomplètes).
    pub const SYSTEM_POWER: u64 = 25;
    /// `memory_unmap(adresse)` : retire une projection faite par
    /// `memory_map` (l'adresse qu'il a rendue) ; l'objet est rendu quand
    /// plus rien ne le désigne, ni poignée ni projection.
    pub const MEMORY_UNMAP: u64 = 26;
    /// `process_abort(message, longueur)` : fin sur une panique. Le noyau
    /// écrit le message (512 octets au plus) au journal et garde un rapport
    /// de panne (« pannes ») : d'où vient l'appel, les adresses de retour
    /// de la pile. Ne revient pas (un noyau antérieur à IA3 répond
    /// `BadSyscall` : `process_exit` alors).
    pub const PROCESS_ABORT: u64 = 27;
    /// `system_query(système, sujet, texte, longueur, tampon, capacité)`
    /// (IA4) : comme `system_read`, avec un texte (128 octets au plus).
    /// Sujet 14, `14 | page << 8` : la fiche des appareils ACPI qui
    /// répondent au texte (chemin, nom ou identifiant `_HID`, `_CID`).
    pub const SYSTEM_QUERY: u64 = 28;
    /// `process_terminate(programme)` (IA5) : arrête un programme, qu'il le
    /// veuille ou non. Il faut le droit `WRITE` sur sa poignée (celle que
    /// rend `process_create`). La demande interrompt sa tâche ;
    /// la poignée signale `TERMINATED` après la fin effective de la tâche
    /// et la restitution de sa mémoire et de ses poignées. Déjà arrêté : rien.
    pub const PROCESS_TERMINATE: u64 = 29;
    /// `device_open(appareil, niveau, masque_bar, *mut DeviceOpened)`.
    /// IA5b : ouvre une fois les BAR mémoire demandées (bits 0 à 5),
    /// sans DMA ni MSI. READ requis ; WRITE aussi au niveau écriture.
    /// Chaque sortie non nulle est une poignée de mémoire, projetable
    /// mais non exécutable. ShouldWait si l'appareil est encore ouvert.
    pub const DEVICE_OPEN: u64 = 30;
    /// Comme PROCESS_CREATE, avec masque de droits conservés dans arg5.
    pub const PROCESS_CREATE_RESTRICTED: u64 = 31;
    /// IA7b : device_dma(registres) → un domaine DMA confiné par l'IOMMU
    /// pour l'appareil de ces registres (ouverts en écriture) ; à passer à
    /// MEMORY_CREATE_DMA à la place du droit DMA. AccessDenied sans IOMMU.
    pub const DEVICE_DMA: u64 = 32;
    /// D2 (docs/16) : `device_prepare(système, appareil, masque_bar,
    /// *mut DevicePrepared)`. Racine seulement (la poignée système) : prépare
    /// l'appareil pour un pilote du **système scellé** (réveil, MSI/MSI-X,
    /// passage direct dans l'IOMMU) et rend ses zones, son interruption et
    /// le droit DMA. Une fois par appareil.
    pub const DEVICE_PREPARE: u64 = 33;
    /// D3 (docs/16) : `mmio_window(système, adresse, taille)` → une mémoire
    /// de registres à adresse fixe (une fiche du catalogue : la page GPIO
    /// de l'amplificateur d'un chipset). Racine seulement ; refusée si elle
    /// touche la mémoire vive ou le premier Mo ; alignée sur 4 Kio, 64 Kio
    /// au plus.
    pub const MMIO_WINDOW: u64 = 34;
    /// D3 (docs/16) : `interrupt_line(système, gsi, mode)` → une
    /// interruption sur une ligne de l'IO-APIC (un appareil derrière un
    /// bus, décrit par l'ACPI). Mode : bit 0 niveau (sinon front), bit 1
    /// actif bas, bit 2 masquée jusqu'au premier acquittement (IA6f, niveau
    /// seulement). Racine seulement.
    pub const INTERRUPT_LINE: u64 = 35;
    /// G6 (docs/18) : `memory_frames(dma, mémoire, début, tampon, capacité)`
    /// → le nombre de pages de la mémoire ; écrit dans le tampon les
    /// adresses physiques de ses pages, depuis la page `début` (autant
    /// qu'il en tient). Pour un pilote (le droit DMA) : le pilote graphique
    /// y lit les images partagées, sans copie. Avec un domaine d'IOMMU, les
    /// pages y sont ouvertes à l'appareil (et gardées tant qu'il vit). La
    /// mémoire d'un périphérique est refusée.
    pub const MEMORY_FRAMES: u64 = 36;
}

/// Les modes de `interrupt_line`.
pub mod line_mode {
    pub const LEVEL: u32 = 1;
    pub const ACTIVE_LOW: u32 = 2;
    pub const MASKED: u32 = 4;
}

/// Niveau d'une tuile de l'écran : sa couleur.
pub mod tile_level {
    pub const NEUTRAL: u8 = 0;
    pub const OK: u8 = 1;
    pub const WARNING: u8 = 2;
    pub const ALERT: u8 = 3;
}

/// Erreurs, renvoyées sous forme négative.
#[repr(i64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Ce numéro de poignée ne désigne rien.
    BadHandle = 1,
    /// La poignée n'a pas le droit nécessaire.
    AccessDenied = 2,
    /// Argument invalide (adresse hors du programme, taille excessive…).
    InvalidArgs = 3,
    /// Rien à lire pour l'instant, ou file pleine : réessayer plus tard.
    ShouldWait = 4,
    /// L'autre bout du canal a été fermé.
    PeerClosed = 5,
    NoMemory = 6,
    TimedOut = 7,
    /// Le tampon fourni est trop petit pour le message.
    BufferTooSmall = 8,
    /// Numéro d'appel système inconnu.
    BadSyscall = 9,
    /// Mauvais type d'objet pour cette opération.
    WrongType = 10,
}

impl Error {
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match -code {
            1 => Self::BadHandle,
            2 => Self::AccessDenied,
            3 => Self::InvalidArgs,
            4 => Self::ShouldWait,
            5 => Self::PeerClosed,
            6 => Self::NoMemory,
            7 => Self::TimedOut,
            8 => Self::BufferTooSmall,
            9 => Self::BadSyscall,
            10 => Self::WrongType,
            _ => return None,
        })
    }
}

/// Droits attachés à une poignée.
pub mod rights {
    pub const DUPLICATE: u32 = 1 << 0;
    /// Peut être envoyée dans un message.
    pub const TRANSFER: u32 = 1 << 1;
    pub const READ: u32 = 1 << 2;
    /// Écrire ; sur la poignée d'un programme : l'arrêter.
    pub const WRITE: u32 = 1 << 3;
    /// Mémoire : peut être projetée dans l'espace du programme.
    pub const MAP: u32 = 1 << 4;
    pub const WAIT: u32 = 1 << 5;
    pub const ALL: u32 = 0x3f;
}

/// Signaux qu'on peut attendre sur un objet.
pub mod signals {
    /// Canal : un message attend d'être lu.
    pub const READABLE: u32 = 1 << 0;
    /// Canal : l'autre bout a été fermé.
    pub const PEER_CLOSED: u32 = 1 << 1;
    /// Processus : il est terminé.
    pub const TERMINATED: u32 = 1 << 2;
    /// Interruption : le périphérique a signalé quelque chose.
    pub const INTERRUPT: u32 = 1 << 3;
}

/// Messages entre un pilote réseau et le programme « réseau » : le premier
/// octet dit de quoi il s'agit.
pub mod net {
    /// Une trame Ethernet suit (dans les deux sens).
    pub const FRAME: u8 = 0;
    /// Le lien est établi ; l'adresse matérielle (6 octets) suit.
    pub const LINK_UP: u8 = 1;
    /// Le lien est perdu (appareil débranché).
    pub const LINK_DOWN: u8 = 2;
}

/// Messages entre un programme et la pile réseau, pour une connexion TCP
/// (premier octet).
pub mod socket {
    /// Programme → pile : se connecter ; adresse IP (4 octets) et port (2
    /// octets, poids fort en tête) suivent.
    pub const CONNECT: u8 = 0x10;
    /// Programme → pile : des données à envoyer suivent (4096 octets au plus).
    pub const SEND: u8 = 0x11;
    /// Programme → pile : fermer la connexion.
    pub const CLOSE: u8 = 0x12;
    /// Pile → programme : connexion établie.
    pub const CONNECTED: u8 = 0x20;
    /// Pile → programme : des données reçues suivent.
    pub const RECEIVED: u8 = 0x21;
    /// Pile → programme : connexion terminée ; la raison (texte) suit.
    pub const CLOSED: u8 = 0x22;
    /// Taille maximale des données d'un message SEND.
    pub const MAX_SEND: usize = 4096;
}

/// Un élément d'une attente multiple (`wait_many`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WaitItem {
    pub handle: u32,
    /// Signaux attendus.
    pub signals: u32,
    /// Signaux observés, remplis au retour.
    pub observed: u32,
}

/// Nombre maximal d'objets dans une attente multiple.
pub const MAX_WAIT_ITEMS: usize = 16;

/// Échéance « jamais » pour les attentes.
pub const FOREVER: u64 = u64::MAX;

/// Taille maximale des données d'un message, et nombre de poignées.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAX_MESSAGE_HANDLES: usize = 64;

/// Ce que `channel_read` a effectivement reçu.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Received {
    pub bytes: u32,
    pub handles: u32,
    /// Numéro d'appel d'une demande faite par `channel_call`, à rendre avec
    /// la réponse ; 0 pour un message ordinaire.
    pub call: u32,
}

/// Ce que `channel_call` envoie, et où ranger la réponse (qui ne porte pas
/// de poignée).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CallArgs {
    pub request: u64,
    pub request_len: u64,
    /// Poignées jointes à la demande (numéros sur 32 bits), qui quittent
    /// l'appelant.
    pub handles: u64,
    pub handle_count: u64,
    pub reply: u64,
    pub reply_cap: u64,
}

/// Ce que « racine » reçoit du noyau au démarrage, dans le premier message
/// de son canal de départ (poignée 1). Les poignées du message suivent
/// l'ordre de [`boot_handles`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BootResources {
    /// L'écran confié au compositeur (H1) : dimensions en pixels.
    pub screen_width: u32,
    pub screen_height: u32,
    /// Pixels par ligne en mémoire.
    pub screen_stride: u32,
    /// 0 : rouge-vert-bleu ; 1 : bleu-vert-rouge.
    pub screen_format: u32,
}

/// Un périphérique PCI recensé par le noyau au démarrage (12 octets). La
/// liste suit, dans un second message, celui des ressources de « racine ».
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub _reserved: u16,
}

/// Rang des poignées dans le message de démarrage de « racine ».
pub mod boot_handles {
    pub const JOURNAL: usize = 0;
    pub const IMAGE_PING: usize = 1;
    pub const IMAGE_PONG: usize = 2;
    pub const IMAGE_INTRUS: usize = 3;
    pub const IMAGE_CLAVIER: usize = 4;
    pub const IMAGE_SHELL: usize = 5;
    pub const IMAGE_USB: usize = 6;
    /// Interruption du contrôleur clavier (IRQ 1).
    pub const KEYBOARD_IRQ: usize = 7;
    /// Ports 0x60 à 0x64 du contrôleur clavier.
    pub const KEYBOARD_PORTS: usize = 8;
    /// Mémoire de l'image de l'écran (au compositeur, H1).
    pub const SCREEN: usize = 9;
    pub const IMAGE_RESEAU: usize = 10;
    /// Le droit de lire l'état du système.
    pub const SYSTEM: usize = 11;
    pub const IMAGE_DISTANT: usize = 12;
    pub const IMAGE_WIFI: usize = 13;
    pub const IMAGE_DISQUE: usize = 14;
    /// Ports 0x900 à 0x9FF, en lecture : la carte mémoire du Chrome EC
    /// (batterie, températures).
    pub const EC_PORTS: usize = 15;
    pub const IMAGE_ENERGIE: usize = 16;
    /// Les tuiles de l'écran que remplissent des programmes : batterie
    /// (« énergie »), Wi-Fi (« réseau-wifi »), console (« distant »).
    pub const TILE_BATTERY: usize = 17;
    pub const TILE_WIFI: usize = 18;
    pub const TILE_CONSOLE: usize = 19;
    pub const IMAGE_PAVE: usize = 20;
    pub const IMAGE_GRAPHIQUE: usize = 21;
    pub const IMAGE_SON: usize = 22;
    /// Le service des fichiers (N1e, docs/12). Une image vide (un octet)
    /// quand il n'y en a pas : racine ne le lance pas.
    pub const IMAGE_FICHIERS: usize = 23;
    /// Les tables AML lues hors du noyau (N4-4, docs/15). Une image vide
    /// quand il n'y en a pas.
    pub const IMAGE_ACPI: usize = 24;
    /// Le compositeur, seul maître de l'écran et des entrées (H1,
    /// docs/17). Une image vide quand il n'y en a pas.
    pub const IMAGE_COMPOSITEUR: usize = 25;
    /// Le second client du compositeur, pour l'essai au banc (H1b,
    /// « lance essai-fenetre »). Une image vide quand il n'y en a pas.
    pub const IMAGE_ESSAI_FENETRE: usize = 26;
    /// Les fichiers de l'interface (H2, docs/17), du système scellé : les
    /// polices (`/interface/texte.ttf`, `/interface/titre.ttf`), le fond
    /// d'écran (`/interface/fond.qoi`). Des mémoires en lecture seule,
    /// projetables : la longueur du fichier (u64, petit-boutiste), puis
    /// ses octets. Une longueur nulle quand il n'y en a pas.
    pub const POLICE_TEXTE: usize = 27;
    pub const POLICE_TITRE: usize = 28;
    pub const FOND_ECRAN: usize = 29;
    /// La galerie de la boîte à outils, pour l'essai au banc (H3,
    /// « lance galerie »). Une image vide quand il n'y en a pas.
    pub const IMAGE_GALERIE: usize = 30;
    /// La police d'icônes de l'interface (H4, `/interface/icones.ttf`),
    /// comme les polices : sa longueur, puis ses octets.
    pub const ICONES: usize = 31;
    /// L'atelier, une tâche pour l'essai au banc (H5, « lance atelier »).
    /// Une image vide quand il n'y en a pas.
    pub const IMAGE_ATELIER: usize = 32;
    /// Fusion, l'interface d'administration (docs/20) : racine la lance
    /// au lieu du shell de Holarch. Une image vide quand il n'y en a pas.
    pub const IMAGE_FUSION: usize = 33;
    pub const COUNT: usize = 34;
}

/// Messages suivants du démarrage de « racine », un par périphérique confié
/// à un pilote : ses registres, son interruption, et le droit de créer de
/// la mémoire DMA (poignées dans cet ordre). Pour le pavé tactile, la
/// troisième poignée est l'interruption du pavé lui-même (pas de DMA).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DeviceResources {
    /// Taille de la zone de registres.
    pub mmio_size: u64,
    pub vendor_id: u16,
    pub device_id: u16,
    /// Le genre de périphérique (`device_kind`).
    pub kind: u32,
    /// Contrôleur I2C : l'appareil à piloter dessus, d'après les tables
    /// ACPI (adresse sur le bus, vitesse du bus en Hz) ; 0 sinon.
    pub i2c_address: u32,
    pub i2c_speed_hz: u32,
    /// Sa ligne d'interruption sur l'IO-APIC (GSI), s'il en a une ; 0 sinon.
    pub i2c_interrupt: u32,
    /// Valeurs propres au genre de périphérique. Processeur graphique :
    /// registres GGC (0x50) et BDSM (0xC0, génération 11) de l'espace de
    /// configuration,
    /// l'adresse physique de l'image du firmware (bas, haut), et celle de
    /// la fenêtre sur la GGTT (BAR 2, bas, haut). Pavé tactile : son
    /// protocole (`touchpad_protocol`), puis le registre de son descripteur
    /// HID (HID sur I2C).
    pub extra: [u32; 6],
}

/// Le protocole d'un pavé tactile (`DeviceResources::extra[0]`).
pub mod touchpad_protocol {
    /// Celui d'Elan (le Chromebook).
    pub const ELAN: u32 = 0;
    /// HID sur I2C, en mode « Precision Touchpad ».
    pub const HID_I2C: u32 = 1;
}

pub mod device_kind {
    /// Appareil PCI sans pilote embarqué : une seule poignée Device,
    /// sans ouverture matérielle. extra[0] = bus << 8 | device << 3 | function.
    pub const AVAILABLE: u32 = 0x100;
    /// Image de la sonde embarquée, message facultatif à une poignée Memory.
    pub const SONDE_IMAGE: u32 = 0x101;
    /// Contrôleur USB xHCI.
    pub const USB: u32 = 0;
    /// Carte Wi-Fi Realtek RTL8822CE.
    pub const WIFI: u32 = 1;
    /// Contrôleur eMMC (SDHCI) du stockage interne.
    pub const EMMC: u32 = 2;
    /// Contrôleur I2C (Intel LPSS, cœur DesignWare) du pavé tactile.
    pub const TOUCHPAD: u32 = 3;
    /// Processeur graphique Intel (registres et GGTT en BAR 0).
    pub const GRAPHICS: u32 = 4;
    /// Contrôleur audio Intel (HDA en BAR 0 ; le DSP en BAR 4, quatrième
    /// poignée).
    pub const AUDIO: u32 = 5;
}

/// Niveaux de device_open : pas de DMA avant le confinement par IOMMU.
pub mod device_level {
    pub const READ: u32 = 1;
    pub const WRITE: u32 = 2;
}

/// Résultat de device_open : une taille et une poignée par BAR.
/// Les BAR non demandées valent zéro. Aucun octet de bourrage (72 octets).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DeviceOpened {
    pub sizes: [u64; 6],
    pub handles: [u32; 6],
}

/// Résultat de device_prepare (D2, docs/16) : taille et poignée de chaque
/// zone demandée (zéro sinon), l'interruption, le droit DMA (80 octets).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DevicePrepared {
    pub sizes: [u64; 6],
    pub memories: [u32; 6],
    pub interrupt: u32,
    pub dma: u32,
    /// D4 : un affichage : l'adresse physique de l'image du firmware ; 0
    /// sinon.
    pub image: u64,
}

/// Un doigt sur le pavé tactile (8 octets).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TouchPoint {
    /// Position, en points du pavé, origine en haut à gauche.
    pub x: u16,
    pub y: u16,
    pub pressure: u8,
    /// 1 si ce doigt est posé ; 0 : emplacement libre.
    pub present: u8,
    /// Étendue du contact, en pistes : largeur (bits 0-3), hauteur (4-7).
    pub size: u8,
    pub _reserved: u8,
}

/// L'état du pavé tactile, que son pilote envoie à chaque rapport
/// (48 octets). Les emplacements des doigts sont stables : un doigt garde
/// le sien tant qu'il reste posé.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TouchEvent {
    /// Taille de la surface, en points.
    pub width: u16,
    pub height: u16,
    /// Boutons (bit 0 : le clic du pavé).
    pub buttons: u8,
    /// Nombre de doigts posés.
    pub count: u8,
    pub _reserved: u16,
    pub fingers: [TouchPoint; 5],
}

/// Un évènement de souris USB, tel que le pilote « usb » l'envoie à chaque
/// rapport (8 octets) : déplacement relatif, en points de la souris.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseEvent {
    pub dx: i16,
    pub dy: i16,
    /// Molette : crans vers le haut (positif) ; inclinaison : vers la
    /// droite (positif).
    pub wheel: i8,
    pub pan: i8,
    /// Boutons enfoncés : bit 0 gauche, 1 droit, 2 milieu, 3 précédent,
    /// 4 suivant…
    pub buttons: u16,
}

/// Un évènement clavier, tel que le pilote l'envoie (8 octets).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyEvent {
    pub kind: u32,
    /// Pour `key::CHAR` : le caractère (Unicode). Dans les bits hauts, les
    /// touches de modification tenues ([`key::MODIFICATEURS`]).
    pub value: u32,
}

/// Les genres de touches, et les touches de modification (H3, docs/17).
///
/// Les modificateurs vont dans les bits hauts de `value` (un caractère
/// tient en 21 bits). Pour une touche spéciale (flèche, Tab…), tous ceux
/// qui sont tenus. Pour un caractère, seulement s'il y a Ctrl ou Alt (un
/// raccourci : Ctrl+C, Alt+F) ; Maj ou AltGr seuls ont déjà choisi le
/// caractère (« A », « € »), ils ne sont pas redits : un programme qui
/// lit `value` comme un caractère n'y voit que ce qui a été tapé.
pub mod key {
    pub const CHAR: u32 = 1;
    pub const ENTER: u32 = 2;
    pub const BACKSPACE: u32 = 3;
    pub const UP: u32 = 4;
    pub const DOWN: u32 = 5;
    pub const LEFT: u32 = 6;
    pub const RIGHT: u32 = 7;
    pub const ESCAPE: u32 = 8;
    pub const TAB: u32 = 9;
    pub const DELETE: u32 = 10;
    pub const HOME: u32 = 11;
    pub const END: u32 = 12;
    pub const PAGE_UP: u32 = 13;
    pub const PAGE_DOWN: u32 = 14;
    /// La touche de Holarch : Recherche (le Chromebook), Windows (un PC).
    /// Le compositeur la donne toujours au shell (le dock, H4).
    pub const HOLARCH: u32 = 15;

    /// Maj tenue.
    pub const MAJ: u32 = 1 << 29;
    /// Ctrl tenue.
    pub const CTRL: u32 = 1 << 30;
    /// Alt (celle de gauche ; celle de droite est AltGr) tenue.
    pub const ALT: u32 = 1 << 31;
    pub const MODIFICATEURS: u32 = MAJ | CTRL | ALT;
}

impl KeyEvent {
    /// Le caractère tapé, s'il en est un et qu'il n'est pas un raccourci
    /// (ni Ctrl ni Alt).
    pub fn caractere(&self) -> Option<char> {
        if self.kind == key::CHAR && self.value & key::MODIFICATEURS == 0 { char::from_u32(self.value) } else { None }
    }

    /// Le caractère de la touche, raccourci ou non (« c » de Ctrl+C).
    pub fn lettre(&self) -> Option<char> {
        if self.kind == key::CHAR { char::from_u32(self.value & !key::MODIFICATEURS) } else { None }
    }

    /// Un raccourci : Ctrl + `c` (sans Alt ; Maj tenue ou non ; la lettre
    /// en minuscule ou en majuscule).
    pub fn ctrl_et(&self, c: char) -> bool {
        self.ctrl() && !self.alt() && self.lettre().is_some_and(|l| l.eq_ignore_ascii_case(&c))
    }

    pub fn maj(&self) -> bool {
        self.value & key::MAJ != 0
    }

    pub fn ctrl(&self) -> bool {
        self.value & key::CTRL != 0
    }

    pub fn alt(&self) -> bool {
        self.value & key::ALT != 0
    }
}
