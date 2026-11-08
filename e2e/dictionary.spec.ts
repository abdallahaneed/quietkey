import { test, expect } from "@playwright/test";
import { calls, expectNoSeriousA11yIssues, failOnConsoleErrors, open } from "./helpers";

failOnConsoleErrors();

const entry = (word: string, misspelling: string | null = null) => ({ id: word, word, misspelling, source: "manual", created_at: 1 });

test.describe("Dictionary", () => {
  test("empty state offers to add the first word", async ({ page }) => {
    await open(page, "dictionary");
    await expect(page.getByText("No words yet")).toBeVisible();
    await expectNoSeriousA11yIssues(page);
  });

  test("add a word with a correction, keyboard only", async ({ page }) => {
    await open(page, "dictionary");
    await page.getByRole("button", { name: "Add your first word" }).click();
    await page.keyboard.type("Aneed");
    await page.getByRole("switch", { name: "Fix a word Quietkey hears wrong" }).click();
    await page.getByLabel("Quietkey writes").fill("a need");
    await page.getByLabel("Quietkey writes").press("Enter");
    await expect(page.getByRole("cell", { name: "Aneed", exact: true })).toBeVisible();
    await expect(page.getByText("Heard as “a need”")).toBeVisible();
    await expectNoSeriousA11yIssues(page);
  });

  test("duplicate word shows the backend's message and keeps the dialog open", async ({ page }) => {
    await open(page, "dictionary", { dictionary: [entry("Timo")] });
    await page.getByRole("button", { name: "Add word" }).click();
    await page.getByLabel("Word", { exact: true }).fill("timo");
    await page.getByRole("button", { name: "Add", exact: true }).click();
    await expect(page.getByRole("dialog")).toContainText("That word already exists");
  });

  test("60 character limit and long words do not break the table", async ({ page }) => {
    const long = "Supercalifragilisticexpialidocious-Hamburg-Logistics-GmbH-01";
    await open(page, "dictionary", { dictionary: [entry(long)] });
    await expect(page.getByTitle(long)).toBeVisible();
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
    expect(overflow).toBe(false);
    await page.getByRole("button", { name: "Add word" }).click();
    await page.getByLabel("Word", { exact: true }).fill("x".repeat(80));
    await expect(page.getByLabel("Word", { exact: true })).toHaveValue("x".repeat(60));
  });

  test("search filters and shows a no-match message", async ({ page }) => {
    await open(page, "dictionary", { dictionary: [entry("Timo"), entry("TanStack Query", "react query")] });
    await page.getByLabel("Search words").fill("react");
    await expect(page.getByRole("cell", { name: "TanStack Query", exact: true })).toBeVisible();
    await expect(page.getByRole("cell", { name: "Timo", exact: true })).toHaveCount(0);
    await page.getByLabel("Search words").fill("zzz");
    await expect(page.getByText("No words match “zzz”.")).toBeVisible();
  });

  test("edit a word", async ({ page }) => {
    await open(page, "dictionary", { dictionary: [entry("Timo")] });
    await page.getByRole("button", { name: "Edit Timo" }).click();
    await page.getByLabel("Word", { exact: true }).fill("Timo Best");
    await page.getByRole("button", { name: "Save" }).click();
    await expect(page.getByRole("cell", { name: "Timo Best", exact: true })).toBeVisible();
  });

  test("delete asks first, Keep cancels, Remove deletes", async ({ page }) => {
    await open(page, "dictionary", { dictionary: [entry("Timo")] });
    await page.getByRole("button", { name: "Delete Timo" }).click();
    await page.getByRole("button", { name: "Keep" }).click();
    await expect(page.getByRole("cell", { name: "Timo", exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Delete Timo" }).click();
    await page.getByRole("button", { name: "Remove" }).click();
    await expect(page.getByText("No words yet")).toBeVisible();
  });

  test("select several and delete them together", async ({ page }) => {
    await open(page, "dictionary", { dictionary: [entry("Alpha"), entry("Beta"), entry("Gamma")] });
    await page.getByRole("checkbox", { name: "Select Alpha" }).check();
    await page.getByRole("checkbox", { name: "Select Beta" }).check();
    await expect(page.getByText("2 selected")).toBeVisible();
    await page.getByRole("button", { name: "Delete selected" }).click();
    await page.getByRole("button", { name: "Remove" }).click();
    await expect(page.getByRole("cell", { name: "Gamma", exact: true })).toBeVisible();
    await expect(page.getByRole("cell", { name: "Alpha", exact: true })).toHaveCount(0);
    await expect(page.getByText("2 words removed.")).toBeVisible();
  });

  test("import a text file: words and corrections, duplicates skipped", async ({ page }) => {
    await open(page, "dictionary", { dictionary: [entry("Timo")] });
    await page.getByLabel("Import file").setInputFiles({
      name: "words.txt", mimeType: "text/plain",
      buffer: Buffer.from("# names\nTimo\nbest impact => best-impact\nHamburg\n!!!\n"),
    });
    await expect(page.getByText("Imported 2 words, skipped 2 (duplicates or invalid).")).toBeVisible();
    await expect(page.getByRole("cell", { name: "best-impact", exact: true })).toBeVisible();
    expect(await calls(page)).toContain("dictionary_import");
  });
});
