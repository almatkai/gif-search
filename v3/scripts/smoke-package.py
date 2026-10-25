#!/usr/bin/env python3
"""Validate and smoke an extracted package using an actual fixture host.

No Tezbar/app source, SDK source, Cargo or Bun is used by the executable.
This fixture acknowledges shell calls; it never opens a real browser.
"""
import argparse
import base64
import hashlib
import json
import os
import pathlib
import queue
import stat
import subprocess
import tempfile
import threading
import uuid
import zipfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", type=pathlib.Path, required=True)
    parser.add_argument("--target", choices=["aarch64-apple-darwin", "x86_64-pc-windows-msvc"], required=True)
    args = parser.parse_args()
    inventory = json.loads(args.archive.with_suffix(".inventory.json").read_text())
    artifact = json.loads(args.archive.with_suffix(".artifact.json").read_text())["artifacts"][args.target]
    assert hashlib.sha256(args.archive.read_bytes()).hexdigest() == artifact["sha256"]
    with tempfile.TemporaryDirectory(prefix="gif-search-smoke-") as directory:
        root = pathlib.Path(directory)
        with zipfile.ZipFile(args.archive) as package:
            assert len(package.namelist()) == len(set(package.namelist())), "Duplicate ZIP members"
            assert sum(entry.file_size for entry in package.infolist()) <= 512 * 1024 * 1024
            manifest = json.loads(package.read("extension.json"))
            assert set(inventory) == set(package.namelist())
            assert manifest["files"][artifact["executable"]] == artifact["executableSha256"]
            assert set(package.namelist()) == set(manifest["files"]) | {"extension.json"}
            for entry in package.infolist():
                relative = pathlib.PurePosixPath(entry.filename)
                assert not relative.is_absolute() and all(part not in ("..", ".") and ":" not in part and "\\" not in part for part in relative.parts)
                assert not stat.S_ISLNK(entry.external_attr >> 16)
                data = package.read(entry)
                assert hashlib.sha256(data).hexdigest() == inventory[entry.filename]
                if entry.filename != "extension.json":
                    assert hashlib.sha256(data).hexdigest() == manifest["files"][entry.filename]
                path = root.joinpath(*relative.parts)
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
        executable = root / "bin" / ("tezbar-gif-search.exe" if args.target.startswith("x86_64-pc-windows") else "tezbar-gif-search")
        executable.chmod(0o755)
        # Preserve OS essentials, but remove developer/runtime lookup from PATH.
        env = {name: value for name, value in os.environ.items() if name in ("SystemRoot", "WINDIR", "TEMP", "TMP")}
        env["PATH"] = os.path.join(env.get("SystemRoot", "C:/Windows"), "System32") if os.name == "nt" else "/usr/bin:/bin"
        info = json.loads(subprocess.check_output([str(executable), "--protocol-info"], env=env, cwd=root, timeout=5))
        assert info["id"] == manifest["id"] and info["version"] == manifest["version"]
        messages = queue.Queue(maxsize=128)
        process = subprocess.Popen([str(executable)], cwd=root, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)

        def read_output():
            for line in process.stdout:
                messages.put(line)
            messages.put(None)

        reader = threading.Thread(target=read_output, daemon=True)
        reader.start()

        def receive():
            line = messages.get(timeout=5)
            assert line is not None, "Unexpected process EOF"
            assert len(line) <= 16 * 1024 * 1024 + 1
            return json.loads(line)

        def send(value):
            process.stdin.write((json.dumps(value, ensure_ascii=False) + "\n").encode())
            process.stdin.flush()

        def request(method, params):
            value = {"type": "request", "id": "h:" + str(uuid.uuid4()), "method": method, "params": params}
            send(value)
            return value["id"]

        def invoke(method, params):
            identity = request(method, params)
            response = receive()
            assert response["id"] == identity
            return response

        try:
            hello = receive()
            assert hello["type"] == "hello" and hello["id"] == manifest["id"] and hello["protocol"] == 1
            assert json.loads(subprocess.check_output([str(executable), "--build-info"], env=env, cwd=root, timeout=5)) == {"fixtureHttp": False}
            gif = {"service":"giphy", "id":"fixture", "title":"Fixture GIF", "url":"https://giphy.com/gifs/fixture",
                   "gif_url":"https://media.giphy.com/fixture.gif?tracking=1", "download_url":"https://media.giphy.com/fixture.gif",
                   "download_name":"fixture-gif.gif", "preview":"https://media.giphy.com/fixture.gif",
                   "large_preview":"https://media.giphy.com/fixture.gif", "metadata":{}}
            migration = root / "migration.json"
            migration.write_text(json.dumps({"giphy-favs":["fixture"], "giphy-recent":["fixture"],
                                            "gifSearchItems":{"giphy:fixture":gif}, "secret":"do-not-import"}))
            cache_key = hashlib.sha256(b"giphy:fixture:https://media.giphy.com/fixture.gif").hexdigest()
            cached = root / "data" / "native-v2" / "gif-search" / "cached-gifs" / cache_key / "fixture-gif.gif"
            cached.parent.mkdir(parents=True)
            cached.write_bytes(base64.b64decode("R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"))
            assert invoke("initialize", {"protocol": 1, "extensionId": manifest["id"], "platform": "Windows" if os.name == "nt" else "macOS",
                                         "packageDir": str(root), "dataDir": str(root / "data"), "tempDir": str(root / "temp"),
                                         "grantedCapabilities": ["clipboard", "window"], "reason": "package-smoke",
                                         "migrationPaths": {"gif-search.native-v2-store":str(migration)}})["result"]["ready"]
            for command in ["favorites", "recents"]:
                response = invoke("session.open", {"sessionId": command, "commandName": command, "preferences": {}, "argumentValues": {}})
                assert response["result"]["mode"] == "view"
                assert response["result"]["root"]["children"][0]["children"][0]["props"]["id"] == "giphy:fixture"
            for action, methods in [("copyGifUrl", ["host.clipboard.write"]), ("copyFile", ["host.clipboard.writeFiles", "host.window.visibility"])]:
                identity = request("session.call", {"sessionId":"favorites", "operation":"extension:invoke-action",
                                                    "payload":{"actionId":f"gif-search:{action}:giphy%3Afixture"}})
                for method in methods:
                    reverse = receive()
                    assert reverse["type"] == "request" and reverse["method"] == method
                    if method == "host.clipboard.write":
                        assert reverse["params"] == {"text":"https://media.giphy.com/fixture.gif"}
                    elif method == "host.clipboard.writeFiles":
                        assert reverse["params"] == {"paths":[str(cached)]}
                    else:
                        assert reverse["params"] == {"visible":False}
                    send({"type":"response", "id":reverse["id"], "result":None})
                response = receive()
                assert response["id"] == identity and response["result"]["mode"] == "view"
            # Exercise search command without networking by selecting local favorites.
            store_path = root / "data" / "gif-search" / "storage.json"
            store = json.loads(store_path.read_text()); assert "secret" not in store
            store["service"] = "favorites"; store_path.write_text(json.dumps(store))
            assert invoke("session.open", {"sessionId":"search", "commandName":"search", "preferences":{}, "argumentValues":{}})["result"]["mode"] == "view"
            for command in ["favorites", "recents", "search"]:
                assert "result" in invoke("session.dispose", {"sessionId":command})
            assert "error" in invoke("session.call", {"sessionId":"favorites", "operation":"extension:refresh-session", "payload":{}})
            assert "result" in invoke("shutdown", {})
            assert process.wait(timeout=6) == 0
            process.stdin.close(); reader.join(timeout=1)
            print(json.dumps({"passed":True, "target":args.target, "fixtureHost":True, "liveClipboard":False,
                              "checks":["inventory","isolated-package","identity","three-commands","migration","link","cached-media","reverse-rpc","dispose","shutdown"]}))
        finally:
            if process.poll() is None:
                process.kill()
            process.wait(timeout=5)
            if not process.stdin.closed:
                process.stdin.close()
            process.stdout.close()


if __name__ == "__main__":
    main()
