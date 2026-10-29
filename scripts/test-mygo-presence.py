#!/usr/bin/env python3
"""Staggered mDNS identity changes and live access revocation, using isolated daemons."""

import argparse
import importlib.util
import json
from pathlib import Path
import secrets
import time


spec = importlib.util.spec_from_file_location("lan_smoke", Path(__file__).with_name("test-mygo-lan.py"))
lan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lan)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    endpoints, checks = [], []
    report = {"isolation": "dummy backends; private config, certificates, ports and PID-verified IPC", "checks": checks}

    def pump():
        for endpoint in endpoints:
            endpoint.pump()

    def wait(predicate, seconds=10):
        start = time.monotonic()
        while time.monotonic() - start < seconds:
            pump()
            if predicate():
                return round(time.monotonic() - start, 3)
            time.sleep(0.01)
        raise TimeoutError("Expected state not observed")

    def hold(seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            pump()
            time.sleep(0.01)

    def sees(a, b):
        return any(d["fingerprint"] == b.latest["PublicKeyFingerprint"] for d in a.latest.get("NearbyDevices", []))

    def check(name, predicate, seconds=5):
        try:
            elapsed = wait(predicate, seconds)
            checks.append({"name": name, "pass": True, "seconds": elapsed})
        except TimeoutError:
            checks.append({"name": name, "pass": False, "timeout_seconds": seconds})

    try:
        for name in ("a", "b"):
            endpoint = lan.Endpoint(name, args.binary.resolve(), output)
            endpoints.append(endpoint)
            endpoint.start()
        a, b = endpoints
        wait(lambda: all("PublicKeyFingerprint" in e.latest for e in endpoints))
        email = "presence-" + secrets.token_hex(8) + "@example.invalid"
        a.send({"SetDiscoveryEmail": email})
        wait(lambda: a.latest.get("DiscoveryEmail") == email)
        hold(4)
        b.send({"SetDiscoveryEmail": email})
        check("first_configured_peer_sees_later_peer", lambda: sees(a, b))
        check("later_peer_sees_first_peer", lambda: sees(b, a))
        marker = len(a.events)
        hold(4)
        checks.append({"name": "discovery_remains_present", "pass": sees(a, b) and all(
            any(d["fingerprint"] == b.latest["PublicKeyFingerprint"] for d in event["NearbyDevices"])
            for event in a.events[marker:] if "NearbyDevices" in event)})
        b.send({"SetDiscoveryEmail": "other-" + email})
        check("remote_email_change_removes_peer", lambda: not sees(a, b))
        b.send({"SetDiscoveryEmail": email})
        check("remote_email_change_restores_peer", lambda: sees(a, b))
        wait(lambda: sees(a, b), 45)
        peer = next(d for d in a.latest["NearbyDevices"] if d["fingerprint"] == b.latest["PublicKeyFingerprint"])
        checks.append({"name": "local_name_matches_remote_discovery", "pass": peer["name"] == b.latest.get("LocalDeviceName")})
        a.send({"ConnectDiscovered": [peer["id"], "right"]})
        fp = a.latest["PublicKeyFingerprint"]
        wait(lambda: any(r["id"] == fp for r in b.latest.get("PairingRequests", [])), 45)
        b.send({"RespondPairing": [fp, True]})

        def state():
            for event in reversed(a.events):
                for kind in ("State", "Created"):
                    if kind in event:
                        return event[kind][2]
            return {}

        wait(lambda: state().get("alive"), 45)
        b.send({"RemoveAuthorizedKey": fp})
        check("authorization_removed", lambda: fp not in b.latest.get("AuthorizedUpdated", {}))
        check("revocation_disconnects_sender", lambda: not state().get("alive"), 3)
        marker = len(a.events)
        hold(4)
        checks.append({"name": "revoked_peer_cannot_reconnect", "pass": not state().get("alive") and all(
            not event[kind][2]["alive"] for event in a.events[marker:]
            for kind in ("State", "Created") if kind in event)})
        report["result"] = "pass" if all(c["pass"] for c in checks) else "fail"
    except Exception as error:
        report.update(result="error", error=str(error))
        raise
    finally:
        for endpoint in reversed(endpoints):
            endpoint.stop()
        for endpoint in endpoints:
            (output / (endpoint.name + "-events.json")).write_text(json.dumps(endpoint.events, indent=2), encoding="utf-8")
        (output / "result.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(report))
    return 0 if report["result"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
