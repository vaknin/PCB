#!/usr/bin/env python3
"""Sends one command to the real-hardware codec spike (main/real.c) and prints its `R {json}`
answers. Run inside the ESP-IDF Python environment (it has pyserial); run-real.sh does that.
  bench.py [-p /dev/ttyACM0] [-t seconds] <command...>     e.g. bench.py enc 0 240 synth psram 0
  bench.py bootlog      reset the chip through the USB bridge's RTS line and print what it says
  bench.py dump FILE    copy the OGG saved on the board (enc ... 1) into FILE, checked by length and sum
  bench.py matrix N OUT.jsonl [mem] [cx,cx..] [mhz,mhz..] [src,src..] [voip|audio]   N encode runs of every combination
  bench.py wake N OUT.jsonl [sleep_ms]    N timer deep sleeps; each line is the `status` after the wake
  bench.py restart N OUT.jsonl            N software resets; each line is the `status` after it"""
import base64
import json
import os
import sys
import time

import serial


def open_port(port, wait=15.0):
    end = time.time() + wait
    while True:
        try:
            if os.path.exists(port):
                # Opening must not reset the chip. Linux raises DTR and RTS on open; pyserial then
                # drops DTR before RTS, and DTR low + RTS high is what the USB Serial/JTAG bridge
                # takes as "reset". So: keep DTR high through the open (RTS drops first), then
                # drop DTR.
                s = serial.Serial()
                s.port, s.baudrate, s.timeout = port, 115200, 0.2
                s.dtr, s.rts = True, False
                s.open()
                s.dtr = False
                return s
        except (serial.SerialException, OSError):
            pass
        if time.time() > end:
            sys.exit(f"{port} did not appear")
        time.sleep(0.05)


def command(port, line, timeout, out=None, blobs=None, quiet=False):
    """out: list that gets each parsed R answer; blobs: list that gets each B line's bytes."""
    s = open_port(port)
    s.reset_input_buffer()
    s.write((line + "\n").encode())
    end, buf, ok = time.time() + timeout, b"", False
    while time.time() < end and not ok:
        try:
            buf += s.read(4096)
        except serial.SerialException:
            break  # the port went away (deep sleep)
        while b"\n" in buf:
            text, buf = buf.split(b"\n", 1)
            text = text.decode(errors="replace").strip()
            if text.startswith("B ") and blobs is not None:
                blobs.append(base64.b64decode(text[2:]))
            elif text.startswith("R "):
                if not quiet:
                    print(text[2:], flush=True)
                if out is not None:
                    out.append(json.loads(text[2:]))
            elif text == "DONE":
                ok = True
    s.close()
    return ok


def bootlog(port, seconds=4.0):
    s = open_port(port)
    s.rts = True  # the USB Serial/JTAG bridge turns RTS into a chip reset
    time.sleep(0.1)
    s.rts = False
    end, out = time.time() + seconds, b""
    while time.time() < end:
        try:
            out += s.read(4096)
        except (serial.SerialException, OSError):
            s.close()
            s = open_port(port)
    print(out.decode(errors="replace"))


def dump(port, path):
    out, blobs = [], []
    if not command(port, "dump", 120, out, blobs, quiet=True) or not out or "bytes" not in out[-1]:
        sys.exit(f"dump failed: {out}")
    data, total = b"".join(blobs), 0
    for b in data:
        total = (total * 31 + b) & 0xFFFFFFFF
    if len(data) != out[-1]["bytes"] or total != out[-1]["sum31"]:
        sys.exit(f"dump damaged: got {len(data)} bytes, sum {total}; board says {out[-1]}")
    open(path, "wb").write(data)
    print(json.dumps({"file": path, "bytes": len(data), "complexity": out[-1]["complexity"], "sum_ok": True}))


def status_after(port, first, n, path, gap):
    """Sends `first` (the board goes away), waits, then asks for `status`; n times."""
    with open(path, "a") as f:
        for i in range(n):
            t0 = time.time()
            command(port, first, 5, quiet=True)
            time.sleep(gap)
            out = []
            for _ in range(40):  # the port comes back when USB re-enumerates
                try:
                    # a `status` sent while the chip is still booting arrives damaged and is
                    # answered with an error: ask again until the real answer comes
                    out.clear()
                    if command(port, "status", 3, out, quiet=True) and out and out[-1].get("test") == "status":
                        break
                    out.clear()
                except (serial.SerialException, OSError):
                    pass
                time.sleep(0.25)
            if not out:
                sys.exit("the board did not come back")
            out[-1]["laptop_s_until_status"] = round(time.time() - t0, 2)
            f.write(json.dumps(out[-1]) + "\n")
            f.flush()
            print(json.dumps(out[-1]), flush=True)


def matrix(port, n, path, mem="psram", cxs="0,1,3,5,10", mhzs="240,160,80", srcs="speech,synth", app="voip"):
    with open(path, "a") as f:
        for rep in range(n):
            for mhz in mhzs.split(","):
                for src in srcs.split(","):
                    for cx in cxs.split(","):
                        out = []
                        if not command(port, f"enc {cx} {mhz} {src} {mem} 0 {app}", 600, out, quiet=True):
                            sys.exit("no DONE from the board")
                        out[-1]["rep"] = rep
                        f.write(json.dumps(out[-1]) + "\n")
                        f.flush()
                        r = out[-1]
                        print(rep, mhz, src, cx, r.get("rtf"), r.get("frame_worst_us"), r.get("error", ""), flush=True)


if __name__ == "__main__":
    args, port, timeout = sys.argv[1:], "/dev/ttyACM0", 120.0
    while args and args[0] in ("-p", "-t"):
        if args[0] == "-p":
            port = args[1]
        else:
            timeout = float(args[1])
        args = args[2:]
    if args == ["bootlog"]:
        bootlog(port)
    elif args and args[0] == "dump":
        dump(port, args[1])
    elif args and args[0] == "matrix":
        matrix(port, int(args[1]), *args[2:])
    elif args and args[0] == "wake":
        ms = args[3] if len(args) > 3 else "3000"
        status_after(port, f"sleep {ms} none", int(args[1]), args[2], int(ms) / 1000 + 1.5)
    elif args and args[0] == "restart":
        status_after(port, "restart", int(args[1]), args[2], 1.0)
    elif not command(port, " ".join(args), timeout):
        sys.exit("no DONE from the board")
