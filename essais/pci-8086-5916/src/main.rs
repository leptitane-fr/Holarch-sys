//! "pci-8086-5916" : pilote pour l'affichage Intel HD Graphics 620 (8086:5916).
//! Étape 1 du guide, "lecture" : lit les registres du contrôleur graphique.
//! Contrat et méthode : guide/09-pilotes.md.
//!
//! Poignées de départ (convention du guide, § 3.3) : 1 registres (BAR 0),
//! 2 interruption, 3 droit DMA, 4 canal de service, 5 journal.

#![no_std]
#![no_main]

use core::fmt::Write;

use aiwos_pilote::{Dma, Mmio, Reply, Served, parse_hex, respond, summary};
use aiwos_rt::{self as rt, DeviceResources, FOREVER, Handle, log, signals};

// Le manifeste (IA6) : affichage Intel HD Graphics 620 (8086:5916).
// Étape 1 : lecture seule des registres graphiques.
aiwos_pilote::pilote!("nom = pci-8086-5916
version = 1
abi = 1
appareil = pci 8086:5916
niveau = lecture
ressources = bar 0
évènements = aucun
écran-seul = aucun
description = Pilote pour l'affichage Intel HD Graphics 620 (Kaby Lake) : étape 1, lecture des registres du contrôleur graphique");

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
            "résumé" => summary(out, 1, format_args!("lecture seule · {} interruptions · Intel HD Graphics 620 (8086:5916)", self.interrupts)),
            "reg" => match words.next().and_then(parse_hex) {
                Some(offset) => {
                    let _ = write!(out, "{offset:#06x} = {:#010x}", self.regs.r32(offset));
                }
                None => {
                    let _ = write!(out, "reg <adresse en hexadécimal>");
                }
            },
            "arrête" => {
                let _ = write!(out, "arrêté");
            }
            _ => self.describe(out),
        }
    }

    /// « état » : identité, taille des registres, registres graphiques clés.
    fn describe(&self, out: &mut Reply) {
        let _ = writeln!(
            out,
            "Intel HD Graphics 620 (8086:5916), registres : {} Mio",
            self.regs.size() / (1024 * 1024)
        );

        // Registres standard PCI Express
        let deven = self.regs.r32(0x0000);
        let _ = writeln!(out, "DEVEN : {:#010x} (Vendor:Device = {:04x}:{:04x})", deven, deven >> 16, deven & 0xFFFF);

        // Registres du contrôleur graphique (Gen9/Kaby Lake)
        if self.regs.size() > 0x2000 {
            let gt_control = self.regs.r32(0x2000);
            let _ = writeln!(out, "GT_CONTROL : {:#010x}", gt_control);
        }
        if self.regs.size() > 0x20D0 {
            let render_standby = self.regs.r32(0x20D0);
            let _ = writeln!(out, "RENDER_STANDBY : {:#010x}", render_standby);
        }
        if self.regs.size() > 0x70000 {
            let pipe_a_config = self.regs.r32(0x70000);
            let _ = writeln!(out, "PIPE_A_CONFIG : {:#010x}", pipe_a_config);
        }

        let _ = writeln!(out, "Taille BAR 0 : {} octets", self.regs.size());
    }

    fn on_interrupt(&mut self) {
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
    let _ = DMA;
    log!(JOURNAL, "prêt (8086:5916), lecture seule : Intel HD Graphics 620");

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