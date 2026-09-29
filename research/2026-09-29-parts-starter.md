# Starter board parts: JLCPCB/LCSC selection (2026-09-29)

Nothing was ordered and no accounts were used.

## How this was checked

- **Primary live source:** JLCPCB's own parts-search API, queried 2026-09-29. This is the same backend that jlcpcb.com/parts uses:
  `POST https://jlcpcb.com/api/overseas-pcb-order/v1/shoppingCart/smtGood/selectSmtComponentList`.
  - It returns the library type (`base` = Basic, `expand` = Extended), `preferredComponentFlag` (Preferred Extended), JLCPCB's assembly stock, price tiers, the datasheet link and the part-detail URL.
  - **Stock** in the tables is JLCPCB's assembly stock. LCSC's own stock is often higher.
  - **Price @10** is JLCPCB's price tier that covers a quantity of 10, in USD.
- **Full Basic and Preferred lists:** enumerated live with an empty keyword. That gave 351 Basic parts and 1,235 Preferred-Extended parts.
  - **None of these categories has a Basic or Preferred part:** Wi-Fi module, USB-C connector, PTC fuse, humidity sensor, JST-SH connector, or 0805 blue LED.
  - **3.3 V LDOs of 500 mA or more:** AMS1117-3.3 is the only Basic one, and there are no Preferred ones.
- **jlcsearch.tscircuit.com** was used only to discover candidates. Its data lags JLCPCB.
  - Example: SRV05-4 C7420376 shows 622k in stock on jlcsearch but only 2 on JLCPCB.
  - Example: ESP32-S3-WROOM-1-N8 shows 6,072 on jlcsearch but 313 on JLCPCB.
  - Don't trust jlcsearch stock or Basic/Preferred flags.
- **Datasheets:** downloaded (LCSC PDF mirror, Espressif, TI) and read with `pdftotext`.
  - Figures quoted as "(DS)" were read from the datasheet.
  - Anything not read from a datasheet or live listing is marked **UNVERIFIED**.
- **KiCad footprints:** checked with `ls` in `/usr/share/kicad/footprints/` (KiCad 10.0.6 library).

JLCPCB Economic PCBA charges a **$3.07 loading fee per unique Extended part**. Basic and Preferred-Extended parts have no fee.

---

## 1. ESP32-S3-WROOM-1 module

All variants are **Extended**. Datasheet: Espressif *ESP32-S3-WROOM-1 & WROOM-1U Datasheet v1.8*, https://www.espressif.com/sites/default/files/documentation/esp32-s3-wroom-1_wroom-1u_datasheet_en.pdf

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| MCU module | ESP32-S3-WROOM-1-N16R8 (16 MB flash, 8 MB octal PSRAM) | C2913202 | Extended | 21,474 | 4.5127 (@1: 5.1348) | SMD 25.5×18 mm | Espressif URL above | https://jlcpcb.com/partdetail/3198300-ESP32_S3_WROOM_1N16R8/C2913202 |
| MCU module | ESP32-S3-WROOM-1-N8R8 (8 MB, 8 MB octal) | C2913201 | Extended | 4,457 | 4.3779 | SMD 25.5×18 mm | same | https://jlcpcb.com/partdetail/3198299-ESP32_S3_WROOM_1N8R8/C2913201 |
| MCU module | ESP32-S3-WROOM-1-N8 (8 MB, no PSRAM) | C2913198 | Extended | **313** | 4.2154 | SMD 25.5×18 mm | same | https://jlcpcb.com/partdetail/3198296-ESP32_S3_WROOM_1N8/C2913198 |
| MCU module | ESP32-S3-WROOM-1-N8R2 (8 MB, 2 MB quad PSRAM) | C2913204 | Extended | 2,829 | 4.0968 | SMD 25.5×18 mm | same | https://jlcpcb.com/partdetail/3198302-ESP32_S3_WROOM_1N8R2/C2913204 |
| MCU module | ESP32-S3-WROOM-1-N16 (16 MB, no PSRAM) | C2913199 | Extended | 905 | 4.5679 | SMD 25.5×18 mm | same | https://jlcpcb.com/partdetail/3198297-ESP32_S3_WROOM_1N16/C2913199 |
| MCU module | ESP32-S3-WROOM-1-N4 (4 MB) | C2913197 | Extended | 3,727 | 3.6420 | SMD 25.5×18 mm | same | https://jlcpcb.com/partdetail/3198295-ESP32_S3_WROOM_1N4/C2913197 |

Datasheet facts (DS, v1.8):
- **Octal PSRAM reserves three pins.** On modules with octal PSRAM (R8 = ESP32-S3R8, and R16V), "pins IO35, IO36, and IO37 are connected to the Octal SPI PSRAM and are not available for other uses". This is footnote b to the pin table. It applies to N16R8 and N8R8. It does not apply to N8, N16 or N8R2 (quad PSRAM).
- **R8 modules have a lower temperature limit.** They are rated to −40 to **65 °C** ambient. Other variants go to 85 °C. Enabling PSRAM ECC raises R8 to 85 °C but costs 1/16 of the PSRAM.
- **Power supply:** 3.0–3.6 V. The external supply must deliver at least **0.5 A** (I_VDD, min 0.5 A).
- **Espressif's reference circuit** puts 22 µF + 0.1 µF on 3V3 at the module.
- **KiCad footprint:** `RF_Module.pretty/ESP32-S3-WROOM-1.kicad_mod`.

## 2. USB-C receptacle (USB 2.0, 16-pin, SMD with through-hole shell legs)

No Basic or Preferred USB-C connector exists at JLCPCB (checked by enumerating the full lists).

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| USB-C | **HRO (Korean Hroparts) TYPE-C-31-M-12** | C165948 | Extended | 444,031 | 0.1856 | SMD 16P, 4 THT shell legs | https://www.lcsc.com/datasheet/lcsc_datasheet_2205251630_Korean-Hroparts-Elec-TYPE-C-31-M-12_C165948.pdf | https://jlcpcb.com/partdetail/Korean_HropartsElec-TYPE_C_31_M12/C165948 |
| USB-C | SHOU HAN TYPE-C 16PIN 2MD(073) | C2765186 | Extended | 985,847 | 0.0743 | SMD 16P | https://jlcpcb.com/api/file/downloadByFileSystemAccessId/8588920841703079936 | https://jlcpcb.com/partdetail/SHOUHAN-TYPE_C_16PIN_2MD_073/C2765186 |
| USB-C | XKB U262-161N-4BVC11 | C319148 | Extended | 54,211 | 0.3844 | SMD 16P | https://www.lcsc.com/datasheet/lcsc_datasheet_2409141623_XKB-Connection-U262-161N-4BVC11_C319148.pdf | https://jlcpcb.com/partdetail/XKBConnection-U262_161N4BVC11/C319148 |

KiCad footprint matching:
- **TYPE-C-31-M-12:** exact match in `Connector_USB.pretty/USB_C_Receptacle_HRO_TYPE-C-31-M-12.kicad_mod`.
  - Verified by `ls` and by reading the file: 16 SMD pads (A1/A4–A9/A12 and B1/B4–B9/B12), 4 plated through-hole shell pads "SH", and 2 non-plated locating holes.
- **SHOU HAN 2MD(073):** has no KiCad footprint. It is often said to share the HRO land pattern, but that is **UNVERIFIED**.
- **XKB:** the closest file is `USB_C_Receptacle_XKB_U262-16XN-4BVC11.kicad_mod`. Whether it matches the -161N part is **UNVERIFIED**.
- **The HRO datasheet PDF is an image.** Dimensions were not text-extracted; the KiCad footprint was drawn from HRO's drawing.
- **CC pins:** add 5.1 kΩ from each of CC1 and CC2 to GND (C27834 below).

## 3. USB ESD protection

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| ESD D+/D−/VBUS | STMicroelectronics USBLC6-2SC6 | C7519 | Extended | 43,247 | 0.1765 | SOT-23-6 | https://www.lcsc.com/datasheet/lcsc_datasheet_2410121836_STMicroelectronics-USBLC6-2SC6_C7519.pdf | https://jlcpcb.com/partdetail/STMicroelectronics-USBLC62SC6/C7519 |
| ESD D+/D−/VBUS | TECH PUBLIC USBLC6-2SC6 (second source) | C2827654 | Extended | 96,298 | 0.0446 | SOT-23-6 | https://www.lcsc.com/datasheet/lcsc_datasheet_2108132230_TECH-PUBLIC-USBLC6-2SC6_C2827654.pdf (image PDF, not text-checked) | https://jlcpcb.com/partdetail/TECHPUBLIC-USBLC62SC6/C2827654 |
| ESD D+/D− (2 lines) | hongjiacheng H5VUT2U | C20615824 | **Pref-Ext** | 129,691 | 0.0579 | SOT-23 | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2401261525_hongjiacheng-H5VUT2U_C20615824.pdf | https://jlcpcb.com/partdetail/hongjiacheng-H5VUT2U/C20615824 |
| TVS on VBUS | hongjiacheng SMF5.0A | C19077497 | **Pref-Ext** | 173,425 | 0.0282 | SOD-123FL | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2312041000_hongjiacheng-SMF5-0A_C19077497.pdf | https://jlcpcb.com/partdetail/hongjiacheng-SMF50A/C19077497 |

Notes:
- **H5VUT2U (DS):** unidirectional 5 V, 0.6 pF typ line-to-GND, IEC 61000-4-2 ±20 kV contact. Pins 1 and 2 are I/O and pin 3 is GND, read from the datasheet function diagram. Re-check at schematic time.
- **SMF5.0A:** 5 V working voltage, 200 W (10/1000 µs), from the JLC listing. Its datasheet was not text-checked, so this is **UNVERIFIED**.
- **The Preferred-Extended pair costs no loading fee.** USBLC6 costs $3.07.
- **SRV05-4 is not usable.** C7420376 is Preferred-Extended but has only 2 in stock live.

## 4. 3.3 V LDO (5 V in, 500 mA or more)

AMS1117-3.3 is the only Basic 3.3 V LDO rated 500 mA or more. No Preferred-Extended one exists (checked by enumerating the full lists).

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | max I | dropout (DS) | output cap (DS) | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|---|---|---|
| LDO | AMS1117-3.3 (AMS) | C6186 | **Basic** | 1,114,456 | 0.2072 | SOT-223 | 1 A | 1.1 V typ / 1.3 V max @ 0.8 A | "22 µF solid tantalum … ensures stability"; ceramic not addressed in DS (**UNVERIFIED** with ceramic) | https://www.lcsc.com/datasheet/lcsc_datasheet_2410121508_Advanced-Monolithic-Systems-AMS1117-3-3_C6186.pdf | https://jlcpcb.com/partdetail/Advanced_MonolithicSystems-AMS1117_33/C6186 |
| LDO | **ST LDL1117S33R** | C435835 | Extended | 13,475 | 0.4435 | SOT-223 | 1.2 A | 350 mV typ / 600 mV max @ 1.2 A | Designed for ceramic output capacitors; X5R/X7R suggested; 1 µF in + 4.7 µF out suggested. Max C_out not read (stability-plan figure is an image; **UNVERIFIED**) | https://www.lcsc.com/datasheet/C435835.pdf | https://jlcpcb.com/partdetail/STMicroelectronics-LDL1117S33R/C435835 |
| LDO | SGMICRO SGM2212-3.3XKC3G/TR | C3294699 | Extended | 11,276 | 0.4874 | SOT-223 | 800 mA | 280 mV typ / 380 mV max @ 500 mA; 610 mV max @ 800 mA | Ceramic; **effective C_out 1–10 µF** (upper limit!) | https://www.lcsc.com/datasheet/C3294699.pdf | https://jlcpcb.com/partdetail/SGMICRO-SGM2212_3_3XKC3GTR/C3294699 |
| LDO | TI TLV75733PDBVR | C485517 | Extended | 101,690 | 0.1990 | SOT-23-5 | 1 A | 300 mV typ / 425 mV max @ 1 A | 1 µF ceramic min, up to 200 µF | https://www.lcsc.com/datasheet/lcsc_datasheet_2304140030_Texas-Instruments-TLV75733PDBVR_C485517.pdf | https://jlcpcb.com/partdetail/TexasInstruments-TLV75733PDBVR/C485517 |
| LDO | Diodes AP2112K-3.3TRG1 | C51118 | Extended | 41,200 | 0.1709 | SOT-23-5 | 600 mA | 250 mV typ / 400 mV max @ 600 mA | "Stable with 1.0 µF flexible cap: ceramic, tantalum, aluminium" (X5R/X7R) | https://www.lcsc.com/datasheet/lcsc_datasheet_2304140030_Diodes-Incorporated-AP2112K-3-3TRG1_C51118.pdf | https://jlcpcb.com/partdetail/DiodesIncorporated-AP2112K_33TRG1/C51118 |
| LDO | Richtek RT9080-33GJ5 | C841192 | Extended | 38,892 | 0.1199 | TSOT-23-5 | 600 mA | 310 mV typ / 530 mV max @ 600 mA | ≥1 µF effective, ceramic or tantalum | https://www.lcsc.com/datasheet/lcsc_datasheet_2009192305_Richtek-Tech-RT9080-33GJ5_C841192.pdf | https://jlcpcb.com/partdetail/RichtekTech-RT908033GJ5/C841192 |
| LDO | MICRONE ME6211C33M5G-N | C82942 | Extended | 170,797 | 0.0606 | SOT-23-5 | 500 mA (at V_in = V_out+1 V) | 100 mV @ 100 mA, 210 mV @ 200 mA (no 500 mA figure) | 1 µF ceramic (test condition; "compatible with low-ESR ceramic") | https://www.lcsc.com/datasheet/lcsc_datasheet_1811131510_MICRONE-Nanjing-Micro-One-Elec-ME6211C33M5G-N_C82942.pdf | https://jlcpcb.com/partdetail/84106-ME6211C33M5GN/C82942 |

Not available:
- **XC6220B331MR:** C22466451 is rated only 300 mA, −20 to 60 °C. The 1 A version, C86534, has 8.5k stock at $0.35, but its datasheet was not read.
- **SGM2212 in SOT-23:** none exists; SOT-223 is the only small package.

Heat (DS θJA values):
- **Heat per 100 mA:** about 1.7 V × I is lost as heat (5 V → 3.3 V), which is 0.17 W at 100 mA.
- **SOT-23-5 parts run hot.** TLV757P DBV is 231 °C/W and RT9080 is 231 °C/W. At a sustained 300 mA (0.5 W) that is a rise of about 115 °C, which risks thermal shutdown during long Wi-Fi transmit.
- **SOT-223 parts stay cooler.** LDL1117 is 120 °C/W, SGM2212 117 °C/W, and AMS1117 55–80 °C/W with copper. At 0.5 W the rise is about 30–60 °C.

Pin compatibility:
- **AMS1117-3.3, LDL1117S33R and SGM2212-3.3 SOT-223 share the pinout** 1 = GND, 2 = OUT (tab), 3 = IN. The LDL1117 and SGM2212 pinouts are from their datasheets.
- **So one SOT-223 footprint accepts any of the three.**
- The AMS1117 pinout is inferred from its datasheet's front-view drawing and standard practice.

## 5. Resettable fuse (PTC) on VBUS, ~500 mA hold

No Basic or Preferred PTC exists at JLCPCB (checked by enumerating the full lists).

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| PTC | **Jinrui JK-nSMD050-30** | C720075 | Extended | 263,347 | 0.0334 | 1206 | https://www.lcsc.com/datasheet/lcsc_datasheet_2008122036_Jinrui-Electronic-Materials-Co--JK-nSMD050-30_C720075.pdf | https://jlcpcb.com/partdetail/764095-JK_nSMD05030/C720075 |
| PTC | Brightking SMD0805B050TF | C269104 | Extended | 1,488,297 | 0.0225 | 0805 | https://www.lcsc.com/datasheet/lcsc_datasheet_2304140030_Brightking-SMD0805B050TF_C269104.pdf | https://jlcpcb.com/partdetail/Brightking-SMD0805B050TF/C269104 |
| PTC | TECHFUSE nSMD050-33V | C70077 | Extended | 222,623 | 0.0572 | 1206 | https://www.lcsc.com/datasheet/lcsc_datasheet_1809211126_TECHFUSE-nSMD050-33V_C70077.pdf | https://jlcpcb.com/partdetail/TECHFUSE-nSMD05033V/C70077 |

JK-nSMD050-30 datasheet values:
- 30 V max, 100 A max fault current
- I_hold 0.50 A and I_trip 1.00 A at 25 °C
- trips within 0.10 s at 8 A
- resistance R_min 0.15 Ω, R_typ 0.30 Ω, R1max 1.0 Ω

The 0805 option is rated only 6 V, with R1max 0.85 Ω (JLC listing, **UNVERIFIED**). KiCad footprint: `Fuse:Fuse_1206_3216Metric`.

## 6. Temperature/humidity sensor, Sensirion SHT40 (I2C 0x44)

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| RH/T sensor | **Sensirion SHT40-AD1B-R2** | C2909890 | Extended | 21,603 | 1.6310 | DFN-4 1.5×1.5 mm | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2110211930_Sensirion-SHT40-AD1B-R2_C2909890.pdf | https://jlcpcb.com/partdetail/Sensirion-SHT40_AD1BR2/C2909890 |
| RH/T sensor | Sensirion SHT40-AD1B-R3 | C2848306 | Extended | 10,177 | 1.4734 | DFN-4 1.5×1.5 mm | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2304140030_Sensirion-SHT40-AD1B-R3_C2848306.pdf | https://jlcpcb.com/partdetail/Sensirion-SHT40_AD1BR3/C2848306 |

Datasheet facts:
- **Address and variants:** "AD1B" means I2C address 0x44. R2 and R3 are the same sensor; R2 comes on 2,500-piece reels and R3 on 10,000-piece reels.
- **Centre pad:** Sensirion recommends *not* soldering the central die pad. That matches the KiCad footprint `Sensor_Humidity.pretty/Sensirion_DFN-4_1.5x1.5mm_P0.8mm_SHT4x_NoCentralPad.kicad_mod`.
- **Supply range:** 1.08–3.6 V.

## 7. Qwiic connector: JST SH 1.0 mm, 4-pin, SMD, horizontal

No Basic or Preferred part exists.

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| Qwiic | **JST SM04B-SRSS-TB(LF)(SN)** (genuine) | C160404 | Extended | 38,264 | 0.2608 | SMD 1 mm, right angle, 2 mounting tabs | https://www.lcsc.com/datasheet/lcsc_datasheet_2304140030_JST-SM04B-SRSS-TB-LF-SN_C160404.pdf | https://jlcpcb.com/partdetail/JST-SM04B_SRSS_TB_LF_SN/C160404 |
| Qwiic | Megastar ZX-SH1.0-4PWT (clone) | C7430446 | Extended | 192,092 | 0.0652 | SMD 1 mm, right angle (卧贴) | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2306191203_Megastar-ZX-SH1-0-4PWT_C7430446.pdf | https://jlcpcb.com/partdetail/Megastar-ZX_SH1_04PWT/C7430446 |
| Qwiic | XUNPU WAFER-SH1.0-4PWB (clone) | C3029343 | Extended | 40,363 | 0.1045 | SMD 1 mm, right angle | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2205271830_XUNPU-WAFER-SH1-0-4PWB_C3029343.pdf | https://jlcpcb.com/partdetail/XUNPU-WAFER_SH1_04PWB/C3029343 |

- **KiCad footprint:** `Connector_JST.pretty/JST_SH_SM04B-SRSS-TB_1x04-1MP_P1.00mm_Horizontal.kicad_mod`. It has 4 signal pads plus 2 "MP" mounting pads, and exactly matches the genuine JST part.
- **Clone land patterns were not compared.** The Megastar drawing gives only electrical specs in its text layer, so clone compatibility is **UNVERIFIED**.
- **Qwiic pin order** is GND, 3V3, SDA, SCL. This is the SparkFun convention, **UNVERIFIED** here.

## 8. Tactile push buttons (×2), SMD

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| Button | **XKB TS-1187A-B-A-B** | C318884 | **Basic** | 535,335 | 0.0205 | SMD 4-pin, 5.1×5.1 mm, 1.5 mm tall, SPST-NO, 1.6 N | https://www.lcsc.com/datasheet/lcsc_datasheet_2304140030_XKB-Connection-TS-1187A-B-A-B_C318884.pdf | https://jlcpcb.com/partdetail/XKBConnection-TS_1187A_B_AB/C318884 |
| Button | XUNPU TS-1088-AR02016 | C720477 | **Basic** | 911,071 | 0.0537 | SMD, 3.9×3.0 mm, 2.0 mm tall, SPST | https://www.lcsc.com/datasheet/lcsc_datasheet_2304140030_XUNPU-TS-1088-AR02016_C720477.pdf | https://jlcpcb.com/partdetail/XUNPU-TS_1088AR02016/C720477 |

- **KiCad footprints:**
  - TS-1187A matches `Button_Switch_SMD.pretty/SW_Push_1P1T_XKB_TS-1187A.kicad_mod`, which has 4 pads and is built from XKB's TS-1187A drawing.
  - TS-1088 matches `SW_SPST_TS-1088-xR020.kicad_mod`, whose description cites this exact LCSC part.
- **Size and pin count** come from the JLC listing and the KiCad footprint. The XKB datasheet PDF is image-only, so it was not text-checked.

## 9. LEDs 0805

No Basic 0805 blue LED exists at JLCPCB, so the status LED is red.

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | Vf (DS) | current (DS) | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|---|---|
| Power LED (green) | Hubei KENTO KT-0805G | C2297 | **Basic** | 1,108,207 | 0.0163 | 0805 | 2.6–3.1 V @ 5 mA | test 5 mA; 30 mA DC max | https://www.lcsc.com/datasheet/lcsc_datasheet_1806151820_Hubei-KENTO-Elec-KT-0805G_C2297.pdf | https://jlcpcb.com/partdetail/Hubei_KENTOElec-KT0805G/C2297 |
| Status LED (red) | NationStar NCD0805R1 | C84256 | **Basic** | 4,930,989 | 0.0134 | 0805 | 1.5 / 2.0 / 2.6 V (min/typ/max) | test 20 mA; 25 mA max | https://www.lcsc.com/datasheet/lcsc_datasheet_2409272203_Foshan-NationStar-Optoelectronics-NCD0805R1_C84256.pdf | https://jlcpcb.com/partdetail/85425-NCD0805R1/C84256 |

- **Green LED on 3.3 V:** its Vf of up to 3.1 V leaves only about 0.2 V across the resistor, so it would be dim or uneven. Feed it from VBUS (5 V) through 1 kΩ, which gives about 2 mA, or through 2.2 kΩ from 5 V.
- **Red LED:** 3.3 V through 1 kΩ gives about 1.3 mA, which is visible.
- **Other Basic 0805 colours:** yellow KT-0805Y (C2296) and white KT-0805W (C34499).

## 10. Passives 0805 (all **Basic**, live-checked)

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| 5.1 kΩ 1% (USB CC) | UNI-ROYAL 0805W8F5101T5E | C27834 | Basic | 3,484,093 | 0.0056 | 0805 | via JLC page | https://jlcpcb.com/partdetail/28584-0805W8F5101T5E/C27834 |
| 10 kΩ 1% | UNI-ROYAL 0805W8F1002T5E | C17414 | Basic | 50,439,082 | 0.0039 | 0805 | via JLC page | https://jlcpcb.com/partdetail/18102-0805W8F1002T5E/C17414 |
| 4.7 kΩ 1% (I2C pull-ups) | UNI-ROYAL 0805W8F4701T5E | C17673 | Basic | 5,212,434 | 0.0050 | 0805 | via JLC page | https://jlcpcb.com/partdetail/18361-0805W8F4701T5E/C17673 |
| 1 kΩ 1% | UNI-ROYAL 0805W8F1001T5E | C17513 | Basic | 31,436,611 | 0.0040 | 0805 | via JLC page | https://jlcpcb.com/partdetail/18201-0805W8F1001T5E/C17513 |
| 2.2 kΩ 1% | UNI-ROYAL 0805W8F2201T5E | C17520 | Basic | 3,642,353 | 0.0046 | 0805 | via JLC page | https://jlcpcb.com/partdetail/18208-0805W8F2201T5E/C17520 |
| 0 Ω | UNI-ROYAL 0805W8F0000T5E | C17477 | Basic | 6,967,890 | 0.0044 | 0805 | via JLC page | https://jlcpcb.com/partdetail/18165-0805W8F0000T5E/C17477 |
| 100 nF 50 V X7R | YAGEO CC0805KRX7R9BB104 | C49678 | Basic | 17,558,908 | 0.0191 | 0805 | via JLC page | https://jlcpcb.com/partdetail/YAGEO-CC0805KRX7R9BB104/C49678 |
| 1 µF 50 V X7R | Samsung CL21B105KBFNNNE | C28323 | Basic | 2,387,270 | 0.0397 | 0805 | via JLC page | https://jlcpcb.com/partdetail/29074-CL21B105KBFNNNE/C28323 |
| 10 µF 25 V X5R | Samsung CL21A106KAYNNNE | C15850 | Basic | 5,240,569 | 0.0788 | 0805 | via JLC page | https://jlcpcb.com/partdetail/16532-CL21A106KAYNNNE/C15850 |
| 22 µF 25 V X5R | Samsung CL21A226MAQNNNE | C45783 | Basic | 4,137,279 | 0.2200 | 0805 | via JLC page | https://jlcpcb.com/partdetail/46786-CL21A226MAQNNNE/C45783 |

- **Useful extra:** 4.7 µF 25 V X5R, C1779 (Basic, 3.0M in stock).
- **Real capacitance under voltage is lower.** An 0805 X5R 22 µF at 3.3 V gives well under 22 µF effective, and 10 µF at 5 V likewise. This is an **UNVERIFIED** estimate, since Samsung's DC-bias curves were not read. It matters for the SGM2212 10 µF limit and for meeting Espressif's 22 µF recommendation.
- **Rated power:** 0805 resistors are 125 mW, 150 V.

---

## Recommended pick per function

| # | Function | Pick | Class | Why |
|---|---|---|---|---|
| 1 | Module | **ESP32-S3-WROOM-1-N16R8, C2913202** | Extended (+$3.07) | Highest JLC stock (21k); the common dev-kit variant. It costs GPIO35–37 and has a 65 °C ambient limit. If those pins are needed, use N8R2 (C2913204, 2.8k in stock). Avoid N8: only 313 in stock. |
| 2 | USB-C | **HRO TYPE-C-31-M-12, C165948** | Extended (+$3.07) | Exact KiCad footprint (`USB_C_Receptacle_HRO_TYPE-C-31-M-12`), 444k in stock, USB 2.0 with D+/D−, through-hole shell legs. |
| 3 | ESD | **H5VUT2U C20615824 (D+/D−) + SMF5.0A C19077497 (VBUS)** | Pref-Ext (no fee) | Low-capacitance (0.6 pF) data-line protection plus a real 200 W TVS on VBUS, with no loading fee. Alternative: ST USBLC6-2SC6, C7519 (Extended, +$3.07). |
| 4 | LDO | **ST LDL1117S33R, C435835** | Extended (+$3.07) | 1.2 A, 350 mV typ dropout, made for ceramic capacitors, SOT-223 (runs cool). Same pinout and footprint as the Basic AMS1117-3.3 (C6186). AMS1117 is the no-fee fallback, but its datasheet specifies a 22 µF tantalum and it drops about 1.1–1.3 V, which is marginal on a sagging USB supply. |
| 5 | PTC | **JK-nSMD050-30, C720075 (1206)** | Extended (+$3.07) | 0.5 A hold / 1 A trip, 30 V, 0.3 Ω typ (less voltage drop than the 0805 options), 263k in stock. |
| 6 | Sensor | **SHT40-AD1B-R2, C2909890** | Extended (+$3.07) | Genuine Sensirion, address 0x44, 21.6k in stock; the KiCad footprint exists. |
| 7 | Qwiic | **JST SM04B-SRSS-TB(LF)(SN), C160404** | Extended (+$3.07) | Genuine part matches the KiCad footprint exactly. Clones cost the same fee, so the saving is only $0.20 each. |
| 8 | Buttons | **XKB TS-1187A-B-A-B, C318884 ×2** | Basic | 5.1×5.1 mm, 4-pin, the KiCad footprint exists, 535k in stock. |
| 9 | LEDs | **KT-0805G C2297 (green, power), NCD0805R1 C84256 (red, status)** | Basic | Basic 0805; no Basic blue exists. |
| 10 | Passives | **C27834, C17414, C17673, C17513, C17520, C17477, C49678, C28323, C15850, C45783** | Basic | All Basic, 0805, and more than 900k in stock each. |

## Risks and flags

- **Extended-part fees:** the recommended set has 6 unique Extended parts: module, USB-C, LDO, PTC, SHT40 and JST. The ESD pair adds none. That comes to **6 × $3.07 = $18.42 per assembly order**.
  - Using the Basic AMS1117 saves $3.07.
  - Choosing USBLC6 instead of the Preferred-Extended ESD pair adds $3.07.
- **JLCPCB stock is far lower than LCSC and jlcsearch show:**
  - **ESP32 module:** N16R8 21k; N8 313; N16 905.
  - **LDOs:** LDL1117 13k; SGM2212 11k. Both are fine for 5–10 boards, but should be re-checked right before ordering.
- **Price moved since the lessons file:** the N16R8 is $5.13 @1 and $4.51 @10, the same @1 figure as in HARDWARE_LESSONS.md.
- **Output capacitor limits:**
  - **SGM2212:** its datasheet caps effective output capacitance at 10 µF. That conflicts with Espressif's 22 µF on 3V3, which is why it isn't the pick.
  - **LDL1117:** its maximum output capacitance is **UNVERIFIED**, because the stability-plan figure is an image. Check it before finalising the 22 µF at the module.
- **SOT-23-5 LDOs are thermally marginal** at 5 V → 3.3 V under sustained Wi-Fi transmit (θJA about 231 °C/W).
- **R8 modules** have a 65 °C ambient limit and reserve GPIO35–37.
- **UNVERIFIED items:**
  - AMS1117 stability with ceramic capacitors
  - the clone JST and SHOU HAN USB-C land patterns
  - SMF5.0A datasheet values
  - the TECH PUBLIC USBLC6 datasheet (image-only)
  - the Qwiic pin order
  - the AMS1117 pinout, which was inferred rather than text-read
