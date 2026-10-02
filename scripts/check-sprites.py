#!/usr/bin/env python3
"""Validate shipped crop coordinates against PNG IHDR dimensions."""
import json
import struct
from pathlib import Path
root = Path(__file__).resolve().parents[1] / 'assets/zorua'
manifest = json.loads((root / 'manifest.json').read_text())
count = 0
for state, frames in manifest.items():
    if not isinstance(frames, list): continue
    for frame in frames:
        data = (root / frame['file']).read_bytes()
        assert data[:8] == b'\x89PNG\r\n\x1a\n'
        width, height = struct.unpack('>II', data[16:24])
        assert frame['x'] + frame['width'] <= width, (state, frame)
        assert frame['y'] + frame['height'] <= height, (state, frame)
        count += 1
print(f'{count} animation crops fit their PNG sheets')
