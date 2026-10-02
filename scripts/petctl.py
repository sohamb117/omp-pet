#!/usr/bin/env python3
"""Control the local companion; useful for development and keyboard launchers."""
import argparse
import json
import os
import socket
parser = argparse.ArgumentParser()
parser.add_argument("control", choices=["quit", "readout", "tuck", "reveal", "reset_placement", "next_session", "reload_sprites"])
parser.add_argument("--socket", default=os.environ.get("OMP_PET_SOCKET", f"/tmp/omp-pet-{os.getuid()}/events.sock"))
args = parser.parse_args()
with socket.socket(socket.AF_UNIX) as client:
    client.connect(args.socket)
    client.sendall((json.dumps({"control": args.control}) + "\n").encode())
