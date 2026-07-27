#!/usr/bin/env python3
"""
upload.py — Téléverse un module .wasm vers l'équipement via UART.

Protocole (identique pour toutes les cartes) :
  1. 4 octets = taille du fichier (uint32 little-endian)
  2. le binaire .wasm, octet par octet

Le port série dépend de la carte :
  - Heltec WiFi LoRa 32 V3   : /dev/ttyUSB0
  - ESP32-C6-DevKitC-1       : /dev/ttyUSB0 (port "UART") — recommandé
  - NUCLEO-WB55RG            : /dev/ttyACM0 (USART1 via ST-LINK)

Usage :
  python3 upload.py --port /dev/ttyACM0 --file metrics.wasm
"""

import argparse
import os
import struct
import sys
import time

try:
    import serial
except ImportError:
    sys.exit("Dependance manquante : pip install pyserial")


def progress(done: int, total: int, width: int = 40) -> str:
    pct = done / total if total else 1.0
    fill = int(width * pct)
    return f"[{'=' * fill}{'-' * (width - fill)}] {done}/{total} ({pct*100:.1f}%)"


def main():
    p = argparse.ArgumentParser(description="Upload UART d'un module .wasm")
    p.add_argument("--port", default="/dev/ttyUSB0", help="port serie")
    p.add_argument("--baud", type=int, default=115200, help="debit (defaut 115200)")
    p.add_argument("--file", default="metrics.wasm", help="fichier .wasm")
    p.add_argument("--chunk", type=int, default=256, help="taille des blocs d'envoi")
    p.add_argument("--settle", type=float, default=0.5,
                   help="delai de stabilisation UART (s)")
    args = p.parse_args()

    if not os.path.isfile(args.file):
        print(f"ERREUR : fichier introuvable : {args.file}")
        print("Compiler d'abord : ./build_wasm.sh")
        sys.exit(1)

    with open(args.file, "rb") as f:
        data = f.read()
    size = len(data)

    print(f"Fichier    : {args.file}")
    print(f"Taille     : {size} octets")
    print(f"Port       : {args.port} @ {args.baud} baud")
    print()

    try:
        ser = serial.Serial(args.port, args.baud, timeout=5)
    except serial.SerialException as e:
        print(f"ERREUR port serie : {e}")
        print("Verifier le port et les droits (groupe dialout/uucp).")
        sys.exit(1)

    print(f"Stabilisation UART ({args.settle}s)...")
    time.sleep(args.settle)
    ser.reset_input_buffer()

    # 1. taille (4 octets, little-endian)
    print("Envoi de la taille...")
    ser.write(struct.pack("<I", size))
    ser.flush()
    time.sleep(0.1)

    # 2. binaire, par blocs
    print("Envoi du binaire WASM...")
    sent = 0
    while sent < size:
        chunk = data[sent:sent + args.chunk]
        ser.write(chunk)
        sent += len(chunk)
        print(f"\r  {progress(sent, size)}", end="", flush=True)
    ser.flush()
    print("\n\nUPLOAD DONE")
    print("Surveiller les logs pour l'execution du module.")
    ser.close()


if __name__ == "__main__":
    main()
