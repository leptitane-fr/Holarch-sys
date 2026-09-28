//! "pci-8086-282a" : pilote pour le contrôleur SATA AHCI Intel (8086:282a).
//! Étape 1 du guide, "lecture" : lit les registres AHCI, ports, disques présents.
//! Contrat et méthode : guide/09-pilotes.md.
//!
//! Poignées de départ (convention du guide, § 3.3) : 1 registres (BAR 0),
//! 2 interruption, 3 droit DMA, 4 canal de service, 5 journal.

#![no_std]
#![no_main]

use core::fmt::Write;

use aiwos_pilote::{Dma, Mmio, Reply, Served, parse_hex, respond, summary};
use aiwos_rt::{self as rt, DeviceResources, FOREVER, Handle, log, signals};

// Le manifeste (IA6) : contrôleur SATA AHCI Intel 8086:282a (Dell Latitude 7490).
// Étape 1 : lecture seule des registres AHCI, ports, disques présents.
aiwos_pilote::pilote!("nom = pci-8086-282a
version = 1
abi = 1
appareil = pci 8086:282a
niveau = lecture
ressources = bar 0
évènements = aucun
écran-seul = aucun
description = Pilote pour le contrôleur SATA AHCI Intel 8086:282a (mode RAID/RST) : étape 1, lecture des registres AHCI, ports, disques présents");

const MMIO: Handle = Handle(1);
const IRQ: Handle = Handle(2);
const DMA: Handle = Handle(3);
const SERVICE: Handle = Handle(4);
const JOURNAL: Handle = Handle(5);

struct Driver {
    regs: Mmio,
    #[allow(dead_code)]
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
            "résumé" => summary(out, 1, format_args!("lecture seule · {} interruptions · SATA AHCI (8086:282a)", self.interrupts)),
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

    /// « état » : identité, taille des registres, registres AHCI clés.
    fn describe(&self, out: &mut Reply) {
        let _ = writeln!(
            out,
            "Intel SATA AHCI (8086:282a), registres : {} Kio",
            self.regs.size() / 1024
        );
        // Registres AHCI principaux (HBA : Host Bus Adapter)
        let cap = self.regs.r32(0x00); // CAP : Capabilities
        let ghc = self.regs.r32(0x04); // GHC : Global Host Control
        let is = self.regs.r32(0x08);  // IS : Interrupt Status
        let pi = self.regs.r32(0x0C);  // PI : Ports Implemented
        let _ = writeln!(out, "CAP : {:#010x}", cap);
        let _ = writeln!(out, "GHC : {:#010x}", ghc);
        let _ = writeln!(out, "IS  : {:#010x}", is);
        let _ = writeln!(out, "PI  : {:#010x} (ports présents)", pi);

        // Lire les ports implémentés (bits 0-31 dans PI)
        let _ = write!(out, "Ports implémentés : ");
        for port in 0..32 {
            if pi & (1 << port) != 0 {
                let _ = write!(out, "{}", port);
                // Lire le registre SATA Status (PxSSTS) pour chaque port
                let ssts = self.regs.r32(0x100 + port * 0x80 + 0x28);
                let _ = write!(out, "(SSTS={:#010x})", ssts);
            }
        }
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
    log!(JOURNAL, "prêt (8086:282a), lecture seule : contrôleur SATA AHCI Intel");

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