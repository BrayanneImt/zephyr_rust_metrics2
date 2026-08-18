//! metrics.rs — Jeu de metriques MIS A JOUR, collecte et grandeurs derivees.
//!
//! Changements vs version precedente :
//!   + node_id, battery_mv, energy_proxy, coap_retransmissions
//!   - idle_ratio_pct, timestamp_ms, net_errors, rssi_dbm, tcp_retransmissions
//!
//! Ce module heberge aussi les COMPTEURS RUNTIME (envois, veille, intervalle),
//! mis a jour par comms.rs et lus ici pour calculer les grandeurs derivees.
//! Tout calcul derive est fait ICI (le serveur ne recalcule rien).

use crate::config::T_C_SECS;
use crate::ffi::*;

// ---------------------------------------------------------------------------
// Compteurs runtime (etat partage). Mis a jour par comms.rs.
// ---------------------------------------------------------------------------
static mut SEND_TOTAL: u32 = 0;
static mut SEND_OK: u32 = 0;
static mut CONSEC_FAILURES: u32 = 0;
static mut LAST_SEND_MS: u32 = 0;
static mut SLEEP_MS_TOTAL: u32 = 0;
static mut CURRENT_INTERVAL_S: u32 = T_C_SECS;

pub fn note_attempt() { unsafe { let v = core::ptr::read(&raw const SEND_TOTAL); core::ptr::write(&raw mut SEND_TOTAL, v + 1); } }
pub fn note_ack(dur_ms: u32) {
    unsafe {
        let ok = core::ptr::read(&raw const SEND_OK);
        core::ptr::write(&raw mut SEND_OK, ok + 1);
        core::ptr::write(&raw mut CONSEC_FAILURES, 0);
        core::ptr::write(&raw mut LAST_SEND_MS, dur_ms);
    }
}
pub fn note_fail(dur_ms: u32) {
    unsafe {
        let cf = core::ptr::read(&raw const CONSEC_FAILURES);
        core::ptr::write(&raw mut CONSEC_FAILURES, cf + 1);
        core::ptr::write(&raw mut LAST_SEND_MS, dur_ms);
    }
}
pub fn add_sleep(ms: u32) { unsafe { let s = core::ptr::read(&raw const SLEEP_MS_TOTAL); core::ptr::write(&raw mut SLEEP_MS_TOTAL, s + ms); } }
pub fn current_interval() -> u32 { unsafe { core::ptr::read(&raw const CURRENT_INTERVAL_S) } }
pub fn set_interval(n: u32) { unsafe { core::ptr::write(&raw mut CURRENT_INTERVAL_S, n); } }

fn success_rate() -> u32 {
    unsafe {
        let total = core::ptr::read(&raw const SEND_TOTAL);
        let ok = core::ptr::read(&raw const SEND_OK);
        if total == 0 { 100 } else { (ok * 100) / total }
    }
}
fn consec_failures() -> u32 { unsafe { core::ptr::read(&raw const CONSEC_FAILURES) } }
fn last_send_ms() -> u32 { unsafe { core::ptr::read(&raw const LAST_SEND_MS) } }
fn sleep_total() -> u32 { unsafe { core::ptr::read(&raw const SLEEP_MS_TOTAL) } }

// ---------------------------------------------------------------------------
// Structure des metriques (jeu final).
// ---------------------------------------------------------------------------
pub struct Metrics {
    pub uptime_ms: u32,             // brut
    pub cpu_usage_pct: u32,         // brut
    pub free_heap_bytes: u32,       // brut
    pub stack_usage_pct: u32,       // brut
    pub active_threads: u32,        // brut
    pub battery_mv: u32,            // brut (0 si non mesuree)
    pub energy_proxy: u32,          // CALCULE ici
    pub last_send_duration_ms: u32, // runtime
    pub tx_success_rate: u32,       // CALCULE ici
    pub consecutive_failures: u32,  // runtime
    pub signal_dbm: i32,            // brut
    pub bytes_tx: u32,              // brut
    pub bytes_rx: u32,              // brut
    pub transport_errors: u32,      // brut
    pub coap_retransmissions: u32,  // brut (source : retransmissions du lien)
    pub sleep_ratio_pct: u32,       // CALCULE ici
    pub reset_count: u32,           // brut
}

/// Collecte l'ensemble des metriques et calcule les grandeurs derivees.
pub fn collect() -> Metrics {
    unsafe {
        let uptime = host_metric_uptime_ms();
        let bytes_tx = host_metric_bytes_tx();
        let bytes_rx = host_metric_bytes_rx();

        // energy_proxy : proxy PORTABLE de l'energie radio cumulee (octets sur
        // l'air). Independant d'un ADC batterie. Sert d'indicateur relatif.
        let energy_proxy = bytes_tx.saturating_add(bytes_rx);

        // sleep_ratio_pct : part du temps passee en veille.
        let slept = sleep_total();
        let sleep_pct = if uptime == 0 {
            0
        } else {
            let r = (slept as u64 * 100) / uptime as u64;
            if r > 100 { 100 } else { r as u32 }
        };

        Metrics {
            uptime_ms: uptime,
            cpu_usage_pct: host_metric_cpu_usage(),
            free_heap_bytes: host_metric_free_heap(),
            stack_usage_pct: host_metric_stack_usage_pct(),
            active_threads: host_metric_active_threads(),
            battery_mv: host_metric_battery_mv(),
            energy_proxy,
            last_send_duration_ms: last_send_ms(),
            tx_success_rate: success_rate(),
            consecutive_failures: consec_failures(),
            signal_dbm: host_metric_signal_dbm(),
            bytes_tx,
            bytes_rx,
            transport_errors: host_metric_transport_errors(),
            coap_retransmissions: host_metric_tcp_retransmissions(),
            sleep_ratio_pct: sleep_pct,
            reset_count: host_metric_reset_count(),
        }
    }
}

/// Champ `status` derive dans le WASM (aligne sur derive_status v2 du serveur).
/// Priorite : cpu > heap > link_down > net_degraded > stack.
pub fn derive_status(m: &Metrics) -> &'static [u8] {
    if m.cpu_usage_pct > 80 { return b"cpu_saturated"; }
    if m.free_heap_bytes < 4096 { return b"heap_low"; }
    if m.consecutive_failures >= 3 { return b"link_down"; }
    if m.tx_success_rate < 70 { return b"net_degraded"; }
    if m.stack_usage_pct > 85 { return b"stack_overflow_risk"; }
    b"ok"
}
