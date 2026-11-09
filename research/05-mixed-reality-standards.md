# 05 — Mixed worlds: open standards, building blocks and constraints

Survey date: **2026-11-09**, from primary sources (W3C, Khronos, OGC, CSA, EUR-Lex, eCFR, project release notes). Four points were re-checked directly: `KHR_gaussian_splatting` ratification, Firefox Web Serial, the Matter 1.6 release and the EDPB statement on smart glasses. Items marked *unverified* could not be confirmed.

Question: what exists to let real places, real devices and people in AR share an osage world — and what constraints apply when in-world actions drive real devices? Companion of [design/02](../design/02-mixed-worlds-and-devices.md).

## 1. AR / XR clients

**WebXR maturity (W3C):** core API and AR Module are Candidate Recommendation Drafts (2026-10-29); Hit Test and Depth Sensing are Working Drafts; Anchors and Plane Detection Editor's Drafts; mesh detection and raw camera access Community Group reports. **No active WebXR geospatial work** (`immersive-web/geo-alignment` dormant since 2022).

| Browser | AR support (Nov 2026) |
|---|---|
| Chrome Android | immersive-ar, hit-test, anchors, depth, raw camera access (since M107); plane detection since Chrome 147 (Apr 2026); no mesh |
| **Safari iOS** | **no WebXR at all** → iPhone AR needs a native client |
| Safari visionOS | immersive-vr, hands, transient-pointer (since Safari 18); no immersive-ar |
| Quest Browser 154.1 | immersive-ar passthrough, hit-test, persistent anchors (8 per site), planes, mesh; WebXR `camera-access` rejected (community report); `getUserMedia` on headset cameras *unverified* |
| Android XR Chrome | AR, hit-test, anchors, hands, depth, light estimation; no planes/mesh/raw camera listed |
| Firefox | no WebXR; Wolvic v1.10 (MPL-2.0) is the open alternative |

**OpenXR** 1.1.65 (2026-10-16). Spatial-entities extensions (entity, anchor, plane, marker, persistence) released 2025-06-10; image, mesh and object tracking added in 1.1.63–1.1.65. Supported by Android XR; PICO claims support; Meta *unverified*; Monado 25.1.0 (BSL-1.0) supports none of them.

**Open visual positioning** (without Google, Apple or Niantic clouds): **no turnkey open solution.** Building blocks: OpenArCloud OSCP (MIT, active), hloc (Apache-2.0, slowing; default SuperPoint/SuperGlue models are non-commercial, LightGlue is Apache-2.0), Kapture (BSD-3), stella_vslam (BSD-2, active), ORB-SLAM3 (GPL-3, unmaintained), Auki posemesh server components (MIT). Immersal is proprietary but self-hostable in its Enterprise tier.

## 2. Georeferencing

- **OGC GeoPose 1.0**: approved standard, published Sep 2023 (1.1 in drafting) — a standard way to express a pose in an Earth frame.
- **OGC 3D Tiles 1.1**: Community Standard since Jan 2023. A **3D Tiles 2.0** work item (public comment Jun–Jul 2026) builds on a draft glTF 2.1 and proposes splats, voxels and time-dynamic data. `EXT_mesh_gpu_instancing` ratified; `EXT_mesh_features` / `EXT_structural_metadata` are Cesium vendor drafts referenced by 3D Tiles 1.1.
- **CesiumJS** 1.147 (2026-11-02, Apache-2.0). No official Cesium for Godot; community "3D Tiles for Godot" (Apache-2.0). **3DTilesRendererJS** 0.5.4 (Apache-2.0): three.js, Babylon.js, PlayCanvas; WGS84 ellipsoid, GlobeControls, ReorientationPlugin.
- **High-precision coordinates:** Bevy 0.20.1 with big_space (0.13 stable for Bevy 0.19, only 0.14-rc for 0.20, MIT/Apache); Godot 4.7.2 needs a `precision=double` build (shaders stay single precision); Babylon.js 9 `useLargeWorldRendering` / floating origin (experimental); three.js r187 none built in.

## 3. Capturing real places

**Gaussian splatting:** `KHR_gaussian_splatting` **ratified (2026-09-03, PR #2642)**; the SPZ compression extension is still an open PR with competing proposals; Niantic SPZ v3.0.0 is MIT. Cesium streams splats with LOD in 3D Tiles (Apr 2026). Renderers: Spark 2.4.0 and SuperSplat (MIT), bevy_gaussian_splatting 9.0.

| Tool | Version | Licence | Notes |
|---|---|---|---|
| COLMAP | 4.2.1 | BSD-3 | GLOMAP merged in 4.0 (Mar 2026), standalone repo archived |
| OpenMVG | 2.1 (2023) | MPL-2.0 | stale |
| OpenMVS | 2.4.0 | AGPL-3.0 | |
| Meshroom / AliceVision | 2025.1.x / 3.3.x | MPL-2.0 | |
| nerfstudio | 1.1.5 | Apache-2.0 | slowing |
| gsplat | 1.5.3 | Apache-2.0 | |
| **Brush** (Rust / WebGPU) | 0.3.0 | Apache-2.0 | same stack family as Bevy/wgpu |
| OpenSplat | 1.2.2 | AGPL-3.0 | |

## 4. Real devices

- **Matter 1.6** (2026-06-17): NFC commissioning, Joint Fabric. **Matter 1.5** (Nov 2025) added **cameras** (WebRTC, PTZ, privacy zones) and closures. SDK connectedhomeip v1.6.1.0, Apache-2.0. Certified product count *unverified* (CSA: "thousands"; a third-party count ≈ 12.4k models).
- **Thread** 1.4.x (OpenThread BSD-3); **MQTT 5.0** (OASIS); **Sparkplug 3.0** (ISO/IEC 20237); Mosquitto 2.1.2 (EPL-2.0/EDL).
- **Home Assistant** 2026.11 (Apache-2.0): ~725k active installs among analytics opt-ins, ~1,560 integrations; WebSocket (`/api/websocket`) and REST APIs; **MCP server and client integrations since 2025.2**.
- **ROS 2:** Lyrical Luth LTS (May 2026–2031), Jazzy LTS to 2029, Humble to 2027; rmw_zenoh is Tier 1. Bridges: rosbridge_suite 4.2.1 (BSD-3), Foxglove bridge in foxglove-sdk (MIT), O3DE ROS 2 Gem active (distros *unverified*), Unity ROS-TCP-Connector unmaintained, Godot rclgd unmaintained; **Bevy has no dedicated crate** (Rust: r2r, ros2-client, rclrs 0.8).
- **Cameras:** ONVIF standalone WebRTC spec (since 24.06); RTSP 2.0 (RFC 7826); **WHIP (RFC 9725)**; WHEP still a draft; go2rtc and MediaMTX (MIT) bridge RTSP to WebRTC.
- **Drones:** MAVLink v2 (definitions and generated C code MIT, generator LGPLv3); MAVSDK 4.1.0 (BSD-3); PX4 1.17 (BSD-3); ArduPilot 4.7.2 (GPL-3).
- **Browser device APIs:** Web Serial in Chrome desktop, Chrome Android 148 (May 2026) and **Firefox 151 desktop** (May 2026, behind an add-on gating prompt). **WebUSB, Web Bluetooth, WebHID, Web NFC: Chromium only. Safari: none** (WebKit opposes all five; Mozilla opposes all but Web Serial).

## 5. Digital-twin vocabularies

- **W3C WoT Thing Description 1.1**: Recommendation (Dec 2023); TD 2.0 only a First Public Working Draft (Nov 2025). node-wot 0.9.2 (EPL-2.0).
- **Asset Administration Shell** v3.2 (Jul 2026), IEC 63278-1:2023; Eclipse BaSyx (MIT).
- **OPC UA** 1.05.06; open62541 (MPL-2.0).
- **NGSI-LD** (ETSI GS CIM 009 v1.9.1); brokers Orion-LD (AGPL), Stellio (Apache), Scorpio (BSD-3).
- **Eclipse Ditto** 3.9.8 (EPL-2.0), supports TD 1.1.

**Most realistic as an engine-facing device vocabulary: WoT TD 1.1** — devices as *properties, actions and events* in JSON/JSON-LD, protocol bindings kept separate, reusable Thing Models; Matter, Home Assistant, ROS, OPC UA and AAS as adapters underneath. NGSI-LD fits shared state rather than commanding devices. Do not target TD 2.0 yet.

## 6. Safety and regulation of remote actuation

**Drones**
- **EU** (Reg. 2019/947): the *open* category requires **visual line of sight (VLOS)**; beyond VLOS needs the *specific* category with a risk assessment (SORA 2.5). C1–C3 drones need direct remote ID and geo-awareness. U-space (Reg. 2021/664, in force since Jan 2023) requires network remote ID only in designated U-space airspace. EASA Opinion 06/2026 proposes remote ID from 100 g (not adopted).
- **US**: Remote ID (Part 89) since 2023-09-16; §107.31 VLOS and §107.35 one aircraft per person (waivable). Part 108 BVLOS rule proposed Aug 2025, **no final rule** as of 2026-11-09.
- **Consequence:** a remote participant flying a real drone "from the world" is, in general, beyond visual line of sight — **not allowed in the open category**. Without a specific authorisation, actuation must come from an on-site pilot with VLOS; remote participants watch or suggest.

**Robots:** ISO 10218-1/-2:2025 (industrial; excludes consumer and public service robots); ISO 13482:2014 being replaced (FDIS, "service robots"); ISO/TS 15066 under revision; ISO 13850 (emergency stop); ISO 13849-1:2023.

**EU Machinery Regulation 2023/1230 (applies 2027-01-20):** software performing a safety function is a safety component; **a remote connection must not create a hazard**; **remote-controlled machines must stop automatically when no validation signal arrives in time** (watchdog); autonomous machines must perform their safety functions themselves even when commanded remotely. → Legal support for **enforcing safety locally**: the world sends intents; the device-side controller enforces geofences, limits, watchdog and e-stop.

**Cybersecurity:** Cyber Resilience Act reporting duties from 2026-09-11, full application 2027-12-11; Radio Equipment Directive cybersecurity rules since 2025-08-01.

## 7. Privacy

- **GDPR household exemption** (Art. 2(2)(c)) covers purely personal use; Recital 18: the GDPR still applies to whoever provides the means (the platform operator). Ryneš (C-212/13): a home camera also covering public space is not household activity. Lindqvist (C-101/01): publishing to an indefinite number of people is not household activity. EDPB Guidelines 3/2019 read the exemption narrowly.
- **EDPB Statement 1/2026 on smart glasses** (2026-10-13): recording must be obvious (light or sound); sharing on a public account makes the user a controller.
- **Implication:** streaming real cameras or captured spaces into a shared multi-user world will likely **fall outside the household exemption**.
- **EU AI Act Art. 5** (since Feb 2025): untargeted scraping to build facial databases is banned; the real-time biometric identification ban targets law enforcement; other remote biometric identification is high-risk (obligations delayed to Dec 2027 by Reg. 2026/1744 — found on EUR-Lex, not re-checked).

## Takeaways for osage

1. **AR is uneven:** Chrome Android and Quest Browser cover WebXR AR; **iPhone AR needs a native client**; no open geospatial WebXR and no turnkey open VPS — osage's `osage-geo` will assemble one from OSCP, stella_vslam, Kapture/hloc-style relocalisation.
2. **Georeference with standards that already exist:** OGC GeoPose for poses, 3D Tiles for anchored content, big_space (Bevy) / floating origin (Babylon) for precision.
3. **Capture pipeline is open end to end:** COLMAP/GLOMAP (BSD-3) → gsplat or **Brush (Rust/WebGPU)** → `KHR_gaussian_splatting` → 3D Tiles.
4. **Device vocabulary:** WoT TD 1.1 as the description envelope; OpenDeviceDB contracts map onto it (state → property, stream/event → event, action → action).
5. **Browser bridges are Chromium-first** (plus Firefox Web Serial); Safari users need a native or desktop bridge.
6. **Safety must be local, with a watchdog** — this is now law in the EU for machinery from 2027; drones flown from a world require on-site VLOS unless specifically authorised.
7. **Privacy:** a shared world makes the operator and often the streaming user GDPR controllers; recording must be visible; no facial recognition features.

## Not verified

Meta's and PICO's shipping status for OpenXR spatial-entities; Quest camera access via `getUserMedia`; Matter certified-product count; O3DE ROS 2 Gem supported distros; reports that the US Part 108 final rule is under White House review (secondary sources); whether ISO 10218-2:2025 absorbs ISO/TS 15066 (secondary sources); current IEC 61508 edition.

## Sources

- WebXR: https://www.w3.org/TR/webxr/ · https://developer.chrome.com/release-notes/147 · https://developers.meta.com/horizon/documentation/web/webxr-mixed-reality/ · https://developer.android.com/develop/xr/develop-with-webxr
- OpenXR: https://github.com/KhronosGroup/OpenXR-SDK/releases
- GeoPose: https://docs.ogc.org/is/21-056r11/21-056r11.html
- 3D Tiles 2.0 work item: https://www.ogc.org/requests/ogc-seeks-public-comment-on-proposed-3d-tiles-2-0-community-standard-work-item/
- Splats: https://github.com/KhronosGroup/glTF/pull/2642 · https://cesium.com/blog/2026/04/27/3d-gaussian-splats-lod/
- Matter: https://csa-iot.org/newsroom/matter-1-6-enables-more-intuitive-setup-multi-ecosystem-experiences-and-context-driven-control/
- Home Assistant: https://analytics.home-assistant.io/
- Web Serial in Firefox: https://hacks.mozilla.org/2026/05/web-serial-support-in-firefox/
- WoT TD 1.1: https://www.w3.org/TR/wot-thing-description11/
- US drone rules: https://www.ecfr.gov/current/title-14/part-107
- EDPB smart glasses: https://www.edpb.europa.eu/news/edpb-publishes-statement-on-the-data-protection-challenges-of-smart-glasses_en
