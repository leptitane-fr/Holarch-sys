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

// Le manifeste (IA6) : à adapter en recopiant le modèle. L'appareil
// d'exemple est le lecteur SD du Chromebook. Pas encore d'« interruption »
// pour un pilote chargé (IA6b) : sa place (poignée 2) reste muette.
aiwos_pilote::pilote!("nom = lecteur-sd
version = 1
abi = 1
appareil = pci 8086:4df8
niveau = lecture
ressources = bar 0
évènements = aucun
écran-seul = aucun
description = Lecteur de carte SD (SDHCI) : lit ses registres, n'écrit rien");

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
            "résumé" => summary(out, 1, format_args!("SDHCI v{} · carte {} · lecture seule", (self.regs.r16(0xFE) & 0xFF) + 1, if self.regs.r32(0x24) & (1 << 16) != 0 { "insérée" } else { "absente" })),
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
        let ps = self.regs.r32(0x24);
        let _ = write!(
            out,
            "état présent (0x24) : {ps:#010x} ; carte {} ; courant max (0x48) : {:#010x}",
            if ps & (1 << 16) != 0 { "insérée" } else { "absente" },
            self.regs.r32(0x48)
        );
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
