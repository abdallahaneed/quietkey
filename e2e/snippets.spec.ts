import { test, expect } from "@playwright/test";
import { expectNoSeriousA11yIssues, failOnConsoleErrors, open } from "./helpers";

failOnConsoleErrors();

const snip = (trigger: string, expansion: string) => ({ id: trigger, trigger, expansion, updated_at: 1 });

test.describe("Snippets", () => {
  test("empty state", async ({ page }) => {
    await open(page, "snippets");
    await expect(page.getByText("No snippets yet")).toBeVisible();
    await expectNoSeriousA11yIssues(page);
  });

  test("create a multi-line snippet", async ({ page }) => {
    await open(page, "snippets");
    await page.getByRole("button", { name: "Create a snippet" }).click();
    await page.getByLabel("What you say").fill("my address");
    await page.getByLabel("Text it becomes").fill("Musterstraße 1\n20095 Hamburg");
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByText("“my address”")).toBeVisible();
    await expect(page.getByText("20095 Hamburg")).toBeVisible();
    await expectNoSeriousA11yIssues(page);
  });

  test("duplicate cue (any capitalisation) is refused with a message", async ({ page }) => {
    await open(page, "snippets", { snippets: [snip("my email", "a@b.de")] });
    await page.getByRole("button", { name: "Add snippet" }).click();
    await page.getByLabel("What you say").fill("My Email");
    await page.getByLabel("Text it becomes").fill("x@y.de");
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByRole("dialog")).toContainText("That snippet already exists");
  });

  test("text over 4,000 characters cannot be saved", async ({ page }) => {
    await open(page, "snippets");
    await page.getByRole("button", { name: "Create a snippet" }).click();
    await page.getByLabel("What you say").fill("long one");
    await page.getByLabel("Text it becomes").fill("a".repeat(4001));
    await expect(page.getByText("Keep it under 4,000 characters")).toBeVisible();
    await expect(page.getByRole("button", { name: "Create" })).toBeDisabled();
  });

  test("search, edit and delete", async ({ page }) => {
    await open(page, "snippets", { snippets: [snip("my email", "a@b.de"), snip("my link", "https://cal.example/me")] });
    await page.getByLabel("Search snippets").fill("cal.example");
    await expect(page.getByText("“my link”")).toBeVisible();
    await expect(page.getByText("“my email”")).toHaveCount(0);
    await page.getByLabel("Search snippets").fill("");
    await page.getByRole("button", { name: "Edit my email" }).click();
    await page.getByLabel("Text it becomes").fill("new@b.de");
    await page.getByRole("button", { name: "Save" }).click();
    await expect(page.getByText("new@b.de")).toBeVisible();
    await page.getByRole("button", { name: "Delete my link" }).click();
    await page.getByRole("button", { name: "Delete", exact: true }).click();
    await expect(page.getByText("“my link”")).toHaveCount(0);
  });

  test("long cue and text stay inside the window", async ({ page }) => {
    await open(page, "snippets", { snippets: [snip("x".repeat(60), "word ".repeat(600))] });
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
    expect(overflow).toBe(false);
  });
});
