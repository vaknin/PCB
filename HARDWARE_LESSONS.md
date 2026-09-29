# Hardware lessons

Read at the start of every session. Only verified facts; each with source and date.
Anything unverified goes in `DECISIONS.md` open questions or is tagged UNVERIFIED.

## Verified part data
| Part | Fact | Source | Date |
|---|---|---|---|
| ESP32-S3-WROOM-1-N16R8 (LCSC C2913202) | JLCPCB **Extended** part (+$3.07 loading fee in Economic), $5.13 @1 | jlcpcb.com/partdetail/3198300 | 2026-09-29 |

(Pinouts, voltage ranges and footprints get added here as each is checked against its datasheet.)

## Fab rules and prices
- **JLCPCB Economic PCBA:**
  - setup $8.18, stencil $1.53, $0.0016 per joint
  - $3.07 per unique Extended part; Basic and Preferred-Extended parts are free
  - (2026-09-09 help page)
- **JLCPCB → Israel:**
  - FedEx ~$30 (6–9 business days), DHL ~$102.
  - No DDP, so VAT is paid on import. An Israeli ID is needed for customs.
- **NextPCB Rev 0 free assembly:**
  - minimum board 50×50 mm, 5 or 10 boards only, parts only from HQ Online, green only
  - (2026-09-29)
- **Israel:**
  - VAT exemption $75, goods only, and only if the invoice lists shipping separately.
  - Above that, 18% VAT on goods plus shipping.
  - (since 2026-06-02)

## Tool gotchas
- **tscircuit autorouter:**
  - It can report success and still leave shorts (overlapping vias, a via on a pad). Always run `tsci check shorts` and an independent KiCad DRC.
  - It does not enforce USB differential pairs.
  - Pin versions: it releases several times a day.
- **tscircuit → KiCad export:**
  - The exported schematic fails KiCad ERC.
  - Pad-number collisions can merge pads.
  - Board cutouts are dropped from gerbers.
- **kicad-cli (v10):** it cannot export Specctra DSN or import SES. Use the SWIG `pcbnew` module (deprecated; removed in KiCad 11).
- **KiCad 10 IPC API:** it needs the GUI running. Headless automation means SWIG or editing the file directly.
- **Freerouting 2.4.x:**
  - It needs Java 25.
  - KiCad's DSN export omits board-edge clearance, so pass `--router.copperToEdgeClearanceUm=500`.
- **atopile 0.15.x:** it cannot read KiCad 10-saved boards.

## Mistakes to avoid
(None yet.)
