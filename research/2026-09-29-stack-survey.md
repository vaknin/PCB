# Stack survey: alternatives to KiCad 10 + pcbgen + Freerouting (2026-09-29)

Researched by session pcb-99 on 2026-09-29, relayed to this session; the facts below are
theirs and were not re-checked here. Owner asked for it to be kept in the project docs.

**Verdict:** KiCad 10 + pcbgen + Freerouting 2.4.1 is still the right stack. Nothing found is
both better and within the owner's rules (1.0+, local CLI, no lock-in). Freerouting 2.4.1
(2026-09-03) is still the latest release.

## Per tool
- **Diode `pcb` / Zener** (github.com/diodeinc/pcb): v0.4.61 (2026-09-27), MIT, ~20 releases a month, KiCad 10.
  - Placement-preserving netlist-to-PCB sync: the netlist owns refs/values/footprints/nets; KiCad owns position/rotation/routing/zones; UUIDs stay stable.
  - Its autorouter was removed on main 2026-09-28. The parts registry needs `pcb auth login`. Also uses SWIG pcbnew.
  - The only credible rival: revisit at 1.0. Worth copying its sync rules.
  - `pcb dfm` (JLCPCB profiles; takes a plain .kicad_pcb since 0.4.55) is usable as an optional advisory second opinion.
- **JITX 4.4.1:** proprietary. Free tier only for CERN-OHL-P open designs; login plus licence refresh. Interactive router only; KiCad export unverified. Out.
- **SKiDL 2.3.0** (2026-07-28): UUIDs are now deterministic (uuid5), but it still writes the old 20230409 format and regeneration loses manual edits. Schematic generation is moving to `schematizer` 0.1.0 (alpha). D-005 stands (its drawing-quality reason still holds).
- **atopile:** retirement confirmed (blog 2026-08-06). 0.15.9 is a bridge release (2026-09-12). Public repo frozen since 2026-03-11; API/MCP only on the enterprise plan. Out.
- **tscircuit 0.0.2652:** 380 releases in 30 days, no 1.0.
  - Router false-success bugs #1964/#2147/#2654 still open; fix PRs #2462 and #2660 closed unmerged.
  - KiCad export has 23 open issues. Out.
- **Flux.ai:** cloud only, credits ($50–250/month). Its MCP can only message Flux's own agent. No KiCad project export. Out.
- **Quilter:** web app only, no API. Free tier trains on your data. Places and routes KiCad boards; KiCad 10 unverified. Out.
- **DeepPCB:** REST API (api.deeppcb.ai) takes .kicad_pcb for "Placement" or "Routing" jobs; $30/h, 30-minute free trial. The only scriptable AI place-and-route. Optional paid fallback for dense boards; results re-checked by our own DRC; needs the owner's OK (costs money).
- **MCP servers:** mixelpixx/KiCAD-MCP-Server 2.8.2 and Konnect wrap the same SWIG, kicad-cli and Freerouting calls we already make; nothing added. No-MCP decision (D-004) confirmed.
  - aklofas/kicad-happy 2.2.1 (MIT) has advisory analysers (power tree, ESD/connector audit, EMC, BOM). Optional second opinion, never a gate.
- **KiCad file libraries:** kiutils, kicad-skip and kicad-sch-api are stalled; kicad-tools 0.22 is too young. Keep our own S-expression writer.
- **Official kicad-python 0.8.0** needs the GUI on KiCad 9/10; headless use arrives with KiCad 11.

## Risk
KiCad plans to remove the SWIG pcbnew bindings in KiCad 11
(dev-docs.kicad.org/en/apis-and-binding/pcbnew/). KiCad 11 is expected around early 2027
(inferred from 10.0.0 on 2026-03-20); Arch will upgrade to it on `-Syu`. Keep every pcbnew
call isolated (`pcbgen/kicad.py`, one process per step). Migration path: write .kicad_pcb
directly, or KiCad 11's headless IPC API.

## Evidence of real results
No 2026 source reports fabricated-board success rates by toolchain. The documented real
orders (StationX; a note.com write-up of a 4-layer, 99-part JLCPCB order) use KiCad-based
flows like ours. In the note.com write-up, an AI review caught 9 wrongly rotated or reversed
parts before ordering, which supports the CPL rotation check in `fab.py`.
