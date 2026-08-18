//! ffi.rs — Contrat hote (host functions importees du firmware natif).
//!
//! Les signatures doivent etre STRICTEMENT identiques cote firmware
//! (table native_symbols de host_api.c). Toute divergence casse la portabilite
//! binaire du .wasm.
//!
//! NOUVEAU : host_metric_battery_mv (tension batterie, 0 si non mesuree).
//! host_metric_tcp_retransmissions est conservee et sert de source a la
//! metrique "coap_retransmissions" (retransmissions du lien).

#[link(wasm_import_module = "env")]
extern "C" {
    // ---- Affichage ----
    pub fn host_print(msg_ptr: *const u8, msg_len: u32);

    // ---- Transport abstrait (Wi-Fi/TCP ou BLE/NUS, transparent pour le WASM) ----
    pub fn host_transport_connect(
        ip_ptr: *const u8, ip_len: u32, port: u32, timeout_secs: u32,
    ) -> i32;
    pub fn host_transport_send(handle: i32, buf_ptr: *const u8, buf_len: u32) -> i32;
    pub fn host_transport_recv(handle: i32, buf_ptr: *mut u8, buf_len: u32) -> i32;
    pub fn host_transport_close(handle: i32);
    pub fn host_transport_wait_ready(timeout_secs: u32) -> i32;

    // ---- Temporisation ----
    pub fn host_sleep(secs: u32);

    // ---- Metriques brutes ----
    pub fn host_metric_cpu_usage() -> u32;
    pub fn host_metric_free_heap() -> u32;
    pub fn host_metric_uptime_ms() -> u32;
    pub fn host_metric_bytes_tx() -> u32;
    pub fn host_metric_bytes_rx() -> u32;
    pub fn host_metric_transport_errors() -> u32;
    pub fn host_metric_stack_usage_pct() -> u32;
    pub fn host_metric_signal_dbm() -> i32;
    pub fn host_metric_reset_count() -> u32;
    pub fn host_metric_active_threads() -> u32;
    pub fn host_metric_tcp_retransmissions() -> u32; // source de coap_retransmissions
    pub fn host_metric_battery_mv() -> u32;           // NOUVEAU (0 si non mesuree)

    // ---- Identite (resolue a l'execution) ----
    pub fn host_get_device_name(buf: *mut u8, cap: u32) -> i32;
    pub fn host_get_device_type(buf: *mut u8, cap: u32) -> i32;
    pub fn host_get_os_name(buf: *mut u8, cap: u32) -> i32;
    pub fn host_get_transport_name(buf: *mut u8, cap: u32) -> i32;
}
