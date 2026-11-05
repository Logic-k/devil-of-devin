# ACP smoke test: initialize + session/new against `devin acp` over stdio.
import json, os, subprocess, sys, threading, time

DEVIN = os.path.expandvars(r"%LOCALAPPDATA%\devin\cli\bin\devin.exe")
cwd = sys.argv[1] if len(sys.argv) > 1 else r"D:\Devil_of_Devin"

proc = subprocess.Popen(
    [DEVIN, "acp", "--model", "swe-2-max"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
    text=True, encoding="utf-8", errors="replace", cwd=cwd, bufsize=1,
)

responses = {}
notifications = []
lock = threading.Lock()

def reader():
    for line in proc.stdout:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except Exception:
            print("NON-JSON:", line[:200]); continue
        if "id" in msg and ("result" in msg or "error" in msg):
            with lock:
                responses[msg["id"]] = msg
        else:
            notifications.append(msg)
            print("NOTIFY:", json.dumps(msg)[:300])

threading.Thread(target=reader, daemon=True).start()

_id = [0]
def call(method, params=None, timeout=30):
    _id[0] += 1
    rid = _id[0]
    proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": rid, "method": method,
                                 "params": params or {}}) + "\n")
    proc.stdin.flush()
    t0 = time.time()
    while time.time() - t0 < timeout:
        with lock:
            if rid in responses:
                return responses.pop(rid)
        time.sleep(0.05)
    return {"error": "timeout"}

init = call("initialize", {
    "protocolVersion": 1,
    "clientCapabilities": {"fs": {"readTextFile": True, "writeTextFile": True},
                           "terminal": True},
    "clientInfo": {"name": "dod-smoke", "version": "0.1"},
})
print("INIT:", json.dumps(init)[:600])

sn = call("session/new", {"cwd": cwd, "mcpServers": []})
print("SESSION/NEW:", json.dumps(sn)[:600])

proc.kill()
