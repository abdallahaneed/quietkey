import { test, expect, type Page } from "@playwright/test";
import { expectNoSeriousA11yIssues, failOnConsoleErrors, open } from "./helpers";

failOnConsoleErrors();

const lastPatch = (page: Page) =>
  page.evaluate(() => {
    const c = (window as any).__fake.calls.filter((x: any) => x.cmd === "settings_update");
    return c.length ? c[c.length - 1].args.patch : null;
  });

const choose = async (page: Page, box: string, option: string) => {
  await page.getByRole("combobox", { name: box }).click();
  await page.getByRole("option", { name: option, exact: true }).click();
};

test.describe("Settings", () => {
  test("loads every section without accessibility issues", async ({ page }) => {
    await open(page, "settings");
    for (const h of ["Shortcuts", "Languages", "Sound", "Speech", "Cleanup", "App", "Privacy"]) await expect(page.getByRole("heading", { name: h, exact: true })).toBeVisible();
    await expectNoSeriousA11yIssues(page);
  });

  test("reset arrows: disabled at the default, reset an edited value", async ({ page }) => {
    await open(page, "settings", { settings: { cleanup_level: "none" } });
    await expect(page.getByRole("button", { name: "Reset theme to default" })).toBeDisabled();
    await page.getByRole("button", { name: "Reset cleanup level to default" }).click();
    expect(await lastPatch(page)).toEqual({ cleanup_level: "light" });
    await expect(page.getByRole("button", { name: "Reset cleanup level to default" })).toBeDisabled();
  });

  test("info icons explain a row", async ({ page }) => {
    await open(page, "settings");
    await page.getByRole("button", { name: "About Microphone" }).hover();
    await expect(page.getByRole("tooltip")).toContainText("falls back to the system default");
  });

  test("start and stop sounds switch", async ({ page }) => {
    await open(page, "settings");
    await page.getByRole("switch", { name: "Start and stop sounds" }).click();
    expect(await lastPatch(page)).toEqual({ sounds: false });
  });

  test("live text switches: on by default, 30 s limit only while live text is on", async ({ page }) => {
    await open(page, "settings");
    const live = page.getByRole("switch", { name: "Show words while you talk" });
    const limit = page.getByRole("switch", { name: "Stop live text after 30 seconds" });
    await expect(live).toBeChecked();
    await expect(limit).not.toBeChecked();
    await expect(page.getByText("Each update reads everything you said so far")).toBeVisible();
    await limit.click();
    expect(await lastPatch(page)).toEqual({ live_text_limit: true });
    await live.click();
    expect(await lastPatch(page)).toEqual({ live_text: false });
    await expect(limit).toBeDisabled();
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
    await expect(page.getByText("Quietkey detects which of these you are speaking.")).toBeVisible();
  });

  test("cleanup level", async ({ page }) => {
    await open(page, "settings");
    await choose(page, "Cleanup level", "Off");
    expect(await lastPatch(page)).toEqual({ cleanup_level: "none" });
  });

  test("Groq: wrong key refused, real key saved to the Keychain, then removed", async ({ page }) => {
    await open(page, "settings");
    await choose(page, "AI cleanup provider", "Groq");
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
    await choose(page, "AI cleanup provider", "Off");
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
    const box = page.getByRole("combobox", { name: "Dictation history" });
    await choose(page, "Dictation history", "Don't keep history");
    await expect(page.getByRole("dialog")).toContainText("All saved dictations are deleted now");
    await page.getByRole("button", { name: "Cancel" }).click();
    await expect(box).toContainText("Keep history");
    await choose(page, "Dictation history", "Delete after 24 hours");
    await page.getByRole("button", { name: "Delete and switch" }).click();
    expect(await lastPatch(page)).toEqual({ storage_mode: "auto_delete_24h" });
    await expect(box).toContainText("Delete after 24 hours");
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
    await page.getByRole("switch", { name: "Start Quietkey when you log in" }).click();
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

test.describe("Models", () => {
  test("graded per language, sorted for the user's languages, without accessibility issues", async ({ page }) => {
    await open(page, "models", { settings: { languages: ["ar"] } });
    await expect(page.getByRole("radio", { name: /Cohere Transcribe Arabic/ })).toContainText("A");
    // Arabic speaker: Cohere first among the downloads, Parakeet (no Arabic) last
    const names = await page.getByRole("radio").evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")!.split(".")[0]));
    expect(names.slice(-1)[0]).toBe("Parakeet v3");
    expect(names.indexOf("Cohere Transcribe Arabic")).toBe(2); // after the two on this Mac
    await expect(page.getByLabel("Arabic: not supported").first()).toBeVisible();
    await expectNoSeriousA11yIssues(page);
  });

  test("search filters the list", async ({ page }) => {
    await open(page, "models");
    await page.getByLabel("Search models").fill("arabic");
    await expect(page.getByRole("radio")).toHaveCount(1);
    await page.getByLabel("Search models").fill("zzz");
    await expect(page.getByText('No model matches "zzz".')).toBeVisible();
  });

  test("pick a model: download progress, other models locked, error unlocks", async ({ page }) => {
    await open(page, "models");
    const small = page.getByRole("radio", { name: /Whisper Small/ });
    await expect(small).toHaveAccessibleName(/190 MB download/);
    await expect(page.getByRole("radio", { name: /Large v3 Turbo/ })).toBeChecked();
    await small.click();
    const sent = await page.evaluate(() => (window as any).__fake.calls.filter((c: any) => c.cmd === "model_select").map((c: any) => c.args));
    expect(sent).toEqual([{ id: "small" }]);
    await page.evaluate(() => (window as any).__fake.emit("model://progress", { id: "small", done: 95_000_000, total: 190_000_000 }));
    await expect(small).toContainText("Downloading 50%");
    await expect(page.getByRole("button", { name: /Speech model: Whisper Small: downloading 50%/ })).toBeVisible(); // footer
    await expect(page.getByRole("radio", { name: /Whisper Base/ })).toBeDisabled();
    await page.evaluate(() => (window as any).__fake.emit("model://error", { id: "small", message: "network is down" }));
    await expect(page.getByText("Could not switch the model: network is down")).toBeVisible();
    await expect(page.getByRole("radio", { name: /Whisper Base/ })).toBeEnabled();
  });

  test("delete a downloaded model; the one in use has no delete", async ({ page }) => {
    await open(page, "models");
    await expect(page.getByRole("button", { name: "Delete Whisper Large v3 Turbo" })).toHaveCount(0);
    await page.getByRole("button", { name: "Delete Whisper Base" }).click();
    await expect(page.getByRole("radio", { name: /Whisper Base/ })).toHaveAccessibleName(/60 MB download/);
  });

  test("footer switches between downloaded models", async ({ page }) => {
    await open(page, "home");
    await page.getByRole("button", { name: /Speech model: Whisper Large v3 Turbo/ }).click();
    await expect(page.getByRole("menuitem", { name: "Cohere Transcribe Arabic" })).toHaveCount(0); // not downloaded
    await page.getByRole("menuitem", { name: "Whisper Base" }).click();
    const sent = await page.evaluate(() => (window as any).__fake.calls.filter((c: any) => c.cmd === "model_select").map((c: any) => c.args));
    expect(sent).toEqual([{ id: "base" }]);
  });
});
