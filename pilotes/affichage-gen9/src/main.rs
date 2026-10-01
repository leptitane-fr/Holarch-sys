//! « affichage-gen9 » : le processeur graphique Intel HD Graphics 620
//! (8086:5916, génération 9, Kaby Lake ; Dell Latitude 7490). Étape 1 du
//! guide, « faire connaissance » : il lit l'état de l'affichage que le
//! firmware a allumé (pipe A, plan 1, écran, rétroéclairage), n'écrit rien
//! sur le matériel. Contrat et méthode : guide/09-pilotes.md.
//!
//! Poignées de départ (convention du guide, § 3.3) : 1 registres (BAR 0,
//! 16 Mio : registres puis GTT), 2 interruption (muette : pas accordée),
//! 3 droit DMA, 4 canal de service, 5 journal.

#![no_std]
#![no_main]

use core::fmt::Write;

use holarch_pilote::{Mmio, Reply, Served, parse_hex, respond, summary};
use holarch_rt::{self as rt, FOREVER, Handle, log, signals};

holarch_pilote::pilote!("nom = affichage-gen9
version = 1
abi = 1
appareil = pci 8086:5916
niveau = lecture
ressources = bar 0
évènements = aucun
écran-seul = aucun
description = Affichage Intel HD Graphics 620 : lit la pipe A, le plan 1, l'écran et le rétroéclairage allumés par le firmware");

const MMIO: Handle = Handle(1);
const SERVICE: Handle = Handle(4);
const JOURNAL: Handle = Handle(5);

// Registres (Linux drivers/gpu/drm/i915/i915_reg.h, 5.15 ; génération 9).
const PIPEASRC: u32 = 0x6001c; // taille de l'image : (largeur-1) << 16 | (hauteur-1)
const PIPEACONF: u32 = 0x70008; // bit 31 : pipe A allumée
const PLANE_CTL_1_A: u32 = 0x70180; // bit 31 : plan 1 allumé
const PLANE_STRIDE_1_A: u32 = 0x70188; // pas, en blocs de 64 octets (linéaire)
const PLANE_SURF_1_A: u32 = 0x7019c; // adresse de l'image dans la GGTT
const PP_STATUS: u32 = 0xc7200; // bit 31 : écran interne alimenté
const BLC_PWM_PCH_CTL1: u32 = 0xc8250; // bit 31 : rétroéclairage (PWM) actif
const BLC_PWM_PCH_CTL2: u32 = 0xc8254; // période (31:16), rapport cyclique (15:0)

struct Driver {
    regs: Mmio,
}

impl Driver {
    fn on(&self, reg: u32) -> bool {
        self.regs.r32(reg) & 1 << 31 != 0
    }

    /// (largeur, hauteur) de l'image de la pipe A.
    fn size(&self) -> (u32, u32) {
        let src = self.regs.r32(PIPEASRC);
        ((src >> 16 & 0x1fff) + 1, (src & 0x1fff) + 1)
    }

    /// Rétroéclairage en % (rapport cyclique sur période), s'il est actif.
    fn backlight(&self) -> Option<u32> {
        let ctl2 = self.regs.r32(BLC_PWM_PCH_CTL2);
        let (period, duty) = (ctl2 >> 16, ctl2 & 0xffff);
        (self.on(BLC_PWM_PCH_CTL1) && period != 0).then(|| duty * 100 / period)
    }

    fn handle(&mut self, request: &str, out: &mut Reply) {
        let mut words = request.split(' ');
        match words.next().unwrap_or("") {
            "résumé" => {
                let (w, h) = self.size();
                summary(out, 1, format_args!("lecture seule · pipe A {} · {w} × {h}", if self.on(PIPEACONF) { "allumée" } else { "éteinte" }));
            }
            "reg" => match words.next().and_then(parse_hex) {
                Some(offset) => {
                    let _ = write!(out, "{offset:#07x} = {:#010x}", self.regs.r32(offset));
                }
                None => {
                    let _ = write!(out, "reg <adresse en hexadécimal>");
                }
            },
            _ => self.describe(out),
        }
    }

    /// « état » : ce que le firmware a allumé.
    fn describe(&self, out: &mut Reply) {
        let (w, h) = self.size();
        let _ = writeln!(out, "Intel HD Graphics 620 (génération 9), registres et GTT : {} Mio", self.regs.size() >> 20);
        let _ = writeln!(out, "Pipe A : {}, image {w} × {h}", if self.on(PIPEACONF) { "allumée" } else { "éteinte" });
        let _ = writeln!(
            out,
            "Plan 1 : {}, adresse GGTT {:#010x}, pas {} octets",
            if self.on(PLANE_CTL_1_A) { "allumé" } else { "éteint" },
            self.regs.r32(PLANE_SURF_1_A) & !0xfff,
            (self.regs.r32(PLANE_STRIDE_1_A) & 0x3ff) * 64
        );
        let _ = writeln!(out, "Écran interne : {}", if self.on(PP_STATUS) { "alimenté" } else { "éteint" });
        match self.backlight() {
            Some(pct) => {
                let _ = writeln!(out, "Rétroéclairage : {pct} %");
            }
            None => {
                let _ = writeln!(out, "Rétroéclairage : réglage non lisible ici (PWM du PCH coupé)");
            }
        }
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
    let (w, h) = driver.size();
    log!(JOURNAL, "prêt ({:04x}:{:04x}), lecture seule : image {w} × {h}", info.vendor_id, info.device_id);

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
