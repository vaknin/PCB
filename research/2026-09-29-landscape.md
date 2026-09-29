# Tool, fab and import landscape (researched 2026-09-29)

Four parallel research agents. Tags: **Q** quoted from a live primary source that day;
**INFERRED** / **UNVERIFIED** as marked. Recheck before relying on anything here; this
space changes monthly.

## atopile
- Local CLI 0.15.9 (PyPI, 2026-09-12) is **soft-deprecated**. The supported version,
  0.16, runs only in the browser with a login at app.atopile.io and has its own
  built-in agent. https://atopile.io/blog/atopile-v16 (2026-08-06)
- Public GitHub `main` has had no commits since 2026-03-11 ("stale"; development moved to a private repo).
- 0.15.x targets **KiCad 9**. A board saved by KiCad 10 breaks the round trip
  (`UnexpectedType ... 'tenting'`, issue #1822, "fixed in v0.16" only).
- Part picking needs `ato auth login` since 0.15.8. After the July 2026 service
  shutdown, part picking broke on older versions (#1829–1832).
- There is no local router. "AI layout" goes through DeepPCB via atopile's gateway (INFERRED from source).
- Outputs: gerbers, plus BOM and pick-and-place in JLCPCB format. DRC runs through kicad-cli, and a DRC failure only fails the build if `fail_on_drcs` is set.
- Packages exist for: ESP32-S3-WROOM, USB-C, LDOs (LDK220, TLV75901), SHT4x, BME280, test points.
- MCP server removed in 0.15. Community skill: github.com/mawildoer/atopile-agent-skill.
- Requires Python >=3.14,<3.15.
- EEBench (eebench.org, by atopile):
  - Opus 5 scored 61.6%, confirmed. The leader is Opus 5.5 (xhigh) at 75.0%.
  - Layout is out of scope in V1.

## tscircuit
- Nothing is 1.0. There are several releases a day (tscircuit 0.0.2652, @tscircuit/cli 0.1.2178,
  capacity-autorouter 0.0.941 as of 2026-09-28). **Pin versions.**
- The CLI prefers Bun and falls back to tsx on Node.
- Official skill: `npx skills add tscircuit/skill` (github.com/tscircuit/skill).
- Autorouter:
  - It is local and works offline. Cloud and Freerouting providers are also available.
  - **Open bugs where the router reports `solved=true` but leaves shorts:**
    - overlapping vias on an ESP32 board (autorouter #2147)
    - a via on a pad (#2654)
    - crossings on the same layer (#1964)
  - USB differential-pair constraints are not enforced (#4958).
  - Always run `tsci check shorts`.
- Own DRC (@tscircuit/checks) with JLCPCB presets (`fabricatorPreset: jlcpcb_standard`).
  **No real ERC.**
- The CLI checker and the web checker have diverged.
- KiCad export (`tsci export -f kicad_pcb|kicad_sch|kicad_zip`) has bugs:
  - the exported schematic fails KiCad ERC (#549)
  - pad-number collisions merge pads (#535)
  - SMD/through-hole attributes are missing (#585)
- Parts come from jlcsearch and include is_basic, is_preferred, stock and price.
  ESP32-S3-WROOM-1-N16R8 (C2913202) is an Extended part.
- Fab output: a zip with gerbers, BOM and pick-and-place (JLCPCB format). Board cutouts are dropped (#3302, open).
- The "Order" button is JLCPCB-only.
- STEP export exists (`tsci export -f step`).
- The Opus 5.5 Bluetooth speaker board is confirmed: "qualified_pass", 47 warnings, $6.47,
  not fabricated. remakebench.com/results/tscircuit-bluetooth-speaker-claude-opus-5-5-high

## KiCad / routing / MCP
- KiCad 10.0.0 was released 2026-03-20. Arch `extra` has 10.0.6.
- `kicad-cli` v10 can do:
  - ERC and DRC with `--format json`, `--exit-code-violations`, `--schematic-parity` and `--refill-zones`
  - gerbers, drill, pos, BOM, STEP, jobsets
- **kicad-cli has no Specctra DSN/SES.** Use the SWIG `pcbnew` module
  (`ExportSpecctraDSN` / `ImportSpecctraSES`). SWIG is deprecated and planned for removal in KiCad 11.
- IPC API (kicad-python 0.8.0) needs a running KiCad GUI in 9 and 10. A headless API server arrives in 11.
- Freerouting **v2.4.1 (2026-09-03) needs Java 25**; v2.2.x runs on Java 21. Two ways to run it:
  - Headless: `java -jar freerouting.jar -de x.dsn -do x.ses --gui.enabled=false`
  - Docker: an official image, non-root since 2.4
  - Either way, pass `--router.copperToEdgeClearanceUm=500`, because KiCad's DSN export leaves the edge clearance out.
- MCP servers:
  - mixelpixx/KiCAD-MCP-Server v2.8.2 (2026-09-25): active, has KiCad 10 fixes, and runs headless on SWIG.
  - Konnect (Rust, IPC API, AGPL, beta) is its successor, but it needs the GUI.
- Fab DRC rules:
  - Cimos/KiCad-CustomDesignRules (JLCPCB and PCBWay `.kicad_dru`, KiCad 9/10, 2026-08).
  - NextPCB's official rules repo was last updated 2024-12 (stale).
- Fab plugins: Bouni/kicad-jlcpcb-tools (active, KiCad 10) and bennymeg Fabrication Toolkit 5.3 (KiCad 10, has a CLI mode).
- SKiDL 2.3.0 (2026-07-28) is active and mature, but produces a netlist or schematic only.
- circuit-synth 0.12.1 looks stalled (last push 2026-03).
- AI routers:
  - DeepPCB: a free trial of about 30 minutes, then $30/h. It has a KiCad plugin.
  - Quilter: a free tier for personal use (designs are used for training). KiCad in and out, and you supply the outline and fixed parts.
- StationX build: verified (app.stationx.net/articles/ai-pcb-design, July 2026).
- CiferTech + NextPCB: verified as a sponsorship. Whether NextPCB assembled his boards is UNVERIFIED.

## Fabs
- **JLCPCB** (help pages updated 2026-09-09, Q):
  - Economic PCBA: setup $8.18, stencil $1.53, $0.0016 per joint, **$3.07 per unique Extended part**. Basic and "Preferred Extended" parts have no loading fee.
  - Standard PCBA: setup $25.56, stencil $8.21, $1.53 per unique part.
  - Through-hole: hand soldering at $3.50 plus about $0.017 per joint.
  - 5× 2-layer PCB: $4 on the web, $2 through the JLCONE app.
  - Coupons for new users: $10 PCBA, $6 PCB, $10 shipping, 2×$10 for 3D printing. Only one coupon per order.
  - Shipping to Israel (0.29 kg): **FedEx $29.59** (6–9 business days), DHL $102. No DDP option, so VAT is paid on import. An Israeli ID number is needed for customs.
  - JLC3DP prints can go in the same cart. Whether PCBA and 3D prints ship in one box is UNVERIFIED.
- **NextPCB**:
  - Rev 0 free assembly (up to $500, first order) is **live**. Conditions:
    - 5 or 10 PCBA only
    - parts only from HQ Online
    - **minimum board 50×50 mm**
    - green solder mask only
    - through-hole is OK
    - Whether the PCB, stencil, parts and shipping are covered is UNVERIFIED; assume they are not.
  - The "$200 off assembly (1–10 pcs)" offer is still advertised. It covers SMT only, turnkey orders only, and runs until 2026-12-31 (terms partly UNVERIFIED).
  - The $0.10 PCB promo is for bare PCBs only.
  - NextPCB has no 3D printing service.
- **PCBWay**: $29 assembly for 1–20 pcs, **free shipping on PCBA** (probably capped at $30, UNVERIFIED), and a $5 new-user coupon.

## Israel import
- The VAT exemption is **$75** since 2026-06-02; the Knesset revoked the $130 order. No pending change was found.
  Sources: Globes, Times of Israel 2026-06-01, ICL Global.
- Shipping does not count toward the $75 **only if the invoice lists it separately**.
- Above $75: 18% VAT on goods plus shipping. From $75 to $500 there is no duty or purchase tax.
- DHL Israel fees:
  - clearance ₪31 for $75–150, ₪71.60 for $150–1,000
  - computer fee ₪20.50 when the value is over $100
  - FedEx fees UNVERIFIED
- AliExpress collects Israeli VAT at checkout on orders over $75.
- AliExpress prices (2026-09-29, before VAT):
  - ESP32-S3 USB-C dev board: ₪10–52
  - SHT40 Qwiic module: about ₪5
  - BME680: ₪27–34
  - Watch out: cheap "BME280" boards are often BMP280.
- Local shop: 4project.co.il sells SparkFun Qwiic boards, VAT included, but has no SHT4x.
