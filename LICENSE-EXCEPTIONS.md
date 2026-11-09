# osage additional permissions — osage-exceptions-1.0

> **DRAFT — not yet in force.** These permissions take effect with the first code commit, once their final text has been reviewed by a lawyer ([research/06](research/06-licensing-review.md), [decision log](design/decisions.md) D-005, D-020 to D-022). Until then the repository contains only documentation.

These are additional permissions under section 7 of the GNU Affero General Public License version 3, granted by the licensors of every file whose header reads:

```
SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
```

(SPDX 3.0 equivalent: `AGPL-3.0-or-later WITH AdditionRef-osage-exceptions-1.0`. REUSE 3.3 and Cargo still use SPDX 2.3, which has no custom exceptions, so the AGPL text and these permissions are combined in [`LICENSES/LicenseRef-osage-AGPL-3.0-or-later-with-exceptions.txt`](LICENSES/LicenseRef-osage-AGPL-3.0-or-later-with-exceptions.txt).)

As section 7 allows, anyone who conveys a copy may remove any of these permissions from that copy. They do not apply to code whose authors did not grant them (for example third-party GPL or AGPL code).

## §A — Vendor GPU components

If you modify this Program, or any covered work, by combining it with a Vendor GPU Component, the licensors grant you additional permission to convey the resulting work, provided that (a) the component is a separate library loaded at run time only through the GPU Plugin Interface documented in the source, and is not statically linked into any executable of the covered work; (b) every function of the covered work remains usable, possibly with lower performance or quality, without it; (c) you do not apply the component's licence terms to the covered work. Corresponding Source of the resulting work excludes the Vendor GPU Component, but includes the source of any adapter (other than vendor libraries linked into it).

"Vendor GPU Component": a library, with the data it loads, published by a manufacturer of graphics or compute processors, implementing upscaling, frame generation, denoising, latency reduction or a similar hardware-specific rendering function, whose source is not available under a free software licence.

This permission also applies under any later version of the GNU Affero General Public License you choose. Its use still requires complying with the component's own licence.

## §B — Application stores

Provided that you otherwise comply with this License for each covered work you convey — including making the Corresponding Source of the exact version conveyed available to every recipient at no charge, under this License, from a public network location named in the store listing and in the application's legal notices, for as long as you offer that version — the licensors grant you additional permission to convey executable versions of the Program through an Application Store as Executable Versions only under the Mozilla Public License 2.0, notwithstanding usage rules, device limits, technical protection measures or other restrictions that the store imposes on recipients. You may not impose further restrictions yourself.

"Application Store": a distribution service operated by a third party whose terms recipients must accept to install software (e.g. Apple App Store, Google Play, Microsoft Store, Steam, console stores).

## §C — World modules

A "World Module" is a program that interacts with the Program only through the World Module Interface documented in the source (for example a WebAssembly module loaded by a region server), and that contains no part of the Program or of any work based on it, other than the published bindings of the World Module Interface, which are licensed under permissive terms.

The licensors grant you additional permission to combine the Program with World Modules, and to convey the resulting work and to let users interact with it remotely through a computer network, under terms of your choice for the World Modules, provided that the Program itself and every modification of it remain subject to this License — including section 13 — and that the Corresponding Source of the Program as you run it is offered as section 13 requires. Corresponding Source of the resulting work excludes World Modules.

## §D — Platform components

If you modify this Program, or any covered work, by combining it with a Platform Component, the licensors grant you additional permission to convey the resulting work, provided that (a) the Platform Component is accessed only through the Platform Interface documented in the source, as a separate library or through the platform's system services; (b) the covered work, apart from the platform-specific adapter, remains usable on at least one platform without any Platform Component; (c) you do not apply the Platform Component's licence terms to the covered work. Corresponding Source of the resulting work excludes the Platform Component and, only to the extent that the platform's confidentiality terms forbid its disclosure, the platform-specific adapter; all other Corresponding Source must be provided.

"Platform Component": a library, service or software development kit published by the operator of a hardware or distribution platform (game console, PC or mobile store, operating system vendor), giving access to that platform's services — accounts, achievements, multiplayer services, payments, storefront or console system interfaces — whose source is not available under a free software licence.
