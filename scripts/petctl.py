#!/usr/bin/env python3
"""Control the local companion; useful for development and keyboard launchers."""
import argparse
import json
import os
import socket
parser = argparse.ArgumentParser()
parser.add_argument("control", choices=["quit", "readout", "tuck", "reveal", "reset_placement", "next_session", "reload_sprites", "status", "load_sprites"])
parser.add_argument("--socket", default=os.environ.get("OMP_PET_SOCKET", f"/tmp/omp-pet-{os.getuid()}/events.sock"))
parser.add_argument("--pack", help="Sprite folder for load_sprites")
args = parser.parse_args()
if args.control == "load_sprites" and not args.pack: parser.error("load_sprites requires --pack")
with socket.socket(socket.AF_UNIX) as client:
    client.connect(args.socket)
    command = {"load_sprites": {"path": os.path.abspath(args.pack)}} if args.control == "load_sprites" else args.control
    client.sendall((json.dumps({"control": command}) + "\n").encode())
    client.settimeout(35)
    reply = client.makefile().readline()
    if not reply: raise SystemExit("Companion closed without acknowledging the command")
    response = json.loads(reply)
    print(json.dumps(response, indent=2))
    if not response.get("ok"): raise SystemExit(1)
