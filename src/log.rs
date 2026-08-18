//! log.rs — Journalisation legere (no_std) via host_print.
//!
//! Ecrit dans un tampon statique LOG_BUF. Format decimal ASCII maison (pas de
//! dependance, pas d'allocation).

use crate::config::LOG_CAP;
use crate::ffi::host_print;

static mut LOG_BUF: [u8; LOG_CAP] = [0u8; LOG_CAP];

/// Ecrit un entier decimal `n` dans `dst[offset..]`, renvoie le nouvel offset.
pub fn write_u32(dst: &mut [u8], offset: usize, mut n: u32) -> usize {
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

/// Journalise un message brut.
pub fn log(msg: &[u8]) {
    unsafe {
        host_print(msg.as_ptr(), msg.len() as u32);
    }
}

/// Journalise "prefixe<n>\n".
pub fn log_num(prefix: &[u8], n: u32) {
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

/// Journalise "prefixe<texte>\n".
pub fn log_text(prefix: &[u8], text: &[u8]) {
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
