//! La bibliothèque des programmes de Holarch : les appels système sous une
//! forme sûre et lisible, le point d'entrée, et de quoi écrire du texte
//! sans allocation.
//!
//! Un programme reçoit ses poignées de départ sous les numéros 1, 2, 3…,
//! dans l'ordre choisi par celui qui l'a lancé.

#![no_std]

#[cfg(feature = "tas")]
pub mod tas;

use core::arch::asm;
use core::fmt;

use holarch_abi::sys;
pub use holarch_abi::{
    BootResources, CallArgs, DeviceOpened, DeviceResources, Error, TouchEvent, TouchPoint, FOREVER, KeyEvent, MAX_WAIT_ITEMS, MouseEvent, PciDevice, Received, WaitItem,
    boot_handles, device_kind, device_level, key, net, rights, signals, socket, tile_level, touchpad_protocol,
};

/// Une poignée : un numéro valable dans ce seul programme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct Handle(pub u32);

pub type Result<T> = core::result::Result<T, Error>;

// --- Appels système bruts -----------------------------------------------------

#[inline(always)]
unsafe fn syscall(number: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> i64 {
    let result: i64;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number as i64 => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            in("r9") a5,
            // `syscall` écrase rcx (adresse de retour) et r11 (drapeaux).
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        )
    };
    result
}

fn check(code: i64) -> Result<u64> {
    if code < 0 { Err(Error::from_code(code).unwrap_or(Error::BadSyscall)) } else { Ok(code as u64) }
}

// --- Appels système ---------------------------------------------------------

pub fn journal_write(journal: Handle, text: &str) -> Result<()> {
    check(unsafe { syscall(sys::JOURNAL_WRITE, journal.0 as u64, text.as_ptr() as u64, text.len() as u64, 0, 0, 0) })
        .map(|_| ())
}

pub fn handle_close(handle: Handle) -> Result<()> {
    check(unsafe { syscall(sys::HANDLE_CLOSE, handle.0 as u64, 0, 0, 0, 0, 0) }).map(|_| ())
}

/// Copie d'une poignée, avec au plus les droits `rights`.
pub fn handle_duplicate(handle: Handle, rights: u32) -> Result<Handle> {
    check(unsafe { syscall(sys::HANDLE_DUPLICATE, handle.0 as u64, rights as u64, 0, 0, 0, 0) })
        .map(|h| Handle(h as u32))
}

pub fn channel_create() -> Result<(Handle, Handle)> {
    let mut out = [0u32; 2];
    check(unsafe { syscall(sys::CHANNEL_CREATE, out.as_mut_ptr() as u64, 0, 0, 0, 0, 0) })?;
    Ok((Handle(out[0]), Handle(out[1])))
}

/// Envoie `data` et les poignées `handles`, qui quittent ce programme.
pub fn channel_write(channel: Handle, data: &[u8], handles: &[Handle]) -> Result<()> {
    channel_reply(channel, 0, data, handles)
}

/// Répond à une demande faite par `channel_call`, en rendant son numéro
/// d'appel (`Received::call`) : sans lui, l'appelant ne reconnaîtrait pas sa
/// réponse.
pub fn channel_reply(channel: Handle, call: u32, data: &[u8], handles: &[Handle]) -> Result<()> {
    check(unsafe {
        syscall(
            sys::CHANNEL_WRITE,
            channel.0 as u64,
            data.as_ptr() as u64,
            data.len() as u64,
            handles.as_ptr() as u64,
            handles.len() as u64,
            call as u64,
        )
    })
    .map(|_| ())
}

/// Lit un message sans attendre (`Error::ShouldWait` s'il n'y en a pas).
pub fn channel_read(channel: Handle, data: &mut [u8], handles: &mut [Handle]) -> Result<Received> {
    let mut received = Received::default();
    check(unsafe {
        syscall(
            sys::CHANNEL_READ,
            channel.0 as u64,
            data.as_mut_ptr() as u64,
            data.len() as u64,
            handles.as_mut_ptr() as u64,
            handles.len() as u64,
            &mut received as *mut Received as u64,
        )
    })?;
    Ok(received)
}

/// Envoie une requête et attend la réponse. Renvoie la taille de la réponse.
pub fn channel_call(channel: Handle, request: &[u8], reply: &mut [u8], deadline_ns: u64) -> Result<usize> {
    channel_call_with(channel, request, &[], reply, deadline_ns)
}

/// Comme `channel_call`, avec des poignées jointes à la requête, qui
/// quittent ce programme.
pub fn channel_call_with(
    channel: Handle,
    request: &[u8],
    handles: &[Handle],
    reply: &mut [u8],
    deadline_ns: u64,
) -> Result<usize> {
    let args = CallArgs {
        request: request.as_ptr() as u64,
        request_len: request.len() as u64,
        handles: handles.as_ptr() as u64,
        handle_count: handles.len() as u64,
        reply: reply.as_mut_ptr() as u64,
        reply_cap: reply.len() as u64,
    };
    check(unsafe { syscall(sys::CHANNEL_CALL, channel.0 as u64, &args as *const CallArgs as u64, deadline_ns, 0, 0, 0) })
        .map(|n| n as usize)
}

/// Attend qu'un des `signals` apparaisse sur l'objet. Renvoie les signaux
/// observés.
pub fn wait_one(handle: Handle, signals: u32, deadline_ns: u64) -> Result<u32> {
    check(unsafe { syscall(sys::WAIT_ONE, handle.0 as u64, signals as u64, deadline_ns, 0, 0, 0) }).map(|s| s as u32)
}

/// Attend qu'un des objets présente un de ses signaux. Remplit `observed`
/// pour chacun et renvoie l'indice du premier prêt.
pub fn wait_many(items: &mut [WaitItem], deadline_ns: u64) -> Result<usize> {
    check(unsafe { syscall(sys::WAIT_MANY, items.as_mut_ptr() as u64, items.len() as u64, deadline_ns, 0, 0, 0) })
        .map(|i| i as usize)
}

/// Un élément d'attente multiple.
pub fn wait_item(handle: Handle, signals: u32) -> WaitItem {
    WaitItem { handle: handle.0, signals, observed: 0 }
}

pub fn memory_create(size: u64) -> Result<Handle> {
    check(unsafe { syscall(sys::MEMORY_CREATE, size, 0, 0, 0, 0, 0) }).map(|h| Handle(h as u32))
}

/// Projette un objet mémoire ; renvoie son adresse.
pub fn memory_map(memory: Handle) -> Result<*mut u8> {
    check(unsafe { syscall(sys::MEMORY_MAP, memory.0 as u64, 0, 0, 0, 0, 0) }).map(|a| a as *mut u8)
}

/// Retire une projection faite par `memory_map` ; l'objet est rendu quand
/// plus rien ne le désigne (ni poignée, ni projection).
pub fn memory_unmap(address: *mut u8) -> Result<()> {
    check(unsafe { syscall(sys::MEMORY_UNMAP, address as u64, 0, 0, 0, 0, 0) }).map(|_| ())
}

/// Lance un programme depuis son image ; `handles` deviennent ses
/// poignées 1, 2, 3…
pub fn process_create(image: Handle, name: &str, handles: &[Handle]) -> Result<Handle> {
    check(unsafe {
        syscall(
            sys::PROCESS_CREATE,
            image.0 as u64,
            name.as_ptr() as u64,
            name.len() as u64,
            handles.as_ptr() as u64,
            handles.len() as u64,
            0,
        )
    })
    .map(|h| Handle(h as u32))
}

/// Arrête un programme (IA5) : il faut le droit d'écriture sur sa poignée
/// (celle que rend [`process_create`]). Demande asynchrone : attendre
/// `TERMINATED` pour savoir que la tâche a quitté le processeur, que sa
/// mémoire a été rendue et que ses poignées ont été fermées.
pub fn process_terminate(process: Handle) -> Result<()> {
    check(unsafe { syscall(sys::PROCESS_TERMINATE, process.0 as u64, 0, 0, 0, 0, 0) }).map(|_| ())
}

/// Retire au lancement tout droit absent du masque, avant la première
/// instruction de l'enfant. Le transfert exige encore TRANSFER chez le parent.
pub fn process_create_restricted(image: Handle, name: &str, handles: &[Handle], mask: u32) -> Result<Handle> {
    check(unsafe {
        syscall(sys::PROCESS_CREATE_RESTRICTED, image.0 as u64, name.as_ptr() as u64,
            name.len() as u64, handles.as_ptr() as u64, handles.len() as u64, mask as u64)
    }).map(|h| Handle(h as u32))
}

/// Ouvre les BAR sélectionnées d'un appareil, sans DMA. Fermer toutes
/// les poignées et retirer toutes leurs projections pour rendre l'accès.
pub fn device_open(device: Handle, level: u32, bars: u32) -> Result<DeviceOpened> {
    let mut out = DeviceOpened::default();
    check(unsafe {
        syscall(sys::DEVICE_OPEN, device.0 as u64, level as u64, bars as u64,
            &mut out as *mut DeviceOpened as u64, 0, 0)
    })?;
    Ok(out)
}

/// D2 (docs/16) : prépare un appareil pour un pilote du système scellé
/// (racine seulement : la poignée système).
pub fn device_prepare(system: Handle, device: Handle, bars: u32) -> Result<holarch_abi::DevicePrepared> {
    let mut out = holarch_abi::DevicePrepared::default();
    check(unsafe {
        syscall(sys::DEVICE_PREPARE, system.0 as u64, device.0 as u64, bars as u64,
            &mut out as *mut holarch_abi::DevicePrepared as u64, 0, 0)
    })?;
    Ok(out)
}

/// D3 (docs/16) : une fenêtre de registres à adresse fixe (racine seulement).
pub fn mmio_window(system: Handle, phys: u64, size: u64) -> Result<Handle> {
    check(unsafe { syscall(sys::MMIO_WINDOW, system.0 as u64, phys, size, 0, 0, 0) }).map(|h| Handle(h as u32))
}

/// D3 (docs/16) : une interruption sur une ligne de l'IO-APIC (racine
/// seulement) ; `mode` : `holarch_abi::line_mode`.
pub fn interrupt_line(system: Handle, gsi: u32, mode: u32) -> Result<Handle> {
    check(unsafe { syscall(sys::INTERRUPT_LINE, system.0 as u64, gsi as u64, mode as u64, 0, 0, 0) }).map(|h| Handle(h as u32))
}

/// IA7b : un domaine DMA confiné par l'IOMMU pour l'appareil des registres
/// `registers` (ouverts en écriture).
pub fn device_dma(registers: Handle) -> Result<Handle> {
    check(unsafe { syscall(sys::DEVICE_DMA, registers.0 as u64, 0, 0, 0, 0, 0) }).map(|h| Handle(h as u32))
}

pub fn process_exit(code: i64) -> ! {
    unsafe { syscall(sys::PROCESS_EXIT, code as u64, 0, 0, 0, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}

/// Nanosecondes depuis le démarrage.
pub fn clock_ns() -> u64 {
    unsafe { syscall(sys::CLOCK_NS, 0, 0, 0, 0, 0, 0) as u64 }
}

/// L'heure UTC, en secondes depuis 1970 (0 si l'horloge est illisible).
pub fn clock_unix() -> u64 {
    unsafe { syscall(sys::CLOCK_UNIX, 0, 0, 0, 0, 0, 0) as u64 }
}

pub fn sleep_ms(ms: u64) {
    unsafe { syscall(sys::SLEEP_NS, ms * 1_000_000, 0, 0, 0, 0, 0) };
}

/// Lit un octet sur un port matériel (pilotes).
pub fn ioport_read(ports: Handle, port: u16) -> Result<u8> {
    check(unsafe { syscall(sys::IOPORT_READ, ports.0 as u64, port as u64, 0, 0, 0, 0) }).map(|v| v as u8)
}

/// Écrit un octet sur un port matériel (pilotes).
pub fn ioport_write(ports: Handle, port: u16, value: u8) -> Result<()> {
    check(unsafe { syscall(sys::IOPORT_WRITE, ports.0 as u64, port as u64, value as u64, 0, 0, 0) }).map(|_| ())
}

/// Efface le signal d'une interruption, avant de traiter le périphérique.
pub fn interrupt_ack(interrupt: Handle) -> Result<()> {
    check(unsafe { syscall(sys::INTERRUPT_ACK, interrupt.0 as u64, 0, 0, 0, 0, 0) }).map(|_| ())
}

/// Mémoire DMA (pilotes) : pages contiguës qu'un périphérique peut lire et
/// écrire directement.
pub fn memory_create_dma(dma: Handle, size: u64) -> Result<Handle> {
    check(unsafe { syscall(sys::MEMORY_CREATE_DMA, dma.0 as u64, size, 0, 0, 0, 0) }).map(|h| Handle(h as u32))
}

/// G6 : les adresses physiques des pages d'une mémoire partagée (depuis la
/// page `first`, autant que `out` en tient), pour un pilote qui a le droit
/// DMA ; rend le nombre total de pages.
pub fn memory_frames(dma: Handle, memory: Handle, first: u64, out: &mut [u64]) -> Result<u64> {
    check(unsafe {
        syscall(sys::MEMORY_FRAMES, dma.0 as u64, memory.0 as u64, first, out.as_mut_ptr() as u64, out.len() as u64, 0)
    })
}

/// Adresse physique d'une mémoire DMA, à donner au périphérique.
pub fn memory_physical(memory: Handle) -> Result<u64> {
    check(unsafe { syscall(sys::MEMORY_PHYSICAL, memory.0 as u64, 0, 0, 0, 0, 0) })
}

/// Lit l'état du système (sujet 0 : mesures et programmes ; 1 : journal ;
/// 2 : données du noyau précédent ; 3 : version du noyau).
pub fn system_read(system: Handle, what: u64, buf: &mut [u8]) -> Result<usize> {
    check(unsafe { syscall(sys::SYSTEM_READ, system.0 as u64, what, buf.as_mut_ptr() as u64, buf.len() as u64, 0, 0) })
        .map(|n| n as usize)
}

/// Comme [`system_read`], avec un texte (IA4 : la fiche d'un appareil
/// ACPI, sujet 14).
pub fn system_query(system: Handle, what: u64, query: &str, buf: &mut [u8]) -> Result<usize> {
    check(unsafe {
        syscall(
            sys::SYSTEM_QUERY,
            system.0 as u64,
            what,
            query.as_ptr() as u64,
            query.len() as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
    })
    .map(|n| n as usize)
}

/// Relance Holarch sur le noyau du paquet de mise à jour signé contenu dans
/// `package` (`len` octets), en transmettant `handover` au noyau suivant.
/// Un réglage du noyau (poignée du système, droit d'écriture).
/// Redémarre (0) ou éteint (1) la machine ; ne revient qu'en cas d'échec.
pub fn system_power(system: Handle, action: u64) -> Result<()> {
    check(unsafe { syscall(sys::SYSTEM_POWER, system.0 as u64, action, 0, 0, 0, 0) }).map(|_| ())
}

pub fn system_set(system: Handle, key: u64, value: u64) -> Result<()> {
    check(unsafe { syscall(sys::SYSTEM_SET, system.0 as u64, key, value, 0, 0, 0) }).map(|_| ())
}

/// Remplit une tuile de l'écran : niveau (`tile_level`), jauge (0 à 100),
/// valeur principale et détail (48 caractères chacun au plus).
pub fn tile_set(tile: Handle, level: u8, gauge: Option<u8>, main: &str, detail: &str) -> Result<()> {
    let mut data = [0u8; 256];
    data[0] = level;
    data[1] = gauge.map_or(255, |g| g.min(100));
    let mut n = 2;
    for part in [main.as_bytes(), b"\n", detail.as_bytes()] {
        let take = part.len().min(data.len() - n);
        data[n..n + take].copy_from_slice(&part[..take]);
        n += take;
    }
    // Ne pas couper un caractère en deux.
    while core::str::from_utf8(&data[2..n]).is_err() {
        n -= 1;
    }
    check(unsafe { syscall(sys::TILE_SET, tile.0 as u64, data.as_ptr() as u64, n as u64, 0, 0, 0) }).map(|_| ())
}

/// Le noyau vérifie la signature et la version. Ne revient qu'en cas
/// d'erreur.
pub fn system_relaunch(system: Handle, package: Handle, len: usize, handover: &[u8]) -> Result<()> {
    check(unsafe {
        syscall(
            sys::SYSTEM_RELAUNCH,
            system.0 as u64,
            package.0 as u64,
            len as u64,
            handover.as_ptr() as u64,
            handover.len() as u64,
            0,
        )
    })
    .map(|_| ())
}

/// Échéance dans `ms` millisecondes, pour les attentes.
pub fn deadline_in_ms(ms: u64) -> u64 {
    clock_ns() + ms * 1_000_000
}

// --- Aléa ------------------------------------------------------------------------

/// Remplit `out` de nombres aléatoires tirés du générateur matériel du
/// processeur (instruction RDRAND, qu'un programme peut exécuter lui-même ;
/// présente chez Intel depuis 2012, dont le Pentium Silver N6000). Faux si
/// le processeur n'en a pas, ou s'il ne répond pas après 10 essais pour un
/// mot, comme le conseille Intel.
pub fn random(out: &mut [u8]) -> bool {
    // CPUID, feuille 1 : bit 30 d'ECX.
    #[allow(unused_unsafe)]
    let features = unsafe { core::arch::x86_64::__cpuid(1) };
    if features.ecx & (1 << 30) == 0 {
        return false;
    }
    for chunk in out.chunks_mut(8) {
        let mut word = None;
        for _ in 0..10 {
            let value: u64;
            let ok: u8;
            unsafe {
                asm!("rdrand {v}", "setc {ok}", v = out(reg) value, ok = out(reg_byte) ok, options(nomem, nostack));
            }
            if ok != 0 {
                word = Some(value);
                break;
            }
        }
        let Some(word) = word else { return false };
        chunk.copy_from_slice(&word.to_le_bytes()[..chunk.len()]);
    }
    true
}

// --- Texte sans allocation -----------------------------------------------------

/// Un texte de taille fixe, rempli avec `write!`.
pub struct Text<const N: usize = 160> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> Text<N> {
    pub const fn new() -> Self {
        Self { buf: [0; N], len: 0 }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> Default for Text<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> fmt::Write for Text<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            let n = c.len_utf8();
            if self.len + n > N {
                return Err(fmt::Error);
            }
            c.encode_utf8(&mut self.buf[self.len..]);
            self.len += n;
        }
        Ok(())
    }
}

/// `log!(journal, "format", args…)` : écrit une ligne dans le journal.
#[macro_export]
macro_rules! log {
    ($journal:expr, $($arg:tt)*) => {{
        let mut text = $crate::Text::<160>::new();
        let _ = core::fmt::Write::write_fmt(&mut text, format_args!($($arg)*));
        let _ = $crate::journal_write($journal, text.as_str());
    }};
}

// --- Point d'entrée --------------------------------------------------------------

/// Déclare la fonction principale du programme.
#[macro_export]
macro_rules! entry {
    ($main:path) => {
        #[unsafe(no_mangle)]
        pub extern "sysv64" fn _start() -> ! {
            $main();
            $crate::process_exit(0)
        }
    };
}

/// Fin sur une panique : le noyau écrit le message au journal et garde un
/// rapport de panne (« pannes »).
pub fn process_abort(message: &str) -> ! {
    unsafe { syscall(sys::PROCESS_ABORT, message.as_ptr() as u64, message.len() as u64, 0, 0, 0, 0) };
    // Un noyau antérieur à IA3 ne connaît pas cet appel.
    process_exit(-1)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Où (fichier:ligne:colonne), puis le message, tronqué s'il est long.
    let mut text = Text::<400>::new();
    if let Some(at) = info.location() {
        let _ = fmt::Write::write_fmt(&mut text, format_args!("{}:{}:{} : ", at.file(), at.line(), at.column()));
    }
    let _ = fmt::Write::write_fmt(&mut text, format_args!("{}", info.message()));
    process_abort(text.as_str())
}
