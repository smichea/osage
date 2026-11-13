// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// Two pages: the game, and the world's leaderboard (D-091).
import { resolve } from "node:path";
import { defineConfig } from "vite";

export default defineConfig({
  build: {
    rollupOptions: {
      input: {
        main: resolve(import.meta.dirname, "index.html"),
        leaderboard: resolve(import.meta.dirname, "leaderboard.html"),
      },
    },
  },
});
