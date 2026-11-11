import { test, expect } from "@playwright/test";
import { installFakeBackend, sampleTranscript } from "./fake-backend";
import { expectNoSeriousA11yIssues, failOnConsoleErrors, open } from "./helpers";

failOnConsoleErrors();

test.describe("Home", () => {
  test("empty state", async ({ page }) => {
    await open(page, "home");
    await expect(page.getByRole("heading", { name: "Your dictations" })).toBeVisible();
    await expect(page.getByText("Nothing dictated yet")).toBeVisible();
    await expectNoSeriousA11yIssues(page);
  });

  test("filled: grouped by day, app name, failed paste marked", async ({ page }) => {
    await open(page, "home", {
      transcripts: [
        sampleTranscript(),
        sampleTranscript({ final_text: "Second note", app_name: "Slack", status: "paste_failed", word_count: 2, created_at: Date.now() - 120_000 }),
      ],
    });
    await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
    await expect(page.getByText("So the meeting is at 6 p.m.")).toBeVisible();
    await expect(page.getByText("Not inserted, copied instead")).toBeVisible();
    await expect(page.getByText(/words in the last 7 days/)).toHaveText("9 words in the last 7 days");
    await expectNoSeriousA11yIssues(page);
  });

  test("long text is clamped and does not overflow the window", async ({ page }) => {
    const long = "Lorem ipsum dolor sit amet ".repeat(80);
    await open(page, "home", { transcripts: [sampleTranscript({ final_text: long, word_count: 400 })] });
    await expect(page.getByText(long.trim().slice(0, 40))).toBeVisible();
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
    expect(overflow).toBe(false);
  });

  test("copy sends the transcript id to the backend", async ({ page }) => {
    const t = sampleTranscript();
    await open(page, "home", { transcripts: [t] });
    await page.getByText(t.final_text).hover();
    await page.getByRole("button", { name: "Copy" }).first().click();
    const calls = await page.evaluate(() => (window as any).__fake.calls.map((c: any) => c.cmd));
    expect(calls).toContain("transcript_copy");
  });

  test("shows and copies what you said before cleanup", async ({ page }) => {
    const t = sampleTranscript();
    await open(page, "home", { transcripts: [t, sampleTranscript({ id: "same", raw_text: "Same text", final_text: "Same text" })] });
    await expect(page.getByRole("button", { name: "Show what you said" })).toHaveCount(1); // not for unchanged text
    await page.getByRole("button", { name: "Show what you said" }).click();
    await expect(page.getByText("um so the meeting is at 6")).toBeVisible();
    await page.getByText(t.final_text).hover();
    await page.getByRole("button", { name: "More" }).first().click();
    await page.getByRole("menuitem", { name: "Copy what you said" }).click();
    const args = await page.evaluate(() => (window as any).__fake.calls.filter((c: any) => c.cmd === "transcript_copy").map((c: any) => c.args));
    expect(args).toEqual([{ id: t.id, raw: true }]);
  });

  test("add a misheard name to the dictionary from a history entry", async ({ page }) => {
    const t = sampleTranscript({ final_text: "Ask Abdullah about it" });
    await open(page, "home", { transcripts: [t] });
    // Select the misheard word in the entry, then use the menu: both fields start from the selection.
    await page.getByText(t.final_text).evaluate((el) => {
      const r = document.createRange();
      const node = el.firstChild!;
      r.setStart(node, 4); r.setEnd(node, 12);
      getSelection()!.removeAllRanges(); getSelection()!.addRange(r);
    });
    await page.getByText(t.final_text).hover();
    await page.getByRole("button", { name: "More" }).first().click();
    await page.getByRole("menuitem", { name: "Add to dictionary" }).click();
    await expect(page.getByLabel("Quietkey wrote (optional)")).toHaveValue("Abdullah");
    await page.getByLabel("Correct spelling").fill("Abdallah");
    await page.getByRole("dialog").getByRole("button", { name: "Add" }).click();
    await expect(page.getByText('"Abdallah" added to your dictionary.')).toBeVisible();
    const args = await page.evaluate(() => (window as any).__fake.calls.filter((c: any) => c.cmd === "dictionary_add").map((c: any) => c.args));
    expect(args).toEqual([{ word: "Abdallah", misspelling: "Abdullah" }]);
  });

  test("delete a dictation from the menu, after confirming", async ({ page }) => {
    const t = sampleTranscript({ final_text: "Delete me please" });
    await open(page, "home", { transcripts: [t, sampleTranscript({ final_text: "Keep me" })] });
    await page.getByText("Delete me please").hover();
    await page.getByRole("button", { name: "More" }).first().click();
    await page.getByRole("menuitem", { name: "Delete" }).click();
    await page.getByRole("button", { name: "Keep" }).click();
    await expect(page.getByText("Delete me please")).toBeVisible();
    await page.getByText("Delete me please").hover();
    await page.getByRole("button", { name: "More" }).first().click();
    await page.getByRole("menuitem", { name: "Delete" }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
    await expect(page.getByText("Delete me please")).toHaveCount(0);
    await expect(page.getByText("Keep me")).toBeVisible();
  });

  test("a new dictation appears live at the top", async ({ page }) => {
    await open(page, "home", { transcripts: [sampleTranscript()] });
    await expect(page.getByText("So the meeting is at 6 p.m.")).toBeVisible();
    await page.evaluate((t) => (window as any).__fake.emit("transcript://created", t), sampleTranscript({ final_text: "Fresh one" }));
    await expect(page.getByRole("listitem").first()).toContainText("Fresh one");
  });

  test("database error is shown, not a blank screen", async ({ page }) => {
    await open(page, "home", { failList: true });
    await expect(page.getByRole("alert")).toContainText("Could not load history");
  });

  test("missing permissions show a setup notice that leads to Setup", async ({ page }) => {
    await open(page, "home", {
      permissions: { accessibility: false, input_monitoring: false, hotkey_running: false, fn_key_conflict: false },
    });
    await page.getByRole("button", { name: "Finish setup" }).click();
    await expect(page.getByRole("heading", { name: "Welcome to Quietkey" })).toBeVisible();
  });
});

test.describe("Setup", () => {
  const fresh = { settings: { onboarding_done: false } };
  const next = (page: import("@playwright/test").Page, name = "Continue") => page.getByRole("button", { name, exact: true }).click();

  test("first start opens onboarding instead of Home, one step at a time", async ({ page }) => {
    await open(page, "home", fresh);
    await expect(page.getByRole("heading", { name: "Welcome to Quietkey" })).toBeVisible();
    await expect(page.getByRole("navigation", { name: "Main" })).toHaveCount(0); // no sidebar during onboarding
    await expect(page.getByRole("list", { name: "Step 1 of 7" })).toBeVisible();
    await expectNoSeriousA11yIssues(page);
    await next(page, "Get started");
    await expect(page.getByRole("heading", { name: "How should speech become text?" })).toBeVisible();
    await page.getByRole("button", { name: "Back" }).click();
    await expect(page.getByRole("heading", { name: "Welcome to Quietkey" })).toBeVisible();
  });

  test("walk through every step and finish: onboarding is saved as done and Home opens", async ({ page }) => {
    await open(page, "home", fresh);
    await next(page, "Get started");
    for (const h of ["How should speech become text?", "Allow three things", "Check your microphone", "One shortcut, every app", "Give it a try"]) {
      await expect(page.getByRole("heading", { name: h })).toBeVisible();
      await expectNoSeriousA11yIssues(page);
      await next(page, h === "Give it a try" ? "Skip" : "Continue");
    }
    await expect(page.getByRole("heading", { name: "You're ready" })).toBeVisible();
    await next(page, "Start dictating");
    await expect(page.getByRole("heading", { name: "Your dictations" })).toBeVisible();
    const patches = await page.evaluate(() => (window as any).__fake.calls.filter((c: any) => c.cmd === "settings_update").map((c: any) => c.args.patch));
    expect(patches).toContainEqual({ onboarding_done: true });
  });

  test("speech step starts the model download while setup goes on", async ({ page }) => {
    await open(page, "home", { ...fresh, noModels: true });
    await next(page, "Get started");
    const cmds = await page.evaluate(() => (window as any).__fake.calls.map((c: any) => c.cmd));
    await expect.poll(() => page.evaluate(() => (window as any).__fake.calls.map((c: any) => c.cmd))).toContain("model_select");
    expect(cmds).not.toContain("hotkey_start");
  });

  test("permissions not granted: Allow buttons, a start notice, and Skip for now", async ({ page }) => {
    await open(page, "home", { ...fresh,
      permissions: { accessibility: false, input_monitoring: false, hotkey_running: false, fn_key_conflict: false, microphone: "denied" },
    });
    await next(page, "Get started"); await next(page);
    await expect(page.getByRole("button", { name: "Allow" })).toHaveCount(3);
    await expect(page.getByRole("button", { name: "Start listening" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Skip for now" })).toBeVisible();
    await expectNoSeriousA11yIssues(page);
  });

  test("Allow asks macOS first, so Quietkey shows up in System Settings", async ({ page }) => {
    await open(page, "home", { ...fresh,
      permissions: { accessibility: false, input_monitoring: true, hotkey_running: true, fn_key_conflict: false, microphone: "undetermined" },
    });
    await next(page, "Get started"); await next(page);
    for (const b of await page.getByRole("button", { name: "Allow" }).all()) await b.click();
    const cmds = await page.evaluate(() => (window as any).__fake.calls.map((c: any) => c.cmd));
    expect(cmds).toContain("request_accessibility");
    expect(cmds).toContain("request_microphone");
  });

  test("macOS fn conflict is explained with a way to fix it", async ({ page }) => {
    await open(page, "home", { ...fresh,
      permissions: { accessibility: true, input_monitoring: true, hotkey_running: true, fn_key_conflict: true, microphone: "granted" },
    });
    await next(page, "Get started"); await next(page);
    await expect(page.getByText(/Press fn key to/)).toBeVisible();
    await expect(page.getByRole("button", { name: "Open Keyboard settings" })).toBeVisible();
  });

  test("all granted: ready message", async ({ page }) => {
    await open(page, "home", fresh);
    await next(page, "Get started"); await next(page);
    await expect(page.getByText("All set. Quietkey can hear your shortcut and type for you.")).toBeVisible();
  });

  test("speech step: pick Groq and paste a key, or stay local and see the models", async ({ page }) => {
    await open(page, "home", fresh);
    await next(page, "Get started");
    const engine = page.getByRole("radiogroup", { name: "Speech engine" });
    await expect(engine.getByRole("radio", { name: /On this Mac/ })).toBeChecked();
    await expect(page.getByRole("radio", { name: /Large v3 Turbo/ })).toBeVisible();
    await engine.getByRole("radio", { name: /Groq/ }).click();
    await expect(page.getByText("Your recordings are sent to Groq")).toBeVisible();
    await page.getByLabel("Groq API key").fill("gsk_" + "b".repeat(40));
    await page.getByRole("button", { name: "Save key" }).click();
    await expect(page.getByText("Groq key saved in your Keychain. You are ready to dictate.")).toBeVisible();
    const patches = await page.evaluate(() => (window as any).__fake.calls.filter((c: any) => c.cmd === "settings_update").map((c: any) => c.args.patch));
    expect(patches).toContainEqual({ stt_engine: "groq" });
    await expectNoSeriousA11yIssues(page);
  });

  test("Setup in the sidebar runs it again, and it can be closed", async ({ page }) => {
    await open(page, "home");
    await page.getByRole("link", { name: "Setup" }).click();
    await expect(page.getByRole("heading", { name: "Welcome to Quietkey" })).toBeVisible();
    await page.getByRole("button", { name: "Close setup" }).click();
    await expect(page.getByRole("heading", { name: "Your dictations" })).toBeVisible();
  });
});

test.describe("Navigation", () => {
  test("keyboard only: Tab to a sidebar link, Enter opens it", async ({ page }) => {
    await open(page, "home");
    const link = page.getByRole("link", { name: "Dictionary" });
    for (let i = 0; i < 6 && !(await link.evaluate((el) => el === document.activeElement)); i++) await page.keyboard.press("Tab");
    await expect(link).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("heading", { name: "Dictionary" })).toBeVisible();
    await expect(link).toHaveAttribute("aria-current", "page");
  });

  test("⌘, opens Settings", async ({ page }) => {
    await open(page, "home");
    await expect(page.getByRole("heading", { name: "Your dictations" })).toBeVisible(); // listener is mounted
    await page.keyboard.press("Meta+Comma");
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  });

  for (const screen of ["insights"]) {
    test(`${screen} placeholder renders without errors`, async ({ page }) => {
      await open(page, screen);
      await expect(page.getByText("Coming soon")).toBeVisible();
      await expectNoSeriousA11yIssues(page);
    });
  }

  test("minimum window size 860x560: no horizontal scroll", async ({ page }) => {
    await page.setViewportSize({ width: 860, height: 560 });
    await open(page, "home", { transcripts: [sampleTranscript()] });
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
    expect(overflow).toBe(false);
  });
});

test.describe("Design system page", () => {
  for (const theme of ["light", "dark"]) {
    test(`${theme} theme has no serious accessibility issues`, async ({ page }) => {
      await open(page, "design");
      await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
      await expectNoSeriousA11yIssues(page);
    });
  }
});

test.describe("Dictation bar", () => {
  test("shows each dictation state", async ({ page }) => {
    await installFakeBackend(page);
    await page.goto("/?w=bar");
    const status = page.getByRole("status");
    await expect(status).toHaveText("Ready, hold fn to talk");
    const emit = (state: string, message: string | null = null) =>
      page.evaluate(([s, m]) => (window as any).__fake.emit("dictation://state", { state: s, message: m }), [state, message] as const);
    await emit("recording");
    await expect(status).toHaveText("Listening");
    await emit("processing");
    await expect(status).toHaveText("Writing it down");
    await emit("error", "No microphone found");
    await expect(status).toContainText("No microphone found");
    await emit("idle");
    await expect(status).toHaveText("Ready, hold fn to talk");
  });
});

test.describe("Bar buttons", () => {
  test("hands-free shows Cancel and Stop; a cancelled dictation offers Undo", async ({ page }) => {
    await installFakeBackend(page);
    await page.goto("/?w=bar");
    const emit = (state: string) => page.evaluate((s) => (window as any).__fake.emit("dictation://state", { state: s, message: null }), state);
    const calls = () => page.evaluate(() => (window as any).__fake.calls.map((c: { cmd: string }) => c.cmd));
    await emit("hands_free");
    await page.getByRole("button", { name: "Stop and insert" }).click();
    await page.getByRole("button", { name: "Cancel" }).click();
    await emit("cancelled");
    await page.getByRole("button", { name: "Undo" }).click();
    const cmds = await calls();
    expect(cmds).toEqual(expect.arrayContaining(["dictation_toggle", "dictation_cancel", "dictation_undo"]));
    // clicks are let through only while buttons show
    expect(cmds).toContain("plugin:window|set_ignore_cursor_events");
  });
});

test.describe("Shortcut editor", () => {
  test("records ⌥ + ⌃ and saves it, Use fn resets", async ({ page }) => {
    await open(page, "settings");
    await page.getByRole("button", { name: "Change" }).click();
    await page.keyboard.down("Alt"); // Playwright sends AltLeft; a single left key is refused by the backend, so use two
    await page.keyboard.down("Control");
    await page.keyboard.up("Control");
    await page.keyboard.up("Alt");
    const saved = await page.evaluate(() => (window as any).__TAURI_INTERNALS__.invoke("settings_get"));
    expect(saved.shortcuts.push_to_talk).toEqual([["LeftOpt", "LeftCtrl"]]);
    expect(saved.shortcuts.hands_free).toEqual([["LeftOpt", "LeftCtrl", "Space"]]);
    await page.getByRole("button", { name: "Use fn" }).click();
    const reset = await page.evaluate(() => (window as any).__TAURI_INTERNALS__.invoke("settings_get"));
    expect(reset.shortcuts.push_to_talk).toEqual([["Fn"]]);
  });

  test("a letter key is refused with a reason", async ({ page }) => {
    await open(page, "settings");
    await page.getByRole("button", { name: "Change" }).click();
    await page.keyboard.press("k");
    await expect(page.getByRole("alert")).toContainText("modifier keys only");
  });
});

test.describe("History cards", () => {
  test("star, save to file, play and transcribe again", async ({ page }) => {
    const t = sampleTranscript({ audio: true });
    await open(page, "home", { transcripts: [t, sampleTranscript({ id: "old", final_text: "No recording here" })] });
    await expect(page.getByRole("button", { name: "Play recording" })).toHaveCount(1); // only the one with audio
    await page.getByRole("button", { name: "Star" }).first().click();
    await expect(page.getByRole("button", { name: "Unstar" })).toHaveAttribute("aria-pressed", "true");
    await page.getByRole("button", { name: "Save to file" }).first().click();
    await expect(page.getByText("Saved to /Users/me/Downloads/Quietkey 2026-11-08 10-15.txt")).toBeVisible();
    await page.getByRole("button", { name: "Transcribe again" }).click();
    await expect(page.getByText("Transcribed again.", { exact: true })).toBeVisible();
    const cmds = await page.evaluate(() => (window as any).__fake.calls.map((c: any) => c.cmd));
    expect(cmds).toEqual(expect.arrayContaining(["transcript_favorite", "transcript_save", "transcript_retranscribe"]));
    await expectNoSeriousA11yIssues(page);
  });
});
