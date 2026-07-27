# zephyr_rust_metrics — Module WASM de métriques (portable)

Module WebAssembly (`no_std`, Rust) qui collecte les métriques système, **les
calcule**, les sérialise en JSON et les transmet via un **transport abstrait**
— sans connaître ni l'OS, ni l'équipement, ni le moyen de communication
(Wi-Fi ou BLE).

Un seul binaire `.wasm` fonctionne sur toutes les cartes cibles.

## Compilation

```bash
./build_wasm.sh          # produit metrics.wasm
```

Nécessite Rust et la cible `wasm32-unknown-unknown` (ajoutée automatiquement).
Aucune dépendance externe.

## Téléversement

```bash
python3 upload.py --port /dev/ttyUSB0    # Heltec / ESP32-C6 (port UART)
python3 upload.py --port /dev/ttyACM0    # NUCLEO-WB55RG
```

## Ce que le module calcule lui-même

Conformément au principe « tout le calcul dans le WASM, le serveur ne fait
que collecter » :

- `idle_ratio_pct = 100 − cpu_usage_pct` ;
- `status` (dérivé de seuils sur cpu / heap / erreurs / stack).

L'hôte ne fournit que des valeurs brutes.

## Identité résolue à l'exécution

`device`, `type`, `os` et `transport` ne sont **pas** codés en dur : le module
les demande à l'hôte au démarrage (`host_get_*`). Le même binaire produit donc
des identités différentes selon la carte.

## Contrat hôte (20 fonctions)

Affichage (`host_print`), transport abstrait
(`host_transport_connect/wait_ready/send/recv/close`, `host_sleep`),
métriques brutes (`host_metric_*`, M1–M10) et identité (`host_get_*`). Les
signatures doivent être **strictement identiques** côté firmware — voir
`../zephyr_wamr_runtime/src/host_api.c`.

## Schéma JSON produit

16 champs : `device, type, os, transport, seq, uptime_ms, cpu_usage_pct,
free_heap_bytes, stack_usage_pct, idle_ratio_pct, bytes_tx, bytes_rx,
transport_errors, signal_dbm, reset_count, status`.

Voir `docs/CONCEPTION.md` §6 pour le détail.
