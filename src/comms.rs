//! comms.rs — Emission de la trame et gestion de l'ACK.
//!
//! Envoie la trame binaire [len(2)][CBOR] via le transport abstrait, attend un
//! ACK, met a jour les compteurs (via metrics::note_*) et applique une
//! eventuelle reconfiguration d'intervalle transmise par le edge.
//!
//! ACK : texte "OK" ou "INTERVAL=<n>" (compatible avec le serveur actuel). Le
//! texte se parse en no_std sans allocation. La reconfiguration ne coute AUCUNE
//! requete supplementaire (elle reutilise la reponse deja echangee).

use crate::cbor;
use crate::config::{INTERVAL_MAX_S, INTERVAL_MIN_S, RX_CAP};
use crate::ffi::*;
use crate::log::{log, log_num};
use crate::metrics::{self, Metrics};

static mut RX_BUF: [u8; RX_CAP] = [0u8; RX_CAP];

/// Cherche "INTERVAL=" dans buf[..len], renvoie la position juste apres le '='.
fn find_interval_tag(buf: &[u8], len: usize) -> Option<usize> {
    const TAG: &[u8] = b"INTERVAL=";
    if len < TAG.len() {
        return None;
    }
    let mut i = 0;
    while i + TAG.len() <= len {
        let mut k = 0;
        while k < TAG.len() && buf[i + k] == TAG[k] {
            k += 1;
        }
        if k == TAG.len() {
            return Some(i + TAG.len());
        }
        i += 1;
    }
    None
}

/// Lit un entier decimal a partir de buf[start..len].
fn parse_u32_at(buf: &[u8], start: usize, len: usize) -> Option<u32> {
    let mut i = start;
    let mut val: u32 = 0;
    let mut seen = false;
    while i < len {
        let c = buf[i];
        if (b'0'..=b'9').contains(&c) {
            val = val.wrapping_mul(10).wrapping_add((c - b'0') as u32);
            seen = true;
            i += 1;
        } else {
            break;
        }
    }
    if seen { Some(val) } else { None }
}

/// Applique un ordre de reconfiguration d'intervalle present dans l'ACK.
fn apply_interval_reconfig(len: usize) {
    let rx: &[u8; RX_CAP] = unsafe { &*(&raw const RX_BUF) };
    let pos = match find_interval_tag(rx, len) {
        Some(p) => p,
        None => return,
    };
    let new_val = match parse_u32_at(rx, pos, len) {
        Some(v) => v,
        None => return,
    };
    if new_val < INTERVAL_MIN_S || new_val > INTERVAL_MAX_S {
        log_num(b"[CONFIG] intervalle hors bornes, ignore : ", new_val);
        return;
    }
    if new_val != metrics::current_interval() {
        metrics::set_interval(new_val);
        log_num(b"[CONFIG] intervalle de collecte mis a jour (s) : ", new_val);
    }
}

/// Emet la trame CBOR de la mesure et traite l'ACK.
pub fn send_metrics(handle: i32, seq: u32, m: &Metrics) {
    log_num(b"[METRICS] seq=", seq);
    log_num(b"  cpu=", m.cpu_usage_pct);
    log_num(b"  heap=", m.free_heap_bytes);
    log_num(b"  batt_mv=", m.battery_mv);
    log_num(b"  txok=", m.tx_success_rate);

    // Construit la trame [len][CBOR] et l'emet.
    let tx_len = cbor::build_frame(m, seq);

    let t0 = unsafe { host_metric_uptime_ms() };
    let sent = unsafe { host_transport_send(handle, cbor::tx_ptr(), tx_len as u32) };

    metrics::note_attempt();

    let mut got_ack = false;
    if sent > 0 {
        log(b"[METRICS] envoye, attente ACK...\n");
        let received = unsafe {
            host_transport_recv(handle, (&raw mut RX_BUF) as *mut u8, (RX_CAP - 1) as u32)
        };
        if received > 0 {
            got_ack = true;
            log(b"[METRICS] ACK recu\n");
            apply_interval_reconfig(received as usize);
        } else {
            log(b"[METRICS] pas d'ACK (sans gravite)\n");
        }
    } else {
        log(b"[METRICS] echec d'emission\n");
    }

    let t1 = unsafe { host_metric_uptime_ms() };
    let dur = t1.wrapping_sub(t0);
    if got_ack {
        metrics::note_ack(dur);
    } else {
        metrics::note_fail(dur);
    }
}
