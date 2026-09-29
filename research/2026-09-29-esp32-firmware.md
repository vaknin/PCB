# capture-clip firmware research: ESP32-S3 sleep, pins, audio, storage, HTTPS, secrets, OTA (2026-09-29)

Device: ESP32-S3-WROOM-1-N16R8 on battery, deep sleep between uses, ICS-43434 I2S mic, RGB LED
(anode on VSYS), 1 MΩ/1 MΩ battery divider, TP4057 charger, RT9080 LDO. ESP-IDF v6.1.
It is a third Capture client writing `source: clip` (Capture SPEC §3, §6, §9).

Nothing was installed. No secrets were read.

**Tags.**
- **VERIFIED** means read on 2026-09-29 in one of:
  - an Espressif PDF: ESP32-S3 Series Datasheet **v2.2** ("DS"), WROOM-1/1U Datasheet **v1.8** ("WROOM"), TRM **v1.8** ("TRM");
  - a file under `~/esp/esp-idf-v6.1` ("IDF:");
  - a registry or web page, cited by URL.
  - Page numbers are PDF pages.
- **INFERRED** / **UNVERIFIED** is my reasoning, or something not checked.

Two helper agents covered the registry and web lookups (esp_audio_codec, esp_muxer, littlefs, Gemini, GitHub, TLS roots). Their findings are cited below with their sources.

---

## 1. Sleep current, wake, GPIOs

### Deep-sleep current
- **Chip, DS Table 5-10 p.68 (VERIFIED), typical, 3.3 V, 25 °C:**
  - RTC memory on, RTC peripherals off: **7 µA**
  - RTC memory and RTC peripherals on: **8 µA**
  - chip off (CHIP_PU low): 1 µA
  - no maximum is given
- **Module, WROOM Table 6-7 p.30 (VERIFIED):** the same 7 / 8 µA.
  - The table's text is identical to the chip's. Whether Espressif measured it on the module (flash and PSRAM included) is **INFERRED** at best.
- **Flash and PSRAM draw nothing in deep sleep.**
  - VERIFIED: "`esp_deep_sleep_start` function forces power down SPI Flash regardless of user configuration" (IDF: `docs/en/api-reference/system/sleep_modes.rst` l.543).
  - VERIFIED: on the N16R8 both memories sit on VDD_SPI (DS p.17 pin table; WROOM p.12 note b).
  - The PSRAM adder of **140 µA (8 MB octal, 3.3 V)** applies to *light* sleep only (DS Table 5-10 footnote 1, VERIFIED).
  - **So light sleep (240 µA + 140 µA) can never meet R2. Every idle state must be deep sleep** (INFERRED).
- **Whole-board estimate** (INFERRED): module 7–8 µA typ plus the rest of the board 6–10 µA (parts research) gives **13–18 µA typ**.
  - That meets the 20 µA target only on typical figures.
  - Measure it at bring-up.

### ext0 vs ext1 on GPIO0
- **GPIO0 is RTC_GPIO0** (WROOM Table 3-1 p.11, pad 27, VERIFIED). The S3 has 22 RTC IOs, GPIO0–21 (IDF: `soc/esp32s3/include/soc/soc_caps.h` `SOC_RTCIO_PIN_COUNT 22`, VERIFIED).
- **ext0** (one pin, one level) keeps the RTC-peripheral domain powered, so it runs at the **8 µA** row (IDF sleep_modes.rst l.205, VERIFIED).
- **ext1** works with RTC peripherals powered down.
  - IDF holds the pin's pull-up or pull-down through sleep by itself, so it runs at the **7 µA** row (sleep_modes.rst l.238–247, VERIFIED).
  - On the S3, ext1 is ANY_HIGH or ANY_LOW, the same level for all its pins. There is no per-pin level (l.235–266; S3 lacks `SOC_PM_SUPPORT_EXT1_WAKEUP_MODE_PER_PIN`, VERIFIED).
- **Pick: the button on ext1 ANY_LOW.**
  - If waking when USB is plugged in is wanted, VBUS-sense goes on **ext0, level high**. That costs about +1 µA typ, because ext0 powers the RTC peripherals (INFERRED from the 7 vs 8 µA rows).
  - Recommended: waking on plug-in lets the queue upload while charging.

### GPIO0 as a strapping pin
- It is **sampled only at Chip Reset**: power-on, brown-out, or super-watchdog (TRM §8.1 p.534, Table 7.1-1 fn.1 p.528, VERIFIED).
- **A deep-sleep wake is a Core Reset** (TRM Table 7.1-1 p.528, code 0x05, VERIFIED).
  - **So holding the button while the chip wakes does not enter download mode.**
  - Download mode needs button held + reset button, or a brown-out while the button is held. Rare and harmless.
- **Download mode:** GPIO0 = 0 with GPIO46 = 0 (the default) selects Joint Download (WROOM Table 4-3 p.14, VERIFIED).
  - Normal flashing never needs the button: "The USB Serial/JTAG Controller is able to put the ESP32-S3 into download mode automatically" (IDF: `docs/en/api-guides/usb-serial-jtag-console.rst` l.59, VERIFIED).
- **Strap hold time:** 3 ms after EN rises (WROOM Table 4-2 p.14, VERIFIED).
  - An external 10 kΩ pull-up to 3V3 with no capacitor (or ≤ 10 nF) keeps any RC far under that (INFERRED).
  - The internal pull-up is about 45 kΩ (DS Table 5-4 p.65, VERIFIED).
  - The external pull-up only draws current while the button is held (330 µA at 10 kΩ, INFERRED).

### Holding pins in deep sleep
- VERIFIED from IDF: `esp_driver_gpio/include/driver/gpio.h` l.420–447 and `src/gpio.c` l.778–796:
  - On an **RTC GPIO**, `gpio_hold_en()` calls `rtc_gpio_hold_en()`, and the level survives deep sleep and the wake reset until `gpio_hold_dis()`.
  - On a **digital-only** GPIO (≥ 22) of the S3, `gpio_hold_en` alone does *not* survive deep sleep. You must also call `gpio_deep_sleep_hold_en()`, and the pin resets to default on wake.
- **So put the LED cathodes, mic power and I2S lines on RTC GPIOs**, and `gpio_hold_en` them before sleep:
  - LED cathodes held high (LED off)
  - mic power held low
  - SCK/WS held low
- Before re-driving them after wake, set the same level first, then `gpio_hold_dis` (gpio.h l.452–458, VERIFIED).

### Pins
- **Strapping:** GPIO0, GPIO3, GPIO45, GPIO46 (TRM §8.1, WROOM Table 4-1 p.13, VERIFIED).
- **Octal PSRAM** takes GPIO35/36/37 on the N16R8: "not available for other uses" (WROOM Table 3-1 note b p.12, VERIFIED).
  - GPIO33/34 are not brought out on WROOM-1 at all (VERIFIED, absent from Table 3-1).
- **Other pins to avoid:**
  - GPIO19/20 are USB.
  - GPIO43/44 are UART0.
  - GPIO39–42 are pad-JTAG pins.
- **ADC1** is GPIO1–10. **ADC2** is shared with Wi-Fi on the S3 (IDF: `docs/en/api-reference/peripherals/adc/adc_oneshot.rst` l.165, VERIFIED), so the battery goes on ADC1.

### GPIO source current
- **IOH = 40 mA typ** at VOH ≥ 2.64 V with drive strength 3 (DS Table 5-4 p.65, VERIFIED). Cumulative IO output current is 1500 mA max (DS Table 5-1).
- **The mic draws 230 µA typ / 300 max at 16 kHz** (low-power mode, parts research: ICS-43434 DS pp.4–5) and 550 µA max in high-performance mode.
  - A GPIO powers it directly with a few mV of drop (INFERRED).
  - **No P-FET is needed.**

### Recommended pin table

All pins below are RTC-capable and none is a strapping pin, except the button, which is on GPIO0 on purpose. Pad numbers are from WROOM Table 3-1 (VERIFIED). Layout can swap pins within the spare RTC set.

| Signal | GPIO | Module pad | Why |
|---|---|---|---|
| BUTTON_N (to GND, 10 kΩ pull-up) | 0 | 27 | RTC wake (ext1 ANY_LOW); doubles as BOOT |
| BAT_ADC | 1 | 39 | ADC1_CH0 |
| VBUS_SENSE (divider from VBUS) | 2 | 38 | RTC wake (ext0 high); ADC1_CH1 if an analog read is wanted |
| CHRG_N (TP4057 pin 1) | 4 | 4 | input; see the leakage note in §7 |
| STDBY_N (TP4057 pin 5) | 5 | 5 | input |
| MIC_PWR (feeds the mic VDD, 0.1 µF at the mic) | 6 | 6 | held low in sleep |
| LED_R_N / LED_G_N / LED_B_N (through resistors) | 8 / 9 / 10 | 12 / 17 / 18 | held high in sleep |
| I2S_SCK / I2S_WS / I2S_SD | 11 / 12 / 13 | 19 / 20 / 21 | SCK/WS held low in sleep; SD has the 100 kΩ pull-down |

- **Spare:** GPIO7, 14, 15, 16 (XTAL_32K pins, free when no 32 kHz crystal is fitted), 17, 18, 21, 38, 47, 48.
- **Avoid:** 3, 45, 46, 19, 20, 35–37, 39–42. GPIO43/44 can go to test pads for UART0.

---

## 2. Audio

### `espressif/esp_audio_codec`
- **Version:** latest **2.6.2**, published 2026-08-07 (registry API `components.espressif.com/api/components/espressif/esp_audio_codec`, VERIFIED).
- **License:** "Espressif Modified MIT", to be used only with Espressif chips. It is a **closed prebuilt library**: `lib/esp32s3/libesp_audio_codec.a`, 4.2 MB (published zip, VERIFIED). Fine for this device.
- **ESP32-S3** is supported (VERIFIED).
- **IDF:** `idf_component.yml` requires `idf >=4.4` with no upper bound, and the 2.5.0 changelog says "Supported build for IDFv6.0" (VERIFIED).
  - v6.1 was not named, so a **test build is needed** (INFERRED).
- **Encoders:** AAC-LC, **Opus**, AMR-NB/WB, ADPCM, G711, PCM, ALAC, LC3, SBC, G722 (README, VERIFIED).
  - Opus config fields: sample rate 8–48 k, mono, 16-bit input, bitrate, frame 2.5–120 ms, VOIP or audio mode, complexity 0–10, FEC, DTX, VBR (`esp_opus_enc.h`, VERIFIED).
  - AAC has `adts_used`, default true, so it writes **ADTS**. At 16 kHz mono it takes 22–96 kbps (`esp_aac_enc.h`, VERIFIED).
- **Opus output is raw packets, not Ogg** (INFERRED, strongly: the header never mentions Ogg).
- **Published performance** is for 48 kHz stereo only, on ESP32-S3R8 (README, VERIFIED):
  - Opus encoder, 90 kbps, complexity 0: 29.4 KB heap, **24.9 % CPU**
  - AAC encoder, 90 kbps: 51.4 KB heap, 12.9 % CPU
  - encoder task stack about 40 KB
  - There is **no 16 kHz mono figure.** At 16 kHz mono, 32 kbps, it should be well under 10 % of one core (INFERRED from 1/6 the samples). **Measure it.**

### Ogg muxing
- **`espressif/esp_muxer` 1.2.3** (2026-08-12, same license, VERIFIED) supports **OGG carrying Opus only**, plus MP4, TS, FLV, WAV, CAF and AVI.
  - Output is a file-path pattern or a streaming `data_cb`, and `ogg_muxer_config_t.page_cache_size` groups packets into pages (`esp_muxer.h`, `ogg_muxer.h`, VERIFIED).
- **Plan:** use `data_cb` and write to our own file, so we control `fsync`.
- **Fallback:** a hand-written Ogg-Opus muxer (RFC 7845 OpusHead/OpusTags, RFC 3533 pages with CRC-32 poly 0x04C11DB7, 48 kHz granule, pre-skip) is **about 150–250 lines of C** (INFERRED).
- **Page grouping matters for size:**
  - One 80-byte packet per page adds 27+1 bytes each, about 35 % overhead.
  - About 50 packets (1 s) per page adds about 2 %.
  - (INFERRED arithmetic)

### Pick: OGG/Opus, 16 kHz mono, 32 kbps, 20 ms frames, VOIP mode
- It is the phone's primary format (Capture SPEC §3) and proven with Gemini.
- Gemini lists `audio/ogg`, `audio/opus` and `audio/aac`, and down-samples audio to 16 kbps internally (https://ai.google.dev/gemini-api/docs/audio, updated 2026-09-23, VERIFIED). So 32 kbps is more than enough.
- **Size:** 32 kbps is 4.0 KB/s, 3.6 MB per 15 min, plus about 2 % Ogg overhead (INFERRED).
- **Crash safety:** Ogg is page-streamable. A file cut after the last complete page decodes (INFERRED; the phone relies on the same property, SPEC §3).
  - Flush a page about every 1 s.
- **Fallback:** AAC-LC ADTS at 32 kbps from the same component.
  - It needs no muxer, since every ADTS frame is self-delimiting, so it is crash-safe by construction.
  - About half the Opus CPU.
  - It is Capture's own fallback (`audio/aac`).
  - Switch to it only if Opus CPU or build problems show up.

### I2S RX for the ICS-43434
- **The mic supports 16 kHz directly** (VERIFIED, ICS-43434 DS-000069 rev 1.2):
  - low-power mode covers fS 6.25–18.75 kHz, and Table 3 is specified at fS = 16 kHz (SNR 64 dBA)
  - SCK period 303–2500 ns, i.e. 0.4–3.3 MHz; 16 kHz × 64 = 1.024 MHz is inside it
  - wake-up ≤ 20 ms after power-on
  - (Table 5, p.8)
- **Config (IDF: `esp_driver_i2s/include/driver/i2s_std.h`, VERIFIED):**
  - `I2S_STD_CLK_DEFAULT_CONFIG(16000)`
  - `I2S_STD_PHILIPS_SLOT_DEFAULT_CONFIG(I2S_DATA_BIT_WIDTH_32BIT, I2S_SLOT_MODE_MONO)`, which gives 64 SCK per frame
  - **then set `slot_cfg.slot_mask = I2S_STD_SLOT_LEFT`**, because on the S3 the default macro sets `I2S_STD_SLOT_BOTH` even in mono (l.140–151)
  - LR tied to GND = left
- **Samples:** read `int32_t`; the 24-bit data is in the top bits (i2s.rst l.632–648, VERIFIED).
  - 16-bit = `s >> 16`, which is unity gain.
  - The mic gives −26 dBFS at 94 dB SPL, so speech at arm's length lands around −55 to −60 dBFS (INFERRED).
  - Use `>> 13` or `>> 14` (+12 to +18 dB) with saturation, or a simple AGC, and tune at bring-up (INFERRED).
  - Drop the first ~50 ms after power-on (settling).
- **DMA sizing** (i2s.rst l.1323–1349, VERIFIED formulas: buffer = frames × slots × bytes ≤ 4092):
  - `dma_frame_num = 320` (20 ms = one Opus frame, 1280 B)
  - `dma_desc_num = 16` (320 ms, about 20 KB of internal RAM)
  - then a PSRAM ring buffer to the encoder and writer tasks
- **Flash writes stall the CPU:** a 4 KB sector erase takes **70 ms typ, 500 ms max** (DS Table 5-11 p.68, VERIFIED), and code that isn't in IRAM stops while flash is written.
  - **Enable `CONFIG_SPIRAM_XIP_FROM_PSRAM`**: code runs from PSRAM, so it keeps running during flash operations (IDF: `esp_psram/esp32s3/Kconfig.spiram` l.49–68, VERIFIED).
  - Alternatively, raise `dma_desc_num` to cover 500 ms.

---

## 3. Storage
- **The IDF v6.1 file-system guide** (`docs/en/api-guides/file-system-considerations.rst`, VERIFIED):
  - FatFS has "low resilience against sudden power-off"
  - SPIFFS is "not being developed and maintained anymore"
  - LittleFS has power-failure protection and is "a recommended choice"
  - FAT with wear levelling in Performance mode can lose a sector on a power cut during an erase (`fatfs.rst` l.302, VERIFIED)
- **`joltwallet/littlefs` 1.22.3** (2026-07-21): MIT, bundles littlefs 2.11, requires `idf >=5.0`, and its README covers IDF v6 (registry and zip, VERIFIED).
  - It is copy-on-write: file data is **committed only at `fsync`/`close`** (upstream DESIGN.md, VERIFIED).
  - Appends are O(1) amortised (VERIFIED).
- **A raw partition used as a ring** would be simplest for the flash, but it needs our own index, recovery code and per-recording metadata (INFERRED). LittleFS gives these for free.
- **Pick: LittleFS.**
  - One file per recording, `q/<id>.ogg`, plus a small `q/<id>.json` sidecar: created, duration, kind = new or addition, target note, attempt count.
  - `fsync` every ~1–2 s (about one Ogg page).
  - A power cut loses at most the last ~2 s. What was synced is a valid, truncated Ogg (INFERRED).
  - On boot, a recording left open is finalised and queued, like the phone's recovery rule "never discard readable audio" (SPEC §3).
  - Keep ≥ 10 % of the partition free (INFERRED; littlefs slows when full).
- **Partition table for 16 MB** (INFERRED sizes). The bootloader is at 0x0, the table at 0x8000, and app partitions are 64 KB aligned.

```
# Name,    Type, SubType,  Offset,   Size
nvs,       data, nvs,      0x9000,   0x6000     # 24 KB: Wi-Fi, secrets, counters (encrypted, §5)
otadata,   data, ota,      0xF000,   0x2000
phy_init,  data, phy,      0x11000,  0x1000
nvs_keys,  data, nvs_keys, 0x12000,  0x1000, encrypted   # reserved; used only by the flash-encryption NVS scheme
ota_0,     app,  ota_0,    0x20000,  0x300000   # 3 MB
ota_1,     app,  ota_1,    0x320000, 0x300000   # 3 MB
storage,   data, littlefs, 0x620000, 0x9D0000   # 9.81 MiB
coredump,  data, coredump, 0xFF0000, 0x10000    # 64 KB, for field crashes
```

- **App size** is about 1.0–1.5 MB (Wi-Fi, TLS, HTTP, OTA, Opus) (INFERRED). 3 MB slots leave room to grow.
  - IDF's own `partitions_two_ota.csv` uses only 1 MB per slot, which is tight for this app (VERIFIED, file read).
- **Storage:** 9.81 MiB at about 4.08 KB/s is about 42 min raw, or **about 37 min with 10 % free**. That meets R5 (≥ 30 min) (INFERRED arithmetic).
  - 2.5 MB slots would give about 46 min, if more is ever needed.

---

## 4. HTTPS (Gemini and GitHub)

### Streaming the Gemini request
- **The upload streams from flash** (IDF: `esp_http_client/include/esp_http_client.h` l.664–745 and example `examples/protocols/esp_http_client`, `http_native_request()` l.752, VERIFIED):
  1. `esp_http_client_open(c, content_length)`
  2. `esp_http_client_write()` in chunks
  3. `esp_http_client_fetch_headers()`
  4. `esp_http_client_read()` in a loop
- **The body is built on the fly:**
  1. the JSON prefix (system instruction, schema, "Process this recording.", or the addition text per SPEC §9)
  2. then the audio read from LittleFS in 3·k-byte blocks and base64-encoded to 4·k bytes each
  3. then the JSON suffix
- **Content-Length** = prefix + 4·⌈n/3⌉ + suffix, known in advance. So no chunked encoding is needed, and nothing larger than one block sits in RAM.
- **Response:**
  - `timeout_ms` applies per socket operation (`esp_http_client.c` l.1962–1971, VERIFIED).
  - Set it to about 120–180 s before `fetch_headers`, because thinking-level "high" can take tens of seconds (INFERRED).
  - Read the (usually chunked) response into a PSRAM buffer (INFERRED).
- **Request size limit:** the Gemini docs disagree.
  - The audio page says "Maximum request size is 20 MB" (https://ai.google.dev/gemini-api/docs/audio, VERIFIED).
  - The file-input page says inline data is "100 MB per request" (https://ai.google.dev/gemini-api/docs/file-input-methods, VERIFIED).
  - Audio is limited to 9.5 h per prompt (VERIFIED).
  - A 15-min body is about 4.8 MB, under both limits. Design to 20 MB.
  - The Files API (2 GB, kept 48 h) is a fallback if that ever changes (VERIFIED, same page).
- **Quota is shared:** the device reuses Capture's key (D-024), so it **shares the 15-per-minute and 500-per-day free-tier quota** with the phone and laptop (SPEC §6, VERIFIED).
  - Firmware must copy SPEC §6's rules:
    - one request at a time
    - ≥ 5 s apart
    - honour the wait a 429 names, or 60 s
    - after a daily-quota 429, wait until 10:00 Israel time
    - which statuses are terminal and which retry
- **Upload time and energy** (INFERRED): 4.8 MB at 0.5–2 MB/s through TLS is about 3–10 s of Wi-Fi TX, well under 1 mAh per note.

### TLS
- **Version:** IDF v6.1 ships **Mbed TLS 4.1.0 with TF-PSA-Crypto 1.1.0** (`components/mbedtls/.../build_info.h`, VERIFIED).
- **Heap:** about 42 KB per session by default, 22 KB with dynamic buffers (IDF `mbedtls.rst`, "Reducing Heap Usage", VERIFIED).
  - With 8 MB PSRAM, set `CONFIG_MBEDTLS_EXTERNAL_MEM_ALLOC=y` and keep the 16 KB input record (Kconfig l.160–260, VERIFIED option names).
  - Sessions run one at a time.
- **Certificate bundle:** `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE` is on by default; attach it with `.crt_bundle_attach = esp_crt_bundle_attach`.
  - Live chains, checked 2026-09-29 with `openssl s_client`:
    - generativelanguage.googleapis.com → **GTS Root R1**
    - api.github.com → **Sectigo Public Server Authentication Root E46** (cross-signed by USERTrust ECC)
    - GitHub release assets → **ISRG Root X1**
  - All three roots are in IDF's `esp_crt_bundle/cacrt_all.pem` and in the common subset (VERIFIED).

### GitHub REST contents API
- **PUT** `/repos/{o}/{r}/contents/{path}` takes `message`, base64 `content`, and `sha` when replacing a file.
  - Files ≤ 1 MB are fully supported (https://docs.github.com/en/rest/repos/contents, VERIFIED). Notes are a few KB.
- **Headers:**
  - `Authorization: Bearer`
  - `Accept: application/vnd.github+json`
  - `X-GitHub-Api-Version: 2026-03-10`
  - **User-Agent is mandatory** (VERIFIED, GitHub REST docs). Set it to `capture-clip/<ver>`; IDF's default "ESP32 HTTP Client/1.0" also passes.
- **Limits:** 5000 requests per hour; secondary limits of **80 content writes per minute and 500 per hour** (VERIFIED). A note costs about 3 writes (counter GET/PUT, note PUT).
- **Per note**, the device repeats SPEC §9:
  - the `next-number` reserve loop (GET, then PUT with sha; 409/422 → retry, 5 tries)
  - storing the number locally before the create
  - creating `notes/<id>.md` with `source: clip`
- **Additions** (a hold):
  1. GET the last note by its id.
  2. Append a `## Added` section with a new 32-hex id.
  3. PUT it with the sha.
  4. On a 409, refetch and merge (SPEC §9 Additions).
- **"The last note"** means the last note this device created, kept in NVS: its id, and its sha if known. If the file is gone (ticked off elsewhere), the addition becomes a new note (INFERRED; the policy needs a D-entry).

---

## 5. Secrets and provisioning
- **NVS encryption has two schemes on the S3** (IDF: `docs/en/api-reference/storage/nvs_encryption.rst`, VERIFIED):
  - **Flash-encryption-based:** keys live in an `nvs_keys` partition protected by flash encryption. It *requires* flash encryption (l.19–39).
  - **HMAC-based:** XTS keys are derived at run time from an eFuse HMAC key (purpose `HMAC_UP`), which is generated and burned on first boot if absent. "This scheme enables us to achieve secure storage … **without enabling flash encryption**" (l.110–131).
    - Set `CONFIG_NVS_ENCRYPTION`, `CONFIG_NVS_SEC_KEY_PROTECT_USING_HMAC` and `CONFIG_NVS_SEC_HMAC_EFUSE_KEY_ID` (0–5; the default −1 must be changed).
    - It uses one eFuse key block, permanently.
- **Flash encryption modes** (IDF: `docs/en/security/flash-encryption.rst`, VERIFIED):
  - **Development mode:**
    - Plaintext images can still be re-flashed only through `idf.py encrypted-flash`.
    - On the S3 it also burns `HARD_DIS_JTAG`, `DIS_DOWNLOAD_ICACHE/DCACHE` and `DIS_LEGACY_SPI_BOOT`, so USB-JTAG debugging is lost (l.287–309).
    - It can be turned off **only once** per non-ESP32 chip (l.1025).
  - **Release mode:**
    - Download mode can no longer encrypt, so updates come only by OTA, or pre-encrypted with a host copy of the key.
    - ROM download is set to Secure Download Mode by default (l.719, l.794).
- **Secure boot:** a signing key to guard, a bootloader that can't be re-flashed, and brick risk.
- **Recommendation for a hobby device the owner re-flashes over USB** (INFERRED):
  - **HMAC-based NVS encryption, no flash encryption, no secure boot.**
  - A flash dump from a lost device shows no plaintext key, token or Wi-Fi password (Wi-Fi credentials are in the same encrypted NVS).
  - It does *not* stop someone who flashes their own firmware and uses the device's HMAC peripheral to derive the keys (INFERRED from the `HMAC_UP` purpose, which lets software on the chip compute with the key).
  - The real control stays D-024's: revoke the key and token.
  - `idf.py flash` keeps working. `erase-flash` wipes the secrets, and the setup step re-sends them.
- **Getting secrets in over USB:**
  - USB Serial/JTAG **disappears in deep sleep** and reconnects on wake (usb-serial-jtag-console.rst l.83, l.101–107, VERIFIED).
  - On the S3 it also stops responding in **light sleep**, and the host may need a replug (l.112–117; the S3 lacks `SOC_USB_SERIAL_JTAG_SUPPORT_LIGHT_SLEEP`, VERIFIED).
  - So: **while VBUS is present the firmware stays awake, with no light sleep, and serves the console.** On battery it deep-sleeps.
  - Protocol: a small line protocol read with `usb_serial_jtag_read_bytes()`, or `esp_console_new_repl_usb_serial_jtag()` with commands (`esp_console.h` l.411, VERIFIED).
    - Do not echo values (a REPL echoes as you type).
    - Send `set <name> <base64>`.
    - Reply `ok <name> <first 8 hex of sha256>` so the laptop can check without the value coming back.
    - Also: `get-info`, `wifi-add`, `test` (self-test), `queue`, `log`.
  - The default command-line buffer is 256 B (`esp_console.h` l.39, VERIFIED). That is enough for a GitHub fine-grained token (about 93 characters, INFERRED), even base64-encoded.
  - Never log secrets.

---

## 6. OTA
- **API** (IDF: `esp_https_ota/include/esp_https_ota.h` l.114–237, VERIFIED):
  - `esp_https_ota()`, or `_begin`, `_perform`, `_finish`
  - `http_client_init_cb` for adding headers
  - rollback via `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` (default n), with the new app confirming itself through `esp_ota_mark_app_valid_cancel_rollback()` (`esp_ota_ops.h` l.358)
- **Private GitHub release assets** redirect to a signed URL.
  - `esp_http_client` follows the redirect **and keeps a manually set `Authorization` header** (`esp_http_client.c` l.1160–1235, VERIFIED).
  - That has broken signed-URL downloads before (INFERRED from reports).
  - Workable fix: turn off auto-redirect, then hand the `Location` URL to OTA with no auth header.
- **Recommendation (INFERRED): Rev A updates over USB only.**
  - `idf.py flash` from the laptop the device already plugs into for setup; no firmware code and no risk.
  - Still ship **ota_0/ota_1, otadata and rollback enabled** now, so adding OTA later changes firmware only, not the partition table.
  - When OTA is wanted: a `capture-notes` command serves the image over HTTPS on the LAN (a self-signed certificate pinned into NVS at setup), and a console or button command triggers it.
  - Don't use `CONFIG_ESP_HTTPS_OTA_ALLOW_HTTP`; its own help text says it is for testing.
  - After an OTA, restart with `esp_restart()`, not deep sleep, because of skip-validate-in-deep-sleep (§8).

---

## 7. Battery, ADC, brown-out, LDO
- **ADC accuracy** after calibration (DS Table 5-6 p.66, VERIFIED, Wi-Fi off, 100 nF on the pin):

  | Attenuation | Range | Error |
  |---|---|---|
  | 0 dB | 0–850 mV | ±5 mV |
  | 2.5 dB | 0–1100 mV | ±6 mV |
  | **6 dB** | **0–1600 mV** | **±10 mV** |
  | **12 dB** | **0–2900 mV** | **±50 mV** |

  - Calibration uses curve fitting: `adc_cali_create_scheme_curve_fitting`, per attenuation, from factory eFuse data (IDF `adc_calibration.rst` l.85–129, VERIFIED).
- **With the planned 1 MΩ/1 MΩ divider**, 4.2 V becomes 2.1 V, which forces 12 dB: **±50 mV at the pin, ±100 mV at the cell** (INFERRED arithmetic). That is coarse for a 3.3 V cutoff.
  - **Circuit change: make it 3:1.** Put two 1 MΩ in series on top (same Basic C26083) over one 1 MΩ, keeping the 100 nF.
    - 4.35 V becomes 1.45 V, which fits the 6 dB range: **±10 mV at the pin, ±30 mV at the cell**.
    - The divider draws 1.4 µA instead of 2.1 µA (INFERRED).
  - Read it at rest (Wi-Fi off, mic off), averaging 16–64 samples.
- **Brown-out detector** (IDF: `esp_hw_support/power_supply/port/esp32s3/Kconfig.power`, VERIFIED):
  - levels 2.44 V (default), 2.56, 2.67, **2.84**, 2.98, 3.19, 3.30 V; "estimates", reset on trigger
  - The 3.3 V flash and PSRAM need **≥ 2.7 V** (DS Tables 5-11/5-12 pp.68–69, VERIFIED), which is above the 2.44 V default.
  - **Set 2.84 V** (level 4), so the chip resets before flash writes go out of spec (INFERRED).
- **RT9080 in dropout:** a P-MOSFET pass device.
  - Dropout is **0.31 V typ / 0.53 V max at 600 mA** (RT9080 DS pp.3–4, VERIFIED), i.e. about 0.52 Ω typ, 0.88 Ω max.
  - In dropout, the output follows VBAT − I·R (INFERRED).
  - At a 355 mA Wi-Fi peak with a 3.3 V cell, the rail is about **3.1 V typ, 2.97 V worst**, at or under the module's 3.0 V minimum (INFERRED).
  - Ground current in dropout is not specified: **UNVERIFIED**. Measure the sleep current at a 3.3 V cell.
- **Firmware thresholds** (INFERRED, all at rest):
  - no Wi-Fi uploads below **3.45 V** (the note stays queued, amber)
  - recording allowed down to **3.3 V**
  - below 3.3 V: red blinking, then deep sleep with only the button armed
  - protected cells cut off at about 3.0 V anyway
- **Charger status pins: a leakage trap** (INFERRED; TP4057 pin ESD structure UNVERIFIED).
  - With USB absent, the TP4057's VCC sits on the VBUS node, held at 0 V by the 100 kΩ gate pull-down.
  - A pull-up to 3V3 on CHRG or STDBY can then leak through the chip's pin protection into that node. That can lift VBUS_SENSE and the P-FET gate.
  - So:
    - **Enable the internal pull-ups on CHRG/STDBY only after VBUS_SENSE reads high**, and turn them off before sleep.
    - Keep the charge LED between VBUS and CHRG (the datasheet circuit), not from 3V3.
    - Feed CHRG to its GPIO through about 100 kΩ, because the charge LED otherwise pulls that node toward VBUS, above the pin's 3.6 V rating.
- **VBUS_SENSE divider:** 100 kΩ over 150 kΩ.
  - 5.0 V gives 3.0 V; 5.5 V gives 3.3 V; 4.4 V gives 2.64 V, still above VIH (0.75 × 3.3 = 2.48 V; DS Table 5-4, VERIFIED) (INFERRED arithmetic).
  - It draws only while USB is present.

---

## 8. Button press vs hold after a deep-sleep wake
- **Boot-time costs:**
  - The PSRAM memory test costs "approximately **1 second per 4 MB**", so about **2 s for 8 MB**. **Turn off `CONFIG_SPIRAM_MEMTEST`** (default y) (IDF: `docs/en/api-guides/performance/speed.rst` l.171 and `esp_psram/Kconfig.spiram.common` l.89, VERIFIED).
  - With it on, a 1 s hold could not be told apart.
- **Other speed-ups** (speed.rst l.166–171 and `examples/system/startup_time/sdkconfig.defaults`, VERIFIED):
  - `CONFIG_BOOTLOADER_SKIP_VALIDATE_IN_DEEP_SLEEP=y`: allowed only without secure boot; don't switch OTA slots via deep sleep (bootloader Kconfig.projbuild l.322–343).
  - `CONFIG_BOOTLOADER_LOG_LEVEL_WARN`, `CONFIG_LOG_DEFAULT_LEVEL_WARN`
  - QIO flash
  - With no PSRAM, the startup example reaches `app_main` at about **37 ms** (its README, VERIFIED, target unstated).
- **Estimate** (INFERRED): with PSRAM init (no memtest) and XIP-from-PSRAM copying about 1.5 MB, **wake to `app_main` is about 100–250 ms.** Measure it with a GPIO toggle at bring-up.
- **So the hold can be measured correctly:**
  1. The wake cause (ext1, GPIO0) proves a press started.
  2. A **deep-sleep wake stub** (example `examples/system/deep_sleep_wake_stub`, VERIFIED present) runs from RTC memory right after ROM and stores the RTC time.
  3. `app_main` reads GPIO0.
     - Already released → **press**.
     - Still low → time from the stub's timestamp. Low for ≥ 1 s in total → **hold**.
  - Start the mic and capture at wake whichever it turns out to be, so no speech is lost; the class only picks new note or addition.
  - Before sleeping again, wait until GPIO0 is high, or the level-triggered wake fires at once (INFERRED).

---

## What this means for the plan
- **Codec:** OGG/Opus, 16 kHz mono, 32 kbps, 20 ms frames.
  - Components: `espressif/esp_audio_codec` 2.6.2 + `espressif/esp_muxer` 1.2.3 (OGG, `data_cb`), our own writer with `fsync` about every 1 s.
  - Both are closed Espressif-only libraries.
  - Fallback: AAC-LC ADTS from the same codec, or a ~200-line Ogg muxer.
  - I2S at 16 kHz, 32-bit left slot (set `slot_mask` LEFT), mic in low-power mode.
  - Test-build on v6.1 first; v6.0 is the last version its changelog names.
- **Partition table:**
  - nvs 24 KB, otadata, phy, nvs_keys (reserved)
  - ota_0 and ota_1 at 3 MB each
  - LittleFS `storage` 9.81 MiB (about 37 min usable)
  - coredump 64 KB
  - (§3)
- **Pin table:** §1. Button GPIO0, BAT_ADC GPIO1, VBUS_SENSE GPIO2, CHRG GPIO4, STDBY GPIO5, MIC_PWR GPIO6, LED R/G/B GPIO8/9/10, I2S SCK/WS/SD GPIO11/12/13. All RTC GPIOs, held in sleep.
- **Secret storage:** HMAC-eFuse-based NVS encryption. No flash encryption, no secure boot, so USB re-flashing stays normal.
  - Provisioning: a no-echo line protocol on USB Serial/JTAG, served while VBUS is present (the device never sleeps on USB).
- **OTA:** USB-only for Rev A. Keep two slots and rollback enabled; a LAN HTTPS pull can come later without changing the partition table.
- **Wake time:** yes, press vs 1 s hold works, if `SPIRAM_MEMTEST` is off (otherwise about 2 s of boot). Expect about 100–250 ms to `app_main`, timed from a wake stub.
- **Circuit changes this implies:**
  1. **No P-FET for the mic:** power it from GPIO6 (0.3 mA max vs 40 mA pad capability).
  2. **Battery divider 3:1** (2 × 1 MΩ over 1 MΩ): ±30 mV instead of ±100 mV, and 1.4 µA instead of 2.1 µA.
  3. **Add VBUS_SENSE** (100 kΩ/150 kΩ to GPIO2), for staying awake on USB, waking on plug-in, and gating the charger-pin pull-ups.
  4. **CHRG to its GPIO through about 100 kΩ**, charge LED from VBUS; STDBY direct. Pull-ups on those pins only while USB is present.
  5. **10 kΩ external pull-up on GPIO0**, and no large capacitor on it.
  6. **Sleep budget:** 7 µA module (ext1 only) or 8 µA (with VBUS wake), plus about 6–9 µA for the board, gives **13–17 µA typ**. RT9080 ground current in dropout is unknown; measure at a 3.3 V cell.
- **Needs a DECISIONS entry:**
  - the brown-out level (2.84 V)
  - the upload and record thresholds (3.45 / 3.3 V)
  - what a hold does when the last note is gone (becomes a new note)
  - the shared Gemini quota: the device must follow SPEC §6's gate and 429 rules
