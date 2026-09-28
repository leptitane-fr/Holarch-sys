//! "pci-8086-15d7" : pilote pour Ethernet Intel I219-LM (8086:15d7).
//! Étape 1 du guide, "lecture" : lit l'état du lien, la vitesse.
//! Contrat et méthode : guide/09-pilotes.md.
//!
//! Poignées de départ (convention du guide, § 3.3) : 1 registres (BAR 0),
//! 2 interruption, 3 droit DMA, 4 canal de service, 5 journal.

#![no_std]
#![no_main]

use core::fmt::Write;

use aiwos_pilote::{Dma, Mmio, Reply, Served, parse_hex, respond, summary};
use aiwos_rt::{self as rt, DeviceResources, FOREVER, Handle, log, signals};

// Le manifeste (IA6) : Ethernet Intel I219-LM (8086:15d7).
// Étape 1 : lecture seule de l'état du lien et de la vitesse.
aiwos_pilote::pilote!("nom = pci-8086-15d7
version = 1
abi = 1
appareil = pci 8086:15d7
niveau = lecture
ressources = bar 0
évènements = aucun
écran-seul = aucun
description = Pilote pour Ethernet Intel I219-LM (e1000e) : étape 1, lecture de l'état du lien, vitesse");

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
    fn handle(&mut self, request: &str, out: &mut Reply) {
        let mut words = request.split(' ');
        match words.next().unwrap_or("") {
            "résumé" => summary(out, 1, format_args!("lecture seule · {} interruptions · Ethernet I219-LM (8086:15d7)", self.interrupts)),
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

    /// « état » : identité, taille des registres, état du lien Ethernet.
    fn describe(&self, out: &mut Reply) {
        let _ = writeln!(
            out,
            "Intel Ethernet I219-LM (8086:15d7), registres : {} Kio",
            self.regs.size() / 1024
        );

        let deven = self.regs.r32(0x0000);
        let _ = writeln!(out, "DEVEN : {:#010x} (Vendor:Device = {:04x}:{:04x})", deven, deven >> 16, deven & 0xFFFF);

        if self.regs.size() > 0x10000 {
            let ctrl = self.regs.r32(0x00000);
            let _ = writeln!(out, "CTRL : {:#010x}", ctrl);
        }
        if self.regs.size() > 0x10008 {
            let status = self.regs.r32(0x00008);
            let _ = writeln!(out, "STATUS : {:#010x}", status);
            let link_up = (status & 0x0000_0002) != 0;
            let _ = writeln!(out, "Lien : {}", if link_up { "UP" } else { "DOWN" });
        }
        if self.regs.size() > 0x10018 {
            let ctrl_ext = self.regs.r32(0x00018);
            let _ = writeln!(out, "CTRL_EXT : {:#010x}", ctrl_ext);
        }
        if self.regs.size() > 0x1001C {
            let status_ext = self.regs.r32(0x0001C);
            let _ = writeln!(out, "STATUS_EXT : {:#010x}", status_ext);
            let link_speed = status_ext & 0x0000_000F;
            let speed_str = match link_speed {
                0b0001 => "10 Mbps",
                0b0010 => "100 Mbps",
                0b0100 => "1 Gbps",
                0b1000 => "2.5 Gbps",
                _ => "inconnu",
            };
            let _ = writeln!(out, "Vitesse : {}", speed_str);
        }
        if self.regs.size() > 0x15403 {
            let mac_low = self.regs.r32(0x05400);
            let mac_high = self.regs.r32(0x05404);
            let _ = writeln!(out, "Adresse MAC (partielle) : {:#010x} {:#010x}", mac_low, mac_high);
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
    log!(JOURNAL, "prêt (8086:15d7), lecture seule : Ethernet Intel I219-LM");

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