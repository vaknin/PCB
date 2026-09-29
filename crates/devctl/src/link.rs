//! How devctl talks to a board: its USB console (`SerialLink`) or the firmware in QEMU
//! (`QemuLink`). Both carry text lines.

use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use pcbgen::sim::Console;
use serialport::{ClearBuffer, SerialPort, SerialPortType, TTYPort};
use zeroize::Zeroizing;

pub trait Link {
    /// The next console line (no CR/LF), or None if none comes within `timeout`.
    fn next_line(&mut self, timeout: Duration) -> Result<Option<String>>;
    /// Sends one line; any log the link keeps gets `shown` instead of it.
    fn send(&mut self, line: &str, shown: &str) -> Result<()>;
    /// Restarts the firmware from boot.
    fn reset(&mut self) -> Result<()>;
    /// Where the link goes, for reports: the port, or "qemu".
    fn name(&self) -> String;
}

impl Link for Box<dyn Link> {
    fn next_line(&mut self, timeout: Duration) -> Result<Option<String>> {
        (**self).next_line(timeout)
    }
    fn send(&mut self, line: &str, shown: &str) -> Result<()> {
        (**self).send(line, shown)
    }
    fn reset(&mut self) -> Result<()> {
        (**self).reset()
    }
    fn name(&self) -> String {
        (**self).name()
    }
}

/// Espressif's USB vendor ID; the S3's USB Serial/JTAG is product 0x1001.
const ESPRESSIF_VID: u16 = 0x303a;
/// USB Serial/JTAG ignores the baud rate.
const BAUD: u32 = 115_200;
/// How long a port that went away (the chip's USB re-enumerating after reset) may take to return.
const REOPEN: Duration = Duration::from_secs(5);

/// The only Espressif USB serial port attached; an error listing the ports if none or several.
pub fn find_port() -> Result<String> {
    let ports = serialport::available_ports().context("listing serial ports")?;
    let esp: Vec<&str> = ports
        .iter()
        .filter(|p| matches!(&p.port_type, SerialPortType::UsbPort(u) if u.vid == ESPRESSIF_VID))
        .map(|p| p.port_name.as_str())
        .collect();
    let usb = || {
        let list: Vec<String> = ports
            .iter()
            .filter_map(|p| match &p.port_type {
                SerialPortType::UsbPort(u) => Some(format!("{} ({:04x}:{:04x} {})", p.port_name, u.vid, u.pid, u.product.as_deref().unwrap_or("?"))),
                _ => None,
            })
            .collect();
        if list.is_empty() { "none".to_string() } else { list.join(", ") }
    };
    match esp.as_slice() {
        [one] => Ok(one.to_string()),
        [] => bail!("no Espressif USB port (VID 303a) found; plug the board in or pass --port. USB serial ports: {}", usb()),
        _ => bail!("several Espressif USB ports ({}); pass --port", esp.join(", ")),
    }
}

/// The board's USB Serial/JTAG console. UNVERIFIED throughout: no board until Phase D.5.
pub struct SerialLink {
    path: String,
    /// The port was auto-detected, so a reopen detects it again (it may come back renamed).
    auto: bool,
    port: Option<TTYPort>,
    buf: Vec<u8>,
}

impl SerialLink {
    /// Opens `port`, or the only Espressif port attached.
    pub fn open(port: Option<&str>) -> Result<SerialLink> {
        let (path, auto) = match port {
            Some(p) => (p.to_string(), false),
            None => (find_port()?, true),
        };
        let mut link = SerialLink { path, auto, port: None, buf: vec![] };
        link.port = Some(link.connect()?);
        Ok(link)
    }

    fn connect(&self) -> Result<TTYPort> {
        let mut p = serialport::new(&self.path, BAUD)
            .timeout(Duration::from_millis(50))
            .open_native()
            .with_context(|| format!("opening {}", self.path))?;
        // Linux asserts DTR and RTS on open. The USB Serial/JTAG reads RTS as reset and DTR low
        // during a reset as "download mode", so both go low at once (UNVERIFIED on hardware).
        p.write_data_terminal_ready(false)?;
        p.write_request_to_send(false)?;
        Ok(p)
    }

    /// Reopens the port after it went away, for up to `REOPEN`.
    fn reconnect(&mut self) -> Result<()> {
        self.port = None;
        let until = Instant::now() + REOPEN;
        loop {
            if self.auto
                && let Ok(p) = find_port()
            {
                self.path = p;
            }
            match self.connect() {
                Ok(p) => {
                    self.port = Some(p);
                    return Ok(());
                }
                Err(e) if Instant::now() >= until => return Err(e.context(format!("{} did not come back within {} s", self.path, REOPEN.as_secs()))),
                Err(_) => sleep(Duration::from_millis(200)),
            }
        }
    }
}

impl Link for SerialLink {
    fn next_line(&mut self, timeout: Duration) -> Result<Option<String>> {
        let until = Instant::now() + timeout;
        loop {
            if let Some(i) = self.buf.iter().position(|&b| b == b'\n') {
                let raw: Vec<u8> = self.buf.drain(..=i).collect();
                return Ok(Some(String::from_utf8_lossy(&raw).trim_end_matches(['\r', '\n']).to_string()));
            }
            if Instant::now() >= until {
                return Ok(None);
            }
            let mut chunk = [0u8; 512];
            let read = match self.port.as_mut() {
                Some(p) => p.read(&mut chunk),
                None => Err(ErrorKind::NotConnected.into()),
            };
            match read {
                Ok(n) if n > 0 => self.buf.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == ErrorKind::TimedOut => {}
                // a read of 0 or an I/O error: the device went away (UNVERIFIED which one Linux gives)
                _ => self.reconnect()?,
            }
        }
    }

    fn send(&mut self, line: &str, _shown: &str) -> Result<()> {
        if self.port.is_none() {
            self.reconnect()?;
        }
        let mut bytes = Zeroizing::new(Vec::with_capacity(line.len() + 1));
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        let p = self.port.as_mut().unwrap();
        // the io error never carries the data written
        p.write_all(&bytes).and_then(|_| p.flush()).with_context(|| format!("writing to {}", self.path))
    }

    /// Like esptool's hard reset over USB Serial/JTAG: RTS high for 200 ms with DTR low.
    fn reset(&mut self) -> Result<()> {
        if self.port.is_none() {
            self.reconnect()?;
        }
        let p = self.port.as_mut().unwrap();
        p.write_data_terminal_ready(false)?;
        p.write_request_to_send(true)?;
        sleep(Duration::from_millis(200));
        // what arrived before the reset is not this boot's; a failure means the port went away
        let released = p.write_request_to_send(false);
        sleep(Duration::from_millis(200));
        if released.is_err() || p.clear(ClearBuffer::Input).is_err() {
            self.reconnect()?;
        }
        self.buf.clear();
        Ok(())
    }

    fn name(&self) -> String {
        self.path.clone()
    }
}

/// The firmware in Espressif's QEMU, booted from a private copy of the board's
/// `firmware/build-qemu/sim_flash.bin` (so NVS writes never reach the sim stage's image; the
/// copy is deleted at the end). It boots on first use; a reset restarts QEMU on the same copy.
pub struct QemuLink {
    qemu: PathBuf,
    image: PathBuf,
    console: Option<Console>,
    /// Console logs of the boots before the last reset.
    past: Vec<String>,
}

impl QemuLink {
    /// `dir` is the board's directory.
    pub fn open(dir: &Path) -> Result<QemuLink> {
        let src = dir.join("firmware/build-qemu/sim_flash.bin");
        if !src.exists() {
            let board = dir.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
            bail!("no QEMU image at {}: run `cargo run --release -p {board} -- sim` first", src.display());
        }
        let qemu = pcbgen::sim::qemu_binary()?;
        static N: AtomicUsize = AtomicUsize::new(0);
        let image = std::env::temp_dir().join(format!("devctl-{}-{}.bin", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::copy(&src, &image).with_context(|| format!("copying {}", src.display()))?;
        Ok(QemuLink { qemu, image, console: None, past: vec![] })
    }

    /// Everything QEMU's console logged, over every boot.
    pub fn console_log(&self) -> Vec<String> {
        let now = self.console.iter().flat_map(|c| c.log.iter().cloned());
        self.past.iter().cloned().chain(now).collect()
    }

    fn console(&mut self) -> Result<&mut Console> {
        if self.console.is_none() {
            self.console = Some(Console::start(&self.qemu, &self.image)?);
        }
        Ok(self.console.as_mut().unwrap())
    }
}

impl Link for QemuLink {
    fn next_line(&mut self, timeout: Duration) -> Result<Option<String>> {
        self.console()?.next_line(timeout)
    }

    fn send(&mut self, line: &str, shown: &str) -> Result<()> {
        self.console()?.send_redacted(line, shown)
    }

    fn reset(&mut self) -> Result<()> {
        if let Some(c) = self.console.take() {
            self.past.extend(c.log.iter().cloned());
            self.past.push("== reset".into());
        } // dropping it kills QEMU
        self.console()?;
        Ok(())
    }

    fn name(&self) -> String {
        "qemu".into()
    }
}

impl Drop for QemuLink {
    fn drop(&mut self) {
        self.console = None;
        let _ = std::fs::remove_file(&self.image);
    }
}
