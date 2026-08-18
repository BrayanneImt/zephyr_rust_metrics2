//! padre.rs — Moteur de decision PADRE.
//!
//! Prediction Adaptative et Decision sous Regulation Energetique.
//! Remplace l'ancienne logique a seuils fixes. Chaque cycle :
//!   E1 estimation en ligne (EWMA + MAD, en decalages entiers) ;
//!   E2 deadband adaptatif   delta = clamp(dmin, dmax, kappa*d) ;
//!   E3 plancher energetique k_min(E_t) (recalcul amorti) ;
//!   E4 decision lexicographique : critique > heartbeat > (delta ET budget).
//!
//! Etat total : ~10 entiers. Aucune allocation, aucun flottant, aucune division
//! sur le chemin chaud (la seule division, k_min, est amortie sur M cycles).

use crate::config::*;
use crate::metrics::Metrics;

// --- Etat persistant du controleur ---
static mut INITED: bool = false;
static mut S: i32 = 0;          // EWMA du CPU
static mut D: i32 = 0;          // MAD du CPU
static mut R: i32 = 0;          // derniere valeur transmise (reference)
static mut AGE: u32 = 0;        // cycles depuis la derniere emission
static mut KMIN: u32 = 1;       // plancher d'espacement energetique courant
static mut MCTR: u32 = 0;       // compteur de recalcul de KMIN
static mut CRIT_TS: u32 = 0;    // horodatage derniere alerte critique
static mut HAS_CRIT: bool = false;
static mut LAST_ERR: u32 = 0;   // transport_errors au dernier envoi
static mut STREAK: u32 = 0;     // cycles consecutifs en forte charge

#[inline]
fn clamp_i32(lo: i32, hi: i32, v: i32) -> i32 {
    if v < lo { lo } else if v > hi { hi } else { v }
}

/// E3 — plancher d'espacement derive du budget energetique.
/// Renvoie k_min (cycles) ; K_MAX = mode survie. Si la batterie n'est pas
/// mesuree (battery_mv == 0, ex. carte alimentee USB), le gating est desactive.
fn k_min_from_budget(m: &Metrics) -> u32 {
    let mv = m.battery_mv;
    if mv == 0 {
        return 1; // pas de mesure batterie -> pas de regulation energetique
    }
    if mv <= BATTERY_EMPTY_MV {
        return K_MAX; // survie
    }
    // Etat de charge lineaire -> energie restante E_t (mC).
    let soc_num = (mv - BATTERY_EMPTY_MV) as u64;
    let soc_den = (BATTERY_FULL_MV - BATTERY_EMPTY_MV) as u64;
    let soc_num = if soc_num > soc_den { soc_den } else { soc_num };
    let e_t = BATTERY_CAPACITY_MC * soc_num / soc_den;

    let n = L_REM_SECS / T_C_SECS as u64;
    let n_ec = n * E_C_MC;
    if e_t <= n_ec {
        return K_MAX; // meme la veille ne tient pas l'horizon -> survie
    }
    let denom = e_t - n_ec;
    // k = ceil(n*e_s / denom)
    let k = (n * E_S_MC + denom - 1) / denom;
    let k = k as u32;
    if k < 1 { 1 } else if k > K_MAX { K_MAX } else { k }
}

/// E4 — alarme critique (sante ou batterie).
fn health_or_battery_critical(m: &Metrics) -> bool {
    if m.free_heap_bytes < HEALTH_HEAP_MIN_BYTES { return true; }
    if m.stack_usage_pct > HEALTH_STACK_MAX_PCT { return true; }
    if m.consecutive_failures >= HEALTH_MAX_CONSEC_FAIL { return true; }
    unsafe {
        if m.transport_errors > core::ptr::read(&raw const LAST_ERR) { return true; }
    }
    if m.battery_mv != 0 && m.battery_mv < BATTERY_CUTOFF_MV { return true; }
    false
}

/// Limitation de debit des alertes critiques : la premiere passe toujours,
/// ensuite au plus une par CRIT_RATE_LIMIT_CYCLES.
fn rate_limit_ok(t: u32) -> bool {
    unsafe {
        if !core::ptr::read(&raw const HAS_CRIT) {
            return true;
        }
        let last = core::ptr::read(&raw const CRIT_TS);
        t.wrapping_sub(last) >= CRIT_RATE_LIMIT_CYCLES
    }
}

/// Decision d'emission pour le cycle courant (t = numero de sequence).
pub fn decide(m: &Metrics, t: u32) -> bool {
    let x = m.cpu_usage_pct as i32;

    // Initialisation : premiere emission, pose la reference.
    unsafe {
        if !core::ptr::read(&raw const INITED) {
            core::ptr::write(&raw mut S, x);
            core::ptr::write(&raw mut D, 0);
            core::ptr::write(&raw mut R, x);
            core::ptr::write(&raw mut AGE, 0);
            core::ptr::write(&raw mut LAST_ERR, m.transport_errors);
            core::ptr::write(&raw mut INITED, true);
            return true;
        }
    }

    // E1 — estimation en ligne (decalages entiers).
    unsafe {
        let s = core::ptr::read(&raw const S);
        let s2 = s + ((x - s) >> EWMA_SHIFT);
        core::ptr::write(&raw mut S, s2);
        let d = core::ptr::read(&raw const D);
        let dev = (x - s2).abs();
        let d2 = d + ((dev - d) >> MAD_SHIFT);
        core::ptr::write(&raw mut D, d2);
    }

    // E2 — deadband adaptatif.
    let d = unsafe { core::ptr::read(&raw const D) };
    let delta = clamp_i32(DELTA_MIN, DELTA_MAX, KAPPA * d);

    // E3 — plancher energetique (recalcul amorti tous les M cycles).
    unsafe {
        let mctr = core::ptr::read(&raw const MCTR);
        if mctr == 0 {
            core::ptr::write(&raw mut KMIN, k_min_from_budget(m));
            core::ptr::write(&raw mut MCTR, ENERGY_RECALC_PERIOD);
        }
        let mctr = core::ptr::read(&raw const MCTR);
        core::ptr::write(&raw mut MCTR, mctr - 1);
    }

    // E4 — decision lexicographique.
    let r = unsafe { core::ptr::read(&raw const R) };
    let age = unsafe { core::ptr::read(&raw const AGE) } + 1; // cycles depuis emission
    let kmin = unsafe { core::ptr::read(&raw const KMIN) };

    let critique = health_or_battery_critical(m) && rate_limit_ok(t);
    let heartbeat = age >= HEARTBEAT_MAX;
    let info_budget = ((x - r).abs() >= delta) && (age >= kmin);

    let emit = critique || heartbeat || info_budget;

    // E5 — mise a jour.
    unsafe {
        if emit {
            core::ptr::write(&raw mut R, x);
            core::ptr::write(&raw mut AGE, 0);
            core::ptr::write(&raw mut LAST_ERR, m.transport_errors);
            if critique {
                core::ptr::write(&raw mut CRIT_TS, t);
                core::ptr::write(&raw mut HAS_CRIT, true);
            }
        } else {
            core::ptr::write(&raw mut AGE, age);
        }
    }
    emit
}

/// Report sous forte charge : delai (s) a AJOUTER a l'intervalle de base.
pub fn compute_defer(cpu: u32) -> u32 {
    unsafe {
        if cpu >= LOAD_HIGH_CPU_PCT {
            let s = core::ptr::read(&raw const STREAK) + 1;
            core::ptr::write(&raw mut STREAK, s);
            let defer = LOAD_DEFER_STEP * s;
            if defer > LOAD_DEFER_MAX { LOAD_DEFER_MAX } else { defer }
        } else {
            core::ptr::write(&raw mut STREAK, 0);
            0
        }
    }
}

/// Vrai si le controleur est en mode survie (budget epuise).
pub fn in_survival() -> bool {
    unsafe { core::ptr::read(&raw const KMIN) == K_MAX }
}
