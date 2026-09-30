#!/usr/bin/env python3
"""A local stand-in for everything the clip talks to, for the QEMU scenarios (run.py).

No real request leaves a simulation and no real secret enters one: the QEMU build swaps
api.github.com and generativelanguage.googleapis.com for this server (main/http.c).

  GET  /sim/time                              the clock (Unix seconds), instead of SNTP
  POST /gemini/v1beta/interactions            takes the streamed request, keeps the audio, checks
                                              it is OGG/Opus (ffprobe when installed), answers
                                              with a canned transcript
  GET/PUT /github/repos/<repo>/contents/<path>   GitHub's contents API as the capture component
                                              uses it: sha on replace, 409 on a stale sha, 422 on
                                              a missing one, 404, 401 on a wrong token
  GET  /update/manifest, /update/image        a firmware update

Failures on demand: set fields of `Mock` (run.py does), or POST /sim/control with JSON.
Python's standard library only. Run alone:  mock_server.py [port]
"""
import base64
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

GEMINI_KEY = "sim-gemini-key-not-real"
GITHUB_TOKEN = "sim-github-token-not-real"
REPO = "sim-owner/sim-notes"
BRANCH = "main"


def git_sha(data: bytes) -> str:
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


class Mock:
    """The server's state; every field a scenario may read or set."""

    def __init__(self, out_dir=None):
        self.lock = threading.RLock()
        self.files = {}            # path -> bytes: the notes repo
        self.log = []              # "GET next-number", "PUT notes/<id>.md", "GEMINI", ...
        self.commits = []          # commit messages, in order
        self.audio = []            # one dict per Gemini request: bytes, codec, duration, addition
        self.out_dir = Path(out_dir) if out_dir else Path(tempfile.mkdtemp(prefix="clip-mock-"))
        self.out_dir.mkdir(parents=True, exist_ok=True)
        # --- failures on demand ---
        self.github_status = []    # statuses for the next GitHub requests (0: drop the connection)
        self.github_lose = 0       # carry out the next n PUTs, then drop the connection (answer lost)
        self.github_edit = None    # (path, bytes): another client writes this just before the next PUT of path
        self.gemini_status = []    # statuses for the next Gemini requests (0: drop)
        self.gemini_answers = []   # dicts {transcript,title,summary} for the next requests
        self.time_offset = 0       # seconds added to the clock
        # --- update ---
        self.manifest = None       # text
        self.image = None          # bytes
        self.image_requests = 0

    def answer(self, addition: bool):
        if self.gemini_answers:
            return self.gemini_answers.pop(0)
        if addition:
            return {"transcript": "Simulated addition number %d." % len(self.audio),
                    "title": "Simulated note, added to", "summary": "A made-up note with an addition."}
        return {"transcript": "Simulated transcript number %d." % len(self.audio),
                "title": "Simulated note %d" % len(self.audio), "summary": "A made-up note."}

    def notes(self):
        with self.lock:
            return {p: d.decode() for p, d in self.files.items() if p.startswith("notes/")}


def probe(path: Path):
    """(codec, seconds) from ffprobe, or (None, None) when it is not installed."""
    if not shutil.which("ffprobe"):
        return None, None
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "stream=codec_name:format=duration", "-of", "json", str(path)],
        capture_output=True, text=True)
    try:
        info = json.loads(out.stdout)
        return info["streams"][0]["codec_name"], float(info["format"]["duration"])
    except (ValueError, KeyError, IndexError):
        return "unreadable", 0.0


def find_audio(node):
    """The inline audio part of an interactions request: (mime type, base64 text)."""
    if isinstance(node, dict):
        if node.get("type") == "audio" and "data" in node:
            return node.get("mime_type"), node["data"]
        node = list(node.values())
    if isinstance(node, list):
        for item in node:
            found = find_audio(item)
            if found:
                return found
    return None


def make_handler(mock: Mock):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *args):
            pass

        def body(self) -> bytes:
            length = int(self.headers.get("Content-Length") or 0)
            data = b""
            while len(data) < length:
                chunk = self.rfile.read(length - len(data))
                if not chunk:
                    break
                data += chunk
            return data

        def send(self, status, payload, content_type="application/json", headers=()):
            data = payload if isinstance(payload, bytes) else (
                payload.encode() if isinstance(payload, str) else json.dumps(payload).encode())
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(data)))
            for key, value in headers:
                self.send_header(key, value)
            self.end_headers()
            self.wfile.write(data)

        def drop(self):
            self.close_connection = True
            try:
                self.connection.shutdown(2)
            except OSError:
                pass

        # ---- routes ----
        def do_GET(self):
            path = self.path.split("?")[0]
            if path == "/sim/time":
                return self.send(200, str(int(time.time()) + mock.time_offset), "text/plain")
            if path == "/update/manifest":
                if mock.manifest is None:
                    return self.send(404, "no manifest", "text/plain")
                return self.send(200, mock.manifest, "text/plain")
            if path == "/update/image":
                with mock.lock:
                    mock.image_requests += 1
                if mock.image is None:
                    return self.send(404, "no image", "text/plain")
                return self.send(200, mock.image, "application/octet-stream")
            if path.startswith("/github/"):
                return self.github("GET", path, None)
            self.send(404, {"message": "Not Found"})

        def do_PUT(self):
            data = self.body()
            path = self.path.split("?")[0]
            if path.startswith("/github/"):
                return self.github("PUT", path, data)
            self.send(404, {"message": "Not Found"})

        def do_POST(self):
            data = self.body()
            path = self.path.split("?")[0]
            if path == "/gemini/v1beta/interactions":
                return self.gemini(data)
            if path == "/sim/control":
                with mock.lock:
                    for key, value in json.loads(data or b"{}").items():
                        setattr(mock, key, value)
                return self.send(200, {"ok": True})
            self.send(404, {"message": "Not Found"})

        def github(self, method, path, data):
            prefix = "/github/repos/%s/contents/" % REPO
            with mock.lock:
                name = path[len(prefix):] if path.startswith(prefix) else None
                mock.log.append("%s %s" % (method, name if name is not None else path))
                if mock.github_status:
                    status = mock.github_status.pop(0)
                    if status == 0:
                        return self.drop()
                    if status != 200:
                        headers = (("retry-after", "1"),) if status == 429 else ()
                        return self.send(status, {"message": "simulated %d" % status}, headers=headers)
                if self.headers.get("Authorization") != "Bearer " + GITHUB_TOKEN:
                    return self.send(401, {"message": "Bad credentials"})
                if not self.headers.get("User-Agent"):
                    return self.send(403, {"message": "Request forbidden by administrative rules (no User-Agent)"})
                if name is None:
                    return self.send(404, {"message": "Not Found"})
                if method == "GET":
                    if name not in mock.files:
                        return self.send(404, {"message": "Not Found"})
                    if "ref=" + BRANCH not in self.path:
                        return self.send(404, {"message": "No commit found for the ref"})
                    content = mock.files[name]
                    wrapped = base64.encodebytes(content).decode()  # in lines, as GitHub sends it
                    return self.send(200, {"name": name.split("/")[-1], "path": name, "sha": git_sha(content),
                                           "encoding": "base64", "content": wrapped})
                try:
                    request = json.loads(data)
                    content = base64.b64decode(request["content"], validate=True)
                    message = request["message"]
                except (ValueError, KeyError):
                    return self.send(422, {"message": "Invalid request"})
                if request.get("branch") != BRANCH:
                    return self.send(404, {"message": "Branch not found"})
                if mock.github_edit and mock.github_edit[0] == name:
                    mock.files[name] = mock.github_edit[1]  # another client got there first
                    mock.commits.append("other: edit %s" % name)
                    mock.github_edit = None
                existing = mock.files.get(name)
                if existing is not None:
                    if "sha" not in request:
                        return self.send(422, {"message": "Invalid request.\n\n\"sha\" wasn't supplied."})
                    if request["sha"] != git_sha(existing):
                        return self.send(409, {"message": "%s does not match %s" % (name, request["sha"])})
                elif "sha" in request:
                    return self.send(404, {"message": "Not Found"})
                mock.files[name] = content
                mock.commits.append(message)
                if mock.github_lose > 0:
                    mock.github_lose -= 1
                    return self.drop()
                return self.send(200 if existing is not None else 201,
                                 {"content": {"path": name, "sha": git_sha(content)}, "commit": {"message": message}})

        def gemini(self, data):
            with mock.lock:
                mock.log.append("GEMINI")
                if mock.gemini_status:
                    status = mock.gemini_status.pop(0)
                    if status == 0:
                        return self.drop()
                    if status != 200:
                        return self.send(status, {"error": {"code": status, "message": "simulated %d" % status,
                                                            "status": "UNAVAILABLE"}})
                if self.headers.get("x-goog-api-key") != GEMINI_KEY:
                    return self.send(400, {"error": {"code": 400, "message": "API key not valid.", "status": "INVALID_ARGUMENT"}})
                record = {"bytes": 0, "codec": None, "seconds": None, "ok": False, "addition": False, "why": ""}
                mock.audio.append(record)
                try:
                    request = json.loads(data)
                except ValueError:
                    record["why"] = "the body is not JSON (%d bytes)" % len(data)
                    return self.send(400, {"error": {"code": 400, "message": record["why"], "status": "INVALID_ARGUMENT"}})
                found = find_audio(request)
                if not found:
                    record["why"] = "no audio part"
                    return self.send(400, {"error": {"code": 400, "message": record["why"], "status": "INVALID_ARGUMENT"}})
                mime, text = found
                try:
                    audio = base64.b64decode(text, validate=True)
                except ValueError:
                    record["why"] = "the audio is not base64"
                    return self.send(400, {"error": {"code": 400, "message": record["why"], "status": "INVALID_ARGUMENT"}})
                path = mock.out_dir / ("gemini-%d.ogg" % len(mock.audio))
                path.write_bytes(audio)
                codec, seconds = probe(path)
                record.update(bytes=len(audio), codec=codec, seconds=seconds, path=str(path), mime=mime,
                              model=request.get("model"),
                              addition="adds to the note above" in json.dumps(request))
                if mime != "audio/ogg" or audio[:4] != b"OggS" or b"OpusHead" not in audio[:64] or codec not in (None, "opus"):
                    record["why"] = "not OGG/Opus"
                    return self.send(400, {"error": {"code": 400, "message": record["why"], "status": "INVALID_ARGUMENT"}})
                record["ok"] = True
                answer = mock.answer(record["addition"])
            self.send(200, {"id": "sim-%d" % len(mock.audio), "status": "completed",
                            "steps": [{"type": "user_input"},
                                      {"type": "model_output", "content": [{"type": "text", "text": json.dumps(answer)}]}],
                            "usage": {"total_input_tokens": 900, "total_output_tokens": 40}})

    return Handler


def start(port=0, out_dir=None):
    """Starts the server on 127.0.0.1 in a thread: (mock, port, server)."""
    mock = Mock(out_dir)
    server = ThreadingHTTPServer(("127.0.0.1", port), make_handler(mock))
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return mock, server.server_address[1], server


if __name__ == "__main__":
    mock, port, server = start(int(sys.argv[1]) if len(sys.argv) > 1 else 8765)
    print("mock server on http://127.0.0.1:%d (QEMU sees it as http://10.0.2.2:%d); audio in %s" % (port, port, mock.out_dir))
    try:
        threading.Event().wait()
    except KeyboardInterrupt:
        pass
