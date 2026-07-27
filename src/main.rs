// ============================================================================
// zephyr_rust_metrics / src/main.rs
//
// Collecteur de metriques systeme, compile en WebAssembly (no_std).
//
// PROPRIETES FONDAMENTALES
// ------------------------
// 1. AUCUNE ligne specifique a un OS, un equipement OU UN MOYEN DE
//    COMMUNICATION. Le module ne connait que des noms de fonctions importees
//    et des entiers 32 bits.
//
// 2. Le module ignore totalement s'il communique par Wi-Fi/TCP ou par
//    BLE/NUS. Il appelle des host functions de TRANSPORT ABSTRAIT :
//        host_transport_connect / _send / _recv / _close
//    C'est le firmware natif qui, a la compilation, branche ces fonctions
//    sur le transport reellement disponible sur la carte.
//
// 3. TOUT LE CALCUL EST FAIT ICI. Le serveur ne fait que collecter et
//    afficher. idle_ratio, derivation du statut, agregations, mise en forme
//    JSON : tout est calcule dans ce module WASM. Le serveur ne recalcule
//    jamais rien.
//
// 4. L'identite (nom d'equipement, type, OS, transport) n'est PAS codee en
//    dur : elle est demandee a l'hote a l'execution via des host functions
//    dediees. Un meme .wasm fonctionne donc sur Heltec (Wi-Fi), ESP32-C6
//    (Wi-Fi) et NUCLEO-WB55RG (BLE), sans recompilation.
//
// Licence : Apache-2.0
// ============================================================================

#![cfg_attr(target_arch = "wasm32", no_std)]
#![cfg_attr(target_arch = "wasm32", no_main)]

#[cfg(all(target_arch = "wasm32", not(test)))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// ============================================================================
// PARAMETRES D'INFRASTRUCTURE
//
// Ces valeurs decrivent le SERVEUR de collecte et le point d'acces, pas
// l'equipement ni l'OS ni le transport. Elles sont identiques pour tous les
// noeuds d'une campagne de mesure et restent donc legitimement dans le
// module.
//
// En transport BLE, SERVER_IP / SERVER_PORT sont ignores : la passerelle BLE
// cote PC se charge de relayer vers le serveur HTTP. Ils sont neanmoins
// transmis a host_transport_connect(), qui les ignore simplement en BLE.
// ============================================================================

static SERVER_IP: &[u8] = b"10.42.0.1";
const SERVER_PORT: u32 = 8080;

const SEND_INTERVAL: u32 = 5; // secondes entre deux collectes
const NETWORK_TIMEOUT: u32 = 30;
const SOCKET_TIMEOUT: u32 = 5;

// Valeurs de repli si l'hote ne repond pas.
static FALLBACK_DEVICE: &[u8] = b"unknown_device";
static FALLBACK_TYPE: &[u8] = b"unknown_type";
static FALLBACK_OS: &[u8] = b"unknown_os";
static FALLBACK_TRANSPORT: &[u8] = b"unknown";

// ============================================================================
// FONCTIONS HOTES — contrat avec le firmware natif
//
// Ces signatures doivent etre STRICTEMENT identiques dans TOUS les firmwares
// (Zephyr Wi-Fi, Zephyr BLE, NuttX...). Toute divergence de nom ou de type
// casse la portabilite du binaire.
// ============================================================================

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    // ---- Affichage --------------------------------------------------------
    fn host_print(msg_ptr: *const u8, msg_len: u32);

    // ---- TRANSPORT ABSTRAIT ----------------------------------------------
    //
    // Le module ne sait PAS quel transport est derriere. Le firmware natif
    // branche ces fonctions sur Wi-Fi/TCP ou BLE/NUS selon la carte.
    //
    // host_transport_connect :
    //   - en Wi-Fi : etablit la connexion Wi-Fi puis ouvre un socket TCP
    //     vers (ip, port), retourne un descripteur >= 0 ou -1.
    //   - en BLE   : demarre l'annonce (advertising) et ATTEND qu'une
    //     passerelle centrale se connecte et souscrive au service NUS ;
    //     ip/port sont ignores. Retourne un descripteur logique >= 0 ou -1.
    fn host_transport_connect(
        ip_ptr: *const u8,
        ip_len: u32,
        port: u32,
        timeout_secs: u32,
    ) -> i32;

    // Emet un bloc de donnees. Retourne le nombre d'octets emis, ou < 0.
    fn host_transport_send(handle: i32, buf_ptr: *const u8, buf_len: u32) -> i32;

    // Recoit un bloc (ACK applicatif). Retourne le nombre d'octets recus,
    // 0 si rien, < 0 en cas d'erreur. Non bloquant au-dela du timeout defini
    // par l'hote.
    fn host_transport_recv(handle: i32, buf_ptr: *mut u8, buf_len: u32) -> i32;

    fn host_transport_close(handle: i32);

    // Attend que le transport soit "pret" :
    //   - Wi-Fi : adresse IP obtenue (DHCP).
    //   - BLE   : un central s'est connecte et a active les notifications.
    // Retourne 0 si pret, -1 sur timeout.
    fn host_transport_wait_ready(timeout_secs: u32) -> i32;

    // ---- Temporisation ----------------------------------------------------
    fn host_sleep(secs: u32);

    // ---- Metriques systeme (valeurs BRUTES uniquement) --------------------
    //
    // L'hote ne fournit que des mesures instantanees brutes. TOUT calcul
    // derive (idle ratio, pourcentages composes, statut) est fait DANS ce
    // module, conformement au principe "tout le calcul dans le WASM".
    //
    //   M1  cpu_usage        pourcentage entier [0..100]
    //   M2  free_heap        octets libres du tas systeme
    //   M3  uptime_ms        millisecondes depuis le demarrage (32 bits)
    //   M4  bytes_tx         octets emis cumules (tout transport confondu)
    //   M5  bytes_rx         octets recus cumules
    //   M6  transport_errors nombre d'erreurs de transmission cumulees
    //   M7  stack_usage_pct  pourcentage entier [0..100]
    //   M9  signal_dbm       RSSI en dBm (Wi-Fi ou BLE), signe negatif, 0 si
    //                        indisponible
    //   M10 reset_count      nombre de demarrages
    fn host_metric_cpu_usage() -> u32;
    fn host_metric_free_heap() -> u32;
    fn host_metric_uptime_ms() -> u32;
    fn host_metric_bytes_tx() -> u32;
    fn host_metric_bytes_rx() -> u32;
    fn host_metric_transport_errors() -> u32;
    fn host_metric_stack_usage_pct() -> u32;
    fn host_metric_signal_dbm() -> i32;
    fn host_metric_reset_count() -> u32;

    // ---- Identite (resolue a l'execution) ---------------------------------
    // Chaque fonction ecrit dans un tampon fourni et retourne le nombre
    // d'octets ecrits, ou -1 si le tampon est trop petit.
    fn host_get_device_name(buf: *mut u8, cap: u32) -> i32;
    fn host_get_device_type(buf: *mut u8, cap: u32) -> i32;
    fn host_get_os_name(buf: *mut u8, cap: u32) -> i32;
    fn host_get_transport_name(buf: *mut u8, cap: u32) -> i32;
}

// ============================================================================
// BUFFERS STATIQUES — aucune allocation dynamique
// ============================================================================

const TX_CAP: usize = 512;
const RX_CAP: usize = 256;
const JSON_CAP: usize = 448;
const LOG_CAP: usize = 160;
const ID_CAP: usize = 32;

static mut TX_BUF: [u8; TX_CAP] = [0u8; TX_CAP];
static mut RX_BUF: [u8; RX_CAP] = [0u8; RX_CAP];
static mut JSON_BUF: [u8; JSON_CAP] = [0u8; JSON_CAP];
static mut LOG_BUF: [u8; LOG_CAP] = [0u8; LOG_CAP];

// Identite, resolue une fois au demarrage.
static mut DEVICE_BUF: [u8; ID_CAP] = [0u8; ID_CAP];
static mut DEVICE_LEN: usize = 0;
static mut TYPE_BUF: [u8; ID_CAP] = [0u8; ID_CAP];
static mut TYPE_LEN: usize = 0;
static mut OS_BUF: [u8; ID_CAP] = [0u8; ID_CAP];
static mut OS_LEN: usize = 0;
static mut TRANSPORT_BUF: [u8; ID_CAP] = [0u8; ID_CAP];
static mut TRANSPORT_LEN: usize = 0;

// ============================================================================
// UTILITAIRES no_std
// ============================================================================

#[inline]
fn write_bytes(dst: &mut [u8], offset: usize, src: &[u8]) -> usize {
    let end = offset + src.len();
    dst[offset..end].copy_from_slice(src);
    end
}

#[inline]
fn write_u32(dst: &mut [u8], offset: usize, mut n: u32) -> usize {
    if n == 0 {
        dst[offset] = b'0';
        return offset + 1;
    }
    let mut tmp = [0u8; 10];
    let mut len = 0usize;
    while n > 0 {
        tmp[len] = b'0' + (n % 10) as u8;
        n /= 10;
        len += 1;
    }
    for i in 0..len {
        dst[offset + i] = tmp[len - 1 - i];
    }
    offset + len
}

#[inline]
fn write_i32(dst: &mut [u8], offset: usize, n: i32) -> usize {
    if n < 0 {
        dst[offset] = b'-';
        write_u32(dst, offset + 1, (-(n as i64)) as u32)
    } else {
        write_u32(dst, offset, n as u32)
    }
}

// ============================================================================
// JOURNALISATION
// ============================================================================

#[cfg(target_arch = "wasm32")]
#[inline]
fn log(msg: &[u8]) {
    unsafe {
        host_print(msg.as_ptr(), msg.len() as u32);
    }
}

#[cfg(target_arch = "wasm32")]
fn log_num(prefix: &[u8], n: u32) {
    let msg_len = unsafe {
        let buf: &mut [u8; LOG_CAP] = &mut *(&raw mut LOG_BUF);
        let plen = prefix.len().min(LOG_CAP - 16);
        buf[..plen].copy_from_slice(&prefix[..plen]);
        let mut i = plen;
        i = write_u32(buf, i, n);
        buf[i] = b'\n';
        i + 1
    };
    unsafe {
        host_print((&raw const LOG_BUF) as *const u8, msg_len as u32);
    }
}

#[cfg(target_arch = "wasm32")]
fn log_text(prefix: &[u8], text: &[u8]) {
    let msg_len = unsafe {
        let buf: &mut [u8; LOG_CAP] = &mut *(&raw mut LOG_BUF);
        let plen = prefix.len().min(LOG_CAP - 2);
        buf[..plen].copy_from_slice(&prefix[..plen]);
        let tlen = text.len().min(LOG_CAP - plen - 1);
        buf[plen..plen + tlen].copy_from_slice(&text[..tlen]);
        buf[plen + tlen] = b'\n';
        plen + tlen + 1
    };
    unsafe {
        host_print((&raw const LOG_BUF) as *const u8, msg_len as u32);
    }
}

// ============================================================================
// IDENTITE — resolue a l'execution via l'hote
// ============================================================================

/// Interroge l'hote et remplit `buf`/`len`, avec repli sur `fallback`.
#[cfg(target_arch = "wasm32")]
fn resolve_one(
    getter: unsafe extern "C" fn(*mut u8, u32) -> i32,
    buf: *mut u8,
    len_out: *mut usize,
    fallback: &[u8],
) {
    unsafe {
        let n = getter(buf, ID_CAP as u32);
        if n > 0 && (n as usize) <= ID_CAP {
            *len_out = n as usize;
        } else {
            let slice = core::slice::from_raw_parts_mut(buf, ID_CAP);
            let l = fallback.len().min(ID_CAP);
            slice[..l].copy_from_slice(&fallback[..l]);
            *len_out = l;
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn resolve_identity() {
    resolve_one(
        host_get_device_name,
        (&raw mut DEVICE_BUF) as *mut u8,
        &raw mut DEVICE_LEN,
        FALLBACK_DEVICE,
    );
    resolve_one(
        host_get_device_type,
        (&raw mut TYPE_BUF) as *mut u8,
        &raw mut TYPE_LEN,
        FALLBACK_TYPE,
    );
    resolve_one(
        host_get_os_name,
        (&raw mut OS_BUF) as *mut u8,
        &raw mut OS_LEN,
        FALLBACK_OS,
    );
    resolve_one(
        host_get_transport_name,
        (&raw mut TRANSPORT_BUF) as *mut u8,
        &raw mut TRANSPORT_LEN,
        FALLBACK_TRANSPORT,
    );
}

#[cfg(target_arch = "wasm32")]
#[inline]
fn id_slice(buf: *const u8, len: *const usize) -> &'static [u8] {
    unsafe {
        let l = core::ptr::read(len);
        core::slice::from_raw_parts(buf, l)
    }
}

// ============================================================================
// COLLECTE ET CALCUL DES METRIQUES
//
// L'hote ne fournit que des valeurs BRUTES. Les grandeurs derivees sont
// calculees ICI :
//   - M8 idle_ratio_pct = 100 - cpu_usage_pct   (calcule dans le WASM)
//   - status            = derive des seuils      (calcule dans le WASM)
// ============================================================================

struct Metrics {
    cpu_usage_pct: u32,     // M1  (brut, hote)
    free_heap_bytes: u32,   // M2  (brut, hote)
    uptime_ms: u32,         // M3  (brut, hote)
    bytes_tx: u32,          // M4  (brut, hote)
    bytes_rx: u32,          // M5  (brut, hote)
    transport_errors: u32,  // M6  (brut, hote)
    stack_usage_pct: u32,   // M7  (brut, hote)
    idle_ratio_pct: u32,    // M8  (CALCULE ici)
    signal_dbm: i32,        // M9  (brut, hote)
    reset_count: u32,       // M10 (brut, hote)
}

#[cfg(target_arch = "wasm32")]
fn collect_metrics() -> Metrics {
    unsafe {
        let cpu = host_metric_cpu_usage();

        // M8 est CALCULE ici, pas fourni par l'hote : c'est une operation
        // derivee, elle doit vivre dans le WASM.
        let idle = if cpu >= 100 { 0 } else { 100 - cpu };

        Metrics {
            cpu_usage_pct: cpu,
            free_heap_bytes: host_metric_free_heap(),
            uptime_ms: host_metric_uptime_ms(),
            bytes_tx: host_metric_bytes_tx(),
            bytes_rx: host_metric_bytes_rx(),
            transport_errors: host_metric_transport_errors(),
            stack_usage_pct: host_metric_stack_usage_pct(),
            idle_ratio_pct: idle,
            signal_dbm: host_metric_signal_dbm(),
            reset_count: host_metric_reset_count(),
        }
    }
}

/// Derive le champ `status` — CALCULE DANS LE WASM.
/// Priorite : cpu_saturated > heap_low > transport_degraded > stack_risk.
fn derive_status(m: &Metrics) -> &'static [u8] {
    if m.cpu_usage_pct > 80 {
        return b"cpu_saturated";
    }
    if m.free_heap_bytes < 4096 {
        return b"heap_low";
    }
    if m.transport_errors > 3 {
        return b"transport_degraded";
    }
    if m.stack_usage_pct > 85 {
        return b"stack_overflow_risk";
    }
    b"ok"
}

// ============================================================================
// SERIALISATION JSON
//
// Schema commun a TOUS les transports. Le champ "transport" indique par quel
// moyen le noeud communique ("wifi" ou "ble"), ce qui permet au serveur de
// distinguer les sources sans rien recalculer.
// ============================================================================

#[cfg(target_arch = "wasm32")]
fn build_json(m: &Metrics, seq: u32) -> usize {
    unsafe {
        let b: &mut [u8; JSON_CAP] = &mut *(&raw mut JSON_BUF);
        let mut i = 0usize;

        i = write_bytes(b, i, b"{");

        i = write_bytes(b, i, b"\"device\":\"");
        i = write_bytes(b, i, id_slice((&raw const DEVICE_BUF) as *const u8, &raw const DEVICE_LEN));
        i = write_bytes(b, i, b"\",");

        i = write_bytes(b, i, b"\"type\":\"");
        i = write_bytes(b, i, id_slice((&raw const TYPE_BUF) as *const u8, &raw const TYPE_LEN));
        i = write_bytes(b, i, b"\",");

        i = write_bytes(b, i, b"\"os\":\"");
        i = write_bytes(b, i, id_slice((&raw const OS_BUF) as *const u8, &raw const OS_LEN));
        i = write_bytes(b, i, b"\",");

        i = write_bytes(b, i, b"\"transport\":\"");
        i = write_bytes(b, i, id_slice((&raw const TRANSPORT_BUF) as *const u8, &raw const TRANSPORT_LEN));
        i = write_bytes(b, i, b"\",");

        i = write_bytes(b, i, b"\"seq\":");
        i = write_u32(b, i, seq);
        i = write_bytes(b, i, b",");

        i = write_bytes(b, i, b"\"uptime_ms\":");
        i = write_u32(b, i, m.uptime_ms);
        i = write_bytes(b, i, b",");

        i = write_bytes(b, i, b"\"cpu_usage_pct\":");
        i = write_u32(b, i, m.cpu_usage_pct);
        i = write_bytes(b, i, b",");

        i = write_bytes(b, i, b"\"free_heap_bytes\":");
        i = write_u32(b, i, m.free_heap_bytes);
        i = write_bytes(b, i, b",");

        i = write_bytes(b, i, b"\"stack_usage_pct\":");
        i = write_u32(b, i, m.stack_usage_pct);
        i = write_bytes(b, i, b",");

        i = write_bytes(b, i, b"\"idle_ratio_pct\":");
        i = write_u32(b, i, m.idle_ratio_pct);
        i = write_bytes(b, i, b",");

        i = write_bytes(b, i, b"\"bytes_tx\":");
        i = write_u32(b, i, m.bytes_tx);
        i = write_bytes(b, i, b",");

        i = write_bytes(b, i, b"\"bytes_rx\":");
        i = write_u32(b, i, m.bytes_rx);
        i = write_bytes(b, i, b",");

        i = write_bytes(b, i, b"\"transport_errors\":");
        i = write_u32(b, i, m.transport_errors);
        i = write_bytes(b, i, b",");

        i = write_bytes(b, i, b"\"signal_dbm\":");
        i = write_i32(b, i, m.signal_dbm);
        i = write_bytes(b, i, b",");

        i = write_bytes(b, i, b"\"reset_count\":");
        i = write_u32(b, i, m.reset_count);
        i = write_bytes(b, i, b",");

        i = write_bytes(b, i, b"\"status\":\"");
        i = write_bytes(b, i, derive_status(m));
        i = write_bytes(b, i, b"\"}");

        i
    }
}

// ============================================================================
// ENVOI D'UNE MESURE
//
// En Wi-Fi, on encapsule dans une requete HTTP POST (le serveur recoit du
// HTTP directement). En BLE, la passerelle attend une seule ligne JSON
// terminee par '\n' ; c'est elle qui construira la requete HTTP cote PC.
//
// Pour garder UN SEUL binaire, on emet TOUJOURS le JSON prefixe d'un en-tete
// HTTP : la passerelle BLE sait extraire le corps JSON, et le serveur Wi-Fi
// recoit du HTTP standard. Le module n'a donc pas a connaitre le transport.
// ============================================================================

#[cfg(target_arch = "wasm32")]
fn send_metrics(handle: i32, seq: u32) {
    log_num(b"[METRICS] seq=", seq);

    let m = collect_metrics();

    log_num(b"  cpu=", m.cpu_usage_pct);
    log_num(b"  idle=", m.idle_ratio_pct);
    log_num(b"  heap_free=", m.free_heap_bytes);
    log_num(b"  stack=", m.stack_usage_pct);
    log_num(b"  tx_err=", m.transport_errors);

    let json_len = build_json(&m, seq);

    let tx_len = unsafe {
        let tx: &mut [u8; TX_CAP] = &mut *(&raw mut TX_BUF);
        let mut j = 0;
        j = write_bytes(tx, j, b"POST /metrics HTTP/1.0\r\nHost: ");
        j = write_bytes(tx, j, SERVER_IP);
        j = write_bytes(tx, j, b"\r\nContent-Type: application/json\r\nContent-Length: ");
        j = write_u32(tx, j, json_len as u32);
        j = write_bytes(tx, j, b"\r\nConnection: close\r\n\r\n");
        let json_slice: &[u8; JSON_CAP] = &*(&raw const JSON_BUF);
        j = write_bytes(tx, j, &json_slice[..json_len]);
        // Saut de ligne final : delimiteur de trame pour la passerelle BLE.
        tx[j] = b'\n';
        j + 1
    };

    let sent = unsafe {
        host_transport_send(handle, (&raw const TX_BUF) as *const u8, tx_len as u32)
    };

    if sent > 0 {
        log(b"[METRICS] envoye, attente ACK...\n");
        let received = unsafe {
            host_transport_recv(handle, (&raw mut RX_BUF) as *mut u8, (RX_CAP - 1) as u32)
        };
        if received > 0 {
            log(b"[METRICS] ACK recu\n");
        } else {
            log(b"[METRICS] pas d'ACK (sans gravite)\n");
        }
    } else {
        log(b"[METRICS] echec d'emission\n");
    }
}

// ============================================================================
// POINT D'ENTREE WASM
// ============================================================================

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn main() {
    log(b"================================================\n");
    log(b" Collecteur de metriques WASM (multi-transport)\n");
    log(b"================================================\n");

    resolve_identity();
    log_text(b"Device    : ", id_slice((&raw const DEVICE_BUF) as *const u8, &raw const DEVICE_LEN));
    log_text(b"Type      : ", id_slice((&raw const TYPE_BUF) as *const u8, &raw const TYPE_LEN));
    log_text(b"OS        : ", id_slice((&raw const OS_BUF) as *const u8, &raw const OS_LEN));
    log_text(b"Transport : ", id_slice((&raw const TRANSPORT_BUF) as *const u8, &raw const TRANSPORT_LEN));
    log(b"------------------------------------------------\n");

    // Etablissement du transport (Wi-Fi ou BLE, le module ne sait pas lequel).
    log(b"Ouverture du transport...\n");
    let handle = unsafe {
        host_transport_connect(SERVER_IP.as_ptr(), SERVER_IP.len() as u32, SERVER_PORT, SOCKET_TIMEOUT)
    };
    if handle < 0 {
        log(b"[ERR] host_transport_connect a echoue\n");
        return;
    }

    log(b"Attente que le transport soit pret...\n");
    let ready = unsafe { host_transport_wait_ready(NETWORK_TIMEOUT) };
    if ready != 0 {
        log(b"[ERR] transport non pret (timeout)\n");
        unsafe { host_transport_close(handle); }
        return;
    }
    log(b"Transport pret\n");

    let mut seq: u32 = 0;
    loop {
        seq += 1;
        send_metrics(handle, seq);
        unsafe { host_sleep(SEND_INTERVAL); }
    }
}

// ============================================================================
// STUB hote (x86_64) — pour `cargo check` / rust-analyzer
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("Cible prevue : wasm32-unknown-unknown. Compiler avec build_wasm.sh");
}
