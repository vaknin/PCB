#!/usr/bin/env python3
"""The capture-clip firmware's QEMU scenarios (D-025 Phase D.4).

Each scenario boots the whole firmware image in Espressif's QEMU (16 MB flash, 8 MB octal PSRAM,
its Ethernet in place of Wi-Fi), drives it over the console the way the board's pins would
(`SIM BUTTON 1`, main/hw_stub.c), and passes or fails on what the firmware prints and on what
the mock server (mock_server.py) ends up holding. Nothing real is contacted and no real secret
is used.

  sim/run.py                 build, then every scenario
  sim/run.py press offline   only these
  sim/run.py --no-build ...  use the images already built
  sim/run.py --list

Results: build-qemu/scenarios.json; each scenario's console log in build-qemu/scenarios/.
QEMU flags and the otadata entry are the `sim` stage's (crates/pcbgen/src/sim.rs).
"""
import argparse
import hashlib
import json
import os
import queue
import re
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import mock_server  # noqa: E402

FW = Path(__file__).resolve().parent.parent
IDF = Path(os.environ.get("IDF_PATH", Path.home() / "esp/esp-idf-v6.1"))
OTADATA_VALID_OTA0 = bytes([1, 0, 0, 0] + [0xFF] * 20 + [2, 0, 0, 0, 0x9A, 0x98, 0x43, 0x47])
QEMU_CRASH_RETRIES = 3
BUILDS = {
    # directory: (extra sdkconfig defaults, version)
    "build-qemu": ([], None),
    "build-qemu-v2": ([], "sim-2"),
    "build-qemu-bad": (["sim/sdkconfig.bad"], "sim-bad"),
}


class Failed(Exception):
    pass


class SimulatorDied(Failed):
    """QEMU itself crashed (a signal), which says nothing about the firmware."""


def idf(script, cwd, log):
    out = subprocess.run(["bash", "-c", ". '%s/export.sh' >/dev/null 2>&1 && %s" % (IDF, script)], cwd=cwd,
                         capture_output=True, text=True)
    Path(log).write_text(out.stdout + out.stderr)
    if out.returncode != 0:
        raise Failed("`%s` failed; log: %s\n%s" % (script, log, "\n".join((out.stdout + out.stderr).splitlines()[-15:])))


def build(name):
    extra, version = BUILDS[name]
    directory = FW / name
    directory.mkdir(exist_ok=True)
    defaults = ";".join(["sdkconfig.defaults", "sdkconfig.qemu"] + extra)
    sdkconfig = directory / "sdkconfig"
    if sdkconfig.exists() and any((FW / d).stat().st_mtime > sdkconfig.stat().st_mtime for d in defaults.split(";")):
        sdkconfig.unlink()
    script = "idf.py -B %s -D SDKCONFIG=%s/sdkconfig -D SDKCONFIG_DEFAULTS='%s'%s build" % (
        name, name, defaults, " -D PROJECT_VER=%s" % version if version else "")
    idf(script, FW, directory / "build.log")


def base_image():
    """The flash image QEMU boots: flash_args merged to 16 MB, plus the "ota_0 valid" otadata entry."""
    directory = FW / "build-qemu"
    image = directory / "scenario_base.bin"
    idf("esptool --chip esp32s3 merge-bin --output scenario_base.bin --pad-to-size 16MB @flash_args", directory,
        directory / "merge.log")
    offset = next(int(line.split()[0], 16) for line in (directory / "flash_args").read_text().splitlines()
                  if line.endswith("ota_data_initial.bin"))
    data = bytearray(image.read_bytes())
    data[offset:offset + len(OTADATA_VALID_OTA0)] = OTADATA_VALID_OTA0
    image.write_bytes(data)
    return image


def qemu_binary():
    tools = Path(os.environ.get("IDF_TOOLS_PATH", Path.home() / ".espressif")) / "tools/qemu-xtensa"
    found = sorted(tools.glob("*/qemu/bin/qemu-system-xtensa"))
    if not found:
        raise Failed("QEMU not installed: idf_tools.py install qemu-xtensa")
    return found[-1]


_preload = []


def qemu_env():
    """The QEMU child's environment: ours, plus the preload that makes it non-dumpable
    (scripts/nodump.sh), so a simulator crash leaves no core dump and raises no desktop crash
    notice. Its exit status is unchanged. Only QEMU gets it: not this script, not the mock server."""
    if not _preload:
        script = FW.parents[2] / "scripts" / "nodump.sh"
        try:
            out = subprocess.run([str(script)], capture_output=True, text=True, timeout=60)
            path = out.stdout.strip().splitlines()[-1] if out.returncode == 0 and out.stdout.strip() else ""
            why = out.stderr.strip() or "exit %d, no path" % out.returncode
        except (OSError, subprocess.TimeoutExpired) as e:
            path, why = "", str(e)
        if not path or not Path(path).is_file():
            print("   warning: %s failed (%s); QEMU runs without it, a QEMU crash will leave a core dump" % (script, why),
                  flush=True)
            path = ""
        _preload.append(path)
    env = dict(os.environ)
    if _preload[0]:
        env["LD_PRELOAD"] = " ".join(filter(None, [_preload[0], env.get("LD_PRELOAD", "")]))
    return env


class Clip:
    """One QEMU run on a flash image. Lines are read in a thread; every wait has a deadline."""

    def __init__(self, image, log, network=True):
        self.image, self.log_path = image, log
        self.lines, self.cursor, self.inbox = [], 0, queue.Queue()
        args = [str(qemu_binary()), "-M", "esp32s3", "-m", "8M", "-drive", "file=%s,if=mtd,format=raw" % image,
                "-global", "driver=ssi_psram,property=is_octal,value=true", "-nographic", "-serial", "mon:stdio"]
        args += os.environ.get("CLIP_QEMU_EXTRA", "").split()
        if network:
            args += ["-nic", "user,model=open_eth"]
        self.proc = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                     env=qemu_env())
        threading.Thread(target=self._read, daemon=True).start()
        self.log = open(log, "a")
        self.log.write("==== QEMU start\n")

    def _read(self):
        for raw in self.proc.stdout:
            self.inbox.put(raw.decode(errors="replace").rstrip("\r\n"))
        self.inbox.put(None)

    def _pump(self, timeout):
        try:
            line = self.inbox.get(timeout=timeout)
        except queue.Empty:
            return False
        if line is None:
            code = self.proc.wait()
            raise (SimulatorDied if code < 0 else Failed)("QEMU exited with code %s (log: %s)" % (code, self.log_path))
        self.lines.append(line)
        self.log.write(line + "\n")
        self.log.flush()
        return True

    def wait(self, pattern, timeout=30, crash_ok=False):
        """The next line (after the last one matched) that `pattern` finds."""
        deadline = time.time() + timeout
        regex = re.compile(pattern)
        while True:
            while self.cursor < len(self.lines):
                line = self.lines[self.cursor]
                self.cursor += 1
                if regex.search(line):
                    return line
                if not crash_ok and ("Guru Meditation" in line or line.startswith("abort()")):
                    self.drain(2)  # the registers and the backtrace into the log
                    raise Failed("the firmware crashed while waiting for /%s/: %s" % (pattern, line))
            left = deadline - time.time()
            if left <= 0:
                if self.proc.poll() is None:  # what the firmware says of itself, into the log
                    self.send("STATUS")
                    self.drain(1)
                raise Failed("timed out after %d s waiting for /%s/ (log: %s)" % (timeout, pattern, self.log_path))
            self._pump(min(left, 0.5))

    def ev(self, name, timeout=30, **fields):
        """The next `CLIP {"ev": name, ...}` line whose fields match: its dict."""
        deadline = time.time() + timeout
        while True:
            line = self.wait(r'^CLIP \{"ev":"%s",' % name, max(deadline - time.time(), 0.1))
            try:
                event = json.loads(line[5:])
            except ValueError:
                raise Failed("not JSON: %s" % line)
            if all(event.get(k) == v for k, v in fields.items()):
                return event

    def seen(self, pattern, since=0):
        regex = re.compile(pattern)
        return [line for line in self.lines[since:] if regex.search(line)]

    def settle(self, pattern=r"^SLEEP \{", timeout=30, since=0):
        """Until a line with `pattern` has been printed at all (before or after the last wait)."""
        deadline = time.time() + timeout
        while not self.seen(pattern, since):
            if time.time() > deadline:
                raise Failed("timed out after %d s waiting for /%s/ (log: %s)" % (timeout, pattern, self.log_path))
            self._pump(0.1)

    def drain(self, seconds=0.3):
        end = time.time() + seconds
        while time.time() < end:
            self._pump(0.05)

    def send(self, line, shown=None):
        self.log.write(">> %s\n" % (shown or line))
        self.proc.stdin.write(line.encode() + b"\n")
        self.proc.stdin.flush()

    def sim(self, what):
        """A SIM line and its acknowledgement; lines printed meanwhile stay for the next wait()."""
        since = len(self.lines)
        self.send("SIM " + what)
        deadline = time.time() + 10
        while not self.seen(r"^SIM OK " + re.escape(what), since):
            if time.time() > deadline:
                raise Failed("no answer to SIM %s (log: %s)" % (what, self.log_path))
            self._pump(0.1)

    def provision(self, port, **more):
        self.wait(r"^PROV READY", 60)
        values = {"wifi_ssid": "sim-network", "wifi_pass": "sim-password", "notes_repo": mock_server.REPO,
                  "gemini_api_key": mock_server.GEMINI_KEY, "github_token": mock_server.GITHUB_TOKEN,
                  "sim_base": "http://10.0.2.2:%d" % port}
        values.update(more)
        for key, value in values.items():
            self.send("PROV SET %s %s" % (key, value.encode().hex()), "PROV SET %s (%d bytes)" % (key, len(value)))
            self.wait(r"^PROV OK %s %d$" % (key, len(value)), 10)

    # ---- what a finger does ----
    def press(self, ms=200):
        # timed by the firmware's clock, not by when this script's next line gets through
        self.sim("PRESS %d" % ms)
        time.sleep(ms / 1000)

    def record(self, seconds, hold=False):
        """Press (or hold), speak for `seconds`, press again."""
        mark = len(self.lines)
        if hold:
            self.sim("BUTTON 1")
            self.ev("addition", 10)
            self.sim("BUTTON 0")
        else:
            self.press()
        self.ev("state", 10, state="recording")
        time.sleep(seconds)
        self.press()
        return mark

    def kill(self):
        if self.log.closed:
            return
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait()
        self.log.write("==== QEMU killed\n")
        self.log.close()


class World:
    """What a scenario gets: a fresh flash image, a mock server, and QEMU runs on them."""

    def __init__(self, name, base, out):
        self.name = name
        self.image = out / (name + ".bin")
        self.log = out / (name + ".log")
        shutil.copyfile(base, self.image)
        self.log.write_text("")
        audio = out / (name + "-audio")
        shutil.rmtree(audio, ignore_errors=True)
        self.mock, self.port, self.server = mock_server.start(0, audio)
        self.clips = []

    def boot(self, provision=True, **more):
        clip = Clip(self.image, self.log)
        self.clips.append(clip)
        clip.wait(r"^BOARD \{", 60)
        if provision:
            clip.provision(self.port, **more)
            clip.settle()  # on the battery with nothing to do, it goes to sleep: a press then wakes it
        else:
            clip.wait(r"^PROV READY", 60)
            clip.settle(r'^CLIP \{"ev":"state"')  # the app's loop runs: pins are looked at from here on
        return clip

    def close(self):
        for clip in self.clips:
            clip.kill()
        if self.server:
            self.server.shutdown()
            self.server.server_close()
            self.server = None


def check(condition, what):
    if not condition:
        raise Failed(what)


def marks(clip):
    """The last heap and stack low-water marks the firmware printed."""
    found = clip.seen(r'^CLIP \{"ev":"marks"')
    return json.loads(found[-1][5:]) if found else {}


def the_note(mock, count=1):
    notes = mock.notes()
    check(len(notes) == count, "expected %d note(s) in the repo, found %d: %s" % (count, len(notes), sorted(notes)))
    return notes


def never_fast_with_radio(clip):
    """The charger's fast rate and the radio never overlap: from the firmware's own lines."""
    fast = up = False
    for line in clip.lines:
        if line.startswith("PIN {"):
            fast = json.loads(line[4:])["charge_fast"]
        elif line.startswith('CLIP {"ev":"net"'):
            up = json.loads(line[5:])["up"]
        elif line.startswith('CLIP {"ev":"state"') and json.loads(line[5:])["state"] == "connecting":
            up = True  # the radio is on from the moment it starts to connect
        check(not (fast and up), "fast charge with the radio on, at: %s" % line)


# ---- the scenarios: each returns its evidence line --------------------------------------------------

def press(w):
    """Press, speak, press: the note is committed."""
    clip = w.boot()
    clip.record(3)
    recorded = clip.ev("recorded", 15)
    check(recorded.get("kind") == "new" and 2500 <= recorded["ms"] <= 4500, "recorded: %s" % recorded)
    clip.ev("state", 15, state="connecting", led="sending")
    clip.ev("net", 30, up=True)
    clip.ev("upload", 60, result="saved", queued=0)
    clip.ev("state", 10, state="showing", led="saved")
    clip.wait(r"^SLEEP \{", 20)
    mock = w.mock
    path, text = next(iter(the_note(mock).items()))
    check(path == "notes/%s.md" % recorded["id"], "the note's path %s is not the recording's id" % path)
    check("source: clip" in text and "\nnum: 1\n" in text, "note front matter: %s" % text[:300])
    check("Simulated transcript number 1." in text and "Simulated note 1" in text, "note text: %s" % text[:400])
    audio = mock.audio[0]
    check(audio["ok"] and audio["mime"] == "audio/ogg" and not audio["addition"], "audio: %s" % audio)
    if audio["codec"] is not None:
        check(audio["codec"] == "opus" and 2.4 <= audio["seconds"] <= 4.6, "ffprobe: %s" % audio)
    check(mock.files.get("next-number", b"").strip() == b"2", "next-number is %r" % mock.files.get("next-number"))
    check(mock.commits[:2] == ["clip: next number 2", "clip: add Simulated note 1"], "commits: %s" % mock.commits)
    status = json.loads(mock.files["devices/clip.json"])
    check(status["device"] == "clip" and status["queued"] == 0 and status["battery_mv"] == 3900, "status file: %s" % status)
    never_fast_with_radio(clip)
    return "note %s committed (number 1, source clip); audio %s %.1f s %d B; commits %s" % (
        path, audio["codec"] or "OggS (no ffprobe)", audio["seconds"] or 0, audio["bytes"], mock.commits)


def hold(w):
    """A hold adds to the last note made here; the status file is not written again."""
    clip = w.boot()
    clip.record(2)
    first = clip.ev("recorded", 15)
    clip.ev("upload", 60, result="saved")
    clip.wait(r"^SLEEP \{", 20)
    clip.record(2, hold=True)
    added = clip.ev("recorded", 15)
    check(added.get("kind") == "addition", "the hold was recorded as %s" % added)
    clip.ev("upload", 60, result="saved", queued=0)
    clip.wait(r"^SLEEP \{", 20)
    mock = w.mock
    path, text = next(iter(the_note(mock).items()))
    check(path == "notes/%s.md" % first["id"], "the addition went to %s" % path)
    check("## Added" in text and "Simulated addition number 2." in text, "no addition in the note: %s" % text)
    check("Simulated transcript number 1." in text, "the note's own text is gone: %s" % text)
    check(mock.audio[1]["ok"] and mock.audio[1]["addition"], "the second request was not an addition: %s" % mock.audio[1])
    check(mock.commits[-1].startswith("clip: add to "), "commits: %s" % mock.commits)
    check(mock.commits.count("clip: status") == 1, "status commits: %s" % mock.commits)
    check(mock.files["next-number"].strip() == b"2", "an addition took a number")
    # a hold with the note gone from GitHub becomes a new note
    del mock.files[path]
    clip.record(2, hold=True)
    clip.ev("upload", 60, result="saved", queued=0)
    notes = the_note(mock)
    check("## Added" not in next(iter(notes.values())), "the orphan addition is not a plain note")
    return "addition in %s (## Added, commit '%s'); 1 status commit for 3 uploads; gone target became note %s" % (
        path, [c for c in mock.commits if c.startswith("clip: add to")][0], next(iter(notes)))


def offline(w):
    """Out of range: the recording is queued, and sent at the next wake with a network."""
    clip = w.boot()
    clip.sim("NET 0")
    clip.record(2)
    clip.ev("recorded", 15)
    clip.ev("net", 30, up=False)
    clip.ev("state", 10, state="showing", led="waiting")
    asleep = json.loads(clip.wait(r"^SLEEP \{", 20)[6:])
    # a minute after the connect failed, less the 3 s the amber light was shown
    check(50000 <= asleep["wake_after_ms"] <= 60000, "the first retry is not in a minute: %s" % asleep)
    check(not w.mock.log, "requests were made with no network: %s" % w.mock.log)
    # a second one, still out of range
    clip.record(2)
    clip.ev("net", 30, up=False)
    clip.wait(r"^SLEEP \{", 20)
    # back in range: the timer wake sends both, oldest first, 5 s apart (Gemini's rate gate)
    clip.sim("NET 1")
    clip.sim("TIMER")
    clip.ev("upload", 60, result="saved", queued=1)
    clip.ev("upload", 60, result="saved", queued=0)
    clip.wait(r"^SLEEP \{", 20)
    notes = the_note(w.mock, 2)
    numbers = sorted(int(re.search(r"\nnum: (\d+)", t).group(1)) for t in notes.values())
    check(numbers == [1, 2], "note numbers: %s" % numbers)
    check(all(a["ok"] for a in w.mock.audio) and len(w.mock.audio) == 2, "audio: %s" % w.mock.audio)
    # and a network that dies mid-upload: nothing is lost, nothing is doubled
    w.mock.github_lose = 1  # the counter's PUT is carried out, its answer lost
    clip.record(2)
    clip.ev("upload", 60, result="saved", queued=0)
    the_note(w.mock, 3)
    return "2 recordings queued offline (timer wake in a minute), both committed after SIM NET 1 (numbers %s); " \
           "a lost GitHub answer still gave exactly 3 notes" % numbers


def power_loss(w):
    """Power lost mid-recording: at the next boot what reached flash is queued and sent."""
    clip = w.boot()
    clip.press()
    clip.ev("state", 10, state="recording")
    time.sleep(4.5)
    clip.kill()  # the plug is pulled
    clip = w.boot(provision=False)
    recovered = clip.ev("recover", 30)
    check(recovered["ok"] and recovered["queued"] == 1 and recovered["dropped"] == 0, "recovery: %s" % recovered)
    clip.ev("upload", 60, result="saved", queued=0)
    text = next(iter(the_note(w.mock).values()))
    audio = w.mock.audio[0]
    check(audio["ok"], "the recovered audio: %s" % audio)
    if audio["codec"] is not None:
        check(2.0 <= audio["seconds"] <= 5.0, "the recovered audio is %.2f s (4.5 s recorded)" % audio["seconds"])
    duration = int(re.search(r"duration_ms: (\d+)", text).group(1))
    check(2000 <= duration <= 5000, "the note's duration is %d ms" % duration)
    # killed again at once, mid-boot, and again: nothing is doubled
    clip.kill()
    clip = w.boot(provision=False)
    check(clip.ev("recover", 30)["queued"] == 0, "a second recovery found something")
    clip.drain(3)
    the_note(w.mock)
    return "killed 4.5 s into a recording; recover queued=1; note committed with %s ms (ffprobe %.2f s); reboot: still 1 note" % (
        duration, audio["seconds"] or 0)


def low_battery(w):
    """Low: records, does not upload. Empty: refuses. A cell that sags under Wi-Fi: Wi-Fi off again."""
    clip = w.boot()
    clip.sim("BATTERY 3600")  # under the 3.65 V upload limit, over the 3.4 V record limit
    clip.record(2)
    clip.ev("recorded", 15)
    clip.ev("state", 10, state="showing", led="waiting")
    clip.ev("state", 10, state="showing", led="low_battery")
    asleep = json.loads(clip.wait(r"^SLEEP \{", 20)[6:])
    check(asleep["wake_after_ms"] == 0, "a low battery set a timer wake: %s" % asleep)
    check(not w.mock.log and not clip.seen(r'"ev":"net"'), "the radio was used on a low battery")
    # empty: no recording at all
    clip.sim("BATTERY 3350")
    mark = len(clip.lines)
    clip.press()
    clip.ev("state", 10, state="showing", led="low_battery")
    clip.wait(r"^SLEEP \{", 20)
    check(not clip.seen(r'"ev":"recorded"', mark), "an empty battery recorded")
    # healthy at rest, but it sags under the radio: Wi-Fi goes off again, the queue waits
    clip.sim("BATTERY 3900")
    clip.sim("LOADED 3500")
    clip.record(2)
    clip.ev("net", 30, up=True)
    clip.ev("battery", 10, sags=True)
    clip.ev("net", 20, up=False)
    clip.wait(r"^SLEEP \{", 30)
    check(not w.mock.log, "an upload was tried on a sagging cell: %s" % w.mock.log)
    # on USB the limits do not apply: both waiting recordings go out, and the fast charge comes
    # on only with the radio off
    clip.sim("LOADED 0")
    clip.sim("BATTERY 3500")
    clip.sim("CHRG 1")
    clip.sim("USB 1")
    clip.ev("upload", 60, result="saved", queued=1)
    clip.ev("upload", 60, result="saved", queued=0)
    clip.ev("state", 30, state="idle")
    clip.drain(1)
    the_note(w.mock, 2)
    never_fast_with_radio(clip)
    pins = [json.loads(line[4:]) for line in clip.seen(r"^PIN \{")]
    check(pins[-1]["charge_fast"] and pins[-1]["stdby_pull"], "on USB with the radio off: %s" % pins[-1])
    status = json.loads(w.mock.files["devices/clip.json"])
    check(status["usb"] and status["charging"] and status["battery_mv"] == 3500, "status file: %s" % status)
    return "3600 mV: recorded, no radio, wake_after 0; 3350 mV: refused; sag to 3500 mV under load: Wi-Fi off, queue kept; " \
           "USB: 2 notes sent, fast charge only with the radio off"


def selftest(w):
    """The six self-tests of board.toml report, none fails, and Wi-Fi's stand-in connects."""
    clip = w.boot()
    clip.sim("USB 1")
    clip.ev("state", 20, state="idle")
    mark = len(clip.lines)
    clip.send("SELFTEST")
    done = json.loads(clip.wait(r"^SELFTEST_DONE", 60)[14:])
    tests = {t["test"]: t for t in (json.loads(line[9:]) for line in clip.seen(r"^SELFTEST \{", mark))}
    wanted = ["mic", "led_rgb", "button", "battery", "usb_sense", "wifi"]
    check(sorted(tests) == sorted(wanted), "tests reported: %s" % sorted(tests))
    check(done["fail"] == 0 and done["missing"] == 0, "summary: %s" % done)
    check(tests["wifi"]["result"] == "pass" and tests["battery"]["result"] == "pass", "wifi/battery: %s" % tests)
    # a failing test is reported as one: a cell reading that makes no sense
    clip.sim("BATTERY 1200")
    mark = len(clip.lines)
    clip.send("SELFTEST")
    bad = json.loads(clip.wait(r"^SELFTEST_DONE", 60)[14:])
    check(bad["fail"] == 1, "a 1.2 V cell passed: %s" % bad)
    return "6 tests: %s; summary %s; a 1200 mV cell fails the battery test" % (
        ", ".join("%s %s" % (n, tests[n]["result"]) for n in wanted), json.dumps(done))


def manifest_for(w, directory, **change):
    image = (FW / directory / "firmware.bin").read_bytes()
    w.mock.image = image
    fields = {"version": BUILDS[directory][1], "url": "http://10.0.2.2:%d/update/image" % w.port, "size": len(image),
              "sha256": hashlib.sha256(image).hexdigest()}
    fields.update(change)
    w.mock.manifest = "# capture-clip firmware (simulation)\n" + "".join("%s=%s\n" % kv for kv in fields.items())
    return fields


def banner(clip, timeout=60):
    return json.loads(clip.wait(r"^BOARD \{", timeout)[6:])


def update(w):
    """On USB, after the queue is sent: a new firmware is fetched, checked, booted and kept."""
    clip = w.boot(update_url="http://10.0.2.2:%d/update/manifest" % w.port)
    old = json.loads(clip.seen(r"^BOARD \{")[0][6:])
    # a manifest whose hash is wrong: nothing changes
    manifest_for(w, "build-qemu-v2", sha256="00" * 32)
    clip.sim("USB 1")
    clip.record(2)
    clip.ev("upload", 60, result="saved")
    failed = clip.ev("update", 120, step="failed")
    check("SHA-256" in failed["detail"], "a wrong hash gave: %s" % failed)
    clip.ev("state", 20, state="idle")
    # unplugged and plugged in again: the real one
    fields = manifest_for(w, "build-qemu-v2")
    clip.sim("USB 0")
    clip.wait(r"^SLEEP \{", 20)
    clip.sim("USB 1")
    clip.record(2)
    clip.ev("upload", 60, result="saved")
    clip.ev("update", 120, step="ready", detail="sim-2")
    clip.ev("update", 30, step="restart")
    new = banner(clip)
    check(new["fw"] == "sim-2" and new["slot"] != old["slot"], "after the update: %s (was %s)" % (new, old))
    clip.ev("update", 60, step="kept", version="sim-2")
    clip.settle(r'^CLIP \{"ev":"state"', since=clip.cursor)
    # it stays after a power cycle, the notes and the provisioning with it
    clip.kill()
    clip = w.boot(provision=False)
    again = json.loads(clip.seen(r"^BOARD \{")[0][6:])
    check(again["fw"] == "sim-2" and again["slot"] == new["slot"], "after a power cycle: %s" % again)
    clip.sim("USB 1")
    clip.record(2)
    clip.ev("upload", 60, result="saved")
    current = clip.ev("update", 60, step="none")
    check("current" in current["detail"], "the new firmware's own check: %s" % current)
    the_note(w.mock, 3)
    return "wrong SHA-256 refused; %s (%s) -> %s (%s), %d bytes, kept after its check and after a power cycle; 3 notes" % (
        old["fw"], old["slot"], new["fw"], new["slot"], fields["size"])


def rollback(w):
    """A new firmware that fails its own check is rolled back, and not fetched again."""
    clip = w.boot(update_url="http://10.0.2.2:%d/update/manifest" % w.port)
    old = json.loads(clip.seen(r"^BOARD \{")[0][6:])
    manifest_for(w, "build-qemu-bad")
    clip.sim("USB 1")
    clip.record(2)
    clip.ev("upload", 60, result="saved")
    clip.ev("update", 120, step="ready", detail="sim-bad")
    trial = banner(clip)
    check(trial["fw"] == "sim-bad" and trial["slot"] != old["slot"], "the trial boot: %s" % trial)
    clip.ev("update", 60, step="rollback", version="sim-bad")
    back = banner(clip)
    check(back["fw"] == old["fw"] and back["slot"] == old["slot"], "after the rollback: %s (was %s)" % (back, old))
    clip.ev("error", 30)
    # the same manifest again: known bad, not downloaded again
    clip.wait(r"^PROV READY", 30)
    clip.settle(r'^CLIP \{"ev":"state"', since=clip.cursor)
    clip.sim("USB 0")
    clip.settle(since=clip.cursor)
    clip.sim("USB 1")
    clip.record(2)
    clip.ev("upload", 60, result="saved")
    skipped = clip.ev("update", 60, step="none")
    check("rolled back" in skipped["detail"] and w.mock.image_requests == 1, "second look: %s, %d downloads" % (
        skipped, w.mock.image_requests))
    the_note(w.mock, 2)
    status = json.loads(w.mock.files["devices/clip.json"])
    check("rolled back" in status["last_error"], "the status file does not tell: %s" % status)
    return "%s booted on trial in %s, failed its check, rolled back to %s (%s); not downloaded again (1 download); " \
           "status file says '%s'" % (trial["fw"], trial["slot"], back["fw"], back["slot"], status["last_error"])


SCENARIOS = {f.__name__: f for f in (press, hold, offline, power_loss, low_battery, selftest, update, rollback)}
NEEDS = {"update": ["build-qemu-v2"], "rollback": ["build-qemu-bad"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("names", nargs="*")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()
    if args.list:
        for name, fn in SCENARIOS.items():
            print("%-12s %s" % (name, fn.__doc__))
        return 0
    names = args.names or list(SCENARIOS)
    unknown = [n for n in names if n not in SCENARIOS]
    if unknown:
        parser.error("no scenario %s" % unknown)
    builds = ["build-qemu"] + sorted({b for n in names for b in NEEDS.get(n, [])})
    try:
        if not args.no_build:
            for name in builds:
                print("== build %s" % name, flush=True)
                build(name)
        base = base_image()
    except Failed as e:
        print("== SCENARIOS: FAIL (%s)" % e)
        return 1
    out = FW / "build-qemu" / "scenarios"
    out.mkdir(exist_ok=True)
    results, low = [], {}
    for name in names:
        started = time.time()
        crashes = 0
        while True:
            world = World(name, base, out)
            try:
                evidence, ok = SCENARIOS[name](world), True
            except SimulatorDied as e:
                # Espressif's QEMU sometimes segfaults (in its translator, while the guest maps or writes flash);
                # that is the simulator, not the firmware: the scenario is run again, and it is said.
                crashes += 1
                world.close()
                if crashes < QEMU_CRASH_RETRIES:
                    shutil.copyfile(world.log, str(world.log) + ".crash%d" % crashes)
                    continue
                evidence, ok = str(e), False
            except Failed as e:
                evidence, ok = "%s | mock log: %s | audio: %s" % (e, world.mock.log[-8:], [
                    {k: v for k, v in a.items() if k != "path"} for a in world.mock.audio[-2:]]), False
            except Exception as e:  # a bug in the scenario is a failure, with its place
                import traceback
                evidence, ok = "%s: %s\n%s" % (type(e).__name__, e, traceback.format_exc()), False
            break
        if crashes:
            evidence += " [QEMU itself crashed %d time(s) first; rerun]" % crashes
        for clip in world.clips:
            for key, value in marks(clip).items():
                if key not in ("store_free", "ev", "t"):
                    low[key] = min(low.get(key, value), value)
        world.close()
        results.append({"scenario": name, "ok": ok, "seconds": round(time.time() - started, 1), "evidence": evidence,
                        "qemu_crashes": crashes, "log": str(world.log.relative_to(FW))})
        print("   %-12s %-5s %5.1f s  %s" % (name, "pass" if ok else "FAIL", time.time() - started, evidence), flush=True)
    ok = all(r["ok"] for r in results)
    (FW / "build-qemu" / "scenarios.json").write_text(json.dumps(
        {"date": time.strftime("%Y-%m-%d"), "ok": ok, "low_water": low, "scenarios": results}, indent=2) + "\n")
    print("   low-water marks (bytes free, the least seen): %s" % json.dumps(low))
    print("== SCENARIOS: %s (%d of %d)" % ("PASS" if ok else "FAIL", sum(r["ok"] for r in results), len(results)))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
