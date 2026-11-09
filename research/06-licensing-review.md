# 06 — Licensing review (D-003, D-005, D-014)

Date: **2026-11-09** · **This is a technical-legal analysis from primary sources, not legal advice.** Points marked as needing a lawyer must be reviewed by one before the first code commit.

Reviewed: [decision log](../design/decisions.md) (D-003, D-005, D-014), `CONTRIBUTING.md`, the README licence section, `LICENSE`, [design/02](../design/02-mixed-worlds-and-devices.md) §3–4.

## Summary

The set-up holds, with corrections:

1. **NVIDIA:** do not ship DLSS/NGX files; load them through the driver, as Blender 5.3 does.
2. **App stores:** reuse the tested Signal/gomuks construction (executables in stores under MPL-2.0) rather than new wording.
3. **SPDX:** `WITH LicenseRef-…` is invalid; custom exceptions use `AdditionRef-` (SPDX 3.0+).
4. **Contributor terms:** the additional permissions bind contributors only if every file header names them (DCO clause (a): "under the open source license indicated in the file").
5. **D-014's compatibility table** needs corrections (§4 below).

## 1. Section 7 additional permissions

- **Valid:** AGPL §7 lets a licensor add permissions on material for which it holds copyright; permissions covering the whole Program are "treated as though they were included in this License". Today the maintainer is the sole copyright holder.
- **Binding later contributions:** not automatic. It works through the DCO ("under the open source license indicated in the file") and GitHub's terms (D.6, contributions under the repository's licence) — so **the permissions must appear in every file header and in CONTRIBUTING**. The FSF FAQ: "when other people modify the program, they do not have to make the same exception for their code." Existing commits have no `Signed-off-by` line.
- **Imported code:** an exception cannot be granted for other authors' code; if permissions cover only part of the Program, "the entire Program remains governed by this License without regard to the additional permissions". Any GPL/AGPL code or dependency without the same permission cancels the benefit for the binaries that contain it — including OpenDeviceDB's AGPL components bundled into an iOS bridge. Bevy and wgpu (MIT OR Apache-2.0) are fine.
- **Removal:** anyone conveying a copy "may at your option remove any additional permissions". Forks can be plain AGPL; patches from them cannot return to files carrying the permissions unless their authors re-grant them. **The project can later drop permissions but never add new ones** — "locked" holds in one direction only. Decide now on any further permission (consoles, NDA or store SDKs, Steamworks…).
- **SPDX:** no listed exception (SPDX list 3.29.0) fits; custom exceptions use `AdditionRef-` (SPDX 3.0+), one per `WITH`. REUSE 3.3 and Cargo still use SPDX 2.3: REUSE says to "create a custom license that embeds the exception"; Cargo uses `license-file`. **AGPL-3.0-only vs -or-later is not chosen yet** (OpenDeviceDB uses -or-later).

## 2. Permission 1 — GPU vendor plugins

**Needed?** A vendor library is not a "System Library" (not part of the normal packaging of a Major Component) and not an "aggregate" (plugins that "make function calls to each other and share data structures" form one program). So the permission is needed whenever osage or a downstream conveys the combination; a user adding the library themselves may "make, run and propagate covered works that you do not convey".

**Draft problems:** condition (c) made compliance depend on the vendor licence (move to a note); "binary form only" is the wrong test (DLSS headers are source, Streamline is MIT, AMD lists its DLLs as MIT) — use "not available under a free software licence"; add latency reduction (Reflex, Anti-Lag, XeLL) and adapters linking vendor static libraries; "same function" is too strict (no open equivalent of DLSS Ray Reconstruction) — use "every function remains usable without it"; state that vendor terms do not apply to osage (§10).

| Vendor | Verdict | Key terms |
|---|---|---|
| **NVIDIA** DLSS / NGX, Streamline DLSS plugins | **unclear — do not ship the files** | RTX SDKs licence (2024-03-14) §4(e): no use "that would cause it to become subject to an open source software license"; redistribution only "as incorporated in object code format into a software application", "without the right to sublicense", downstream terms "at least as protective" (§2c); cloud only on vGPU (Supp. 3); notify NVIDIA before commercial release (Supp. 4); logos (§7.1b); no life-critical use (§4f — relevant when the same app drives real devices, D-009). Streamline's framework is MIT but its "DLSS-G plugin is provided as prebuilt DLLs only". **Model: Blender 5.3** (GPL, no exception) uses the SDK only "for the type definitions"; "the implementation is loaded through the system-level NVIDIA driver" (driver ≥ 620). → never commit NVIDIA headers, separate adapter, DLL supplied by the driver or the user; ask NVIDIA in writing (licence §19). |
| **AMD** FidelityFX SDK 2.4.0 / FSR 4 | **allowed** | "distribute copies … in binary form only, and to permit persons to whom the Software is provided to do the same", no reverse engineering; the licence file lists the `signedbin` DLLs (incl. FSR 4) under MIT — unusual, ask AMD to confirm. No open-source clause; can ship as an optional package. |
| **Intel** XeSS 3.0.2 | **allowed** | "use and redistribute the software … in binary form only … without modification", keep the notice, no reverse engineering; headers MIT; Windows binaries only. |

## 3. Permission 2 — application stores

- **Apple:** use limited to "personal, noncommercial purposes", 10 devices / 5 computers, Family Sharing 6; "You may not tamper with or circumvent any security technology" (FairPlay); the standard EULA forbids redistribution, network use by several devices and modification; a custom EULA "may not provide for usage rules … in conflict with" Apple's terms. FSF (2010): the usage rules "are imposed on you no matter how the software is licensed" — conflicts with AGPL §10. Guideline 2.5.2 (no downloaded code changing the app) also affects WASM components. AGPL "Installation Information" does not apply (only when the device itself is sold).
- **Google Play:** "personal, non-commercial use only", the distribution agreement "will supersede the EULA" — milder. Developer verification for sideloading from 2026-09-30 in four countries, global in 2027.
- **VLC precedent:** removed from the App Store in Jan 2011 after one contributor's complaint; relicensing needed every author ("a famous one, who refused to answer. His code was therefore rewritten"); back in 2013 under MPL-2.0.
- **Wording precedents:** Signal / gomuks (AGPL): convey "through the Apple App Store non-source executable versions … as Executable Versions only under the Mozilla Public License version 2.0", otherwise complying with the AGPL. Mosh, Nextcloud, iSH, Blink: a promise "not to pursue any license violation" (weaker).
- **Apple practice:** AGPL apps are in the store but their owners use CLAs (Signal, Threema, Element X, Mastodon); multi-contributor GPL apps without CLA also exist (Lichess mobile, Kiwix). Apple acts on complaints (guideline 5.2) — the permission removes that risk.
- **AGPL §13 (network use):** applies "if you modify the Program", including "servers for games that are played online" (FSF FAQ). **A modified gateway or region server — including world logic compiled into it — must offer its source to every player and agent, even on proprietary clients.** Add a `source_offer` (URL + version) to `Welcome`, shown by clients. The three.js client is conveyed as minified JS: link its source. World assets are data, but scripts calling osage host bindings are "effectively linked" (FSF FAQ) and fall under the AGPL.

## 4. D-014 — in-process vs out-of-process components

- **FSF:** "pipes, sockets and command-line arguments … normally are separate programs" unless the exchange is "intimate enough, exchanging complex internal data structures". Public, versioned OpenDeviceDB contracts used by several hosts → a separate process speaking them is likely separate. In-process WASM with host "bindings" is "effectively linked" → treat as dynamic linking. D-014's conservative choice is right.
- **Contrary view:** the European Commission's licence-compatibility matrix: "linking two programs does not produce a single derivative of both"; Directive 2009/24 "seems to invalidate the idea of 'strong copyleft'". D-014 is safe under both views.
- **Out-of-process runner fixes:** no AGPL code in it (including OpenDeviceDB's own AGPL host code); GPL-2.0-compatible host functions → licence it **MIT**; wasmtime is `Apache-2.0 WITH LLVM-exception` (written for GPLv2 combination); avoid Apache-2.0-only code; take the MIT option of the SDKs. Running non-commercial components out of process does not lift their terms — osage should not redistribute them. The split matters only when someone ships a bridge with components preloaded; AGPL §6 then also requires Installation Information for hubs sold with osage.
- **Corrected table:**
  - **In process:** 0BSD, BSD-2/3-Clause, ISC, Zlib; LGPL-2.1-only (its §3 allows conversion to GPL) and LGPL-3.0; GPL-3.0 and AGPL-3.0 (AGPL §13); MPL-2.0 unless "Incompatible With Secondary Licenses"; EUPL-1.2 (appendix lists AGPLv3); EPL-2.0 only with the Exhibit A secondary-licence notice; CC BY-SA 4.0 (one way to GPLv3).
  - **Out of process:** BSD-4-Clause; EPL-1.0; EPL-2.0 without the notice; CDDL; GPL-2.0-only (including `WITH Classpath-exception-2.0`); non-free licences.
  - **Unknown, `NOASSERTION`, `LicenseRef-`:** out of process by default.
  - Evaluate whole SPDX expressions: `AND` → every part must fit; `OR` → at least one.

## 5. Interactions

- Apache-2.0 spec and MIT OR Apache-2.0 SDKs inside AGPL parts: compatible (FSF lists Apache-2.0 as GPLv3-compatible). Code flows one way: keep shared code and the GPU plugin interface in permissive crates.
- **Spec licence:** Apache-2.0's patent grant covers making and using "the Work" (the spec text), not independent implementations — **D-003's "protects every implementer" is doubtful**. The Community Specification License 1.0 grants patents "for Licensee's Implementation".
- **AI-assisted contributions:** US Copyright Office: "Copyright does not extend to purely AI-generated material"; "prompts do not alone provide sufficient control". *Thaler* (D.C. Cir. 2025), certiorari denied 2026-03-02. EU: "author's own intellectual creation" (Infopaq); Prague (2023) and Munich (Feb 2026) denied protection to AI output. The licence stays valid, but purely AI-generated parts may be unprotected (AGPL unenforceable on them). DCO: the responsible human signs off, consistent with the Linux kernel rule "AI agents MUST NOT add Signed-off-by tags"; QEMU declines AI-generated content because DCO (b)/(c) compliance "is unclear". Keep `Co-Authored-By` trailers as provenance; never name an AI in copyright notices.

## 6. Recommendations

**Revised Permission A (osage-exceptions-1.0, §A):**

> Additional permission under GNU AGPL version 3 section 7. If you modify this Program, or any covered work, by combining it with a Vendor GPU Component, the licensors grant you additional permission to convey the resulting work, provided that (a) the component is a separate library loaded at run time only through the GPU Plugin Interface documented in the source, and is not statically linked into any executable of the covered work; (b) every function of the covered work remains usable, possibly with lower performance or quality, without it; (c) you do not apply the component's licence terms to the covered work. Corresponding Source of the resulting work excludes the Vendor GPU Component, but includes the source of any adapter (other than vendor libraries linked into it). "Vendor GPU Component": a library, with the data it loads, published by a manufacturer of graphics or compute processors, implementing upscaling, frame generation, denoising, latency reduction or a similar hardware-specific rendering function, whose source is not available under a free software licence. This permission also applies under any later AGPL version you choose. Its use still requires complying with the component's own licence.

**Revised Permission B (§B):**

> Provided that you otherwise comply with this License for each covered work you convey — including making the Corresponding Source of the exact version conveyed available to every recipient at no charge, under this License, from a public network location named in the store listing and in the application's legal notices, for as long as you offer that version — the licensors grant you additional permission to convey executable versions of the Program through an Application Store as Executable Versions only under the Mozilla Public License 2.0, notwithstanding usage rules, device limits, technical protection measures or other restrictions that the store imposes on recipients. You may not impose further restrictions yourself. "Application Store": a distribution service operated by a third party whose terms recipients must accept to install software (e.g. Apple App Store, Google Play, Microsoft Store).

**Placement:** `LICENSE` stays the unmodified AGPL; add `LICENSE-EXCEPTIONS.md` (versioned §A, §B). File headers:

<!-- REUSE-IgnoreStart -->
```
SPDX-FileCopyrightText: The osage contributors
SPDX-License-Identifier: AGPL-3.0-or-later WITH AdditionRef-osage-exceptions-1.0
```
<!-- REUSE-IgnoreEnd -->

plus a line pointing to `LICENSE-EXCEPTIONS.md` (§7 notice). Crates: Cargo `license-file`; REUSE 3.3: a `LicenseRef-` file embedding the AGPL text plus the exceptions.

**CONTRIBUTING additions:** contributions are licensed under the licence in each file's header, including the permissions — files without the header are rejected; no third-party GPL/AGPL code without the same permission from its authors; agents never sign off — the human signs off after reviewing the change and checking it copies no third-party code; AGPL code never moves into the SDK or spec directories.

**Checklist before the first code commit:**

1. Choose AGPL-3.0-only or -or-later.
2. Final §A/§B text reviewed; decide any further permission now (consoles, store SDKs) — it cannot be added later.
3. `LICENSE-EXCEPTIONS.md`, `LICENSES/`, file headers; README and CONTRIBUTING updated.
4. CI: DCO check, `reuse lint`, cargo-deny licence allowlist.
5. A permissive GPU Plugin Interface crate; never commit vendor headers.
6. Write to NVIDIA; ask AMD to confirm its binary licensing.
7. `source_offer` in the protocol's `Welcome`.
8. Settle the spec licence (patent coverage).
9. Out-of-process runner as a separate MIT project speaking only OpenDeviceDB contracts; ask OpenDeviceDB to grant the app-store permission on its AGPL components.

**Needs a lawyer:** the final permission text (especially the MPL route vs a direct waiver); NVIDIA §4(e), §2(c), §4(f); whether contributor terms carried by the DCO and file headers hold under the governing law; whether the process and WASM boundaries hold under EU vs US law; the spec's patent licence; protection of AI-generated code.

## Not verified

Whether NVIDIA driver ≥ 620 also supplies DLSS upscaling and frame generation (or only the denoiser Blender uses); AMD's intent in listing binaries under MIT; whether the REUSE tool accepts `AdditionRef`; Google Play terms dated 2026-12-17; US case law on linking and derivative works; any Heather Meeker analysis (none found); the FSFE post on removability (fetch failed); whether Apple ever checks licences.

## Sources

- AGPL-3.0: `LICENSE`; FSF GPL FAQ https://www.gnu.org/licenses/gpl-faq.html (#GPLPlugins, #GPLIncompatibleLibs, #IfInterpreterIsGPL, #AGPLv3InteractingRemotely); FSF licence list https://www.gnu.org/licenses/license-list.html
- SPDX 3.0.1 expressions https://spdx.github.io/spdx-spec/v3.0.1/annexes/spdx-license-expressions/ · REUSE https://reuse.software/spec-3.3/ · https://reuse.software/faq/ · Cargo https://doc.rust-lang.org/cargo/reference/manifest.html
- DCO https://developercertificate.org/ · GitHub terms https://docs.github.com/en/site-policy/github-terms/github-terms-of-service
- NVIDIA https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt · Streamline https://github.com/NVIDIA-RTX/Streamline
- AMD https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK · Intel https://github.com/intel/xess/blob/main/LICENSE.txt
- Blender https://projects.blender.org/blender/blender/pulls/153077 · dlss_wgpu https://github.com/bevyengine/dlss_wgpu
- Apple https://www.apple.com/legal/internet-services/itunes/ · https://www.apple.com/legal/internet-services/itunes/dev/stdeula/ · https://developer.apple.com/app-store/review/guidelines/ · https://developer.apple.com/support/terms/apple-developer-program-license-agreement/
- Google https://play.google.com/about/play-terms/index.html · https://play.google/developer-distribution-agreement.html · https://developer.android.com/developer-verification
- VLC https://mailman.videolan.org/pipermail/vlc-devel/2010-October/076868.html · https://www.videolan.org/press/lgpl-libvlc.html · https://www.videolan.org/press/ios2out.html · FSF https://www.fsf.org/blogs/licensing/more-about-the-app-store-gpl-enforcement
- Precedents https://github.com/gomuks/gomuks/blob/main/LICENSE.iOS · https://github.com/signalapp/libsignal-protocol-c · https://github.com/mobile-shell/mosh/blob/master/COPYING.iOS
- EU compatibility matrix https://interoperable-europe.ec.europa.eu/node/143854
- Community Specification License https://github.com/CommunitySpecification/Community_Specification
- Linux kernel https://github.com/torvalds/linux/blob/master/Documentation/process/coding-assistants.rst · QEMU https://gitlab.com/qemu-project/qemu/-/raw/master/docs/devel/code-provenance.rst
- US Copyright Office https://www.copyright.gov/ai/Copyright-and-Artificial-Intelligence-Part-2-Copyrightability-Report.pdf · Thaler https://media.cadc.uscourts.gov/opinions/docs/2025/03/23-5233.pdf
- EU: Infopaq https://publications.europa.eu/resource/celex/62008CJ0005
