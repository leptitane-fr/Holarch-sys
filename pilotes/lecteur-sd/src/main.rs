//! « lecteur-sd » : lecteur de carte SD (contrôleur SDHCI, PCI 8086:4df8).
//! Étape 1 du guide, « faire connaissance », version 2 : lecture seule,
//! plus la surveillance de la carte (Present State 0x24, toutes les 200 ms).
//! Contrat et méthode : guide/09-pilotes.md.
//!
//! Poignées de départ (convention du guide, § 3.3) : 1 registres,
//! 2 interruption, 3 droit DMA, 4 canal de service, 5 journal.

#![no_std]
#![no_main]

use core::fmt::Write;

use aiwos_pilote::{Dma, Mmio, Reply, Served, parse_hex, respond, summary};
use aiwos_rt::{self as rt, DeviceResources, Handle, log, signals};

// Le manifeste (IA6). Niveau lecture : aucune écriture sur le matériel.
aiwos_pilote::pilote!("nom = lecteur-sd
version = 2
abi = 1
appareil = pci 8086:4df8
niveau = lecture
ressources = bar 0
évènements = aucun
écran-seul = aucun
description = Lecteur de carte SD (SDHCI) : lit ses registres, n'écrit rien ; surveille la carte");

const MMIO: Handle = Handle(1);
const IRQ: Handle = Handle(2);
const DMA: Handle = Handle(3);
const SERVICE: Handle = Handle(4);
const JOURNAL: Handle = Handle(5);

/// SDHCI Present State, décalage 0x24.
const PRESENT_STATE: u32 = 0x24;
const CARD_INSERTED: u32 = 1 << 16;
const CARD_STABLE: u32 = 1 << 17;
/// Bit 19 : 1 = écriture permise, 0 = protégée (SDHCI 3.00).
const WRITE_ENABLED: u32 = 1 << 19;

struct Driver {
    regs: Mmio,
    info: DeviceResources,
    interrupts: u32,
    /// Une mémoire DMA, prise quand il en faudra une (étape 2 et
    /// suivantes).
    #[allow(dead_code)]
    dma: Option<Dma>,
    /// Dernier état « carte insérée » stable vu.
    card_present: bool,
    insertions: u32,
    removals: u32,
}

impl Driver {
    /// Une demande de racine (les mots après le nom de la commande).
    fn handle(&mut self, request: &str, out: &mut Reply) {
        let mut words = request.split(' ');
        match words.next().unwrap_or("") {
            // Pour le panneau Matériel : « niveau\tétat ».
            "résumé" => {
                let present = self.regs.r32(PRESENT_STATE) & CARD_INSERTED != 0;
                if present {
                    summary(out, 1, format_args!("carte présente"));
                } else {
                    summary(out, 0, format_args!("carte absente"));
                }
            }
            "carte" => self.card(out),
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

    /// « carte » : état actuel et compteurs d'insertions / retraits.
    fn card(&self, out: &mut Reply) {
        let ps = self.regs.r32(PRESENT_STATE);
        let (present, stable, protected) = card_words(ps);
        let _ = writeln!(out, "carte {present}, {stable}, {protected}");
        let _ = write!(out, "insertions {}, retraits {}", self.insertions, self.removals);
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
        let ver = self.regs.r16(0xFE);
        let spec = match ver & 0xFF {
            0 => "1.00", 1 => "2.00", 2 => "3.00", 3 => "4.00", 4 => "4.10", 5 => "4.20", _ => "?",
        };
        let _ = writeln!(out, "version hôte (0xFE) : {ver:#06x} → SDHCI {spec}, fournisseur {}", ver >> 8);
        let c0 = self.regs.r32(0x40);
        let c1 = self.regs.r32(0x44);
        let _ = writeln!(out, "capacités (0x40/0x44) : {c0:#010x} {c1:#010x}");
        let tens = |b: u32, s: &'static str| if c0 & (1 << b) != 0 { s } else { "" };
        let _ = writeln!(out, "tensions :{}{}{}", tens(24, " 3,3 V"), tens(25, " 3,0 V"), tens(26, " 1,8 V"));
        let base = (c0 >> 8) & 0xFF;
        let tclk = c0 & 0x3F;
        let unit = if c0 & (1 << 7) != 0 { "MHz" } else { "kHz" };
        let _ = writeln!(out, "horloge de base : {base} MHz ; délai : {tclk} {unit}");
        let blk = 512u32 << ((c0 >> 16) & 3);
        let oui = |b: bool| if b { "oui" } else { "non" };
        let _ = writeln!(
            out,
            "bloc max {blk} o ; bus 8 bits {} ; ADMA2 {} ; SDMA {} ; grande vitesse {} ; suspension {} ; 64 bits {} ; interruption asynchrone {}",
            oui(c0 & (1 << 18) != 0), oui(c0 & (1 << 19) != 0), oui(c0 & (1 << 22) != 0),
            oui(c0 & (1 << 21) != 0), oui(c0 & (1 << 23) != 0), oui(c0 & (1 << 28) != 0), oui(c0 & (1 << 29) != 0)
        );
        let slot = match (c0 >> 30) & 3 { 0 => "amovible", 1 => "intégré", 2 => "bus partagé", _ => "?" };
        let _ = writeln!(
            out,
            "emplacement {slot} ; SDR50 {} ; SDR104 {} ; DDR50 {} ; multiplicateur d'horloge {}",
            oui(c1 & 1 != 0), oui(c1 & 2 != 0), oui(c1 & 4 != 0), (c1 >> 16) & 0xFF
        );
        let ps = self.regs.r32(PRESENT_STATE);
        let _ = write!(
            out,
            "état présent (0x24) : {ps:#010x} ; carte {} ; courant max (0x48) : {:#010x}",
            if ps & CARD_INSERTED != 0 { "insérée" } else { "absente" },
            self.regs.r32(0x48)
        );
    }

    /// Lit Present State. Compte et journalise chaque insertion ou retrait
    /// une fois l'état stable (bit 17) : le bit 16 n'est valable qu'alors.
    fn poll_card(&mut self) {
        let ps = self.regs.r32(PRESENT_STATE);
        if ps == u32::MAX {
            return;
        }
        if ps & CARD_STABLE == 0 {
            return;
        }
        let present = ps & CARD_INSERTED != 0;
        if present == self.card_present {
            return;
        }
        self.card_present = present;
        if present {
            self.insertions = self.insertions.saturating_add(1);
        } else {
            self.removals = self.removals.saturating_add(1);
        }
        let (p, s, w) = card_words(ps);
        log!(JOURNAL, "carte {p}, {s}, {w}");
    }

    fn on_interrupt(&mut self) {
        // D'abord effacer le signal, puis lire la cause dans les registres.
        let _ = rt::interrupt_ack(IRQ);
        self.interrupts += 1;
    }
}

fn card_words(ps: u32) -> (&'static str, &'static str, &'static str) {
    (
        if ps & CARD_INSERTED != 0 { "présente" } else { "absente" },
        if ps & CARD_STABLE != 0 { "stable" } else { "instable" },
        if ps & WRITE_ENABLED == 0 { "protégée en écriture" } else { "non protégée en écriture" },
    )
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
    let ps = regs.r32(PRESENT_STATE);
    let mut driver = Driver {
        regs,
        info,
        interrupts: 0,
        dma: None,
        card_present: ps != u32::MAX && ps & CARD_INSERTED != 0,
        insertions: 0,
        removals: 0,
    };
    // Exemple, pour plus tard : `Dma::new(DMA, 4096)`.
    let _ = DMA;
    log!(JOURNAL, "prêt ({:04x}:{:04x}), lecture seule", info.vendor_id, info.device_id);

    let mut request = [0u8; 256];
    loop {
        let mut items = [
            rt::wait_item(SERVICE, signals::READABLE | signals::PEER_CLOSED),
            rt::wait_item(IRQ, signals::INTERRUPT),
        ];
        // Échéance ~200 ms : interroger Present State sans boucle active
        // (guide § 3.6, § 8 : TimedOut n'est pas une erreur fatale).
        match rt::wait_many(&mut items, rt::deadline_in_ms(200)) {
            Err(rt::Error::TimedOut) => {
                driver.poll_card();
                continue;
            }
            Err(_) => return,
            Ok(_) => {}
        }
        driver.poll_card();
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
