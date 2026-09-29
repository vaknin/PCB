# Enclosure tooling, fab printing and design rules (phase 8, capture-clip)

Date: 2026-09-29. Every fact is tagged **VERIFIED** (source + read 2026-09-29, or run on this laptop
today) or **UNVERIFIED / INFERRED**. Nothing was installed system-wide. The test installs were in the
session scratchpad: a uv venv with CadQuery, the OpenSCAD snapshot AppImage, and downloaded 3D models.

## 1. CAD tool for a code-defined, headless enclosure

| | OpenSCAD 2021.01 (Arch) | OpenSCAD dev snapshot | **CadQuery 2.8.0** | build123d 0.13.0 | FreeCAD 1.1.4 |
|---|---|---|---|---|---|
| Latest stable | 2021.01 is still the newest *release* (VERIFIED, api.github.com/repos/openscad/openscad/releases) | only snapshots; newest `OpenSCAD-2026.09.29-x86_64.AppImage`, 84.5 MB (VERIFIED, files.openscad.org/snapshots) | 2.8.0, 2026-06-20 (VERIFIED, PyPI + GitHub releases) | 0.13.0, 2026-09-21: **not 1.0** (VERIFIED, PyPI) | 1.1.4, 2026-09-28 (VERIFIED, GitHub releases) |
| Kernel | CGAL (slow) | Manifold backend, `--backend=manifold` (VERIFIED: ran here; openscad.org/downloads calls Manifold "a huge performance boost") | OCCT B-rep via `cadquery-ocp` 7.9.3.1.1 (`cadquery` pins `cadquery-ocp>=7.9.3.1,<8.0`; OCP 8.0.1 exists but isn't used yet) (VERIFIED, PyPI requires_dist) | OCCT, `cadquery-ocp-novtk` 8.0 (VERIFIED, PyPI) | OCCT |
| Install without sudo | pacman (needs sudo) | AppImage: download, `chmod +x`, run; `APPIMAGE_EXTRACT_AND_RUN=1` avoids FUSE (VERIFIED, ran here) | `uv venv --python 3.13 && uv pip install cadquery==2.8.0`: **1.7 GB venv** (pulls VTK 9.6.2, trame, casadi, scipy) (VERIFIED, ran here). cp314 wheels exist too (VERIFIED, PyPI) | uv/pip, same way | AppImage, 820 MB, bundled Python 3.11 (VERIFIED, release assets) |
| STEP import (KiCad board) | **no**: `import()` takes STL/OFF/OBJ/AMF/3MF/DXF/SVG/PDF/JSON only (VERIFIED, en.wikibooks.org/wiki/OpenSCAD_User_Manual/Importing_Geometry) | same; only a mesh (`kicad-cli pcb export stl`) could go in | **yes**: imported the starter board STEP (69 solids) in 1.4 s (VERIFIED, ran here) | yes (OCCT) | yes |
| STEP export | no (VERIFIED: `-o t.step` → "Invalid suffix step") | no | yes (VERIFIED) | yes | yes |
| STL export | yes | yes (VERIFIED) | yes (VERIFIED) | yes | yes |
| Interference volume | only mesh CSG (`intersection()`, then read the volume elsewhere) | same | `case.intersect(board).Volume()`: 1367.9 mm³ for a deliberately low lid, 0.0 for one that fits, 0.3–0.5 s (VERIFIED, ran here) | same OCCT ops | same OCCT ops, in a much heavier runtime |
| Minimum clearance | no | no | `case.distance(board)`: exact B-rep distance, 0.25 mm on the test case, 0.07 s (VERIFIED, ran here) | yes | yes |
| Headless PNG render | `-o x.png` (VERIFIED, ran; needs no display) | same | `cadquery.vis.show(obj, screenshot="x.png", interact=False)` rendered a shaded PNG **with DISPLAY and WAYLAND_DISPLAY unset** (VTK printed a harmless "bad X server" warning) (VERIFIED, ran here). `exporters.export(..., "x.svg")` gives hidden-line SVG. `Assembly.export("x.glb")` gives glTF for a 3D viewer (VERIFIED, 1.7 MB for the starter) | similar | `freecadcmd`; rendering needs the GUI or extras (INFERRED) |
| Maturity | old but stable | dev builds only (fails the owner's "stable" rule) | 2.x since 2020, regular releases, widely used (INFERRED from release history) | pre-1.0 and changing fast (0.11 → 0.13 in 3 months, VERIFIED) | stable, but GUI-first; its Python API isn't meant for headless pipelines (INFERRED) |

**Pick: CadQuery 2.8.0 in a project uv venv.** It is the only stable (≥ 1.0) option that is scriptable and
headless, imports the KiCad STEP, computes exact interference and clearance, exports both STEP and STL,
and renders PNGs without a display. It is Python, which fits the rules: the geometry kernel's
ecosystem is where Python wins, and it runs as a short batch job, not a long-lived program.

- Rust-native CAD with STEP and booleans at production quality doesn't exist yet (INFERRED; the OCCT
  bindings for Rust are young), so Python is the exception here.
- pcbgen (Rust) stays the orchestrator: it writes the inputs and runs `enclosure/.venv/bin/python
  enclosure/build.py` the same way it runs kicad-cli.
- Venv size (1.7 GB) is the main cost. `build123d` would be lighter (novtk), but it is pre-1.0.

## 2. KiCad → case data

### `kicad-cli pcb export step` (KiCad 10.0.6, VERIFIED from `--help` and test runs)
- Useful flags:
  - `--no-dnp`, `--no-unspecified`, `--board-only`, `--no-components`, `--component-filter "U1,SW*"`
  - `--subst-models` (use a same-named STEP in place of a VRML)
  - `-D KICAD10_3DMODEL_DIR=<dir>` (sets where the models are)
  - `--include-tracks/pads/zones/silkscreen`, `--fuse-shapes`, `--user-origin`, `--grid-origin`/`--drill-origin`, `--no-optimize-step`
  - `kicad-cli pcb export` also has `stl`, `glb`, `brep`, `vrml`, `stpz`, `xao`, and `pcb render` (raytraced PNG/JPEG with `--side`, `--rotate`, `--zoom`, `--quality high`)
- **A missing model is only a warning.** It prints `File not found: ${KICAD10_3DMODEL_DIR}/…step` and `Could not add 3D model for R6.`, still writes the STEP, and **exits 0** (VERIFIED on the starter board). The pipeline must parse stderr and fail on any "Could not add 3D model" for a part that matters. Otherwise the fit check passes against a board with its parts missing.
- `--subst-models` is irrelevant now: the Arch 3D package has 7,251 `.step` files and **no** `.wrl` (VERIFIED, archlinux.org/packages/extra/any/kicad-library-3d/files/).
- 0.6 s without models, 2.2 s with them (VERIFIED).

### `kicad-library-3d` isn't needed
- Arch `extra/kicad-library-3d` 10.0.6-1: 252 MB download, **3.37 GB installed** (VERIFIED, archlinux.org package JSON).
- Single models download from KiCad's GitLab at the matching tag, e.g.
  `https://gitlab.com/kicad/libraries/kicad-packages3D/-/raw/10.0.6/RF_Module.3dshapes/ESP32-S3-WROOM-1.step`
  (VERIFIED, HTTP 200, 1.9 MB).
- Put them in a project cache (e.g. `enclosure/3dmodels/<Lib>.3dshapes/`) and pass
  `-D KICAD10_3DMODEL_DIR=…`. That worked on the starter board (VERIFIED).

### Our parts (footprint `(model …)` lines read from `/usr/share/kicad/footprints`; model files checked at tag 10.0.6)

| part | footprint's model reference | model in KiCad's library? | height (model bbox) |
|---|---|---|---|
| ESP32-S3-WROOM-1 | `RF_Module.3dshapes/ESP32-S3-WROOM-1.step` | **yes** (VERIFIED, downloaded) | 3.10 mm, 18×25.5 |
| 1TS009A 6×6×5 | `Button_Switch_SMD.3dshapes/SW_Push_1TS009xxxx-xxxx-xxxx_6x6x5mm.step` | **yes** | 5.00 mm |
| TS-1088 (reset) | `Button_Switch_SMD.3dshapes/SW_SPST_TS-1088-xR020.step` | **yes** | 2.00 mm |
| ICS-43434 | `Sensor_Audio.3dshapes/InvenSense_ICS-43434-6_3.5x2.65mm.step` | **yes** | 0.98 mm |
| HRO TYPE-C-31-M-12 | `Connector_USB.3dshapes/USB_C_Receptacle_HRO_TYPE-C-31-M-12.step` | **no**: 404 at 10.0.6, 9.0.0, 8.0.0, 7.0.0 and master; not in the Arch file list (VERIFIED) | EasyEDA model: −0.85…3.25 mm |
| LiteOn LTST-C19HE1WT | `LED_SMD.3dshapes/LED_LiteOn_LTST-C19HE1WT.step` | **no** (same checks, VERIFIED) | EasyEDA model: −0.11…0.25 mm (looks too thin, UNVERIFIED) |
| JST S2B-PH-SM4-TB | `Connector_JST.3dshapes/JST_PH_S2B-PH-SM4-TB_1x02-1MP_P2.00mm_Horizontal.step` | **no**; only the through-hole `S2B-PH-K` exists (VERIFIED) | EasyEDA model: 5.50 mm |

- The footprints reference files that KiCad doesn't ship. Without a fix, the three parts that matter most to the case (USB-C mouth, LED, battery plug) are silently missing from the STEP.
- **Fix:** `easyeda2kicad` 1.0.1 (PyPI, 2026-04-06, VERIFIED) fetched STEP models for C165948, C458749 and C295747 into the same venv (VERIFIED, ran here).
- **Their origins and rotations don't match KiCad's footprint origins** (e.g. the JST bbox is X −4.95…2.95, Y −1.65…6.95). Each needs an `(offset …)`/`(rotate …)` in pcbgen's footprint copy, checked once against the F.Fab outline (UNVERIFIED until done).
- HARDWARE_LESSONS already notes the USB-C footprint's origin is the body centre, with the body ending 3.7 mm below it.

### Alternative or complement: pcbgen writes `enclosure/board.json`
pcbgen already parses `.kicad_pcb`, so it can write, deterministically and with no 3D models:
- the Edge.Cuts outline, thickness and mounting holes
- per part: ref, side, position, rotation, the courtyard and F.Fab boxes, and a **height from a small table in pcbgen** (datasheet values, each marked VERIFIED/INFERRED like the rest)
- features the case must meet: button centre and stem top, LED centre, the mic sound-hole centre (the footprint NPTH), the USB-C mouth plane and centre, the reset button centre, and the JST mouth and direction

The case generator should be driven from this JSON (positions of openings). The KiCad STEP stays the
independent collision check. Two sources that must agree make a real gate.

## 3. JLCPCB 3D printing (JLC3DP)

### Materials and prices, today (VERIFIED)
The instant-quote page's own 1×1×1 cm demo part, with each material selected (jlc3dp.com/3d-printing-quote, headless browser, not signed in). These are near-minimum prices, not our case:

| process | material | colours | demo 1 cm³ | notes |
|---|---|---|---|---|
| SLA | **9600 Resin** | White | $0.32 | HDT 59 °C, elongation 5.5 %, tensile 55 MPa, recommended wall > 0.8 mm, 48 h build (jlc3dp.com/help/article/photosensitive-9600-resin, updated 2025-04-16) |
| SLA | 8001 Resin | Transparent / Translucent | $1.97 | transparent gets sanding + oil spray by default; bubbles possible (…/photosensitive-8001-resin) |
| SLA | JLC Black / Black Resin | Black / Grayish Black | $1.09 | |
| SLA | Imagine Black | Black | $2.15 | |
| SLA | LEDO 6060, 8228, CBY, Grey, 9000HE, JLC Temp, X | various | $0.32 | |
| MJF | **PA12-HP / PA11-HP / PA12S-HP** | Black / Natural Gray (PA12S: Grayish White) | $1.05 | PA12-HP: HDT 175 °C, elongation 20 %, wall 1 mm, 72 h build (…/PA12-HP-Nylon, updated 2026-07-30) |
| MJF | PAC-HP | Multicolor | $5.26 | |
| SLS | 3201PA-F, 1172Pro, 3301PA | Grayish Black / White | $1.05 | 3401GB $0.95 |
| FDM | ABS, ASA, PC-ABS, TPU… | Black/White etc. | $3.16 | PLA-P $1.05; FDM minimum part size is **30×30×10 mm** |

- The home page lists SLA "From $0.30, build time 2 days", MJF/SLS/FDM "From $1.00, 3 days" and SLA tolerance "down to ±0.2 mm" (VERIFIED, jlc3dp.com).
- File types: **STL, STP, STEP, OBJ, 3MF**. The upload box says "Wall thickness > 1.2mm, Thinnest part ≥ 0.8mm" (VERIFIED, quote page).
- **No minimum order**; "from $0.30" (VERIFIED, home page). There is a special charge "starting from $8 per part" for complex, hollow or internal structures, plus 1–2 days (VERIFIED, jlc3dp.com/help/article/3d-printing-special-charges-case, updated 2026-09-04). An open two-part shell shouldn't trigger it (INFERRED); a closed hollow one would.
- July 2026 price cuts (PAC-HP −70 %, PA12-CF −24 %…) and finishing price rises (spray paint +16.67 %) (VERIFIED, jlc3dp.com/news/materials-finishing-pricing-update-july2026).
- **Price for our case (INFERRED):** two open shells about 60×35×15 mm with 1.2–1.5 mm walls come to roughly 8–12 cm³ of material. Scaling from the demo floor, that is about **$2–5 in 9600 resin** and **$4–10 in MJF PA12**, plus a button cap at the minimum price.
  - A community data point: two small resin parts cost $2.70 *including shipping* (hackaday.io/page/395320, 2024).
  - A real quote needs a file upload. I didn't upload anything, per the browser skill's rule of no cart or form actions.

### Design rules (VERIFIED, jlc3dp.com/help/article/3d-printing-design-guideline)
- **Wall thickness by part size:**

  | material | 10×10 mm | 50×50 mm | 100×100 mm |
  |---|---|---|---|
  | resin | 0.8 mm | 1.0 mm | 1.5 mm |
  | nylon | 1.2 mm | 1.5 mm | 2.0 mm |
  | FDM | — | 1.6 mm | 2.0 mm |

  Snaps, bosses and fasteners should be "more than 1.5mm".
- **Minimum holes:**
  - SLA: Ø1.0 mm (depth 1–3 mm), Ø1.5 mm (up to 4.5 mm deep)
  - others: Ø1.5 mm (up to 4.5 mm deep)
  - Ø2.0 mm if oil-sprayed
- **Embossed or engraved text:** 0.8 mm deep and wide (resin, nylon).
- **Assembly clearance:**
  - static fit: resin 0.2 mm, nylon 0.2–0.4 mm, FDM 0.5 mm
  - moving parts: resin/FDM 0.5 mm, nylon 0.6 mm
- **Tolerance:**
  - resin ±0.2 mm (≤ 100 mm); nylon and FDM ±0.3 mm
  - holes ±0.3 mm (resin, nylon)
  - resin drifts over time: ±0.15 mm after 1–3 days, ±0.2 mm after 3–7 days
- **Escape holes:** Ø ≥ 2.5 mm (hollow parts only).
- **Size limits:** SLA/MJF minimum 5×5×5 mm; FDM minimum 30×30×10 mm.

### Same parcel as the PCBs
- **VERIFIED:** JLC3DP and JLCPCB orders go in one cart with combined payment: "product orders of JLCPCB and JLC3DP are together, and you can choose to combine the payment" (jlc3dp.com/help/article/how-to-combine-orders-for-jlc3dp-and-jlcpcb-products, updated 2025-08-26).
- **VERIFIED:** "The orders within one same batch order number will be shipped together" (JLCPCB staff answer, jlcpcb.com/help/answers/detail/171).
- **VERIFIED:** JLCPCB's Combine Shipping service holds finished orders and ships them as one consolidated parcel. Storage is free for 15 days, then $1.5–2.5 per m³ per day (jlcpcb.com/help/article/combine-shipping-service, updated 2026-09-09). The page doesn't list which product lines qualify.
- **Caution (VERIFIED, dated):** JLCPCB on X, 2024-07-08: "We offer combined shipping for: 1. PCBs, Stencils, 3D Printing with CNC; 2. PCBs with Stencils and SMT" (x.com/JLCPCB/status/1810118023573025191). That lists 3D printing and *assembled (SMT)* boards in **separate groups**.
- **Answer: bare PCBs plus 3D prints in one parcel: yes. Assembled boards (PCBA) plus 3D prints: UNVERIFIED** (possibly not, as of 2024).
  - Ask JLC live chat before the order, or check whether both land in the same Combine Shipping queue at checkout.
  - Worst case, the case ships separately: one extra small parcel, shipping cost not checked.

### Alternative: PCBWay
- Offers 3D printing and "Combine shipping: pack and deliver the PCBs together with products that have already been paid for or produced". Orders paid together ship together (VERIFIED, pcbway.com/helpcenter/lead_time/Orders_Combination__Shipping_and_Payment_Policy.html).
- It can also *assemble* the board into printed parts if you tick "Ship in assembly" and supply a 2D drawing (VERIFIED, pcbway.com/helpcenter/quality_control_for_cnc_machining/Can_PCBWay_assemble…).
- Prices not checked. Moving the PCBA to PCBWay would undo the JLCPCB-based parts and cost pipeline (D-021/D-023), so it is a fallback only.

## 4. Design rules for the capture-clip case (INFERRED unless cited)

- **Material, round 1: 9600 white resin.**
  - It is cheapest, has the finest detail (±0.2 mm, Ø1.0 mm holes), and a thin white wall diffuses the LED.
  - Downsides (VERIFIED from its datasheet): brittle (5.5 % elongation), HDT 59 °C, yellows in UV. So no long snap arms, and not left in a hot car.
  - Fallback if it cracks: MJF PA12 black (tough, 175 °C), with an LED hole instead of a glowing wall.
- **Walls:** 1.2–1.5 mm, which is the upload checker's 1.2 mm and the guide's 1.0 mm at 50×50 plus margin. Bosses and snaps ≥ 1.5 mm (VERIFIED guide).
- **Clearance:**
  - PCB edge to wall: 0.3–0.5 mm per side (resin static fit 0.2 mm + ±0.2 mm tolerance).
  - Components to lid: ≥ 0.5 mm.
  - Battery pocket: +0.5 mm per side and +0.5 mm in thickness, because pouch cells swell.
- **Main button (1TS009A, 5.0 mm, 1.8 N):**
  - A separate printed cap (plunger) with a flange wider than its hole, so it can't fall out. The body is 0.5 mm smaller than the hole (moving-fit clearance, VERIFIED guide) and it rests on the switch stem with ~0.2 mm pre-gap.
  - An integral cantilever flexure is a poor fit for 9600 resin (5.5 % elongation). It is OK in PA12 (20 %): an arm ~10–12 mm long, 0.8–1.0 mm thick.
  - Model the switch travel (≈0.25 mm, typical for this class, UNVERIFIED for this part) so the cap never bottoms on the case before the switch clicks.
- **LED (1.6×1.6 mm, top-emitting):**
  - Resin: a pocket in the lid leaving a 0.5–0.8 mm wall over the LED (SLA 5×5 mm features allow 0.5 mm, VERIFIED guide), plus a light-blocking rib around it so the light doesn't bleed across the case.
  - PA12: a Ø1.5–2 mm hole, or a small 8001 clear light-pipe part (press fit, 0.2 mm) sitting ≤ 0.5 mm above the LED.
- **Microphone (bottom port, 0.5 mm PCB hole):**
  - TDK AN-1003: the PCB hole must be ≥ 0.25 mm, typically 0.5–1 mm. The seal between PCB, gasket and case matters, and gasket stiffness doesn't. **Keep the gasket cavity small or put the board against the case**, otherwise a Helmholtz resonance adds a high-frequency peak: f = c·D / (4π·V·(L + πD/2)) (VERIFIED, invensense AN-1003 via cdiweb.com mirror).
  - **Conflict to settle in layout:** the port opens on the board's *back*, which is where the battery sits.
  - So place the mic outside the battery footprint. The back shell gets a printed chimney (a tube Ø3 mm outside, ~Ø1 mm bore) that reaches the PCB's back face, with a thin closed-cell foam or silicone ring. The case hole is Ø1.0–1.5 mm (SLA Ø1.0 minimum).
  - Alternatively put the mic on the back side facing out, which costs double-sided assembly.
- **USB-C:**
  - Opening about 9.5×3.8 mm around the receptacle mouth (the receptacle shell is about 8.9×3.2 mm, from the EasyEDA bbox; UNVERIFIED vs the datasheet) with 0.3 mm per side.
  - The outer face must leave room for the cable plug's overmold, commonly cited as ≤ 12.35×6.5 mm (UNVERIFIED; USB Type-C spec R2.0 §3.10.3 / Fig. 3-80 covers this, but the numbers are in an image).
  - Keep the wall at the mouth thin (≤ 1 mm) or recessed. A known failure is a receptacle set 3.4 mm behind the wall, so cables don't seat (github.com/dudgeon/count-fidget/issues/4, secondary).
- **Reset pinhole:** Ø1.2–1.5 mm (≥ SLA Ø1.0 minimum), lined up over the TS-1088 (2.0 mm tall). A short internal guide tube stops the pin from wandering onto parts. Test with a paper clip (~0.8–1 mm).
- **LiPo (36×17×7.8 mm, JST PH lead):**
  - A pocket in the back shell with ribs on all four sides, and a foam pad or two crush ribs pressing it gently from the lid. It must not rest on sharp component leads; a rib floor or a Kapton-tape layer separates it from the PCB back.
  - Route the lead in a channel to the JST, whose mouth needs ~10 mm of clear space to plug in.
  - Nothing may press the cell hard: puncture risk.
- **Closing the case:**
  - Heat-set inserts are out (no tools).
  - Round 1: **2× M2 self-tapping screws for plastic** into printed bosses (Ø4.5 mm OD, ~Ø1.6–1.7 mm pilot). Only a small screwdriver is needed, and it survives re-opening a few times.
  - Snap fits only in PA12, or as short, shallow lips in resin.
  - The PCB is held by the same bosses (M2 holes, if the board has them) or clamped between ribs of both shells.
- **Antenna:** the case is plastic, so fine. Keep screws, battery and any metal away from the module's antenna end (INFERRED from Espressif's antenna keep-out guidance, not re-read today).

## 5. Fit and collision check (proposed `enclosure` stage, all INFERRED design; the building blocks VERIFIED above)

1. **pcbgen:**
   - writes `enclosure/board.json`
   - runs `kicad-cli pcb export step --no-dnp -D KICAD10_3DMODEL_DIR=enclosure/3dmodels -o enclosure/board.step`
   - **fails if stderr has "Could not add 3D model"** for any part whose height > 0.5 mm, or for the button, LED, USB-C, JST or mic
2. **`build.py` (CadQuery):** builds the shells, button cap and gasket chimney from `board.json` and `case.toml`, holding the design parameters: walls, clearances, material.
3. **Gates:**
   - interference: `shell.intersect(board).Volume() > 0.001 mm³` fails, for each shell and the cap
   - clearance: `shell.distance(board) ≥ clearance`, reported per part
     - The STEP imports as one compound of solids. `Assembly.importStep` fails on KiCad's STEP ("Unique name is required. C_0805_2012Metric is already in the assembly", VERIFIED). So match each solid's centroid against the refs in `board.json` to name the offender.
   - battery: a box of the cell's size (+ swelling margin) must also have zero interference
   - features: each opening's axis passes through its target (button stem, LED, mic hole, USB mouth, reset button) within 0.2 mm
   - printability: minimum wall ≥ 1.2 mm, holes ≥ 1.0 (SLA) / 1.5 mm (MJF), and part bboxes above the minimum size. Use a simple ray or offset test, or trust JLC's upload DFM for the rest.
   - assembly: the cap's travel is modelled (cap pressed by the switch travel stays clear of the case)
4. **Outputs:**
   - `case-top.step/.stl`, `case-bottom.step/.stl`, `button.step/.stl`
   - `fit.json` (volumes, clearances, pass/fail), which `check` adds to `gates.json`
   - `case-*.png` (VTK screenshot, iso and exploded views), with an optional `fit.glb` for an interactive 3D view on the review page (three.js from a CDN is allowed in artifacts)
   - Plus `kicad-cli pcb render --rotate` PNGs of the bare board.

## What this means for the plan
- **Tool pick: CadQuery 2.8.0** (stable, OCCT B-rep, STEP in/out, exact interference and distance, headless PNG). It is one Python batch script behind pcbgen, which fits the "Python where its ecosystem wins" rule.
  - The OpenSCAD snapshot works headless but can't read or write STEP, and it isn't a stable release.
  - build123d is pre-1.0; FreeCAD is an 820 MB GUI-first AppImage.
- **Install (no sudo):**
  - `cd enclosure && uv venv --python 3.13 && uv pip install cadquery==2.8.0 easyeda2kicad==1.0.1`
  - Pin it in a `pyproject.toml`/`uv.lock` (~1.7 GB, gitignored `.venv`). Per D-001, nothing global.
  - No `kicad-library-3d` (3.4 GB). Fetch the ~6 needed models from GitLab tag 10.0.6 into `enclosure/3dmodels/` (committed or cached), plus the 3 EasyEDA models, with offsets fixed once.
- **pcbgen must export:**
  - `board.json` (outline, holes, per-part ref, side, position, rotation, bounding box, height from a table; the feature points for the button, LED, mic hole, USB-C mouth, reset and JST)
  - the STEP with the model dir set
  - a hard failure on missing-model warnings, because kicad-cli exits 0
- **Rough cost per case (INFERRED):** ~$2–5 in 9600 white resin, ~$4–10 in MJF PA12, plus a button cap near the $0.30–1.05 minimum. Get a real quote by uploading the STEP/STL once the owner OKs it.
- **Same parcel:**
  - Bare PCBs + JLC3DP: **yes** (VERIFIED: combined cart and payment; one batch ships together).
  - **Assembled PCBA + JLC3DP: UNVERIFIED.** JLCPCB's 2024 statement grouped SMT separately from 3D printing. Confirm with JLC chat (or at checkout via Combine Shipping) before the first order. Otherwise budget for a second small parcel.
- **Layout feedback for capture-clip now:** put the bottom-port mic outside the battery's footprint (or on the back side), with its PCB hole where a printed chimney can seal it. Put the USB-C mouth flush with the board edge; keep the button and LED clear of the module's antenna end; and add two M2 holes the case can use.
