//! « ahci » : le contrôleur SATA d'Intel en mode AHCI ou RAID/RST
//! (8086:282a, Dell Latitude 7490). Étape 1 du guide, « faire
//! connaissance » : il lit les registres AHCI, n'écrit rien sur le
//! matériel. Contrat et méthode : guide/09-pilotes.md.
//!
//! Les registres AHCI sont dans la BAR 5 (« ABAR », AHCI 1.3.1 § 2.1.11),
//! pas dans la BAR 0 : la fiche de l'appareil la montre, 2 Kio.
//!
//! Poignées de départ (convention du guide, § 3.3) : 1 registres (BAR 5),
//! 2 interruption (muette : pas accordée), 3 droit DMA, 4 canal de
//! service, 5 journal.

#![no_std]
#![no_main]

use core::fmt::Write;

use aiwos_pilote::{Mmio, Reply, Served, parse_hex, respond, summary};
use aiwos_rt::{self as rt, FOREVER, Handle, log, signals};

aiwos_pilote::pilote!("nom = ahci
version = 1
abi = 1
appareil = pci 8086:282a
niveau = lecture
ressources = bar 5
évènements = aucun
écran-seul = aucun
description = Contrôleur SATA Intel (AHCI, RAID/RST) : lit ses capacités, ses ports et les disques branchés");

const MMIO: Handle = Handle(1);
const SERVICE: Handle = Handle(4);
const JOURNAL: Handle = Handle(5);

// Registres généraux (AHCI 1.3.1 § 3.1).
const CAP: u32 = 0x00; // § 3.1.1 capacités
const GHC: u32 = 0x04; // § 3.1.2 commande générale
const PI: u32 = 0x0c; // § 3.1.4 ports présents
const VS: u32 = 0x10; // § 3.1.5 version
// Registres d'un port : 0x100 + 0x80 × n (§ 3.3).
const PORT_BASE: u32 = 0x100;
const PORT_SIZE: u32 = 0x80;
const PX_SIG: u32 = 0x24; // § 3.3.9 signature de l'appareil
const PX_SSTS: u32 = 0x28; // § 3.3.10 état du lien SATA

struct Driver {
    regs: Mmio,
}

impl Driver {
    fn handle(&mut self, request: &str, out: &mut Reply) {
        let mut words = request.split(' ');
        match words.next().unwrap_or("") {
            "résumé" => {
                let (ports, disks) = self.count();
                summary(out, 1, format_args!("lecture seule · {ports} port(s), {disks} appareil(s) branché(s)"));
            }
            "reg" => match words.next().and_then(parse_hex) {
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

    /// Les ports présents, et ceux où un appareil a établi le lien.
    fn count(&self) -> (u32, u32) {
        let pi = self.regs.r32(PI);
        let disks = (0..32).filter(|&n| pi & 1 << n != 0 && self.regs.r32(port(n) + PX_SSTS) & 0xf == 3).count();
        (pi.count_ones(), disks as u32)
    }

    /// « état » : version, capacités, et chaque port présent.
    fn describe(&self, out: &mut Reply) {
        let (cap, ghc, pi, vs) = (self.regs.r32(CAP), self.regs.r32(GHC), self.regs.r32(PI), self.regs.r32(VS));
        // VS : majeure (31:16), mineure (15:0) ; 0x00010301 = AHCI 1.3.1.
        let _ = writeln!(out, "AHCI, version {vs:#010x}, registres : {} Kio", self.regs.size() / 1024);
        // CAP : NP (bits 4:0) = ports - 1 ; ISS (23:20) vitesse maximale ;
        // S64A (31) adresses 64 bits. GHC : AE (31) mode AHCI actif.
        let _ = writeln!(
            out,
            "CAP {cap:#010x} : {} ports au plus, {}, adresses {} bits ; GHC {ghc:#010x} : mode AHCI {}",
            (cap & 0x1f) + 1,
            speed((cap >> 20) & 0xf),
            if cap & 1 << 31 != 0 { 64 } else { 32 },
            if ghc & 1 << 31 != 0 { "actif" } else { "coupé" }
        );
        let _ = writeln!(out, "Ports présents (PI) : {pi:#010x}");
        for n in (0..32).filter(|n| pi & 1 << n != 0) {
            let (sig, ssts) = (self.regs.r32(port(n) + PX_SIG), self.regs.r32(port(n) + PX_SSTS));
            // SSTS : DET (3:0) = 3 appareil présent et lien établi ;
            // SPD (7:4) vitesse négociée.
            let state = match ssts & 0xf {
                0 => "vide",
                1 => "appareil détecté, lien non établi",
                3 => "appareil branché, lien établi",
                4 => "port coupé",
                _ => "état inconnu",
            };
            let _ = write!(out, "  port {n} : {state}");
            if ssts & 0xf == 3 {
                // Signatures : AHCI § 3.3.9 et Linux ahci.h (ATA 0x00000101,
                // ATAPI 0xeb140101, multiplicateur de ports 0x96690101).
                let kind = match sig {
                    0x0000_0101 => "disque (ATA)",
                    0xeb14_0101 => "lecteur (ATAPI)",
                    0x9669_0101 => "multiplicateur de ports",
                    _ => "signature inconnue",
                };
                let _ = write!(out, ", {}, {kind} (SIG {sig:#010x})", speed((ssts >> 4) & 0xf));
            }
            let _ = writeln!(out);
        }
    }
}

fn port(n: u32) -> u32 {
    PORT_BASE + PORT_SIZE * n
}

/// Génération SATA (CAP.ISS, PxSSTS.SPD).
fn speed(generation: u32) -> &'static str {
    match generation {
        1 => "1,5 Gbit/s",
        2 => "3 Gbit/s",
        3 => "6 Gbit/s",
        _ => "vitesse inconnue",
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
    let mut driver = Driver { regs };
    let (ports, disks) = driver.count();
    log!(JOURNAL, "prêt ({:04x}:{:04x}), lecture seule : {ports} port(s), {disks} appareil(s) branché(s)", info.vendor_id, info.device_id);

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
