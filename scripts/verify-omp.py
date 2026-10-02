#!/usr/bin/env python3
"""Verify /pet in real OMP without a model request or real credentials."""
import json
import os
from pathlib import Path
import select
import subprocess
import time
root = Path(__file__).resolve().parents[1]
state = root / 'work/omp-verification'
state.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, PI_CODING_AGENT_DIR=str(state),
           OPENAI_API_KEY='omp-pet-local-verification', OPENAI_BASE_URL='http://127.0.0.1:9')
args = ['omp', '--model', 'openai/gpt-5.2', '--mode', 'rpc', '--no-session', '--no-tools',
        '--no-lsp', '--no-title', '--no-skills', '--no-rules']
with (state / 'runtime.log').open('wb') as errors:
    process = subprocess.Popen(args, cwd=root, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors)
    buffer = b''
    registered = False
    result = None
    notification = None
    deadline = time.monotonic() + 30
    try:
        while time.monotonic() < deadline and (result is None or notification is None):
            ready, _, _ = select.select([process.stdout], [], [], 1)
            if not ready:
                if process.poll() is not None: break
                continue
            chunk = os.read(process.stdout.fileno(), 65536)
            if not chunk: break
            buffer += chunk
            while b'\n' in buffer:
                line, buffer = buffer.split(b'\n', 1)
                try: event = json.loads(line)
                except ValueError: continue
                if event.get('type') == 'available_commands_update' and not registered:
                    registered = any(c.get('name') == 'pet' and c.get('source') == 'extension' for c in event['commands'])
                    if not registered: raise RuntimeError('/pet was not registered')
                    process.stdin.write(b'{"id":"pet-check","type":"prompt","message":"/pet status"}\n')
                    process.stdin.flush()
                assert event.get('type') != 'agent_start', 'Slash command unexpectedly invoked an agent'
                if event.get('type') == 'extension_ui_request' and event.get('method') == 'notify':
                    try: status = json.loads(event.get('message', ''))
                    except ValueError: status = None
                    if isinstance(status, dict) and status.get('ok') and 'pid' in status: notification = status
                if event.get('id') == 'pet-check'  and event.get('type') == 'response':
                    result = event
                    assert result.get('success'), result
        assert registered and result is not None and notification is not None, 'OMP did not deliver native /pet status'
        print('Real OMP: /pet registered, native status received, no agent invocation')
        (state / 'result.json').write_text(json.dumps({"response": result, "native_status": notification}, indent=2) + '\n')
    finally:
        process.stdin.close()
        try: process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.terminate()
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
