import { defineConfig, devices } from "@playwright/test";

// UI tests run in Chromium against the Vite dev server with a fake Tauri backend (see e2e/fake-backend.ts).
// The dictation flows themselves are tested against the real app by scripts/e2e/suite.sh.
export default defineConfig({
  testDir: "e2e",
  timeout: 20_000,
  use: { baseURL: "http://localhost:1420", viewport: { width: 1000, height: 680 } },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1000, height: 680 } } }],
  webServer: { command: "npm run dev", url: "http://localhost:1420", reuseExistingServer: true, timeout: 60_000 },
});
