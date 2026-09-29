# Open-source ESP32-S3 simulation vs Wokwi (2026-09-29)

Asked by the owner: "50 minutes sound like nothing, are you sure Wokwi is the best one? no
open-source method?" Research by a read-only agent; each fact VERIFIED (source) or UNVERIFIED.
Nothing was installed.

## 1. Espressif's QEMU fork (GPL)
- VERIFIED: ESP-IDF v6.1 already knows it. `tools/tools.json` has `qemu-xtensa` for esp32 and
  esp32s3, version `esp-develop-9.2.2-20260417`. `idf.py qemu monitor` / `idf.py qemu gdb` work for
  the S3 (`tools/idf_py_actions/qemu_ext.py`, `docs/en/api-guides/tools/qemu.rst`).
- VERIFIED: that is the latest release (2026-04-17: SD card emulation, missing S3 vector
  instructions). The 2025-02 release added octal PSRAM and a virtual RGB display
  (github.com/espressif/qemu/releases).
- VERIFIED: not installed (nothing in `~/.espressif/tools` or on PATH). Install:
  `python $IDF_PATH/tools/idf_tools.py install qemu-xtensa`.
- VERIFIED: `libslirp` (user-mode networking) is missing on this laptop (`pacman -Q`); libgcrypt,
  pixman, sdl2-compat and glib2 are present. `pytest-embedded-qemu` is not in the IDF venv.
- VERIFIED, S3 supported: both cores, UART, interrupt matrix, GPIO strapping only, flash (2–16 MB)
  and flash encryption, octal PSRAM (up to 32 MB; `idf.py` sets octal from
  `CONFIG_SPIRAM_MODE_OCT`), eFuse, RNG, DMA, AES/SHA/RSA/HMAC/DS, SysTimer, timer groups, CAN,
  OpenCores Ethernet, a virtual RGB framebuffer
  (github.com/espressif/esp-toolchain-docs/blob/main/qemu/README.md and `esp32s3/README.md`).
- VERIFIED, not supported: USB (so no USB Serial/JTAG console), Wi-Fi, BT, RMT, LEDC, I2S, I2C,
  general SPI, the GPIO matrix/IOMUX (no button or LED I/O), Secure Boot, the RTC watchdog.
- UNVERIFIED: ADC and deep sleep aren't mentioned; assume they don't work.
- VERIFIED: networking is OpenCores Ethernet with NAT (`-nic user,model=open_eth`), which needs
  `CONFIG_ETH_USE_OPENETH`, a QEMU-only driver (`components/esp_eth/Kconfig`). HTTPS to real APIs
  should then work (UNVERIFIED on the S3).
- UNVERIFIED: instruction-accurate, not cycle-accurate: Opus encoding runs correctly but gives no
  real timing.

## 2. ESP-IDF `linux` host target (experimental)
- VERIFIED (`docs/en/api-guides/host-apps.rst`): runs on the host: FreeRTOS (POSIX simulator),
  esp_event, esp_http_client/server, esp_https_server, esp_netif, esp_partition, esp_tls, mbedtls,
  lwip, nvs_flash, fatfs, spiffs, mqtt, json, log, heap, pthread. Mock-only: driver, esp_timer,
  spi_flash, tcp_transport.
- VERIFIED: single core; `printf` from several tasks can crash; blocking calls like `select()`
  confuse the scheduler (same doc).
- VERIFIED: the `esp_http_client` and `https_mbedtls` examples list Linux as a target. `esp_wifi`
  builds only a `remote` stub on linux.
- UNVERIFIED: esp_littlefs (joltwallet) doesn't document host support; test LittleFS in QEMU.

## 3. Renode
- VERIFIED: no official ESP32-S3 support. Community projects only (`mithro/renode-espemu` for the
  C3, `xobs/renode-esp32s3`), both immature. The official supported-boards list was not checked.

## 4. Wokwi
- VERIFIED: the free CI quota is 50 minutes a month of simulated time, summed over all tests;
  Hobby and Hobby+ 200, Pro 2,000 (docs.wokwi.com/wokwi-ci/getting-started). Prices: Community €0,
  Hobby €5.6, Hobby+ €8.1, Pro €20 per seat (wokwi.com/pricing).
- VERIFIED: Wokwi for VS Code needs a licence (a free account can create one) and internet unless
  on Pro (docs.wokwi.com/vscode/getting-started, /vscode/offline-mode). Not an open-source local
  option. UNVERIFIED: whether VS Code use counts against the CI minutes.
- From `research/2026-09-29-wokwi.md`: Wokwi simulates GPIO, ADC, LEDC and Wi-Fi on the S3, but not
  I2S or deep sleep.

## Conclusion
No single open-source simulator covers the S3's peripherals. Use four layers:
1. the `linux` target and plain gcc for the logic, unlimited
2. QEMU for the whole image (boot, storage, PSRAM, encoder), unlimited
3. Wokwi for the few GPIO/ADC checks QEMU can't do, within the free 50 minutes
4. the real board (and the owner's dev board once) for I2S, Wi-Fi, USB, deep sleep and timing

`docs/plan.md` has the details.
