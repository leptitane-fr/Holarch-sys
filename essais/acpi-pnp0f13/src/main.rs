//! "acpi-pnp0f13" : pilote pour le pavé tactile PS/2 (PNP0F13, interruption 12).
//! Étape 1 du guide, "lecture" : lit l'état des ports PS/2.
//! Contrat et méthode : guide/09-pilotes.md, famille D (ports I/O historiques).
//!
//! Poignées de départ (convention du guide, § 3.3) : 1 registres (BAR 0),
//! 2 interruption, 3 droit DMA, 4 canal de service, 5 journal.

#![no_std]
#![no_main]

use core::fmt::Write;

use aiwos_pilote::{Reply, Served, parse_hex, respond, summary};
use aiwos_rt::{self as rt, DeviceResources, FOREVER, Handle, log, signals};

// Le manifeste (IA6) : pavé tactile PS/2 (PNP0F13, _SB.PCI0.LPCB.PS2M).
// Étape 1 : lecture seule des ports PS/2.
aiwos_pilote::pilote!("nom = acpi-pnp0f13
version = 1
abi = 1
appareil = acpi PNP0F13
niveau = lecture
ressources = bar 0
évènements = aucun
écran-seul = aucun
description = Pilote pour le pavé tactile PS/2 (PNP0F13, interruption 12) : étape 1, lecture des ports i8042");

const MMIO: Handle = Handle(1);
const IRQ: Handle = Handle(2);
const DMA: Handle = Handle(3);
const SERVICE: Handle = Handle(4);
const JOURNAL: Handle = Handle(5);

struct Driver {
    regs: Option<Mmio>,
    #[allow(dead_code)]
    info: Option<DeviceResources>,
    interrupts: u32,
    /// Une mémoire DMA, prise quand il en faudra une (étape 2 et
    /// suivantes).
    #[allow(dead_code)]
    dma: Option<()>,
}

impl Driver {
    fn handle(&mut self, request: &str, out: &mut Reply) {
        let mut words = request.split(' ');
        match words.next().unwrap_or("") {
            "résumé" => summary(out, 1, format_args!("lecture seule · {} interruptions · Pavé tactile PS/2 (PNP0F13)", self.interrupts)),
            "reg" => {
                let _ = write!(out, "reg : non applicable (ports I/O, utiliser 'port <adresse>')");
            }
            "port" => match words.next().and_then(parse_hex) {
                Some(port) => {
                    if let Some(regs) = &self.regs {
                        match regs.r8(port as usize) {
                            Ok(value) => {
                                let _ = write!(out, "port {port:#04x} = {value:#04x}", value);
                            }
                            Err(e) => {
                                let _ = write!(out, "erreur lecture port {port:#04x} : {e:?}");
                            }
                        }
                    } else {
                        let _ = write!(out, "registres non mappés");
                    }
                }
                None => {
                    let _ = write!(out, "port <adresse en hexadécimal>");
                }
            },
            "arrête" => {
                let _ = write!(out, "arrêté");
            }
            _ => self.describe(out),
        }
    }

    /// « état » : identité, état des ports PS/2.
    fn describe(&self, out: &mut Reply) {
        let _ = writeln!(out, "Pavé tactile PS/2 (PNP0F13, _SB.PCI0.LPCB.PS2M)");
        let _ = writeln!(out, "Interruption : IRQ 12 (ligne historique)");

        if let Some(regs) = &self.regs {
            // Lire le statut (0x64)
            match regs.r8(0x64 as usize) {
                Ok(status) => {
                    let _ = writeln!(out, "Statut (0x64) : {status:#04x}");
                    let output_full = (status & 0x01) != 0;
                    let input_full = (status & 0x02) != 0;
                    let system_flag = (status & 0x04) != 0;
                    let command = (status & 0x08) != 0;
                    let _ = writeln!(
                        out,
                        "  Output Full: {}, Input Full: {}, System Flag: {}, Command: {}",
                        if output_full { "oui" } else { "non" },
                        if input_full { "oui" } else { "non" },
                        if system_flag { "oui" } else { "non" },
                        if command { "oui" } else { "non" }
                    );
                }
                Err(e) => {
                    let _ = writeln!(out, "Erreur lecture port 0x64 : {e:?}");
                }
            }

            // Lire les données (0x60)
            match regs.r8(0x60 as usize) {
                Ok(data) => {
                    let _ = writeln!(out, "Données (0x60) : {data:#04x}");
                }
                Err(e) => {
                    let _ = writeln!(out, "Erreur lecture port 0x60 : {e:?}");
                }
            }
        } else {
            let _ = writeln!(out, "Registres non initialisés");
        }
    }

    fn on_interrupt(&mut self) {
        let _ = rt::interrupt_ack(IRQ);
        self.interrupts += 1;
    }
}

fn main() {
    let info = aiwos_pilote::resources(SERVICE);
    let regs = info.and_then(|inf| Mmio::map(MMIO, inf.mmio_size).ok());
    let mut driver = Driver { regs, info, interrupts: 0, dma: None };
    log!(JOURNAL, "prêt (PNP0F13), lecture seule : pavé tactile PS/2");

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

// Mock Mmio pour acpi-pnp0f13 (simplifié pour les ports I/O)
struct Mmio {
    base: usize,
    size: usize,
}

impl Mmio {
    fn map(handle: Handle, size: u64) -> Result<Self, &'static str> {
        Ok(Self { base: 0, size: size as usize })
    }

    fn r8(&self, offset: usize) -> Result<u8, &'static str> {
        if offset + 1 > self.size {
            Err("out of bounds")
        } else {
            // Lecture simulée des ports I/O (0x60, 0x64)
            unsafe {
                let port = if offset == 0x60 { 0x60 } else if offset == 0x64 { 0x64 } else { return Err("invalid port") };
                let mut value: u8 = 0;
                asm!("in al, dx", in("dx") port, out("al") value);
                Ok(value)
            }
        }
    }
}

rt::entry!(main);