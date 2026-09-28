//! "pci-8086-1903" : pilote pour le contrôleur USB xHCI Intel (8086:1903).
//! Étape 1 du guide, "lecture" : lit l'état du contrôleur USB.
//! Contrat et méthode : guide/09-pilotes.md.
//!
//! Poignées de départ (convention du guide, § 3.3) : 1 registres (BAR 0),
//! 2 interruption, 3 droit DMA, 4 canal de service, 5 journal.

#![no_std]
#![no_main]

use core::fmt::Write;

use aiwos_pilote::{Dma, Mmio, Reply, Served, parse_hex, respond, summary};
use aiwos_rt::{self as rt, DeviceResources, FOREVER, Handle, log, signals};

// Le manifeste (IA6) : contrôleur USB xHCI Intel (8086:1903).
// Étape 1 : lecture seule de l'état du contrôleur.
aiwos_pilote::pilote!("nom = pci-8086-1903
version = 1
abi = 1
appareil = pci 8086:1903
niveau = lecture
ressources = bar 0
évènements = aucun
écran-seul = aucun
description = Pilote pour le contrôleur USB xHCI Intel (8086:1903) : étape 1, lecture de l'état du contrôleur");

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
    #[allow(dead_code)]
    dma: Option<Dma>,
}

impl Driver {
    fn handle(&mut self, request: &str, out: &mut Reply) {
        let mut words = request.split(' ');
        match words.next().unwrap_or("") {
            "résumé" => summary(out, 1, format_args!("lecture seule · {} interruptions · USB xHCI (8086:1903)", self.interrupts)),
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

    /// « état » : identité, taille des registres, état du contrôleur USB xHCI.
    fn describe(&self, out: &mut Reply) {
        let _ = writeln!(
            out,
            "Intel USB xHCI (8086:1903), registres : {} Kio",
            self.regs.size() / 1024
        );

        let deven = self.regs.r32(0x0000);
        let _ = writeln!(out, "DEVEN : {:#010x} (Vendor:Device = {:04x}:{:04x})", deven, deven >> 16, deven & 0xFFFF);

        let caplength = self.regs.r8(0x0000) as u32;
        let _ = writeln!(out, "CAPLENGTH : {:#04x}", caplength);

        let hciversion = self.regs.r16(0x0002);
        let _ = writeln!(out, "HCIVERSION : {:#06x}", hciversion);

        if self.regs.size() > 0x0008 {
            let hcsparams1 = self.regs.r32(0x0004);
            let max_ports = (hcsparams1 >> 24) & 0xFF;
            let _ = writeln!(out, "HCSPARAMS1 : {:#010x} (Max Ports: {})", hcsparams1, max_ports);
        }
        if self.regs.size() > 0x000C {
            let hcsparams2 = self.regs.r32(0x0008);
            let _ = writeln!(out, "HCSPARAMS2 : {:#010x}", hcsparams2);
        }
        if self.regs.size() > 0x0014 {
            let hccparams = self.regs.r32(0x0010);
            let _ = writeln!(out, "HCCPARAMS : {:#010x}", hccparams);
        }
        if self.regs.size() > 0x0018 {
            let usbcmd = self.regs.r32(0x0014);
            let _ = writeln!(out, "USBCMD : {:#010x}", usbcmd);
        }
        if self.regs.size() > 0x001C {
            let usbsts = self.regs.r32(0x0018);
            let _ = writeln!(out, "USBSTS : {:#010x}", usbsts);
            let hch = (usbsts & 0x0000_0001) != 0;
            let _ = writeln!(out, "Contrôleur : {}", if hch { "ACTIF" } else { "INACTIF" });
        }
        if self.regs.size() > 0x0020 {
            let dnctrl = self.regs.r32(0x001C);
            let _ = writeln!(out, "DNCTRL : {:#010x}", dnctrl);
        }
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
    log!(JOURNAL, "prêt (8086:1903), lecture seule : contrôleur USB xHCI Intel");

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