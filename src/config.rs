//! config.rs — Constantes et seuils du module.
//!
//! Toutes les valeurs qui pilotent la decision d'emission (approche PADRE) sont
//! regroupees ici. Chaque seuil est soit une limite physique, soit derive d'un
//! budget, soit calibre statistiquement (cf. APPROCHE_DECISION_FRUGALE).

// --- Infrastructure serveur (transmise a host_transport_connect ; ignoree en BLE) ---
pub static SERVER_IP: &[u8] = b"10.42.0.1";
pub const SERVER_PORT: u32 = 8080;
pub const NETWORK_TIMEOUT: u32 = 30;
pub const SOCKET_TIMEOUT: u32 = 5;

// --- Rythme de base ---
pub const T_C_SECS: u32 = 2;          // periode de base d'un cycle (s)
pub const T_MIN_S: u32 = 2;           // borne basse de l'intervalle
pub const T_MAX_S: u32 = 10;          // borne haute (mode survie)
pub const INTERVAL_MIN_S: u32 = 1;    // bornes de reconfiguration a chaud
pub const INTERVAL_MAX_S: u32 = 300;

// --- PADRE / E1 : estimation en ligne (EWMA + MAD), en decalages ---
pub const EWMA_SHIFT: u32 = 3;        // a -> alpha = 1/8
pub const MAD_SHIFT: u32 = 3;         // b -> beta  = 1/8

// --- PADRE / E2 : deadband adaptatif ---
pub const KAPPA: i32 = 3;             // delta ~ 2.4 sigma (faux decl. ~1-2 %)
pub const DELTA_MIN: i32 = 3;         // plancher = resolution de mesure
pub const DELTA_MAX: i32 = 10;        // plafond = precision de supervision

// --- PADRE / E4 : fraicheur (heartbeat) ---
pub const HEARTBEAT_MAX: u32 = 15;    // N_max : age max sans emission

// --- PADRE / E4 : preemption critique (seuils materiels/physiques) ---
pub const HEALTH_HEAP_MIN_BYTES: u32 = 4096;
pub const HEALTH_STACK_MAX_PCT: u32 = 85;
pub const HEALTH_MAX_CONSEC_FAIL: u32 = 3;
pub const BATTERY_CUTOFF_MV: u32 = 3300;    // coupure Li-ion -> alerte + survie
pub const CRIT_RATE_LIMIT_CYCLES: u32 = 30; // >= 1 alerte critique / 30 cycles

// --- PADRE : report sous forte charge (facteur de charge borne) ---
pub const LOAD_HIGH_CPU_PCT: u32 = 90;
pub const LOAD_DEFER_STEP: u32 = 2;
pub const LOAD_DEFER_MAX: u32 = 4;

// --- PADRE / E3 : regulation energetique (budget -> plancher k_min) ---
pub const ENERGY_RECALC_PERIOD: u32 = 30;   // M : recalcul amorti de k_min
pub const K_MAX: u32 = 255;                 // espacement max / sentinelle survie
pub const E_C_MC: u64 = 5;                  // energie d'un cycle sans emission (mC)
pub const E_S_MC: u64 = 20;                 // surcout d'une emission (mC)
pub const L_REM_SECS: u64 = 86_400;         // horizon de vie vise (24 h)
// Approximation lineaire de l'etat de charge a partir de battery_mv.
pub const BATTERY_FULL_MV: u32 = 4200;
pub const BATTERY_EMPTY_MV: u32 = 3300;
pub const BATTERY_CAPACITY_MC: u64 = 3_600_000; // 1000 mAh en mC

// --- Capacites des tampons statiques (aucune allocation dynamique) ---
pub const CBOR_CAP: usize = 512;
pub const TX_CAP: usize = 544;   // 2 (prefixe longueur) + CBOR
pub const RX_CAP: usize = 128;
pub const LOG_CAP: usize = 160;
pub const ID_CAP: usize = 32;

// --- Valeurs de repli d'identite ---
pub static FALLBACK_DEVICE: &[u8] = b"unknown_device";
pub static FALLBACK_TYPE: &[u8] = b"unknown_type";
pub static FALLBACK_OS: &[u8] = b"unknown_os";
pub static FALLBACK_TRANSPORT: &[u8] = b"unknown";
