# capture-clip: cost options (2026-09-30) — PARTIAL, stopped early

Question from the owner: are we ordering this the cheapest sensible way? Is there a better fab? Can we avoid the "Extended part" loading fees, even at some cost in function?

**Status: stopped before it was finished, at the coordinator's request.** The JLCPCB side (fees, substitutes, tiers at JLCPCB) is done. The other fabs, the off-the-shelf route and the prices of hand-solder parts are **not done**; see "Not done yet" at the end. Nothing was ordered, no account was used, no file other than this one was changed.

Tags: **VERIFIED** = fetched today (2026-09-30) from the source named. **INFERRED** = my arithmetic, model or judgement.

## Summary so far (JLCPCB only)

All totals are for 5 bare boards with 2 assembled. "Alone" = this design ships by itself (FedEx $29.59, goods under $75 so no VAT). "Shared" = one of about five designs in a parcel (shipping share about $6.50, 18 % VAT on everything), which is the owner's real pattern (D-021). Battery and printed case are not included in any row; they are the same for all of them.

| Option | Goods | Delivered alone (per device) | Delivered in a shared parcel (INFERRED) | What changes for the owner | Confidence |
|---|---|---|---|---|---|
| (a) As designed, 8 fees | $66.89 | $96.48 ($48.24) | about $86.6 | nothing | High: live prices, fees from JLCPCB's price page |
| (b) Main button → a free ("Basic") button, 7 fees | $63.49 | $93.08 ($46.54) | about $82.6 | a lower button with a lighter click; the printed cap hides it. Small layout and case change | High on price; feel is INFERRED |
| (c) b + three separate lights instead of the RGB one + the older diode, 5 fees | $57.89 | $87.48 ($43.74) | about $76.0 | "sending" shows white instead of blue; amber is red and green side by side; sleep current about 18 instead of 13 µA (no practical change in battery life, but almost no margin to the 20 µA target, and worse when warm) | Medium: prices verified, the effects are INFERRED |
| (d) c + battery connector left off, owner solders it, 4 fees | $54.35 + a connector (cents; price not checked) | $83.94 ($41.97) | about $71.8 | two easy through-hole solder joints per board | Medium |
| Add-on to any row: cheaper microphone (TDK T3902) | −$5.53 | −$5.53 | about −$6.5 | firmware reads a different microphone type (PDM); one footprint detail is unverified, so it adds first-order risk | Medium-low |
| Other fabs (PCBWay, NextPCB, Elecrow, Seeed, AISLER) | not done | | | | |
| Off-the-shelf dev board + microphone in a printed case | not done | | | | |

What I would pick, if asked (not a decision): (b), and nothing further until the other fabs are priced. The remaining fee-saving swaps save $3 each and each costs something visible or some risk; a respin costs far more than $3.

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

## Not done yet

Two background research agents were started for the items below and had not reported when this was stopped; their results are **not** in this file.

1. **Other fabs for the same job** (PCBWay, NextPCB/HQ incl. the free-assembly offer and its 50 × 50 mm minimum, Elecrow, Seeed Fusion, AISLER, others): no figures yet. Next: each public calculator for 5 × 30×60 mm 2-layer plus assembly of 2; their setup/stencil/per-part fees; minimum assembled quantity; how parts are sourced and whether a human must quote the BOM; shipping to Israel; whether the JLCPCB-style BOM/CPL is accepted. NextPCB's offer would need the board enlarged to 50 × 50 mm or more.
2. **Off-the-shelf route for 2 devices** (e.g. Seeed XIAO ESP32S3 Sense, Unexpected Maker TinyS3/FeatherS3, Adafruit Feather ESP32-S3, M5Stack products with microphone and battery, plus an I2S microphone breakout): prices, flash/PSRAM size, verified sleep current, what must be soldered, and what is given up (size, sleep current, light, storage).
3. **Hand-solder parts prices delivered to Israel** (through-hole JST PH, a protected 400–500 mAh cell, solder/flux/tweezers) from AliExpress Choice or LCSC.
4. **Ask JLCPCB support:** do My Parts Lib / consigned parts pay the Extended fee on each order? Is the fee charged once when two orders in one cart use the same part?
5. **Before adopting any swap:** re-run the datasheet check for the new part (TS-1187A footprint, RB160M-30 leakage when warm, T3902 pin 1), the case fit, and `cost`. Check the three-LED look with the owner on a choice page (R7's colours change).
6. **Not re-verified today:** FedEx $29.59 to Israel and the $75 VAT line (both from 2026-09-29 research); JLCONE's $2 PCB; JLCPCB coupons.
7. **Summary table rows for other fabs and the off-the-shelf route**, and a final per-working-device comparison across all routes.
