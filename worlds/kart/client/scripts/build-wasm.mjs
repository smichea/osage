// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// Builds the kart physics to WebAssembly and copies it to public/physics.wasm.

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";

const cargo = (args) => execFileSync("cargo", args, { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
cargo(["build", "-q", "-p", "osage-kart-physics", "--target", "wasm32-unknown-unknown", "--release"]);
const { target_directory } = JSON.parse(cargo(["metadata", "--format-version", "1", "--no-deps"]));
mkdirSync("public", { recursive: true });
copyFileSync(`${target_directory}/wasm32-unknown-unknown/release/osage_kart_physics.wasm`, "public/physics.wasm");
