//! cbor.rs — Serialisation CBOR (RFC 8949) et trame binaire.
//!
//! Remplace JSON + en-tete HTTP. Le WASM produit un ENREGISTREMENT CBOR (carte
//! a longueur indefinie, cles TEXTE identiques aux anciens noms JSON) puis le
//! prefixe d'une longueur sur 2 octets (big-endian) :
//!
//!     [ len_hi ][ len_lo ][ ... CBOR ... ]
//!
//! POURQUOI un prefixe de longueur et non un delimiteur '\n' : le CBOR est
//! BINAIRE et peut contenir l'octet 0x0A. Un delimiteur textuel casserait le
//! reassemblage cote passerelle/serveur. Le prefixe de longueur est sur pour
//! TCP comme pour BLE/NUS.
//!
//! Cote serveur : lire 2 octets de longueur, lire N octets, `cbor2.loads()` ->
//! dict identique a l'ancien JSON (memes cles) => logique serveur inchangee.
//!
//! Cles conservees identiques a l'ancien schema pour une migration a cout nul,
//! MOINS les champs elagues, PLUS les nouveaux (node_id, battery_mv,
//! energy_proxy, coap_retransmissions).

use crate::config::{CBOR_CAP, TX_CAP};
use crate::identity;
use crate::metrics::{derive_status, Metrics};

static mut CBOR_BUF: [u8; CBOR_CAP] = [0u8; CBOR_CAP];
static mut TX_BUF: [u8; TX_CAP] = [0u8; TX_CAP];

// --- Primitives d'encodage CBOR -------------------------------------------

#[inline]
fn put(buf: &mut [u8], i: usize, b: u8) -> usize {
    buf[i] = b;
    i + 1
}

/// Entier non signe (type majeur 0).
fn put_uint(buf: &mut [u8], mut i: usize, v: u32) -> usize {
    if v < 24 {
        i = put(buf, i, v as u8);
    } else if v < 0x100 {
        i = put(buf, i, 0x18);
        i = put(buf, i, v as u8);
    } else if v < 0x1_0000 {
        i = put(buf, i, 0x19);
        i = put(buf, i, (v >> 8) as u8);
        i = put(buf, i, v as u8);
    } else {
        i = put(buf, i, 0x1A);
        i = put(buf, i, (v >> 24) as u8);
        i = put(buf, i, (v >> 16) as u8);
        i = put(buf, i, (v >> 8) as u8);
        i = put(buf, i, v as u8);
    }
    i
}

/// Entier signe (type majeur 1 pour les negatifs, 0 sinon).
fn put_int(buf: &mut [u8], i: usize, n: i32) -> usize {
    if n >= 0 {
        put_uint(buf, i, n as u32)
    } else {
        // negatif : valeur codee m = -1 - n, base 0x20.
        let m = (-1 - n) as u32;
        let mut i = i;
        if m < 24 {
            i = put(buf, i, 0x20 + m as u8);
        } else if m < 0x100 {
            i = put(buf, i, 0x38);
            i = put(buf, i, m as u8);
        } else if m < 0x1_0000 {
            i = put(buf, i, 0x39);
            i = put(buf, i, (m >> 8) as u8);
            i = put(buf, i, m as u8);
        } else {
            i = put(buf, i, 0x3A);
            i = put(buf, i, (m >> 24) as u8);
            i = put(buf, i, (m >> 16) as u8);
            i = put(buf, i, (m >> 8) as u8);
            i = put(buf, i, m as u8);
        }
        i
    }
}

/// Chaine de texte UTF-8 (type majeur 3).
fn put_tstr(buf: &mut [u8], mut i: usize, s: &[u8]) -> usize {
    let len = s.len();
    if len < 24 {
        i = put(buf, i, 0x60 + len as u8);
    } else {
        i = put(buf, i, 0x78);
        i = put(buf, i, len as u8);
    }
    buf[i..i + len].copy_from_slice(s);
    i + len
}

#[inline]
fn kv_u32(buf: &mut [u8], i: usize, k: &[u8], v: u32) -> usize {
    let i = put_tstr(buf, i, k);
    put_uint(buf, i, v)
}
#[inline]
fn kv_i32(buf: &mut [u8], i: usize, k: &[u8], v: i32) -> usize {
    let i = put_tstr(buf, i, k);
    put_int(buf, i, v)
}
#[inline]
fn kv_str(buf: &mut [u8], i: usize, k: &[u8], v: &[u8]) -> usize {
    let i = put_tstr(buf, i, k);
    put_tstr(buf, i, v)
}

// --- Construction de l'enregistrement --------------------------------------

/// Construit l'enregistrement CBOR dans CBOR_BUF, renvoie sa longueur.
fn build_record(m: &Metrics, seq: u32) -> usize {
    unsafe {
        let b: &mut [u8; CBOR_CAP] = &mut *(&raw mut CBOR_BUF);
        let mut i = 0usize;

        // carte a longueur INDEFINIE (0xBF ... 0xFF) : pas de comptage a tenir.
        i = put(b, i, 0xBF);

        // Identite
        i = kv_u32(b, i, b"node_id", identity::node_id());
        i = kv_str(b, i, b"device", identity::device());
        i = kv_str(b, i, b"type", identity::dev_type());
        i = kv_str(b, i, b"os", identity::os());
        i = kv_str(b, i, b"transport", identity::transport());

        // Sequence / temps
        i = kv_u32(b, i, b"seq", seq);
        i = kv_u32(b, i, b"uptime_ms", m.uptime_ms);

        // Ressources de calcul
        i = kv_u32(b, i, b"cpu_usage_pct", m.cpu_usage_pct);
        i = kv_u32(b, i, b"free_heap_bytes", m.free_heap_bytes);
        i = kv_u32(b, i, b"stack_usage_pct", m.stack_usage_pct);
        i = kv_u32(b, i, b"active_threads", m.active_threads);

        // Energie
        i = kv_u32(b, i, b"battery_mv", m.battery_mv);
        i = kv_u32(b, i, b"energy_proxy", m.energy_proxy);
        i = kv_u32(b, i, b"sleep_ratio_pct", m.sleep_ratio_pct);

        // Communication / reseau
        i = kv_i32(b, i, b"signal_dbm", m.signal_dbm);
        i = kv_u32(b, i, b"last_send_duration_ms", m.last_send_duration_ms);
        i = kv_u32(b, i, b"bytes_tx", m.bytes_tx);
        i = kv_u32(b, i, b"bytes_rx", m.bytes_rx);
        i = kv_u32(b, i, b"transport_errors", m.transport_errors);
        i = kv_u32(b, i, b"coap_retransmissions", m.coap_retransmissions);

        // Fiabilite
        i = kv_u32(b, i, b"tx_success_rate", m.tx_success_rate);
        i = kv_u32(b, i, b"consecutive_failures", m.consecutive_failures);

        // Cycle de vie + sante
        i = kv_u32(b, i, b"reset_count", m.reset_count);
        i = kv_str(b, i, b"status", derive_status(m));

        // fin de carte indefinie
        i = put(b, i, 0xFF);
        i
    }
}

/// Construit la TRAME complete [len(2, BE)][CBOR] dans TX_BUF, renvoie sa
/// longueur totale. C'est ce tampon qui est passe a host_transport_send.
pub fn build_frame(m: &Metrics, seq: u32) -> usize {
    let clen = build_record(m, seq);
    unsafe {
        let cbor: &[u8; CBOR_CAP] = &*(&raw const CBOR_BUF);
        let tx: &mut [u8; TX_CAP] = &mut *(&raw mut TX_BUF);
        tx[0] = (clen >> 8) as u8;
        tx[1] = clen as u8;
        tx[2..2 + clen].copy_from_slice(&cbor[..clen]);
        clen + 2
    }
}

/// Pointeur sur TX_BUF (pour host_transport_send).
pub fn tx_ptr() -> *const u8 {
    (&raw const TX_BUF) as *const u8
}
