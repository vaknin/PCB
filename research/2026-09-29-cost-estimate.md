# Cost estimate: JLCPCB Economic PCBA, shipped to Israel (2026-09-29)

Nothing was ordered, no account was used and no cart was filled. Prices come from JLCPCB's public parts-search API, the public quote form (read only; nothing saved to a cart) and the public shipping calculator. All figures are in USD, fetched on 2026-09-29.

Tags: **VERIFIED** = fetched today from the URL given. **FROM-LESSONS** = taken from `HARDWARE_LESSONS.md` and not re-checked. **INFERRED** = my estimate or model.

## Summary

- **The starter board, 5 assembled boards, delivered: about $129.** About $20 of that is Israeli VAT. The goods come to $79.49, which is just over the $75 VAT-free line.
- **Assemble 4 of the 5 boards instead: about $101 with no VAT.** You still get 5 bare boards. With 2 assembled it is about $84. Whether JLCPCB lets you pick 3 or 4 is **INFERRED**; check the options on the order form.
- **A typical small hobby board** (a module, 3–5 special parts): **about $85–$130 for 5**, depending mainly on whether the goods land under $75.
- **Per board, starter design:** about **$26 each at 5**, **$17 at 10** and **$11.50 at 30**. The fixed costs are about $58 per order (setup, stencil, special-part fees, shipping) and get shared across more boards.
- **Biggest costs:** the ESP32 module ($5.13 each, about 55% of the parts bill at 5 boards), shipping ($29.59 FedEx), the $3.07 fee for each special ("Extended") part ($18.42 for six), and the 18% VAT once goods pass $75.
- **Several different designs per shipment (the owner's real pattern, D-021):** each design as its own order, 5 bare PCBs with 2 assembled, all in one combined parcel: about **$72 per design** for the starter and **$52–70** for a typical small design, VAT included (section 5, INFERRED).
- Money-saving levers, largest first:
  - Keep goods under $75 (assemble fewer boards, or drop one or two Extended parts).
  - Use Basic parts where a Basic part exists.
  - Order the PCB through the JLCONE desktop app: $2 instead of $4 (VERIFIED on the quote page). The app itself was not tried.

## 1. Starter BOM priced at JLCPCB (qty 5 boards)

Source for every row: JLCPCB parts API `POST https://jlcpcb.com/api/overseas-pcb-order/v1/shoppingCart/smtGood/selectSmtComponentList` (keyword = LCSC code), **VERIFIED 2026-09-29**. The part page is `https://jlcpcb.com/partdetail/<code>`.

- **Order qty model (INFERRED):** order = max(needed + attrition, minimum). Attrition is the API's `lossNumber` and the minimum is its `leastPatchNumber`. Unit price is the tier for the order qty.
- Pref-Ext (Preferred Extended) parts have no loading fee.

| Ref | LCSC | Part | Library | Per board | Min / attrition | Order qty | Unit $ | Line $ |
|---|---|---|---|---|---|---|---|---|
| U1 | C2913202 | ESP32-S3-WROOM-1-N16R8 | **Extended** | 1 | 0 / 0 | 5 | 5.1332 (1–9) | 25.67 |
| U4 | C2909890 | SHT40-AD1B-R2 | **Extended** | 1 | 0 / 0 | 5 | 1.9022 (1–9) | 9.51 |
| U2 | C435835 | LDL1117S33R | **Extended** | 1 | 2 / 0 | 5 | 0.528 | 2.64 |
| J2 | C160404 | JST SM04B-SRSS-TB | **Extended** | 1 | 5 / 0 | 5 | 0.2608 | 1.30 |
| J1 | C165948 | HRO TYPE-C-31-M-12 | **Extended** | 1 | 2 / 0 | 5 | 0.1856 | 0.93 |
| F1 | C89653 | MF-NSMF075-2 PTC | **Extended** | 1 | 5 / 4 | 9 | 0.0368 | 0.33 |
| U3 | C20615824 | H5VUT2U ESD | Pref-Ext | 1 | 10 / 4 | 10 | 0.0579 | 0.58 |
| D3 | C19077497 | SMF5.0A TVS | Pref-Ext | 1 | 15 / 4 | 15 | 0.0282 | 0.42 |
| C3 | C45783 | 22 µF 0805 | Basic | 1 | 2 / 0 | 5 | 0.22 | 1.10 |
| C1 | C15850 | 10 µF 0805 | Basic | 1 | 20 / 10 | 20 | 0.065 | 1.30 |
| C2,C5 | C28323 | 1 µF 0805 | Basic | 2 | 20 / 10 | 20 | 0.0397 | 0.79 |
| C4,C6 | C49678 | 100 nF 0805 | Basic | 2 | 20 / 10 | 20 | 0.0191 | 0.38 |
| D1 | C2297 | green LED 0805 | Basic | 1 | 20 / 10 | 20 | 0.0163 | 0.33 |
| D2 | C84256 | red LED 0805 | Basic | 1 | 20 / 10 | 20 | 0.0134 | 0.27 |
| SW1,SW2 | C318884 | TS-1187A button | Basic | 2 | 5 / 5 | 15 | 0.0205 | 0.31 |
| R1,R2 | C27834 | 5.1 k | Basic | 2 | 20 / 10 | 20 | 0.0056 | 0.11 |
| R3,R6 | C17513 | 1 k | Basic | 2 | 20 / 10 | 20 | 0.0040 | 0.08 |
| R4,R5 | C17414 | 10 k | Basic | 2 | 20 / 10 | 20 | 0.0033 | 0.07 |
| R7,R8 | C17673 | 4.7 k | Basic | 2 | 20 / 6 | 20 | 0.0050 | 0.10 |
| | | **Parts total, 5 boards** | | | | | | **46.22** |

- **Unique Extended (fee-bearing) parts:** 6 (U1, U4, U2, J2, J1, F1), so 6 × $3.07 = **$18.42**.
- **Parts total at 10 boards:** $78.96 (ESP32 $4.5111, SHT40 $1.631, LDL1117 $0.4435).
- **Parts total at 30 boards:** $210.71 (ESP32 $4.0237, SHT40 $1.462, LDL1117 $0.3834).
- These tiers are from the same API, VERIFIED.
- **Since the earlier review:** the SHT40 1–9 tier is $1.90, not the $1.63 used before (that was the 10+ tier). Everything else is within a few cents of `research/2026-09-29-parts-starter.md`.
- **Stock today (JLCPCB assembly stock):** ESP32 module 31,596; SHT40 21,499. None of the parts is short.

## 2. Fixed costs

| Item | Value | Tag / source |
|---|---|---|
| PCB 2-layer, 50×50 or 100×100, qty 5 | $4.00 ("Special Offer") | **VERIFIED** https://cart.jlcpcb.com/quote (defaults: FR-4, 1.6 mm, green, HASL) |
| PCB, qty 10 (either size) | $5.00 | **VERIFIED**, same |
| PCB 50×50, qty 30 | $11.30 ($4 engineering + $7.30 board) | **VERIFIED**, same |
| PCB 100×100, qty 30 | $32.90 ($4 + $28.90) | **VERIFIED**, same |
| PCB ordered via the JLCONE desktop app (qty 5) | $2.00 | **VERIFIED** (price shown on the quote page; the app itself was not tried) |
| Economic PCBA setup | $8.18 | **VERIFIED** https://jlcpcb.com/help/article/pcb-assembly-price (page dated 2026-09-09) |
| Stencil (Economic) | $1.53 | **VERIFIED**, same |
| SMT joints | $0.0016 per joint | **VERIFIED**, same |
| Extended part loading fee | $3.07 per unique part | **VERIFIED**, same |
| Hand-soldering labour (only if through-hole parts) | $3.58 per order + $0.0164 per joint | **VERIFIED**, same |
| Economic PCBA quantity range | 2–50 boards | **VERIFIED** https://jlcpcb.com/capabilities/pcb-assembly-capabilities |
| Standard PCBA (for comparison) | $25.56 setup single-sided / $51.12 double-sided; stencil $8.21 / $16.42 | **VERIFIED**, pricing page |
| Starter joints per board | ~143 (150 pads minus 7 test points), so $0.23 per board | **INFERRED** (pad count from `boards/starter/kicad/starter.kicad_pcb`; JLCPCB's own count may differ) |
| Shipping to Israel, FedEx Express (6–9 business days) | $29.59 up to 0.5 kg; $35.74 at 1 kg; $50.50 at 2 kg | **VERIFIED** via the shipping calculator API behind https://jlcpcb.com/shipping (`queryOrderShippingVo`, country IL) |
| Shipping to Israel, DHL Express (3–6 days) | $102.02 up to 0.5 kg; $115.83 at 1 kg | **VERIFIED**, same |
| DDP (tax prepaid) to Israel | not offered (only DHL and FedEx, plus own-account options) | **VERIFIED**, same |
| Parcel weight: 5 or 10 small boards ≤ 0.5 kg; 30 boards 0.5–1 kg (50×50) or 1–2 kg (100×100) | | **INFERRED** |
| Israeli VAT | 0 if goods ≤ $75, shipping excluded; otherwise 18% × (goods + shipping) | FROM-LESSONS; the $75 threshold (since 2026-06-02, shipping excluded) is re-confirmed by search results today: VATupdate 2026-06-10 https://www.vatupdate.com/2026/06/10/israel-restores-75-vat-exemption-threshold-for-personal-imports/ and comGateway https://www.comgateway.com/blogs/the-quick-cheat-sheet-for-israel-customs-and-import-taxes/ |
| How shipping counts toward VAT | Shipping must be a separate line on the invoice, or it counts as goods | FROM-LESSONS |
| Customs duty, and the courier's clearance/handling fee when VAT is due | not priced; duty on PCBs and parts is usually 0; FedEx may add a clearance fee of roughly $5–15 | **INFERRED**, not verified |
| Coupons ("Save $30 / Save $20" on the quote page, "assembly from $0 with coupon") | not counted | **VERIFIED** as shown; eligibility for this account unknown |

## 3A. Starter board as designed

| Line | 5 boards | 10 boards | 30 boards |
|---|---|---|---|
| PCB | 4.00 | 5.00 | 11.30 |
| Setup + stencil | 9.71 | 9.71 | 9.71 |
| Joints (143 per board) | 1.14 | 2.29 | 6.86 |
| Extended fees (6) | 18.42 | 18.42 | 18.42 |
| Parts | 46.22 | 78.96 | 210.71 |
| **Goods (VAT test)** | **79.49** | **114.37** | **257.01** |
| Shipping (FedEx) | 29.59 | 29.59 | 35.74 (INFERRED ≤ 1 kg) |
| VAT 18% (goods > $75) | 19.63 | 25.91 | 52.69 |
| **Total delivered** | **$128.72** | **$169.88** | **$345.44** |
| **Per board** | **$25.74** | **$16.99** | **$11.51** |

**Assemble only some of the 5 PCBs.** You still receive 5 boards; the extras come bare.

| Assembled | Goods | VAT | Total | Per assembled board |
|---|---|---|---|---|
| 2 | 54.66 | 0 | **$84.25** | $42.13 |
| 3 | 62.94 | 0 | **$92.53** | $30.84 |
| 4 | 71.22 | 0 | **$100.81** | $25.20 |
| 5 | 79.49 | 19.63 | $128.72 | $25.74 |

- Assembling 4 of 5 costs less per board than assembling all 5, because it stays under the VAT line.
- **INFERRED:** that Economic PCBA lets you pick 3 or 4. 2 is its minimum (VERIFIED). Check the "PCBA Qty" choices on the order form.
- **Other ways under $75 at 5 assembled boards (INFERRED):**
  - F1 PTC → Basic 0 Ω jumper saves about $3.35.
  - PCB through JLCONE saves $2.
  - Together that is about $74.1: under the line, but only just.

## 3B. A "typical" small hobby board (assumption)

Assumed:
- 2-layer board, up to 100×100 mm, green, HASL.
- About 30–40 placed parts, 180–250 joints, all on one side.
- One Wi-Fi/MCU module at ESP32 prices ($5.13 / $4.51 / $4.02 at 5 / 10 / 30).
- 3–5 unique Extended parts in total, including the module.
- Other Extended parts cost $0.60–$3.00 per board combined.
- 10–14 Basic lines, which cost $3–6 at 5 boards (minimum orders dominate), $3.5–7 at 10 and $6–12 at 30.

This is all **INFERRED**. The fees, PCB prices and shipping rates used are the VERIFIED ones above.

| | 5 boards | 10 boards | 30 boards |
|---|---|---|---|
| Goods | $56–78 | $81–116 | $205–293 |
| Shipping | $29.59 | $29.59–35.74 | $35.74–50.50 |
| VAT | $0–19 | $20–27 | $43–62 |
| **Total delivered** | **$86–127** | **$131–179** | **$284–405** |
| **Per board** | **$17–25** | **$13–18** | **$9.50–13.50** |

At 5 boards the low end stays VAT-free and the high end does not. The $75 line is the biggest swing in the whole estimate: about $20.

## 3C. How fixed costs dilute (starter board)

"Once per order" is the same whatever the quantity:
- setup $8.18
- stencil $1.53
- Extended fees $18.42
- shipping $29.59–35.74

That comes to about $58–64 per order.

| Qty | Once-per-order costs | Per-board costs (parts + joints + PCB share) | VAT per board | **Per board delivered** |
|---|---|---|---|---|
| 5 | $57.72 → $11.54 each | $10.27 | $3.93 | **$25.74** |
| 10 | $57.72 → $5.77 each | $8.63 | $2.59 | **$16.99** |
| 30 | $63.87 → $2.13 each | $7.63 | $1.76 | **$11.51** |

The per-board part cost falls too, because the ESP32 gets cheaper in the 10+ and 30+ price tiers.

## 4. Once per design vs per board

| Once per design (or per order) | Per board |
|---|---|
| PCBA setup $8.18 and stencil $1.53 (each order) | Parts (the module dominates: $4–5.13) |
| Extended loading fees, $3.07 × unique Extended parts (each order) | Joints, about $0.23–0.40 |
| Shipping, $29.59+ (each order) | PCB board area (tiny at 5–10 boards, which are a flat $4–5) |
| Minimum-order leftovers of cheap Basic parts (about $3–6) | VAT at 18% once goods pass $75 (applies to the whole order, shipping included) |
| PCB engineering fee ($4, from 30 boards up) | |

## 5. Per design: five different designs in one parcel (D-021, option A)

The owner's real pattern: 1–2 copies each of about 5 different boards per shipment, never 5 copies of one board. Under D-021, each design is its own order with 5 bare PCBs, 2 of them assembled. JLCPCB's "Combine Shipping" then sends the orders in one parcel.

- **Rules used (all VERIFIED 2026-09-29):**
  - Economic PCBA takes 2–50 boards (https://jlcpcb.com/capabilities/pcb-assembly-capabilities).
  - 5 bare with 2 assembled is allowed; the 3 blank ones may come back with solder on them (https://jlcpcb.com/help/article/pcb-assembly-faqs-part-2).
  - Combine Shipping holds orders and ships them as one parcel. It is free for the first 15 days (https://jlcpcb.com/help/article/combine-shipping-service).
- **Assumptions (INFERRED):**
  - Setup, stencil and Extended fees are charged in every order, so each design pays its own.
  - Shipping is paid once per parcel. Five orders of 5 small boards (25 PCBs) weigh 0.5–1 kg, so FedEx is $29.59–35.74, split 5 ways.
  - The parcel's goods are far over $75, so 18% VAT applies to goods plus shipping.
  - That 5 is the smallest bare-PCB quantity is **not confirmed**.

**The starter as one of the five** (parts for 2 boards priced with the same minimums as section 1):

| Line | One design |
|---|---|
| PCB (5 bare) | 4.00 |
| Setup + stencil | 9.71 |
| Joints (2 × 143) | 0.46 |
| Extended fees (6) | 18.42 |
| Parts for 2 boards | 22.08 |
| **Goods** | **54.66** |
| Shipping share (1/5 of $29.59–35.74) | 5.92–7.15 |
| VAT 18% on goods + shipping share | 10.90–11.13 |
| **Per design, delivered** | **$71.50–72.95** |

**A typical small design** (section 3B's assumptions, at 2 boards; INFERRED):

| | Low (3 Extended parts) | High (5 Extended parts) |
|---|---|---|
| Goods | $37.96 | $52.13 |
| Shipping share | $5.92–7.15 | $5.92–7.15 |
| VAT | $7.90–8.12 | $10.45–10.67 |
| **Per design, delivered** | **$52–53** | **$68.50–70** |
| **Five such designs** | **$259–266** | **$342–350** |

**What the shared parcel does:**
- **Shipping:** each design pays $6–7 instead of $29.59.
- **VAT:** a parcel of five designs is always over $75, so VAT adds 18%. A single design shipped alone at 2 assembled stays under $75 and pays none. Even so, the starter costs $84.25 alone (section 3A) against ~$72 in the shared parcel, and a low typical design $67.55 against ~$52.
- **The fees are the rest.** Setup, stencil and Extended fees are $28–35 of each design's goods. The Extended fees alone are $9–18, and each design pays them again, even when several designs use the same module.
- My earlier rough figure of $250–300 for five designs holds for designs at the low end. Designs with 5 Extended parts reach about $350.

**Ways to cut it (from D-021):**
- Use Basic parts wherever one exists. Each Extended part dropped saves $3.07 plus its VAT, per design.
- Pre-order a module that recurs (e.g. the ESP32) into "My Parts Lib". Pre-ordered parts are stored free and used first in later orders (VERIFIED, https://jlcpcb.com/help/article/smt-reorder-process-overview). Whether that avoids the $3.07 fee on each later order is **unknown**; check before relying on it.
- A shared panel (D-021 option B) pays setup, stencil and each Extended fee once for all designs. That saves roughly $40–70 per shipment (INFERRED, not priced), at the cost of the constraints listed in D-021.

## Bottom-side (double-sided) assembly

- **Economic PCBA does offer "single & double sided placement"** (VERIFIED, capabilities page).
- **The public price page lists no separate double-sided fee for Economic.** Standard PCBA doubles setup to $51.12 and the stencil to $16.42 (VERIFIED).
- **Estimate for Economic double-sided:** about +$10 per order, for a second setup and stencil (**INFERRED**, not verified). It could force Standard PCBA, which would add about $50–60.
- **The starter board has no bottom-side parts** (all "Top" in `starter-cpl.csv`), so this does not apply to it.

## Could not verify

- Whether the Economic PCBA quantity can be 3 or 4 (of 5 PCBs).
- Whether JLCPCB counts joints the same way (143 per board).
- The double-sided Economic fee.
- Parcel weights.
- Courier clearance fees, and customs duty other than VAT.
- Coupon eligibility.
- Whether JLCPCB's cart applies the minimum/attrition quantities exactly as modelled.
- The real cart total, with shipping as its own invoice line, is the final check before paying.
