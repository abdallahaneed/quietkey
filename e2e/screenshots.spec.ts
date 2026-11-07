// Saves screenshots of each screen for visual review. Runs only when asked:
//   SCREENSHOT_DIR=/path npx playwright test e2e/screenshots.spec.ts
import { test } from "@playwright/test";
import { sampleTranscript } from "./fake-backend";
import { open } from "./helpers";

const dir = process.env.SCREENSHOT_DIR;
test.skip(!dir, "set SCREENSHOT_DIR to save screenshots");

const data = {
  transcripts: [
    sampleTranscript(),
    sampleTranscript({ final_text: "Please send the notes to test@example.com, thanks a lot.", app_name: "Slack", created_at: Date.now() - 3_600_000 }),
  ],
  dictionary: [
    { id: "1", word: "Aneed", misspelling: "a need", source: "manual", created_at: 1 },
    { id: "2", word: "TanStack Query", misspelling: "react query", source: "manual", created_at: 1 },
    { id: "3", word: "Timo", misspelling: null, source: "import", created_at: 1 },
  ],
  snippets: [
    { id: "1", trigger: "my email", expansion: "name@example.com", updated_at: 1 },
    { id: "2", trigger: "my address", expansion: "Musterstraße 1\n20095 Hamburg", updated_at: 1 },
  ],
};

for (const theme of ["light", "dark"]) {
  for (const [screen, height] of [["home", 680], ["dictionary", 680], ["snippets", 680], ["settings", 2300]] as const) {
    test(`${screen} ${theme}`, async ({ page }) => {
      await page.setViewportSize({ width: 1000, height });
      await open(page, screen, data);
      await page.getByRole("heading", { level: 1 }).waitFor();
      await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
      await page.waitForTimeout(300);
      await page.screenshot({ path: `${dir}/${screen}-${theme}.png` });
    });
  }
}
