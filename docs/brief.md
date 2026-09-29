# Hardware project handoff: AI-designed custom PCBs

(Owner's original brief, 2026-09-29. Verbatim; keep as the source of truth for goals and rules.)

## Who I am
I build software projects with AI coding agents. I'm not an electrical engineer, and I don't want to do circuit design. That's your job. I'm happy to talk to you, make decisions, and do basic physical work: plugging things in, simple soldering (through-hole parts, headers, connectors), flashing firmware. I have a soldering iron at home. I don't own a 3D printer.

My ideal is doing nothing except talking to you. But if doing some of the work myself makes a project significantly cheaper (for example, soldering a few parts or buying components on AliExpress instead of having the fab source them), I'm open to it. Show me the numbers and let me decide.

I live in Haifa, Israel, so orders ship from China to Israel.

## The goal
Build a repeatable pipeline where I describe a hardware idea and end up with a working custom PCB in my hands:
idea → you design the circuit as code → automated checks → fab builds it (ideally fully assembled) → you write the firmware → enclosure.

The first project is: [DESCRIBE IT HERE, or leave this line and ask me].
If I haven't decided, propose a simple starter board that proves the pipeline: an ESP32-S3 module, USB-C power and flashing, a status LED, and a sensor.

## Why this approach
LLMs are strong at circuit-level design and at writing code, and weak at spatial layout. The code-first approach plays to that: the circuit is written as code that you generate, compile and check, and autorouters or dedicated tools handle the geometry.

What I learned from research in September 2026 (verify anything you rely on; this space changes monthly):
- EEBench grades AI-designed circuits in simulation. Anthropic models scored at the top (Opus 5: 61.6%). That's good, not reliable, and the benchmark doesn't test layout.
- In public tests, Opus 5.5 designed a Bluetooth-speaker board in tscircuit, but the checker flagged 47 warnings. In a separate RF test, one of four antennas it laid out failed because of a layout mistake.
- Simple, module-based boards go well. People report Claude-designed boards working on the first order.
- StationX's documented build caught all five AI near-misses using deterministic gates (datasheet checks, design-rule check, sanity tests), not the AI's own confidence.
- The YouTuber who inspired this (CiferTech) uses NextPCB for his ESP32 boards.

## Tool candidates (evaluate, then pick)
1. atopile: the default candidate. Circuits in the "ato" language, compiles to KiCad, picks parts from JLCPCB/LCSC, and the compiler catches errors. The team behind it runs EEBench, and a Claude Code agent skill exists for it. Weakness: no autorouting, so layout happens in KiCad.
2. tscircuit: TypeScript/React. It has a built-in autorouter, pulls JLCPCB footprints, and has an order button to JLCPCB. It's the most end-to-end option, but less proven.
3. SKiDL (pure Python, netlist-focused, has a Claude Code plugin) and circuit-synth (Python to KiCad, with Claude Code agents and JLCPCB lookup). Consider these if they beat the others.
4. For layout and routing: KiCad with an MCP server (for example mixelpixx/KiCAD-MCP-Server, which does placement, Freerouting, DRC/ERC and fab exports). DeepPCB and Quilter are AI routers, as a fallback.

Before committing, run a small bake-off: build the same tiny test board in atopile (with KiCad and Freerouting for layout) and in tscircuit. Compare how much of the process you can finish without me, the check results, and the quality of the fab output. Then recommend one, with reasons, and I'll confirm.

## Design rules for my boards
- Build around pre-certified modules (like the ESP32-S3-WROOM or MINI) rather than bare chips. Don't design antennas or RF.
- For the first revision: USB-powered, low voltage only. No mains voltage, no battery charging, no motor drivers, nothing safety-critical. Those can come in later revisions using proven reference designs.
- Use parts that are in stock at the chosen fab. Prefer JLCPCB "basic" parts, because extended parts add fees.
- Use hand-solderable sizes (0805 or larger, no QFN or BGA) for any part I might solder myself with a basic soldering iron.
- Include debug aids: test points, a power LED, a status LED, and a way to flash over USB.

## Verification gates (nothing gets ordered until all of these pass)
- The electrical rules check (ERC) and the design-rules check (DRC) pass with zero errors, using the fab's own rules. Read the actual reports; never trust an exit code.
- Every IC's pinout, voltage range and footprint is checked against its datasheet. Keep a table of what was verified and where.
- A separate reviewer subagent, one that did not build the design, reviews it for power, manufacturability, layout and cost.
- The fab's own manufacturability check passes on upload.
- Mark anything you inferred rather than verified, and tell me.
- Before the custom PCB, prove the firmware and the concept on a dev board with plug-in sensors (Qwiic, STEMMA QT or Grove) where possible. Use Wokwi simulation where it helps.

## Cost optimization (important to me)
For each order, compare these options in a table:
- A) Fully assembled by the fab (turnkey).
- B) Fab assembles the cheap surface-mount parts; I solder the through-hole parts or connectors, or add modules I buy on AliExpress.
- C) Bare PCB plus parts I buy on AliExpress, and I solder everything. Only offer this if every part is hand-solderable with a basic iron.

Include PCB, parts, assembly fees, extended-part fees, shipping and Israeli import VAT. Since June 2, 2026, the personal-import VAT exemption is $75, counting the goods value only (not shipping); above that, 18% VAT applies. Double-check that this is still current.

Check current fab promotions. As of September 2026, NextPCB offered free assembly on the first "Rev 0" order (up to $500) and up to $200 off assembly for 1–10 boards. Compare against JLCPCB, which can also assemble through-hole parts.

Tell me the savings and the extra work involved. Default to option A unless the savings are significant. If an option needs any tool or supply beyond my soldering iron (solder, flux, tweezers, a multimeter), list it with its cost.

## How to work with me
- Explain in plain language; I'm not an EE. Don't make me read schematics unless I ask.
- Only ask me about decisions that are really mine: features, budget, tradeoffs. Decide the technical details yourself and log the reasoning.
- Don't give me calendar timelines (no "week 1–2"). Sequence the work by phases and dependencies.
- Use git from the start. Keep a DECISIONS.md, and a HARDWARE_LESSONS.md with verified pinouts, fab rules and mistakes to avoid, which you read at the start of every session.
- Before anything that costs money, give me a clear summary and wait for my OK.

## Phases
0. Check my laptop (OS, Python, Node, KiCad) and install what's needed.
1. Tool bake-off and recommendation.
2. Write the first project's spec together with me.
3. Prototype on a dev board and get the firmware working.
4. Design the PCB.
5. Run all the verification gates.
6. Prepare the fab package and the cost comparison, then I approve the order.
7. Bring-up: when the boards arrive, guide me step by step (what to plug in, what to solder, what to measure, what to flash).
8. Design an enclosure (OpenSCAD or similar) and have it printed by the fab's 3D-printing service. If possible, add it to the same order so it ships with the boards.
