//! « e1000e » : la carte Ethernet Intel I219-LM (8086:15d7, Dell Latitude
//! 7490). Étape 1 du guide, « faire connaissance » : il lit l'état du
//! lien (branché, vitesse, duplex), n'écrit rien sur le matériel. Contrat
//! et méthode : guide/09-pilotes.md.
//!
//! L'adresse MAC n'est jamais lue ni montrée : elle identifie la machine,
//! et ce que dit un pilote peut finir dans un rapport publié.
//!
//! Poignées de départ (convention du guide, § 3.3) : 1 registres (BAR 0),
//! 2 interruption (muette : pas accordée), 3 droit DMA, 4 canal de
//! service, 5 journal.

#![no_std]
#![no_main]

use core::fmt::Write;

use holarch_pilote::{Mmio, Reply, Served, parse_hex, respond, summary};
use holarch_rt::{self as rt, FOREVER, Handle, log, signals};

holarch_pilote::pilote!("nom = e1000e
version = 1
abi = 1
appareil = pci 8086:15d7
niveau = lecture
ressources = bar 0
évènements = aucun
écran-seul = aucun
description = Ethernet Intel I219-LM : lit l'état du lien (branché, vitesse, duplex), sans l'adresse MAC");

const MMIO: Handle = Handle(1);
const SERVICE: Handle = Handle(4);
const JOURNAL: Handle = Handle(5);

// Registres (Linux drivers/net/ethernet/intel/e1000e/regs.h, 5.15).
const CTRL: u32 = 0x0000;
const STATUS: u32 = 0x0008;
const CTRL_EXT: u32 = 0x0018;

// Bits de STATUS (Linux e1000e/defines.h, 5.15).
const STATUS_FD: u32 = 0x0000_0001; // duplex intégral
const STATUS_LU: u32 = 0x0000_0002; // lien établi
const STATUS_SPEED_100: u32 = 0x0000_0040;
const STATUS_SPEED_1000: u32 = 0x0000_0080;

struct Driver {
    regs: Mmio,
}

/// L'état du lien, d'après STATUS.
struct Link {
    up: bool,
    speed: &'static str,
    full: bool,
}

impl Driver {
    fn link(&self) -> Link {
        let s = self.regs.r32(STATUS);
        // SPEED (bits 7:6) : 00 = 10, 01 = 100, 1x = 1000 Mbit/s.
        let speed = if s & STATUS_SPEED_1000 != 0 {
            "1000 Mbit/s"
        } else if s & STATUS_SPEED_100 != 0 {
            "100 Mbit/s"
        } else {
            "10 Mbit/s"
        };
        Link { up: s & STATUS_LU != 0, speed, full: s & STATUS_FD != 0 }
    }

    fn handle(&mut self, request: &str, out: &mut Reply) {
        let mut words = request.split(' ');
        match words.next().unwrap_or("") {
            "résumé" => {
                let l = self.link();
                if l.up {
                    summary(out, 1, format_args!("lecture seule · câble branché, {}, {}", l.speed, if l.full { "duplex intégral" } else { "semi-duplex" }));
                } else {
                    summary(out, 1, format_args!("lecture seule · pas de lien (câble débranché ?)"));
                }
            }
            "reg" => match words.next().and_then(parse_hex) {
                // Pas la zone de l'adresse MAC (0x5400 à 0x547f : RAL/RAH).
                Some(offset) if (0x5400..0x5480).contains(&offset) => {
                    let _ = write!(out, "{offset:#06x} : adresse MAC, jamais montrée");
                }
                Some(offset) => {
                    let _ = write!(out, "{offset:#06x} = {:#010x}", self.regs.r32(offset));
                }
                None => {
                    let _ = write!(out, "reg <adresse en hexadécimal>");
                }
            },
            _ => self.describe(out),
        }
    }

    /// « état » : le lien, et les registres de contrôle bruts.
    fn describe(&self, out: &mut Reply) {
        let l = self.link();
        let _ = writeln!(out, "Ethernet Intel I219-LM, registres : {} Kio", self.regs.size() / 1024);
        if l.up {
            let _ = writeln!(out, "Lien : établi, {}, {}", l.speed, if l.full { "duplex intégral" } else { "semi-duplex" });
        } else {
            let _ = writeln!(out, "Lien : absent (câble débranché, ou carte en veille)");
        }
        let _ = writeln!(
            out,
            "CTRL {:#010x} · STATUS {:#010x} · CTRL_EXT {:#010x}",
            self.regs.r32(CTRL),
            self.regs.r32(STATUS),
            self.regs.r32(CTRL_EXT)
        );
    }
}

fn main() {
    let Some(info) = holarch_pilote::resources(SERVICE) else {
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
    let mut driver = Driver { regs };
    let l = driver.link();
    log!(JOURNAL, "prêt ({:04x}:{:04x}), lecture seule : lien {}", info.vendor_id, info.device_id, if l.up { l.speed } else { "absent" });

    let mut request = [0u8; 256];
    loop {
        let mut items = [rt::wait_item(SERVICE, signals::READABLE | signals::PEER_CLOSED)];
        if rt::wait_many(&mut items, FOREVER).is_err() {
            return;
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
