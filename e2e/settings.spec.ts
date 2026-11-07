import { test, expect, type Page } from "@playwright/test";
import { expectNoSeriousA11yIssues, failOnConsoleErrors, open } from "./helpers";

failOnConsoleErrors();

const lastPatch = (page: Page) =>
  page.evaluate(() => {
    const c = (window as any).__fake.calls.filter((x: any) => x.cmd === "settings_update");
    return c.length ? c[c.length - 1].args.patch : null;
  });

test.describe("Settings", () => {
  test("loads every section without accessibility issues", async ({ page }) => {
    await open(page, "settings");
    for (const h of ["General", "Cleanup", "Shortcuts", "System", "Privacy"]) await expect(page.getByRole("heading", { name: h, exact: true })).toBeVisible();
    await expectNoSeriousA11yIssues(page);
  });

  test("pick a microphone", async ({ page }) => {
    await open(page, "settings");
    await page.getByRole("combobox", { name: "Microphone" }).click();
    await page.getByRole("option", { name: "USB Headset" }).click();
    expect(await lastPatch(page)).toEqual({ mic_id: "USB Headset" });
    await expect(page.getByRole("combobox", { name: "Microphone" })).toContainText("USB Headset");
  });

  test("a saved microphone that is unplugged is shown as not connected", async ({ page }) => {
    await open(page, "settings", { settings: { mic_id: "Old Headset" } });
    await expect(page.getByRole("combobox", { name: "Microphone" })).toContainText("Old Headset (not connected)");
  });

  test("dark theme applies at once", async ({ page }) => {
    await open(page, "settings");
    await page.getByRole("combobox", { name: "Theme" }).click();
    await page.getByRole("option", { name: "Dark" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  });

  test("languages: the last one cannot be removed, a second one switches to detection", async ({ page }) => {
    await open(page, "settings");
    await page.getByRole("checkbox", { name: "English" }).click(); // refused: it is the last language
    await expect(page.getByRole("alert")).toContainText("Keep at least one language");
    await expect(page.getByRole("checkbox", { name: "English" })).toBeChecked();
    await page.getByRole("checkbox", { name: "German" }).check();
    expect(await lastPatch(page)).toEqual({ languages: ["en", "de"] });
    await expect(page.getByText("Utter detects which of these you are speaking.")).toBeVisible();
  });

  test("cleanup level", async ({ page }) => {
    await open(page, "settings");
    await page.getByRole("radio", { name: /^Off Paste exactly/ }).click();
    expect(await lastPatch(page)).toEqual({ cleanup_level: "none" });
  });

  test("Groq: wrong key refused, real key saved to the Keychain, then removed", async ({ page }) => {
    await open(page, "settings");
    await page.getByRole("radio", { name: /^Groq/ }).click();
    expect(await lastPatch(page)).toEqual({ llm_provider: "groq" });
    await page.getByLabel("Groq API key").fill("hello");
    await page.getByRole("button", { name: "Save key" }).click();
    await expect(page.getByText("does not look like a Groq API key")).toBeVisible();
    await page.getByLabel("Groq API key").fill("gsk_" + "a".repeat(40));
    await page.getByRole("button", { name: "Save key" }).click();
    await expect(page.getByText("Saved in your macOS Keychain.")).toBeVisible();
    await page.getByRole("button", { name: "Remove key" }).click();
    await expect(page.getByLabel("Groq API key")).toBeVisible();
  });

  test("try cleanup: result with a provider, clear error without a key", async ({ page }) => {
    await open(page, "settings", { settings: { llm_provider: "groq" } });
    await page.getByRole("button", { name: "Try cleanup on a sample" }).click();
    await expect(page.getByText("no Groq API key saved")).toBeVisible();
    await page.getByRole("radio", { name: /^Off Only the built-in rules/ }).click();
    await page.getByRole("button", { name: "Try cleanup on a sample" }).click();
    await expect(page.getByText("So I think we should meet at 6 tomorrow.")).toBeVisible();
  });

  test("Ollama on another machine is refused and the old address kept", async ({ page }) => {
    await open(page, "settings", { settings: { llm_provider: "ollama" } });
    const address = page.getByLabel("Address");
    await address.fill("http://10.0.0.5:11434");
    await address.blur();
    await expect(page.getByRole("alert")).toContainText("Ollama must run on this Mac");
    const saved = await page.evaluate(() => (window as any).__TAURI_INTERNALS__.invoke("settings_get"));
    expect(saved.ollama_url).toBe("http://127.0.0.1:11434");
  });

  test("turning history off asks first; Cancel keeps it", async ({ page }) => {
    await open(page, "settings");
    await page.getByRole("radio", { name: /^Don't keep history/ }).click();
    await expect(page.getByRole("dialog")).toContainText("All saved dictations are deleted now");
    await page.getByRole("button", { name: "Cancel" }).click();
    await expect(page.getByRole("radio", { name: /^Keep history/ })).toBeChecked();
    await page.getByRole("radio", { name: /^Delete after 24 hours/ }).click();
    await page.getByRole("button", { name: "Delete and switch" }).click();
    expect(await lastPatch(page)).toEqual({ storage_mode: "auto_delete_24h" });
    await expect(page.getByRole("radio", { name: /^Delete after 24 hours/ })).toBeChecked();
  });

  test("delete data needs the word DELETE", async ({ page }) => {
    await open(page, "settings", { dictionary: [{ id: "a", word: "Timo", misspelling: null, source: "manual", created_at: 1 }] });
    await expect(page.getByRole("button", { name: "Delete selected…" })).toBeDisabled();
    await page.getByRole("checkbox", { name: "Dictionary" }).check();
    await page.getByRole("button", { name: "Delete selected…" }).click();
    const del = page.getByRole("dialog").getByRole("button", { name: "Delete", exact: true });
    await expect(del).toBeDisabled();
    await page.getByLabel("Type DELETE to confirm").fill("delete");
    await expect(del).toBeDisabled();
    await page.getByLabel("Type DELETE to confirm").fill("DELETE");
    await del.click();
    await expect(page.getByText("Deleted 1 item.")).toBeVisible();
  });

  test("system switches and the shortcut list", async ({ page }) => {
    await open(page, "settings");
    await page.getByRole("switch", { name: "Show the dictation bar" }).click();
    expect(await lastPatch(page)).toEqual({ show_bar: false });
    await page.getByRole("switch", { name: "Start Utter when you log in" }).click();
    expect(await lastPatch(page)).toEqual({ launch_at_login: false });
    await expect(page.getByText("Paste last dictation again")).toBeVisible();
  });

  test("warns about fn used by macOS and other dictation apps", async ({ page }) => {
    await open(page, "settings", { permissions: { accessibility: true, input_monitoring: true, hotkey_running: true, fn_key_conflict: true, other_dictation_apps: ["VoiceInk"] } });
    await expect(page.getByText(/macOS also uses fn/)).toBeVisible();
    await expect(page.getByText(/VoiceInk is running/)).toBeVisible();
  });

  test("no shortcut warnings when nothing conflicts", async ({ page }) => {
    await open(page, "settings");
    await expect(page.getByRole("heading", { name: "Shortcuts" })).toBeVisible();
    await expect(page.getByText(/macOS also uses fn|is running/)).toHaveCount(0);
  });
});
