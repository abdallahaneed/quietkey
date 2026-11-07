import { test, expect } from "@playwright/test";
import { expectNoSeriousA11yIssues, failOnConsoleErrors, open } from "./helpers";

failOnConsoleErrors();

test.describe("Style", () => {
  test("four kinds of app, each with only its own tones", async ({ page }) => {
    await open(page, "style");
    for (const h of ["Personal messages", "Work chat", "Email", "Everything else"]) {
      await expect(page.getByRole("heading", { name: h })).toBeVisible();
    }
    const personal = page.getByRole("radiogroup", { name: "Tone for Personal messages" });
    await expect(personal.getByRole("radio", { name: /Texting/ })).toBeVisible();
    await expect(personal.getByRole("radio", { name: /Upbeat/ })).toHaveCount(0);
    const work = page.getByRole("radiogroup", { name: "Tone for Work chat" });
    await expect(work.getByRole("radio", { name: /Texting/ })).toHaveCount(0);
    await expectNoSeriousA11yIssues(page);
  });

  test("choosing a tone saves it", async ({ page }) => {
    await open(page, "style");
    const work = page.getByRole("radiogroup", { name: "Tone for Work chat" });
    await work.getByRole("radio", { name: /Upbeat/ }).click();
    await expect(work.getByRole("radio", { name: /Upbeat/ })).toBeChecked();
    const calls = await page.evaluate(() => (window as any).__fake.calls.filter((c: any) => c.cmd === "style_set").map((c: any) => c.args));
    expect(calls).toEqual([{ category: "work", tone: "excited" }]);
  });

  test("keyboard only: arrow keys move between tones", async ({ page }) => {
    await open(page, "style");
    const personal = page.getByRole("radiogroup", { name: "Tone for Personal messages" });
    await personal.getByRole("radio", { name: /Tidy/ }).focus();
    await page.keyboard.press("ArrowDown", { delay: 50 }); // Radix moves focus on a timer; a real key press lasts this long
    await expect(personal.getByRole("radio", { name: /Relaxed/ })).toBeChecked();
  });

  test("says when tones only partly apply: no AI cleanup", async ({ page }) => {
    await open(page, "style");
    await expect(page.getByText(/Without AI cleanup, a tone only changes/)).toBeVisible();
  });

  test("says when tones do nothing: cleanup off", async ({ page }) => {
    await open(page, "style", { settings: { cleanup_level: "none" } });
    await expect(page.getByText(/Cleanup is off/)).toBeVisible();
  });

  test("no notice with AI cleanup on", async ({ page }) => {
    await open(page, "style", { settings: { llm_provider: "ollama", cleanup_level: "light" } });
    await expect(page.getByRole("heading", { name: "Email" })).toBeVisible();
    await expect(page.getByText(/Without AI cleanup|Cleanup is off/)).toHaveCount(0);
  });

  test("load error offers a retry", async ({ page }) => {
    await open(page, "style", { failStyles: true });
    await expect(page.getByText(/Could not load styles/)).toBeVisible();
    await expect(page.getByRole("button", { name: "Try again" })).toBeVisible();
  });

  test("fits a narrow window", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    await open(page, "style");
    await expect(page.getByRole("heading", { name: "Email" })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
  });
});
