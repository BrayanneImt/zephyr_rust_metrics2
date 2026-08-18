//! identity.rs — Identite du noeud, resolue a l'execution via l'hote.
//!
//! device / type / os / transport ne sont PAS codes en dur : ils sont demandes
//! a l'hote au demarrage. node_id est un identifiant court (hachage FNV-1a du
//! nom d'equipement) qui remplacera l'identite complete dans une future trame
//! de session ; pour l'instant il accompagne chaque mesure.

use crate::config::*;
use crate::ffi::*;

static mut DEVICE_BUF: [u8; ID_CAP] = [0u8; ID_CAP];
static mut DEVICE_LEN: usize = 0;
static mut TYPE_BUF: [u8; ID_CAP] = [0u8; ID_CAP];
static mut TYPE_LEN: usize = 0;
static mut OS_BUF: [u8; ID_CAP] = [0u8; ID_CAP];
static mut OS_LEN: usize = 0;
static mut TRANSPORT_BUF: [u8; ID_CAP] = [0u8; ID_CAP];
static mut TRANSPORT_LEN: usize = 0;
static mut NODE_ID: u32 = 0;

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

/// Hachage FNV-1a 32 bits (stable, sans allocation) -> node_id.
fn fnv1a(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Interroge l'hote et memorise device/type/os/transport, puis derive node_id.
pub fn resolve() {
    resolve_one(host_get_device_name, (&raw mut DEVICE_BUF) as *mut u8, &raw mut DEVICE_LEN, FALLBACK_DEVICE);
    resolve_one(host_get_device_type, (&raw mut TYPE_BUF) as *mut u8, &raw mut TYPE_LEN, FALLBACK_TYPE);
    resolve_one(host_get_os_name, (&raw mut OS_BUF) as *mut u8, &raw mut OS_LEN, FALLBACK_OS);
    resolve_one(host_get_transport_name, (&raw mut TRANSPORT_BUF) as *mut u8, &raw mut TRANSPORT_LEN, FALLBACK_TRANSPORT);
    unsafe {
        core::ptr::write(&raw mut NODE_ID, fnv1a(device()));
    }
}

#[inline]
fn slice(buf: *const u8, len: *const usize) -> &'static [u8] {
    unsafe {
        let l = core::ptr::read(len);
        core::slice::from_raw_parts(buf, l)
    }
}

pub fn device() -> &'static [u8] { slice((&raw const DEVICE_BUF) as *const u8, &raw const DEVICE_LEN) }
pub fn dev_type() -> &'static [u8] { slice((&raw const TYPE_BUF) as *const u8, &raw const TYPE_LEN) }
pub fn os() -> &'static [u8] { slice((&raw const OS_BUF) as *const u8, &raw const OS_LEN) }
pub fn transport() -> &'static [u8] { slice((&raw const TRANSPORT_BUF) as *const u8, &raw const TRANSPORT_LEN) }
pub fn node_id() -> u32 { unsafe { core::ptr::read(&raw const NODE_ID) } }
