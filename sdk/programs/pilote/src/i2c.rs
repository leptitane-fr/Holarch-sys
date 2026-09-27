//! Le contrôleur I2C d'Intel (LPSS) : un cœur DesignWare de Synopsys
//! (DW_apb_i2c), ici en maître, par interrogation (sans interruption).
//!
//! Registres du cœur de 0x00 à 0xFF (noms `IC_*` de Synopsys), puis ceux
//! d'Intel dès 0x200 (LPSS : réinitialisation…). Rôles et valeurs d'après
//! les pilotes Linux `i2c-designware` et `intel-lpss`, lus comme
//! documentation.
//!
//! Une transaction : l'adresse de l'appareil dans `IC_TAR`, puis chaque
//! octet à écrire, et chaque octet à lire, déposé comme une commande dans
//! la file d'émission (`IC_DATA_CMD`) ; le contrôleur envoie START,
//! l'adresse, les octets, RESTART entre écriture et lecture, STOP après la
//! dernière commande. Les octets lus arrivent dans la file de réception.

use core::ptr::{read_volatile, write_volatile};

use aiwos_rt as rt;

const IC_CON: u32 = 0x00;
const IC_TAR: u32 = 0x04;
const IC_DATA_CMD: u32 = 0x10;
const IC_SS_SCL_HCNT: u32 = 0x14;
const IC_SS_SCL_LCNT: u32 = 0x18;
const IC_FS_SCL_HCNT: u32 = 0x1C;
const IC_FS_SCL_LCNT: u32 = 0x20;
const IC_INTR_MASK: u32 = 0x30;
const IC_RAW_INTR_STAT: u32 = 0x34;
const IC_RX_TL: u32 = 0x38;
const IC_TX_TL: u32 = 0x3C;
const IC_CLR_INTR: u32 = 0x40;
const IC_CLR_TX_ABRT: u32 = 0x54;
const IC_CLR_STOP_DET: u32 = 0x60;
const IC_ENABLE: u32 = 0x6C;
const IC_STATUS: u32 = 0x70;
const IC_SDA_HOLD: u32 = 0x7C;
const IC_TX_ABRT_SOURCE: u32 = 0x80;
const IC_ENABLE_STATUS: u32 = 0x9C;
const IC_FS_SPKLEN: u32 = 0xA0;
const IC_COMP_PARAM_1: u32 = 0xF4;
const IC_COMP_VERSION: u32 = 0xF8;
const IC_COMP_TYPE: u32 = 0xFC;
/// La signature du cœur DesignWare (« DW », puis 0x0140).
const DW_TYPE: u32 = 0x4457_0140;

/// Registre d'Intel : réinitialisation du contrôleur (bits 0 et 1) et de
/// son DMA (bit 2). À 1 : libérés.
const LPSS_RESETS: u32 = 0x204;
const LPSS_RELEASED: u32 = 0x7;

const ENABLE_ON: u32 = 1 << 0;
/// Abandon de la transaction en cours : le contrôleur vide ses files et
/// libère le bus, puis remet ce bit à 0. Ne s'écrit que contrôleur allumé.
const ENABLE_ABORT: u32 = 1 << 1;

const CON_MASTER: u32 = 1 << 0;
const CON_SPEED_STANDARD: u32 = 1 << 1;
const CON_SPEED_FAST: u32 = 2 << 1;
const CON_RESTART_EN: u32 = 1 << 5;
const CON_SLAVE_DISABLE: u32 = 1 << 6;

const CMD_READ: u32 = 1 << 8;
const CMD_STOP: u32 = 1 << 9;
const CMD_RESTART: u32 = 1 << 10;

const INTR_TX_ABRT: u32 = 1 << 6;
const INTR_STOP_DET: u32 = 1 << 9;

const STATUS_TFNF: u32 = 1 << 1;
const STATUS_RFNE: u32 = 1 << 3;

/// L'horloge du contrôleur, supposée quand le firmware n'a rien réglé : la
/// plus rapide des puces d'Intel récentes (216 MHz). Si la vraie est plus
/// lente, le bus l'est aussi : jamais plus rapide que demandé.
const ASSUMED_CLOCK_KHZ: u32 = 216_000;

#[derive(Clone, Copy)]
struct Regs(usize);

impl Regs {
    fn r32(self, reg: u32) -> u32 {
        unsafe { read_volatile((self.0 + reg as usize) as *const u32) }
    }

    fn w32(self, reg: u32, value: u32) {
        unsafe { write_volatile((self.0 + reg as usize) as *mut u32, value) }
    }
}

/// Ce qui peut faire échouer une transaction.
#[derive(Clone, Copy, Debug)]
pub enum Fault {
    /// Pas de cœur DesignWare à cette adresse (valeur lue).
    NotDesignWare(u32),
    /// Le contrôleur ne s'arrête pas (ou ne démarre pas).
    Stuck,
    /// Transaction abandonnée par le contrôleur : la cause
    /// (`IC_TX_ABRT_SOURCE`).
    Abort(u32),
    /// Trop long : octets envoyés et reçus jusque-là.
    Timeout { sent: usize, received: usize },
}

impl core::fmt::Display for Fault {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match *self {
            Fault::NotDesignWare(v) => write!(f, "pas de contrôleur DesignWare (signature {v:#010x})"),
            Fault::Stuck => write!(f, "le contrôleur ne change pas d'état"),
            Fault::Abort(source) if source & 1 != 0 => {
                write!(f, "personne ne répond à cette adresse (source {source:#x})")
            }
            Fault::Abort(source) if source & 8 != 0 => write!(f, "l'appareil a refusé un octet (source {source:#x})"),
            Fault::Abort(source) if source & 1 << 12 != 0 => {
                write!(f, "bus disputé : arbitrage perdu (source {source:#x})")
            }
            Fault::Abort(source) => write!(f, "transaction abandonnée (source {source:#x})"),
            Fault::Timeout { sent, received } => {
                write!(f, "trop long : {sent} commande(s) envoyée(s), {received} octet(s) reçu(s)")
            }
        }
    }
}

/// Les réglages de l'horloge du bus : durées haute et basse de SCL, et
/// maintien de SDA, en cycles de l'horloge du contrôleur.
#[derive(Clone, Copy)]
pub struct Timing {
    pub high: u32,
    pub low: u32,
    pub hold: u32,
    /// D'où ils viennent : laissés par le firmware, ou calculés.
    pub from_firmware: bool,
}

/// Ce que montrent les registres, sans rien toucher.
pub struct State {
    pub comp_type: u32,
    pub version: u32,
    pub param: u32,
    pub resets: u32,
    pub con: u32,
    pub tar: u32,
    pub standard: (u32, u32),
    pub fast: (u32, u32),
    pub hold: u32,
    pub spike: u32,
    pub enabled: u32,
    pub status: u32,
}

pub struct Controller {
    regs: Regs,
    fast: bool,
    rx_depth: usize,
    pub timing: Timing,
    /// Trouvé allumé (une transaction laissée en suspens), ou réinitialisé :
    /// pour le journal.
    pub recovered: Option<&'static str>,
}

/// Lit l'état du contrôleur, sans rien écrire.
pub fn state(base: usize) -> State {
    let r = Regs(base);
    State {
        comp_type: r.r32(IC_COMP_TYPE),
        version: r.r32(IC_COMP_VERSION),
        param: r.r32(IC_COMP_PARAM_1),
        resets: r.r32(LPSS_RESETS),
        con: r.r32(IC_CON),
        tar: r.r32(IC_TAR),
        standard: (r.r32(IC_SS_SCL_HCNT), r.r32(IC_SS_SCL_LCNT)),
        fast: (r.r32(IC_FS_SCL_HCNT), r.r32(IC_FS_SCL_LCNT)),
        hold: r.r32(IC_SDA_HOLD),
        spike: r.r32(IC_FS_SPKLEN),
        enabled: r.r32(IC_ENABLE_STATUS),
        status: r.r32(IC_STATUS),
    }
}

/// Durées d'horloge pour `speed_hz`, à `clock_khz` : celles de la norme I2C
/// (mode rapide : 600 ns haut, 1300 ns bas ; standard : 4000 et 4700), plus
/// 300 ns de temps de descente, comme Linux (`i2c_dw_scl_hcnt/lcnt`).
fn computed(clock_khz: u32, fast: bool) -> Timing {
    let (high_ns, low_ns, fall_ns) = if fast { (600, 1300, 300) } else { (4000, 4700, 300) };
    let cycles = |ns: u64| ((clock_khz as u64 * ns + 500_000) / 1_000_000) as u32;
    Timing {
        high: cycles(high_ns + fall_ns).saturating_sub(3).max(6),
        low: cycles(low_ns + fall_ns).saturating_sub(1).max(8),
        hold: cycles(300).max(1),
        from_firmware: false,
    }
}

impl Controller {
    /// Prépare le contrôleur à parler à `address` à `speed_hz` : sortie de
    /// réinitialisation, maître, adresses de 7 bits, sans interruption. Les
    /// durées d'horloge laissées par le firmware (coreboot règle le bus pour
    /// son propre usage) sont gardées ; sinon, calculées.
    pub fn new(base: usize, address: u16, speed_hz: u32) -> Result<Self, Fault> {
        let regs = Regs(base);
        if regs.r32(LPSS_RESETS) & LPSS_RELEASED != LPSS_RELEASED {
            regs.w32(LPSS_RESETS, LPSS_RELEASED);
            sleep_us(1000);
        }
        let comp_type = regs.r32(IC_COMP_TYPE);
        if comp_type != DW_TYPE {
            return Err(Fault::NotDesignWare(comp_type));
        }
        let fast = speed_hz > 100_000;
        let (high_reg, low_reg) = if fast { (IC_FS_SCL_HCNT, IC_FS_SCL_LCNT) } else { (IC_SS_SCL_HCNT, IC_SS_SCL_LCNT) };
        let (high, low) = (regs.r32(high_reg), regs.r32(low_reg));
        let timing = if high >= 6 && low >= 8 && high < 0x1_0000 && low < 0x1_0000 {
            Timing { high, low, hold: regs.r32(IC_SDA_HOLD) & 0xFFFF, from_firmware: true }
        } else {
            computed(ASSUMED_CLOCK_KHZ, fast)
        };
        let param = regs.r32(IC_COMP_PARAM_1);
        let was_on = regs.r32(IC_ENABLE_STATUS) & 1 != 0;
        let mut controller =
            Controller { regs, fast, rx_depth: (((param >> 8) & 0xFF) + 1) as usize, timing, recovered: None };
        // En dernier recours, un contrôleur qui ne s'arrête pas est
        // réinitialisé (registre d'Intel) ; il oublie alors ses durées
        // d'horloge et son filtre, rendus ci-dessous.
        let spike = regs.r32(IC_FS_SPKLEN);
        let reset = controller.disable().is_err();
        if reset {
            regs.w32(LPSS_RESETS, 0);
            sleep_us(1000);
            regs.w32(LPSS_RESETS, LPSS_RELEASED);
            sleep_us(1000);
            regs.w32(IC_FS_SPKLEN, spike);
            controller.disable()?;
        }
        controller.recovered = if reset {
            Some("réinitialisé : il ne s'arrêtait pas")
        } else if was_on {
            Some("trouvé en marche (transaction laissée en suspens), arrêté")
        } else {
            None
        };
        let speed = if fast { CON_SPEED_FAST } else { CON_SPEED_STANDARD };
        regs.w32(IC_CON, CON_MASTER | speed | CON_RESTART_EN | CON_SLAVE_DISABLE);
        regs.w32(IC_TAR, address as u32 & 0x7F);
        if !timing.from_firmware || reset {
            regs.w32(high_reg, timing.high);
            regs.w32(low_reg, timing.low);
            regs.w32(IC_SDA_HOLD, timing.hold);
        }
        regs.w32(IC_INTR_MASK, 0);
        regs.w32(IC_RX_TL, 0);
        regs.w32(IC_TX_TL, 0);
        Ok(controller)
    }

    pub fn fast(&self) -> bool {
        self.fast
    }

    fn set_enabled(&self, on: bool) -> Result<(), Fault> {
        self.regs.w32(IC_ENABLE, on as u32);
        // L'arrêt attend la fin de ce qui est en cours sur le bus.
        for _ in 0..200 {
            if (self.regs.r32(IC_ENABLE_STATUS) & 1 != 0) == on {
                return Ok(());
            }
            sleep_us(100);
        }
        Err(Fault::Stuck)
    }

    fn disable(&self) -> Result<(), Fault> {
        if self.set_enabled(false).is_ok() {
            return Ok(());
        }
        // Une transaction restée en suspens (le maître garde le bus, file
        // d'émission vide) ne s'arrête jamais d'elle-même. Relevé le 25/09 :
        // une relance d'Aiwos au milieu d'un échange avec le pavé. On
        // l'abandonne, puis on arrête.
        self.regs.w32(IC_ENABLE, ENABLE_ON);
        sleep_us(100);
        self.regs.w32(IC_ENABLE, ENABLE_ON | ENABLE_ABORT);
        for _ in 0..100 {
            if self.regs.r32(IC_ENABLE) & ENABLE_ABORT == 0 {
                break;
            }
            sleep_us(100);
        }
        let _ = self.regs.r32(IC_CLR_TX_ABRT);
        self.set_enabled(false)
    }

    /// Écrit `write`, puis lit `read.len()` octets (après un RESTART), en
    /// une transaction. Le contrôleur n'est actif que pendant celle-ci.
    pub fn transfer(&self, write: &[u8], read: &mut [u8]) -> Result<(), Fault> {
        self.set_enabled(true)?;
        let result = self.run(write, read);
        let stopped = self.disable();
        result.and(stopped)
    }

    fn run(&self, write: &[u8], read: &mut [u8]) -> Result<(), Fault> {
        let r = self.regs;
        let _ = r.r32(IC_CLR_INTR);
        let total = write.len() + read.len();
        // 9 bits par octet, et de la marge : 20 ms, plus 100 µs par octet.
        let deadline = rt::clock_ns() + 20_000_000 + total as u64 * 100_000;
        let (mut sent, mut received) = (0, 0);
        while sent < total || received < read.len() {
            // Des commandes, tant que la file d'émission a de la place et
            // que la file de réception pourra recevoir les octets demandés.
            while sent < total && r.r32(IC_STATUS) & STATUS_TFNF != 0 {
                let reads_asked = sent.saturating_sub(write.len());
                if sent >= write.len() && reads_asked - received >= self.rx_depth {
                    break;
                }
                let mut command = if sent < write.len() { write[sent] as u32 } else { CMD_READ };
                if sent == write.len() && sent > 0 {
                    command |= CMD_RESTART;
                }
                if sent + 1 == total {
                    command |= CMD_STOP;
                }
                r.w32(IC_DATA_CMD, command);
                sent += 1;
            }
            while received < read.len() && r.r32(IC_STATUS) & STATUS_RFNE != 0 {
                read[received] = r.r32(IC_DATA_CMD) as u8;
                received += 1;
            }
            if r.r32(IC_RAW_INTR_STAT) & INTR_TX_ABRT != 0 {
                let source = r.r32(IC_TX_ABRT_SOURCE);
                let _ = r.r32(IC_CLR_TX_ABRT);
                return Err(Fault::Abort(source));
            }
            if rt::clock_ns() > deadline {
                return Err(Fault::Timeout { sent, received });
            }
        }
        // Le STOP : la transaction est finie sur le bus.
        while r.r32(IC_RAW_INTR_STAT) & INTR_STOP_DET == 0 {
            if rt::clock_ns() > deadline {
                return Err(Fault::Timeout { sent, received });
            }
        }
        let _ = r.r32(IC_CLR_STOP_DET);
        Ok(())
    }
}

fn sleep_us(us: u64) {
    let end = rt::clock_ns() + us * 1000;
    while rt::clock_ns() < end {
        core::hint::spin_loop();
    }
}
