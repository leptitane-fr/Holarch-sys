//! La bibliothèque des pilotes d'Aiwos (IA0, 25/09/2026) : ce que chaque
//! pilote réécrivait pour lui seul, en un seul endroit, sans allocation.
//!
//! - [`Mmio`] : une zone de registres, bornée ; attentes à échéance ;
//! - [`resources`] : le premier message du canal de service, la
//!   description du périphérique (`DeviceResources`) ;
//! - [`respond`] : lire une demande du service et y répondre, avec son
//!   numéro d'appel ;
//! - [`summary`] : la réponse à « résumé », au format du panneau Matériel ;
//! - [`Dma`] : une mémoire DMA, son adresse et son adresse physique ;
//! - [`parse_hex`] : une adresse de registre tapée dans une demande ;
//! - [`i2c`] : le contrôleur I2C d'Intel (LPSS, cœur DesignWare), celui
//!   du pavé tactile et de la puce du casque.
//!
//! Le contrat complet d'un pilote (poignées, protocole de service,
//! verbes, règles) : docs/09-pilotes.md ; un pilote qui s'en sert :
//! `programs/modele`.

#![no_std]

pub mod i2c;

use core::fmt::{self, Write};
use core::ptr::{read_volatile, write_volatile};

use aiwos_rt::{self as rt, DeviceResources, FOREVER, Handle, Text, signals};

/// La taille d'une réponse de service (celle qu'attend racine).
pub type Reply = Text<4096>;

// --- Manifeste ---------------------------------------------------------------------

/// Le manifeste d'un pilote, écrit dans l'ELF même (section
/// `.aiwos.pilote`, jamais chargée en mémoire) : il ne se sépare jamais du
/// code, et la signature couvre les deux. Une ligne « clé = valeur » par
/// champ ; Aiwos le vérifie avant tout accord (docs/10, IA6) :
///
/// ```ignore
/// aiwos_pilote::pilote!("nom = lecteur-sd
/// version = 1
/// abi = 1
/// appareil = pci 8086:4df8
/// niveau = lecture
/// ressources = bar 0 ; interruption
/// évènements = aucun
/// écran-seul = aucun
/// description = Lecteur de carte SD (SDHCI), étape 1 : lecture seule");
/// ```
#[macro_export]
macro_rules! pilote {
    ($text:literal) => {
        #[used]
        #[unsafe(link_section = ".aiwos.pilote")]
        static AIWOS_PILOTE: [u8; $text.len()] = {
            let text: &[u8] = $text.as_bytes();
            let mut out = [0u8; $text.len()];
            let mut i = 0;
            while i < text.len() {
                out[i] = text[i];
                i += 1;
            }
            out
        };
    };
}

// --- Registres ---------------------------------------------------------------------

/// Une zone de registres projetée : chaque accès est vérifié contre sa
/// taille et son alignement. Hors de la zone, une lecture rend des bits à
/// 1 (comme un périphérique absent ou éteint) et une écriture ne fait
/// rien.
#[derive(Clone, Copy, Debug)]
pub struct Mmio {
    base: usize,
    size: u64,
}

impl Mmio {
    /// Projette la zone de registres `handle` (sa taille : `mmio_size` de
    /// [`DeviceResources`], ou celle du périphérique).
    pub fn map(handle: Handle, size: u64) -> rt::Result<Self> {
        Ok(Self { base: rt::memory_map(handle)? as usize, size })
    }

    /// Une zone déjà projetée.
    pub const fn new(base: usize, size: u64) -> Self {
        Self { base, size }
    }

    pub const fn base(&self) -> usize {
        self.base
    }

    pub const fn size(&self) -> u64 {
        self.size
    }

    fn inside(&self, offset: u32, width: u32) -> bool {
        offset % width == 0 && offset as u64 + width as u64 <= self.size
    }

    pub fn r8(&self, offset: u32) -> u8 {
        if !self.inside(offset, 1) {
            return u8::MAX;
        }
        unsafe { read_volatile((self.base + offset as usize) as *const u8) }
    }

    pub fn r16(&self, offset: u32) -> u16 {
        if !self.inside(offset, 2) {
            return u16::MAX;
        }
        unsafe { read_volatile((self.base + offset as usize) as *const u16) }
    }

    pub fn r32(&self, offset: u32) -> u32 {
        if !self.inside(offset, 4) {
            return u32::MAX;
        }
        unsafe { read_volatile((self.base + offset as usize) as *const u32) }
    }

    pub fn w8(&self, offset: u32, value: u8) {
        if self.inside(offset, 1) {
            unsafe { write_volatile((self.base + offset as usize) as *mut u8, value) }
        }
    }

    pub fn w16(&self, offset: u32, value: u16) {
        if self.inside(offset, 2) {
            unsafe { write_volatile((self.base + offset as usize) as *mut u16, value) }
        }
    }

    pub fn w32(&self, offset: u32, value: u32) {
        if self.inside(offset, 4) {
            unsafe { write_volatile((self.base + offset as usize) as *mut u32, value) }
        }
    }

    /// Lit, efface les bits `clear`, met les bits `set`, réécrit.
    pub fn modify32(&self, offset: u32, clear: u32, set: u32) {
        self.w32(offset, self.r32(offset) & !clear | set);
    }

    /// Attend que `(registre & masque) == valeur`, `ms` millisecondes au
    /// plus. Faux à l'échéance : le dire au journal, avec la valeur lue.
    pub fn wait32(&self, offset: u32, mask: u32, value: u32, ms: u64) -> bool {
        let deadline = rt::deadline_in_ms(ms);
        loop {
            if self.r32(offset) & mask == value {
                return true;
            }
            if rt::clock_ns() >= deadline {
                return false;
            }
            core::hint::spin_loop();
        }
    }
}

// --- Le canal de service ------------------------------------------------------------

/// Le premier message du canal de service : la description du
/// périphérique, écrite par racine avant toute demande. Attend qu'elle
/// arrive.
pub fn resources(service: Handle) -> Option<DeviceResources> {
    let mut info = DeviceResources::default();
    let bytes = unsafe {
        core::slice::from_raw_parts_mut(&mut info as *mut DeviceResources as *mut u8, size_of::<DeviceResources>())
    };
    rt::wait_one(service, signals::READABLE, FOREVER).ok()?;
    rt::channel_read(service, bytes, &mut []).ok()?;
    Some(info)
}

/// Ce qu'a donné [`respond`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Served {
    /// Une demande lue, et sa réponse envoyée.
    Answered,
    /// Rien à lire (ou une demande illisible, ignorée).
    Nothing,
    /// racine a fermé le canal : le pilote n'a plus qu'à finir.
    Closed,
}

/// Lit une demande du canal de service, sans attendre, et y répond :
/// `answer(demande, réponse)` écrit la réponse (texte de 4 Kio au plus),
/// envoyée avec le numéro d'appel de la demande. À appeler quand
/// `wait_many` a vu le signal `READABLE` du canal. `buffer` : la plus
/// longue demande attendue (256 octets suffisent d'ordinaire).
pub fn respond(service: Handle, buffer: &mut [u8], answer: impl FnOnce(&str, &mut Reply)) -> Served {
    match rt::channel_read(service, buffer, &mut []) {
        Ok(r) => {
            let mut reply = Reply::new();
            let text = core::str::from_utf8(&buffer[..r.bytes as usize]).unwrap_or("");
            answer(text.trim(), &mut reply);
            let _ = rt::channel_reply(service, r.call, reply.as_str().as_bytes(), &[]);
            Served::Answered
        }
        Err(rt::Error::PeerClosed) => Served::Closed,
        Err(_) => Served::Nothing,
    }
}

/// La réponse à « résumé » pour le panneau Matériel : `niveau<TAB>état`.
/// Niveaux (ceux des tuiles, `tile_level`) : 0 neutre, 1 bon, 2 attention,
/// 3 alerte. Des lignes de détail peuvent suivre.
pub fn summary(out: &mut Reply, level: u8, state: fmt::Arguments) {
    let _ = write!(out, "{}\t{state}", level.min(3));
}

// --- Mémoire DMA ---------------------------------------------------------------------

/// Une mémoire DMA : pages contiguës, sous 4 Gio, que le périphérique lit
/// et écrit directement. `address` pour le pilote, `physical` pour le
/// périphérique.
#[derive(Clone, Copy, Debug)]
pub struct Dma {
    pub handle: Handle,
    pub address: usize,
    pub physical: u64,
    pub size: u64,
}

impl Dma {
    /// `right` : la poignée du droit DMA (la troisième, d'ordinaire).
    pub fn new(right: Handle, size: u64) -> rt::Result<Self> {
        let handle = rt::memory_create_dma(right, size)?;
        let physical = rt::memory_physical(handle)?;
        let address = rt::memory_map(handle)? as usize;
        Ok(Self { handle, address, physical, size })
    }
}

// --- Demandes --------------------------------------------------------------------------

/// Une adresse tapée dans une demande, en hexadécimal (« 0x70180 » ou
/// « 70180 »).
pub fn parse_hex(word: &str) -> Option<u32> {
    let digits = word.strip_prefix("0x").or_else(|| word.strip_prefix("0X")).unwrap_or(word);
    u32::from_str_radix(digits, 16).ok()
}
