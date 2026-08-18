// ============================================================================
// zephyr_rust_metrics2 / src/main.rs
//
// Point d'entree du module WASM (no_std). L'essentiel de la logique est
// deporte dans des modules pour la maintenance et la lisibilite :
//
//   config.rs    constantes et seuils (PADRE, energie, buffers)
//   ffi.rs       contrat hote (host functions)
//   log.rs       journalisation legere
//   identity.rs  identite du noeud + node_id
//   metrics.rs   jeu de metriques (mis a jour) + collecte + derivations
//   cbor.rs      serialisation CBOR + trame binaire a prefixe de longueur
//   padre.rs     moteur de decision PADRE (adaptatif, prouve, frugal)
//   comms.rs     emission + ACK + reconfiguration a chaud
//
// PROPRIETES CONSERVEES
//   - un seul .wasm pour toutes les cartes (identite resolue a l'execution) ;
//   - transport abstrait (le WASM ignore Wi-Fi vs BLE) ;
//   - tout le calcul derive est fait ICI, le serveur ne recalcule rien.
//
// CHANGEMENTS
//   - format JSON  -> CBOR (trame [len(2)][CBOR]) ;
//   - jeu de metriques mis a jour (cf. metrics.rs) ;
//   - decision d'emission remplacee par PADRE (cf. padre.rs).
//
// Licence : Apache-2.0
// ============================================================================

#![no_std]
#![no_main]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

mod cbor;
mod comms;
mod config;
mod ffi;
mod identity;
mod log;
mod metrics;
mod padre;

use config::*;
use ffi::*;
use log::{log, log_num, log_text};

#[no_mangle]
pub extern "C" fn main() {
    log(b"================================================\n");
    log(b" Collecteur WASM multi-transport (PADRE + CBOR)\n");
    log(b"================================================\n");

    // Identite (resolue via l'hote, jamais codee en dur).
    identity::resolve();
    log_text(b"Device    : ", identity::device());
    log_text(b"Type      : ", identity::dev_type());
    log_text(b"OS        : ", identity::os());
    log_text(b"Transport : ", identity::transport());
    log_num(b"node_id   : ", identity::node_id());
    log(b"------------------------------------------------\n");

    // Etablissement du transport (Wi-Fi ou BLE, transparent pour le WASM).
    log(b"Ouverture du transport...\n");
    let handle = unsafe {
        host_transport_connect(
            SERVER_IP.as_ptr(),
            SERVER_IP.len() as u32,
            SERVER_PORT,
            SOCKET_TIMEOUT,
        )
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

    // ------------------------------------------------------------------
    // BOUCLE PRINCIPALE
    //   1. collecte des metriques ;
    //   2. decision d'emission (PADRE : alarmes > heartbeat > delta+budget) ;
    //   3. emission CBOR si decidee ;
    //   4. report sous forte charge + mode survie ;
    //   5. veille de duree adaptee, cumul du temps de veille.
    // ------------------------------------------------------------------
    let mut seq: u32 = 0;
    loop {
        seq += 1;

        let m = metrics::collect();
        let cpu = m.cpu_usage_pct;

        let emit = padre::decide(&m, seq);
        if emit {
            comms::send_metrics(handle, seq, &m);
        } else {
            log(b"[FILTRE] mesure taire (PADRE)\n");
        }

        // Rythme : intervalle courant (reconfigurable) ou T_MAX en survie,
        // plus le report sous forte charge, borne [T_MIN, T_MAX].
        let defer = padre::compute_defer(cpu);
        if defer > 0 {
            log_num(b"[LOAD] forte charge, report (s) =", defer);
        }
        let base = if padre::in_survival() {
            log(b"[SURVIE] budget energetique epuise\n");
            T_MAX_S
        } else {
            metrics::current_interval()
        };
        let mut sleep_secs = base + defer;
        if sleep_secs < T_MIN_S { sleep_secs = T_MIN_S; }
        if sleep_secs > T_MAX_S { sleep_secs = T_MAX_S; }

        unsafe { host_sleep(sleep_secs); }
        metrics::add_sleep(sleep_secs * 1000);
    }
}
