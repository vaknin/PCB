# capture-clip: cost options (2026-09-30)

Question from the owner: are we ordering this the cheapest sensible way? Is there a better fab? Can we avoid the "Extended part" loading fees, even at some cost in function?

**Status: two passes.** The JLCPCB side (fees, substitutes, tiers) was done first. A second pass the same day added the other fabs (section 3), ready-made boards (section 4), hand-solder parts (section 5) and a per-device comparison (section 6). Several fabs cannot be priced without a login, an upload or a person's quote; those gaps are listed under "Not done yet". Nothing was ordered, no account was used, no design file was uploaded, no file other than this one was changed.

Tags: **VERIFIED** = fetched today (2026-09-30) from the source named. **INFERRED** = my arithmetic, model or judgement.

## Summary

All totals are for 5 bare boards with 2 assembled. "Alone" = this design ships by itself (FedEx $29.59, goods under $75 so no VAT). "Shared" = one of about five designs in a parcel (shipping share about $6.50, 18 % VAT on everything), which is the owner's real pattern (D-021). Battery and printed case are not included in any row; they are the same for all of them.

| Option | Goods | Delivered alone (per device) | Delivered in a shared parcel (INFERRED) | What changes for the owner | Confidence |
|---|---|---|---|---|---|
| (a) As designed, 8 fees | $66.89 | $96.48 ($48.24) | about $86.6 | nothing | High: live prices, fees from JLCPCB's price page |
| (b) Main button → a free ("Basic") button, 7 fees | $63.49 | $93.08 ($46.54) | about $82.6 | a lower button with a lighter click; the printed cap hides it. Small layout and case change | High on price; feel is INFERRED |
| (c) b + three separate lights instead of the RGB one + the older diode, 5 fees | $57.89 | $87.48 ($43.74) | about $76.0 | "sending" shows white instead of blue; amber is red and green side by side; sleep current about 18 instead of 13 µA (no practical change in battery life, but almost no margin to the 20 µA target, and worse when warm) | Medium: prices verified, the effects are INFERRED |
| (d) c + battery connector left off, owner solders it, 4 fees | $54.35 + a connector (cents; price not checked) | $83.94 ($41.97) | about $71.8 | two easy through-hole solder joints per board | Medium |
| Add-on to any row: cheaper microphone (TDK T3902) | −$5.53 | −$5.53 | about −$6.5 | firmware reads a different microphone type (PDM); one footprint detail is unverified, so it adds first-order risk | Medium-low |
| PCBWay, same board (section 3) | $34.00 fees + parts by quote (unknown; $28.04 at JLCPCB) | about $78–115 ($39–58) (INFERRED) | not estimated; at most about $5 under JLCPCB | BOM reworked to manufacturer part numbers; a person quotes the parts | Low: fees verified, parts unknown |
| Elecrow, same board | $89.90 fees + parts by quote | over $150 (INFERRED) | | nothing gained | Medium |
| NextPCB Rev 0 (free assembly, first order) | needs login: not priced | unknown | | board must grow to 50 × 50 mm and 5 must be assembled | none |
| Seeed Fusion, AISLER | need login / upload: not priced | unknown | | | none |
| 2 × M5Stack StickS3, ready-made (section 4) | ₪215.42 ≈ $70, battery and case included | about $70 ($35) | n/a (AliExpress, separate order) | a different-looking finished stick with a screen; 250 mAh, 8 MB storage; firmware must be ported | High on price, low on fit |
| 2 × Seeed XIAO ESP32S3 Sense + cells | about $48–52 | about $48–52 ($24–26) | n/a | 3 mA asleep by Seeed's own table (about 5.5 days idle); soldering | High that it fails the sleep requirement |
| 2 × Adafruit Feather ESP32-S3 + microphone breakout + cells | about $63.50 + shipping (unknown) | unknown | n/a | hand-wired, 4 MB storage, about 100 µA asleep | Medium |

What I would pick, if asked (not a decision): stay at JLCPCB with (b). No other fab is clearly cheaper (section 3); the one cheaper route per device is a ready-made M5Stack StickS3, which is a different product and the owner's call (section 6). The remaining fee-saving swaps save $3 each and each costs something visible or some risk; a respin costs far more than $3.

**The owner's idea of "paying with a smaller battery" does not apply here.** None of the fee-bearing parts is there because of the battery size. Four of the eight fees cannot be avoided at JLCPCB at all (module, microphone, USB-C socket, regulator), whatever the battery.

## 1. The eight Extended parts

Source for class, stock and price: JLCPCB parts API (`selectSmtComponentList`), queried 2026-09-30. For "no free substitute exists" I downloaded JLCPCB's complete Basic list (351 parts) and Preferred Extended list (1,235 parts) today and searched them; that is **VERIFIED** for today's lists.

| Ref | LCSC | Part | Free substitute at JLCPCB? | What the substitute costs in function |
|---|---|---|---|---|
| U1 | C2913202 | ESP32-S3-WROOM-1-N16R8 module, $5.136 | **No.** No ESP32 or any Wi-Fi module is Basic or Preferred (VERIFIED). Every WROOM-1 / MINI-1 variant is Extended | Fee unavoidable. Other variants are no cheaper in a useful way (N8R8 $5.00) |
| MK1 | C5656610 | ICS-43434 I2S microphone, $4.31, stock 1,269 | **No.** No microphone of any kind is Basic or Preferred (VERIFIED) | Fee unavoidable. A cheaper Extended one exists: TDK T3902 (C3171752), $1.55, stock 7,928 (VERIFIED). Saves $5.53 for 2 boards. It is a PDM microphone: firmware change, borrowed footprint whose pin-1 corner is unverified (parts research §1). Same fee |
| J1 | C165948 | USB-C socket, $0.186 | **No.** The two lists contain no connectors at all (VERIFIED) | Fee unavoidable |
| U4 | C723789 | HE9073 3.3 V regulator, $0.116 | **No.** Free regulators are AMS1117 (5 mA idle current, needs 4.4 V in: cannot run from the cell), XC6206 (200 mA: too little for Wi-Fi's 355 mA), HT75xx (100–150 mA) (VERIFIED list; the consequences are INFERRED from listing values) | Fee unavoidable while the board runs from a battery |
| J2 | C295747 | JST PH battery socket, $0.236 | **No** (no connectors). The clone C3029440 is also Extended | The fee goes only if JLCPCB does not fit it: owner solders a through-hole JST PH (or the cell's two wires) into two holes. Needs the footprint changed to the through-hole version. Saves $3.54 |
| SW1 | C319409 | 6×6×5 mm main button, $0.094 | **Yes.** Basic TS-1187A (C318884, 5.1×5.1×1.5 mm, $0.0205), or reuse the reset button's part TS-1088 (C720477, Basic, 2 mm tall) | Lower and lighter click (1.6 N vs 1.8 N per listings); the printed cap needs a longer stem. Footprint and case change. Saves $3.40 |
| D4 | C458749 | RGB light, $0.073 | **Partly.** No RGB and no blue LED is free. Free 0805 LEDs: red (C84256, already on the board), green (C2297), white (C34499), yellow (C2296) | Three small lights under one window instead of one. "Sending" becomes white instead of blue; amber is red + green next to each other, mixed only by the case window (INFERRED to look acceptable; not tested). Spec R7's colours change, so this is the owner's call. Saves $2.78 |
| D2 | C509936 | RB168MM-40 low-leak diode, $0.117 | **Yes, with a cost.** RB160M-30 (C7502715, Preferred, $0.032), the part used before the power-path fix | Its leakage is about 6 µA instead of under 0.55 µA (datasheet check). Sleep total about 18.1 µA instead of 12.65 µA (INFERRED sum: 8 + 0.7 + 2 + 1.4 + 6). On a 400 mAh cell both are years on paper, so battery life does not visibly change; but the margin to the 20 µA requirement (R2) drops to under 2 µA and the leak grows when warm, so R2 could fail in a warm room. SOD-123 footprint instead of SOD-123F. Saves $2.82 |

### Fee rules (JLCPCB)

- **Economic PCBA: $3.07 per Extended part type; Basic and Preferred Extended pay nothing.** VERIFIED, https://jlcpcb.com/help/article/pcb-assembly-price (page dated 2026-09-09) and https://jlcpcb.com/help/article/pcb-assembly-faqs.
- **Standard PCBA is worse, not better:** setup $25.56, stencil $8.21, and a $1.53 loading fee on *every* part type, Basic included. VERIFIED, same price page. For this board's 26 part types that is about $39.78 in loading fees alone (INFERRED arithmetic) against $24.56 now.
- **Fees are per assembly order.** The FAQ says the fee is per Extended component used in an order. That two designs in one parcel each pay for the same module is INFERRED from that (each design is its own order); JLCPCB's pages do not say it in those words.
- **Parts pre-bought into "My Parts Lib" or consigned:** JLCPCB's pages on pre-order, consignment and using your own parts (all read today) say storage is free and **say nothing about the loading fee**. UNVERIFIED either way. INFERRED: the fee still applies, because it pays for loading a reel into the machine, not for the part. Needs a question to JLCPCB support.
- **Through-hole parts fitted by JLCPCB:** $3.58 per order plus $0.0164 per joint (VERIFIED, price page). So having them fit a through-hole battery socket saves nothing against the $3.07 fee.
- **A shared panel** (D-021 option B) pays each fee once for all designs on it; not priced here.

## 2. Tiers at JLCPCB

Arithmetic from today's API prices, with the cost stage's quantity model (needed + attrition, at least the minimum): INFERRED totals from VERIFIED unit prices.

| Change | Fee | Parts | Net |
|---|---|---|---|
| SW1 → TS-1187A | −3.07 | −0.47 + 0.14 | **−3.40** |
| D4 → red + green + white 0805 | −3.07 | −0.44 + 0.33 + 0.40 | **−2.78** |
| D2 → RB160M-30 | −3.07 | −0.23 + 0.48 (minimum 15) | **−2.82** |
| J2 not fitted | −3.07 | −0.47 | **−3.54** (plus a few cents for a connector bought elsewhere) |
| MK1 → T3902 | 0 | −8.63 + 3.10 | **−5.53** |

- (a) $66.89. (b) $63.49. (c) $57.89. (d) $54.35. Each minus $5.53 with the cheaper microphone.
- Floor at JLCPCB with everything above: **$48.82** goods, 4 fees ($12.28) left.
- In a shared parcel every dollar saved is worth $1.18 (VAT).
- **Hand-soldering beyond the battery socket (INFERRED, from the brief's "simple soldering" and the part shapes):**
  - Battery socket, through-hole: easy.
  - Main button, 6×6 J-lead: doable, but it is free to swap instead.
  - USB-C socket (16 pins at 0.5 mm): not realistic with a basic iron.
  - Microphone (pads underneath): not possible with an iron.
  - Module (41 edge pads at 1.27 mm plus a pad underneath): possible for a practised hand, a real risk for a first-timer, and it saves only $3.07. Not advised.
- Also known from earlier research, not re-checked today: ordering the bare PCB through the JLCONE desktop app is $2 instead of $4.

## 3. Other fabs for the same job (added 2026-09-30, second pass)

Job priced: 5 bare boards 30 × 60 mm, 2 layers, 1.6 mm, green; 2 of them assembled on one side; 26 part types, 46 parts. All figures read today (2026-09-30) from each fab's public calculator or page, logged out, no files uploaded. **No fab other than JLCPCB shows the price of the parts without an upload, a login or a person's quote**, so every total below has a hole where the parts go. For scale: the parts cost $28.04 at JLCPCB (VERIFIED, `boards/capture-clip/fab/cost.json`); JLCPCB's fees without parts are $38.85 (INFERRED: 66.89 − 28.04).

| Fab | Fees without parts (VERIFIED unless marked) | Parts | Min. assembled | Shipping to Israel | Our BOM/CPL as they are? |
|---|---|---|---|---|---|
| **JLCPCB** (baseline) | $38.85: PCB $4, setup $8.18, stencil $1.53, joints $0.57, 8 × $3.07 Extended fees | $28.04, instant price from its own stock | 2 | FedEx $29.59 | yes |
| **PCBWay** | **$34.00**: PCB $5.00, assembly $29.00 for 2 pcs (a special price; the calculator's own list price is $203.62), stencil free with assembly. No per-part fee in the calculator | **Unknown: a person quotes the BOM after upload.** PCBWay buys parts per project from distributors and says it adds no markup | calculator accepts 2 (the capabilities page says "as low as 5 pieces": conflict, UNVERIFIED which holds) | Global Standard $16.28 (6–10 days), FedEx $37.21, DHL $102.11 (for a PCB + assembly order) | Partly. Gerbers and position file yes; the BOM needs manufacturer part numbers, ours lists LCSC numbers (INFERRED; the file-format page was not read) |
| **Elecrow** | **$89.90**: PCB $4.90, assembly $73.00, stencil $12.00 | **Unknown: "Project manager will give the quotation within 1 working day"** | 1 | Economy Express $9.43, DHL $100.23 | No: Elecrow's own BOM template is asked for; position file required |
| **NextPCB, standard service** | PCB $19.99 shown (whether my 30 × 60 entry registered is UNVERIFIED); stencil free with assembly; assembly fee needs the BOM and position files uploaded: **not obtained** | Unknown without upload. Sources: HQ Online, DigiKey, Mouser | 1 | not shown before upload; DHL/FedEx | BOM and position file as spreadsheets; their sample format (INFERRED that ours needs re-columning) |
| **NextPCB "Rev 0" (free assembly on the first order, up to $500)** | Assembly fee $0 on the first order. PCB, parts and shipping are not said to be free: assume paid. **Price needs a login: stopped there** | HQ Online stock only; a part not in HQ Online is simply left off | **5 or 10** (not 2) | not shown | same as above |
| **Seeed Fusion** | PCB $9.90 for 10 pcs (the calculator's default quantity; 5 not tried). Assembly: **"Please login / create an account before uploading your file": stopped there.** Fee lines exist (setup, consumables, assembly, stencil) but show $0.00 until a BOM is uploaded | Unknown (login). Seeed's own parts list ("OPL") plus distributors | calculator default 5 (lower not tried) | the page advertises free express shipping on assembly orders; conditions not read | unknown |
| **AISLER** (Germany/Netherlands) | PCB €15.62 for 3 boards (INFERRED from the published formula €12 + 18 cm² × €0.067 × 3, before VAT; boards come in threes, so 6 boards €19.24). Stencil not charged separately. Assembly fee: **only shown after uploading the project: not obtained** | Bought from Western distributors, price shown instantly after upload. LCSC-only parts (HE9073, TP4057, the buttons) would need substitutes (INFERRED) | 1 | standard untracked post is free worldwide; UPS priced at checkout. No VAT charged outside the EU. Israel not checked specifically | No: native KiCad file or ODB++, parts assigned in their tool |

Sources (all 2026-09-30):
- PCBWay: assembly and PCB price from the calculator behind https://www.pcbway.com/quotesmt.aspx and https://www.pcbway.com/orderonline.aspx (its `/Quote/GetDeliveryDays` call: 2 pcs, 26 unique parts, 46 SMD parts → $29.00; 5 × 30 × 60 mm → $5.00). Shipping from the same page's `/Order/GetShipTypeIncludePrice`, country Israel. "PCBA provides free stencil", "$29/20 pcs": https://www.pcbway.com/pcb-assembly.html. Sourcing and "as low as 5 pieces": https://www.pcbway.com/assembly-capabilities.html. The "free shipping on PCBA" noted on 2026-09-29 did **not** show in today's calculator.
- Elecrow: https://www.elecrow.com/pcb-assembly.html (form filled with 30 × 60 mm, 5 PCBs, 2 assembled, 26 unique parts, 46 parts, Israel). That the assembly cost used my part counts is INFERRED from the form updating.
- NextPCB: https://www.nextpcb.com/pcb-assembly-quote (calculator), https://www.nextpcb.com/rev0-pcba (Rev 0 limits: 50 × 50 to 150 × 250 mm, green only, 5 or 10 assembled, HQ Online parts only, login to start), https://www.nextpcb.com/pcb-assembly-services (stencil free with assembly), https://www.nextpcb.com/free-pcb-prototype (the $20-off first PCB order excludes assembly orders; expires 2026-12-31), https://www.nextpcb.com/blog/Free-PCB-Assembly (the "$200 off" coupon needs an assembly fee of $300 or more, so it does not apply to a job this small).
- Seeed: https://www.seeedstudio.com/fusion_pcb.html. The old "free assembly for 5 boards" offer is ended: https://support.seeed.cc/portal/en/kb/articles/termination-of-the-5-pieces-pcba-free-assembly-offer. Coupons shown today: "$50 off PCBA for new customers", valid until 2026-10-30; it needs an account.
- AISLER: https://community.aisler.net/t/our-simple-pricing/102, https://aisler.net/en/products/assembly, https://community.aisler.net/t/shipping-methods/672.

What this means (INFERRED):
- **PCBWay is the only one that could come out cheaper, and only slightly.** Its fees are $4.85 lower than JLCPCB's and its slow shipping is $13.31 lower. If PCBWay's parts cost the same $28.04, the job alone is about **$78 delivered** against $96.48. But parts bought per project usually cost more than from JLCPCB's shelf (minimum packs, no shared reels); a parts quote of $45 brings goods to $79, over the $75 VAT line, and the total to about $112. So the honest range is **$78–115**, settled only by uploading the BOM for a quote (needs an account; not done). The $29 is a marked-down special and may carry conditions I could not see.
- **In the owner's real pattern (five designs in one parcel)** JLCPCB's shipping share is already about $6.50 per design, so PCBWay's shipping advantage disappears and only the $4.85 fee difference is left, minus whatever the parts cost extra.
- **Elecrow is dearer by about $50** before parts.
- **NextPCB Rev 0 does not fit:** the board would have to grow from 30 × 60 to at least 50 × 50 mm (a different, fatter device), five boards must be assembled instead of two (five sets of parts, about $70 instead of $28: INFERRED from JLCPCB prices), and whether HQ Online stocks the module, the microphone and the regulator was not checked. The saving is only the assembly fee.
- **Worth knowing, not priced:** NextPCB runs a sponsorship ("Accelerator #4") that gives **2 assembled boards free, parts and worldwide shipping included**, for original ESP32-S3 designs: https://www.nextpcb.com/blog/esp32-s3-free-pcba-prototypes-nextpcb-accelerator. It is applied for by e-mail, hobby projects are accepted only if they "contribute to the open-source community", the design must be original and not already public, and NextPCB must be allowed to show it in its marketing. Whether the campaign is still open is UNVERIFIED (the page has no end date). This is a publicity trade, so it is the owner's call.
- **AISLER and Seeed:** no number without an upload or login. AISLER is European-made with Western-distributor parts; nothing suggests it undercuts JLCPCB for a board built around LCSC parts.

## 4. Off-the-shelf route for 2 devices (added 2026-09-30)

Ready-made boards instead of a custom one. Prices read today. Exchange rate used for conversions: ₪3.068 per $ (open.er-api.com, 2026-09-30); every $ figure converted from ₪ is INFERRED. AliExpress prices are Choice listings shipped to Israel, before VAT (none is due on an order under $75), free shipping over ₪42; the browser profile looked signed in (no new-buyer price markers), but that is INFERRED. Sleep currents are **the maker's published figures, not measured by us**.

| Product | Price each | Flash / extra RAM | Microphone | Battery and charging | Published sleep current | To solder | Where, to Israel |
|---|---|---|---|---|---|---|---|
| **Our custom board** (for comparison) | $48.24 delivered alone, without cell and case | 16 MB / 8 MB | ICS-43434, I2S | socket + charger on board; 400 mAh cell bought separately | 12.65 µA (INFERRED sum, section 1) | nothing | JLCPCB |
| **M5Stack StickS3** (finished gadget in a case: screen, 2+ buttons, speaker) | $21.50 at M5Stack; **₪107.71 ≈ $35.11** on AliExpress Choice (official listing; "upcoming price" ₪96.94) | 8 MB / 8 MB | MEMS microphone behind an ES8311 audio chip (I2S, but the chip must be set up over I2C) | 250 mAh cell inside, USB-C charging | "Power off: 4.2 V @ 14.02 µA"; low-power states 52 and 102 µA | **nothing** | AliExpress Choice (VERIFIED listing); M5Stack's own shop: shipping shown only at checkout, not checked |
| **M5Stack Capsule v1.1** (finished, in a case: wake button, microSD slot, clock chip, buzzer, no screen) | $19.95, in stock | 8 MB / none (the first version's chip is ESP32-S3FN8; same for v1.1 is INFERRED) | SPM1423, PDM type | 250 mAh inside, USB-C charging | "Sleep current: 4.2 V @ 35 µA" | **nothing** | M5Stack shop; shipping at checkout, not checked. Not found on AliExpress Choice today |
| **Seeed XIAO ESP32S3 Sense** (bare board 21 × 17.8 mm plus a clip-on mic/camera/microSD board) | $13.90 at Seeed; ₪55–63 ≈ $18–21 on AliExpress Choice (third-party sellers, 4.3–4.9 stars) | 8 MB / 8 MB | digital PDM microphone | charger on board (50–100 mA per Seeed's table; which column is the Sense is unclear); **no battery socket**, two small pads underneath | Seeed's own table: bare XIAO ESP32S3 **14 µA**, but the **Sense: 3 mA** in deep sleep | battery wires to two small pads; a button and a light if wanted (it has one single-colour LED and tiny boot/reset buttons) | AliExpress Choice; Seeed shop shipping not checked |
| **Adafruit Feather ESP32-S3 (4 MB / 2 MB) + Adafruit ICS-43434 breakout** | $17.50 + $8.95 = $26.45 | 4 MB / 2 MB | ICS-43434, I2S: the same microphone as ours | charger and battery socket on board, battery gauge chip | "~100 µA" in deep sleep | 5 wires to the microphone, a button, pin headers | Adafruit; shipping to Israel not checked |
| **Unexpected Maker TinyS3[D] + the same microphone breakout** | $20.00 + $8.95 = $28.95 | 8 MB / 8 MB | as above | charger on board; battery **pads** only, the socket must be soldered | "ultra low", **no number published** (UNVERIFIED) | microphone wires, battery socket, a button | ships from Australia; the maker's own shop page says overseas postage is expensive; not priced |

Left out: **M5StickC Plus2** ($19.95) uses the older ESP32 chip, not the S3 our firmware is written for. Atom Echo-type products have no battery.

Sources (2026-09-30): https://shop.m5stack.com/products/m5sticks3-esp32s3-mini-iot-dev-kit and https://docs.m5stack.com/en/core/StickS3; AliExpress item 1005012064663942; https://shop.m5stack.com/products/m5stack-capsule-kit-v1-1-with-m5stamps3a and https://docs.m5stack.com/en/core/M5Capsule; https://www.seeedstudio.com/XIAO-ESP32S3-Sense-p-5639.html and https://wiki.seeedstudio.com/xiao_esp32s3_getting_started/ (power table); AliExpress items 1005009532378267, 1005009855991485; https://www.adafruit.com/product/5477, https://www.adafruit.com/product/6049; https://unexpectedmaker.com/shop.html, https://esp32s3.com/tinys3d.html.

What is given up against the custom board (INFERRED unless a figure is quoted above):

- **StickS3** is the only one that is a finished, closed device with nothing to solder and a sleep figure (14 µA) as good as ours. Given up: the battery is 250 mAh instead of 400 (recording and upload time per charge drop by about a third; idle life is still many months); storage is 8 MB instead of 16 (room for queued recordings roughly halves or worse; the firmware's storage layout must be redone); the light is a small screen instead of an RGB light (it can show colours and text, but draws more while on); the shape is a 48 × 24 × 15 mm stick with M5Stack's look, no clip, and no custom case. The microphone sits behind an audio chip, so the firmware's microphone driver, pin map, power-off logic and tests must be reworked: real work, size not estimated. Microphone quality for speech: 65 dB signal-to-noise per M5Stack, the same figure class as ours; not compared by ear.
- **Capsule v1.1** is also finished and has a microSD slot (far more storage than ours), but no extra RAM: whether the firmware (Opus encoder, Wi-Fi, TLS upload) fits without the 8 MB of extra RAM is UNVERIFIED and is the main risk. 35 µA asleep on 250 mAh is still about 300 days idle. PDM microphone: driver change. Light: one small RGB LED on the inner module (INFERRED from the StampS3 design; not read today).
- **XIAO Sense** is the cheapest and smallest, but by Seeed's own table it draws 3 mA asleep: a 400 mAh cell is empty in about 5.5 days doing nothing, against years for ours. That fails requirement R2 (20 µA) by a factor of 150. It also needs the battery soldered to tiny pads and has no usable main button or status light without adding them. Not recommended for a device that must wait on a shelf.
- **Feather + microphone** keeps our exact microphone, so the least firmware change on the audio side, but 4 MB of storage is too little for the current layout (16 MB), sleep is about 100 µA (400 mAh lasts about 5.5 months idle instead of years), and it is a hand-wired stack of two boards about 51 × 23 mm (Feather size, from memory, UNVERIFIED) plus wires, with a printed case needed. The owner has said he has no time or energy for soldering (DECISIONS.md).
- **TinyS3 + microphone**: as the Feather but with full memory, more soldering, no published sleep figure, and the dearest postage.

## 5. Hand-solder parts, delivered to Israel (added 2026-09-30)

Rough prices, AliExpress Choice listings shipped to Israel, read today, before VAT, free shipping on orders over ₪42.

| Part | Price | Source | Notes |
|---|---|---|---|
| JST PH 2-pin socket, through-hole, right-angle | about **₪4.68 for 10** (≈ $0.15 each, INFERRED conversion) | AliExpress item 1005006027334406 ("10PCS SH/JST/ZH/PH/XH … Male Pin Header Socket Dip/Right Angle", 4.9 stars, 1,000+ sold). Similar: item 1005008563429883, ₪4.15 for 20 | The price is the listing's headline; the exact "PH 2.0, 2P, right-angle DIP" variant's price was not opened. Confirms "cents" in option (d) |
| LiPo cell 400 mAh, size 602530 (6 × 25 × 30 mm) | **₪16.28 each** (≈ $5.31) | AliExpress item 1005007103616809 (4.8 stars, 500+ sold; delivery estimate 12–22 Oct) | Seller states over-charge, over-discharge, over-current and short-circuit protection. **Whether it comes with a JST PH plug, and which wire is plus, is UNVERIFIED** (a buyer question mentions PH 2.0; a review describes soldering it). Polarity must be checked with a meter before plugging in: a reversed cell destroys the board |
| LiPo cell 300 mAh, 602030 | ₪12.77 (≈ $4.16) | AliExpress item 1005008076861547 (4.9 stars, 104 sold) | same caveats |
| LiPo cell 2,000 mAh, 103450, "JST PH 2.0 mm 2-pin plug" in the title | ₪19.68 (≈ $6.42) | AliExpress item 1005009590723370 (4.8 stars, 4,000+ sold) | too big for this case (10 × 34 × 50 mm); listed to show that plugged cells exist on Choice |
| Known-good alternative: Adafruit #3898, 400 mAh, protected, JST PH, polarity matching our socket | $6.95 | https://www.adafruit.com/product/3898 (read 2026-09-29, parts research §10) | shipping from the US to Israel not checked; lithium cells often cannot go by cheap post |

Two cells cost about **$11–14** by either source (INFERRED). That amount is the same for every custom-board option and is already inside the StickS3 and Capsule prices. Solder, flux and tweezers were not priced (bounded out).

## 6. Per working device, all routes (added 2026-09-30)

"Working device" = electronics + battery, for 2 devices, delivered to Israel, ordered alone. A printed case is extra for the custom and bare-board routes and already included for the M5Stack ones. All totals INFERRED from the verified prices above; a cell is taken as $5.31 (AliExpress) where one is needed.

| Route | For 2 devices | Per device | What the owner gets | Confidence |
|---|---|---|---|---|
| Custom board at JLCPCB, as designed (a) | $96.48 + 2 cells $10.62 = **about $107** | **about $54** + case | exactly the designed device; 3 spare bare boards | High on the board; medium on the cell |
| Custom board at JLCPCB, all fee swaps (d) + cheaper microphone | $78.41 + $10.62 = about $89 | about $45 + case | slightly changed light, button and microphone; one solder job | Medium |
| Custom board at PCBWay | $78–115 + $10.62 = **about $89–126** | about $44–63 + case | the designed device, if their parts quote matches JLCPCB's | **Low**: parts price needs a human quote |
| Custom board at Elecrow | over $150 + cells | over $80 | same | Medium (parts still unquoted, but fees alone decide it) |
| NextPCB Rev 0 / Seeed / AISLER | not priceable without login or upload | unknown | Rev 0 would need a larger board and 5 assembled | none |
| NextPCB ESP32-S3 sponsorship | possibly $0 + cells | possibly about $5 | the designed device, in exchange for publicity; acceptance not guaranteed | Low (campaign status UNVERIFIED) |
| **2 × M5Stack StickS3** (AliExpress Choice) | ₪215.42 ≈ **$70** (under the $75 VAT line) | **about $35**, case and battery included | a finished stick with a screen; smaller battery, half the storage; firmware must be ported to it | High on price; **low on fit** (port not tried) |
| 2 × M5Stack Capsule v1.1 | $39.90 + shipping (unknown) | $20 + shipping | finished capsule with microSD; no extra RAM | Medium on price; low on fit |
| 2 × XIAO ESP32S3 Sense + 2 cells | about $37–41 + $10.62 = about $48–52 | about $24–26 + case | tiny, but about 5.5 days of idle battery life and fiddly soldering | High on price; high that it fails the sleep requirement (Seeed's own figure) |
| 2 × Feather ESP32-S3 + microphone + 2 cells | $52.90 + $10.62 + shipping (unknown) | about $32 + shipping + case | hand-wired, 4 MB storage, about 100 µA asleep | Medium |

Reading (INFERRED, not a decision):
- **No other fab clearly beats JLCPCB.** PCBWay might save $5–18 on an order shipped alone, or cost $20 more; it cannot be known without a quote, and in a shared parcel the possible saving shrinks to about $5. Changing fab also means redoing the parts list for another stock and re-checking every footprint against different parts, which risks more than it saves.
- **The only route that is clearly cheaper per working device is a ready-made M5Stack StickS3**, about $35 against about $54 plus a case, with nothing to solder and an equal published sleep current. The price of that saving is a different-looking device, a smaller battery, half the storage and a firmware port. Whether that trade is acceptable is the owner's call (look, feel and features); whether the port is feasible should be checked before it is offered as a real option.
- The cheap bare boards (XIAO, Feather, TinyS3) save money only on paper: they need soldering the owner does not want, and the XIAO Sense fails the battery-life requirement outright.

## Not done yet

Items 1–3 and 7 of the first pass are now filled in above (sections 3–6). Still open:

1. **Parts prices at the other fabs.** PCBWay, Elecrow and NextPCB price the BOM only after an upload and a person's quote; Seeed and NextPCB Rev 0 need a login; AISLER needs the project uploaded. None of that was done (no accounts, no uploads). To settle PCBWay, the owner would have to approve uploading the BOM there.
2. **Shipping to Israel from M5Stack's, Seeed's, Adafruit's and Unexpected Maker's own shops** (shown only at checkout). AISLER to Israel specifically.
3. **StickS3 / Capsule fit:** can the firmware run with 8 MB flash (StickS3) or without extra RAM (Capsule); ES8311 / PDM microphone driver; what the published "power off" figure means for wake-by-button. No board was tested.
4. **LiPo listing details:** plug type and polarity of the AliExpress 400 mAh cell.
5. **Ask JLCPCB support:** do My Parts Lib / consigned parts pay the Extended fee on each order? Is the fee charged once when two orders in one cart use the same part?
6. **Before adopting any swap:** re-run the datasheet check for the new part (TS-1187A footprint, RB160M-30 leakage when warm, T3902 pin 1), the case fit, and `cost`. Check the three-LED look with the owner on a choice page (R7's colours change).
7. **Not re-verified today:** FedEx $29.59 to Israel and the $75 VAT line (both from 2026-09-29 research); JLCONE's $2 PCB; JLCPCB coupons.
