#!/usr/bin/env python3
"""Real mDNS/DTLS smoke with two isolated dummy-backend daemons, not desktop acceptance."""

import argparse
import json
import os
from pathlib import Path
import re
import secrets
import select
import socket
import subprocess
import time


ROOT = Path(__file__).resolve().parents[1]


def free_port(kind):
    with socket.socket(socket.AF_INET, kind) as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


class Endpoint:
    def __init__(self, name, binary, output):
        self.name, self.binary, self.output = name, binary, output
        self.owner = secrets.token_hex(32)
        self.config = output / (name + ".toml")
        self.ipc_port = free_port(socket.SOCK_STREAM)
        self.udp_port = free_port(socket.SOCK_DGRAM)
        self.process = self.sock = self.log = None
        self.verified = False
        self.buffer = b""
        self.events, self.latest = [], {}

    def start(self):
        self.config.write_text("mdns_discovery = true\n", encoding="utf-8")
        env = os.environ.copy()
        env["MOUSEHOP_OWNER_TOKEN"] = self.owner
        env["MOUSEHOP_DISCOVERY_NAME"] = "mousehop-smoke-" + secrets.token_hex(6) + "-" + self.name
        if os.name == "nt":
            env["MOUSEHOP_IPC_ADDR"] = f"127.0.0.1:{self.ipc_port}"
            address, family = ("127.0.0.1", self.ipc_port), socket.AF_INET
        else:
            address, family = str(self.output / (self.name + ".sock")), socket.AF_UNIX
            if len(os.fsencode(address)) >= 100:
                raise ValueError("Choose a shorter --output path for Unix sockets")
            env["MOUSEHOP_SOCKET_PATH"] = address
        self.log = open(self.output / (self.name + ".log"), "wb")
        self.process = subprocess.Popen([
            str(self.binary), "--config", str(self.config),
            "--cert-path", str(self.output / (self.name + ".pem")),
            "--port", str(self.udp_port), "--capture-backend", "dummy",
            "--emulation-backend", "dummy", "daemon",
        ], env=env, stdout=self.log, stderr=self.log,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"{self.name} exited: {self.process.returncode}")
            candidate = socket.socket(family, socket.SOCK_STREAM)
            candidate.settimeout(0.3)
            try:
                candidate.connect(address)
            except (ConnectionRefusedError, FileNotFoundError, socket.timeout):
                candidate.close()
                time.sleep(0.1)
                continue
            self.sock = candidate
            self.send("Sync")
            while time.monotonic() < deadline:
                self.pump()
                identity = self.latest.get("ServiceIdentity")
                if identity is not None:
                    if identity["pid"] != self.process.pid:
                        raise RuntimeError("IPC is owned by another process; no mutation sent")
                    if identity.get("ipc_version") != 8:
                        raise RuntimeError("Daemon IPC version does not support this frontend")
                    self.verified = True
                    return
                time.sleep(0.02)
            break
        raise TimeoutError(f"{self.name} did not identify its isolated IPC")

    def send(self, event):
        if event != "Sync" and not self.verified:
            raise RuntimeError("Refusing mutation before verifying the spawned daemon PID")
        self.sock.sendall(json.dumps(event).encode() + b"\n")

    def pump(self):
        if self.sock is None or not select.select([self.sock], [], [], 0)[0]:
            return
        chunk = self.sock.recv(1024 * 1024)
        if not chunk:
            raise RuntimeError(f"{self.name} IPC closed")
        self.buffer += chunk
        if len(self.buffer) > 4 * 1024 * 1024:
            raise RuntimeError("Oversized IPC frame")
        while b"\n" in self.buffer:
            line, self.buffer = self.buffer.split(b"\n", 1)
            event = json.loads(line)
            self.events.append(event)
            self.latest.update(event)

    def stop(self):
        if self.process is not None:
            if self.sock is not None and self.verified and self.process.poll() is None:
                try:
                    self.send({"Shutdown": self.owner})
                except OSError:
                    pass
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        if self.sock is not None:
            self.sock.close()
        if self.log is not None:
            self.log.close()
        (self.output / (self.name + "-events.json")).write_text(
            json.dumps(self.events, indent=2), encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug" / ("motionduo.exe" if os.name == "nt" else "motionduo"))
    parser.add_argument("--output", type=Path, default=ROOT / "target/mygo-qa" / ("lan-smoke-" + time.strftime("%Y%m%d-%H%M%S") + "-" + secrets.token_hex(2)))
    args = parser.parse_args()
    binary, output = args.binary.resolve(), args.output.resolve()
    if not binary.is_file():
        parser.error("Build the development backend first")
    output.mkdir(parents=True, exist_ok=False)
    endpoints = []
    report = {"isolation": "same-host dummy daemons; independent config, cert, IPC and UDP; PID checked before mutation"}

    def wait(predicate, seconds=30):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            for endpoint in endpoints:
                endpoint.pump()
            if predicate():
                return
            time.sleep(0.02)
        raise TimeoutError("Expected state not observed")

    try:
        for name in ("a", "b"):
            endpoint = Endpoint(name, binary, output)
            endpoints.append(endpoint)  # Cleanup also covers partial startup.
            endpoint.start()
        a, b = endpoints
        wait(lambda: all("PublicKeyFingerprint" in e.latest for e in endpoints))
        report["spawned_pid_verified"] = True
        email = "mygo-smoke-" + secrets.token_hex(6) + "@example.invalid"
        for endpoint in endpoints:
            endpoint.send({"SetDiscoveryEmail": email})
        wait(lambda: bool(a.latest.get("NearbyDevices")) and bool(b.latest.get("NearbyDevices")), 45)
        report["same_email_discovery"] = True
        marker = len(a.events)
        a.send({"SetDiscoveryEmail": email})
        wait(lambda: any("NearbyDevices" in e for e in a.events[marker:]))
        assert a.latest["NearbyDevices"], "Unchanged email cleared nearby devices"
        report["unchanged_email_preserves_discovery"] = True
        peer_b = next(d for d in a.latest["NearbyDevices"] if d["fingerprint"] == b.latest["PublicKeyFingerprint"])
        a.send({"ConnectDiscovered": [peer_b["id"], "right"]})
        wait(lambda: bool(b.latest.get("PairingRequests")), 45)
        request = next(r for r in b.latest["PairingRequests"] if r["id"].startswith(a.latest["PublicKeyFingerprint"] + "@"))
        assert not b.latest.get("AuthorizedUpdated"), "Discovery silently authorized control"
        assert request["name"] != "Nearby device", "Request missing the discovered device name"
        report["approval_is_required"] = True
        b.send({"RespondPairing": [request["id"], False]})
        wait(lambda: a.latest.get("State", [None,{},{}])[1].get("approval_paused", False))
        handle = a.latest["State"][0]
        wait(lambda: 'approval_paused = true' in a.config.read_text(encoding="utf-8"))
        old_request = request["id"]
        marker = len(b.events)
        deadline = time.monotonic() + 7
        while time.monotonic() < deadline:
            for e in endpoints: e.pump()
            time.sleep(.02)
        assert not b.latest["PairingRequests"], "Denied request resurfaced automatically"
        report["deny_persistently_pauses_without_reinvite"] = True
        a.send({"RetryApproval": handle})
        a.send({"RetryApproval": handle})
        wait(lambda: bool(b.latest["PairingRequests"]))
        request = b.latest["PairingRequests"][0]
        assert request["id"] != old_request, "Explicit retry reused nonce"
        b.send({"RespondPairing": [old_request, True]})
        for e in endpoints: e.pump()
        assert not b.latest.get("AuthorizedUpdated"), "Stale approval authorized identity"
        b.send({"RespondPairing": [request["id"], True]})
        wait(lambda: a.latest["PublicKeyFingerprint"] in b.latest.get("AuthorizedUpdated", {}))
        wait(lambda: any(kind in e and e[kind][2].get("alive") for e in a.events for kind in ("State", "Created")), 45)
        report["allow_binds_actual_certificate"] = True
        report["explicit_retry_fresh_nonce_and_duplicate_click_suppression"] = True
        b.send({"RemoveAuthorizedKey": a.latest["PublicKeyFingerprint"]})
        wait(lambda: a.latest.get("State", [None,{},{}])[1].get("approval_paused", False))
        wait(lambda: not a.latest["State"][2].get("alive",False))
        deadline = time.monotonic()+7
        while time.monotonic()<deadline:
            for e in endpoints: e.pump()
            time.sleep(.02)
        assert not b.latest["PairingRequests"], "Revoke automatically invited again"
        report["revoke_disconnects_and_silently_pauses"] = True
        a.send({"RetryApproval": handle})
        wait(lambda: bool(b.latest["PairingRequests"]))
        b.send({"RespondPairing": [b.latest["PairingRequests"][0]["id"],True]})
        wait(lambda: a.latest.get("State",[None,{},{}])[2].get("alive",False),45)
        report["background_retry_connects_after_allow"] = True
        assert not a.latest.get("AuthorizedUpdated"), "Approval silently granted reverse access"
        # A pending request expires independently of the frontend and stays paused.
        b.send({"RemoveAuthorizedKey": a.latest["PublicKeyFingerprint"]})
        wait(lambda:a.latest.get("State",[None,{},{}])[1].get("approval_paused",False))
        a.send({"RetryApproval":handle})
        wait(lambda:bool(b.latest["PairingRequests"]))
        expired_id=b.latest["PairingRequests"][0]["id"]
        wait(lambda:a.latest.get("State",[None,{},{}])[1].get("approval_paused",False),75)
        wait(lambda:not b.latest["PairingRequests"],10)
        b.send({"RespondPairing":[expired_id,True]})
        a.send("Sync")
        wait(lambda:bool(a.latest.get("Enumerate")))
        assert a.latest["PublicKeyFingerprint"] not in b.latest.get("AuthorizedUpdated",{}), "Expired approval granted access"
        report["timeout_pauses_and_late_approval_is_ignored"]=True
        a.send({"RetryApproval":handle})
        wait(lambda:bool(b.latest["PairingRequests"]))
        b.send({"RespondPairing":[b.latest["PairingRequests"][0]["id"],True]})
        wait(lambda:a.latest.get("State",[None,{},{}])[2].get("alive",False),45)
        assert "expected_fingerprint" in a.config.read_text(encoding="utf-8"), "Target identity was not persisted"
        report["outgoing_identity_pin_persisted"] = True

        # One drag commits both fields. Every emitted client State must already
        # contain the final pair, including capture deactivation/reactivation.
        a.send("Sync")
        wait(lambda: bool(a.latest.get("Enumerate")))
        original = next(row for row in a.latest["Enumerate"] if row[1].get("expected_fingerprint") == b.latest["PublicKeyFingerprint"])
        handle = original[0]
        before = (original[1]["pos"], original[1]["alignment"])
        marker = len(a.events)
        a.send({"UpdateScreenPlacement": [handle, "left", "bottom_left"]})
        wait(lambda: any(e.get("State", [None])[0] == handle and (e["State"][1]["pos"], e["State"][1]["alignment"]) == ("left", "bottom_left") for e in a.events[marker:]))
        changed = [e["State"] for e in a.events[marker:] if e.get("State", [None])[0] == handle]
        assert all((row[1]["pos"], row[1]["alignment"]) in (before, ("left", "bottom_left")) for row in changed), "Observed a half-applied drag placement"
        wait(lambda: 'alignment = "bottom_left"' in a.config.read_text(encoding="utf-8") and 'position = "left"' in a.config.read_text(encoding="utf-8"))
        report["atomic_drag_edge_and_corner_persisted"] = True

        # Settings are changed only on this PID-verified isolated endpoint.
        chord = ["KeyRightCtrl", "KeyRightShift", "KeyF12"]
        marker = len(a.events)
        a.send({"SetReleaseBind": chord})
        wait(lambda: any(e.get("ReleaseBind") == chord for e in a.events[marker:]))

        def saved_bind():
            match = re.search(r"(?m)^release_bind\s*=\s*\[([^\]]*)\]", a.config.read_text(encoding="utf-8"))
            return re.findall(r'"([^"]+)"', match[1]) if match else None

        wait(lambda: saved_bind() == chord)
        report["sided_release_shortcut_persisted"] = True
        for threshold in (1500, 0):
            marker = len(a.events)
            a.send({"SetReleaseThreshold": threshold})
            wait(lambda: any(e.get("ReleaseThreshold") == threshold for e in a.events[marker:]))
            wait(lambda: re.search(rf"(?m)^release_threshold_px\s*=\s*{threshold}\s*$", a.config.read_text(encoding="utf-8")) is not None)
        report["release_threshold_and_disable_persisted"] = True
        marker = len(a.events)
        a.send({"SetReleaseBind": []})
        default_chord = ["KeyLeftCtrl", "KeyLeftShift", "KeyLeftMeta", "KeyLeftAlt"]
        wait(lambda: any(e.get("ReleaseBind") == default_chord for e in a.events[marker:]))
        wait(lambda: saved_bind() is None)
        report["empty_release_shortcut_restores_default"] = True

        # Advanced endpoint editing must preserve identity, layout and inactive state.
        a.send({"Activate": [handle, False]})
        wait(lambda: a.latest.get("State", [None])[0] == handle and not a.latest["State"][2]["active"])
        marker = len(a.events)
        connection = {"hostname": None, "fix_ips": ["192.0.2.77"], "port": 4253, "cmd": None}
        a.send({"UpdateConnection": [handle, connection]})
        wait(lambda: any(e.get("State", [None])[0] == handle and all(e["State"][1][k] == v for k, v in connection.items()) for e in a.events[marker:]))
        edited = [e["State"] for e in a.events[marker:] if e.get("State", [None])[0] == handle]
        assert all(all(row[1][k] == v for k, v in connection.items()) for row in edited), "Observed a partial endpoint edit"
        assert all(not row[2]["active"] and row[1]["expected_fingerprint"] == b.latest["PublicKeyFingerprint"] and (row[1]["pos"], row[1]["alignment"]) == ("left", "bottom_left") for row in edited), "Endpoint edit changed identity, layout or activation"
        wait(lambda: '192.0.2.77' in a.config.read_text(encoding="utf-8") and 'port = 4253' in a.config.read_text(encoding="utf-8"))
        report["atomic_endpoint_edit_preserves_identity_and_layout"] = True

        b.send({"SetDiscoveryEmail": "different-group@example.invalid"})
        wait(lambda: b.latest.get("DiscoveryEmail") == "different-group@example.invalid" and not b.latest.get("NearbyDevices"))
        report["email_change_clears_discovery"] = True
        report["result"] = "pass"
    except Exception as error:
        report.update(result="fail", error=str(error))
        raise
    finally:
        for endpoint in reversed(endpoints):
            endpoint.stop()
        (output / "result.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps({"evidence": str(output), **report}))


if __name__ == "__main__":
    main()
