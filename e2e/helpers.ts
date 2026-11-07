import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { installFakeBackend, type FakeState } from "./fake-backend";

/** Call once per spec file: every test then fails on a console error or an uncaught exception. */
export function failOnConsoleErrors() {
  let errors: string[] = [];
  test.beforeEach(({ page }) => {
    errors = [];
    page.on("console", (m) => {
      if (m.type() === "error" && !m.text().includes("favicon")) errors.push(m.text());
    });
    page.on("pageerror", (e) => errors.push(e.message));
  });
  test.afterEach(() => expect(errors, "console errors").toEqual([]));
}

export async function open(page: Page, hash: string, state?: FakeState) {
  await installFakeBackend(page, state);
  await page.goto(`/#${hash}`);
}

export async function expectNoSeriousA11yIssues(page: Page) {
  const { violations } = await new AxeBuilder({ page }).analyze();
  const serious = violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => `${v.id}: ${v.help} -> ${v.nodes.map((n) => n.html.slice(0, 90)).join(" | ")}`);
  expect(serious).toEqual([]);
}

export const calls = (page: Page) => page.evaluate(() => (window as any).__fake.calls.map((c: any) => c.cmd) as string[]);
