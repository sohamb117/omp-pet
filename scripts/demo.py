#!/usr/bin/env python3
"""Feed a native pet realistic states without an OMP installation or model call."""
import argparse
import json
import os
import socket
import time

parser = argparse.ArgumentParser()
parser.add_argument("--seconds", type=float, default=45)
parser.add_argument("--socket", default=os.environ.get("OMP_PET_SOCKET", f"/tmp/omp-pet-{os.getuid()}/events.sock"))
args = parser.parse_args()
started = time.monotonic()
with socket.socket(socket.AF_UNIX) as client:
    client.connect(args.socket)
    seq = 0
    while time.monotonic() - started < args.seconds:
        elapsed = time.monotonic() - started
        activity, tool = ("working", "Running cargo check") if elapsed < args.seconds * .7 else ("idle", None)
        seq += 1
        snapshot = dict(version=1, session_id="demo-script", seq=seq, project="omp-pet",
            task="Build a native desktop companion", activity=activity, tool=tool,
            context=dict(tokens=42000, window=100000, percent=42))
        client.sendall((json.dumps(snapshot) + "\n").encode())
        time.sleep(1)
