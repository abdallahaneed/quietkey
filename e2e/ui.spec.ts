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
    await expect(page.getByRole("heading", { name: "Set up dictation" })).toBeVisible();
  });
});

test.describe("Setup", () => {
  test("permissions not granted: Allow buttons and a start notice", async ({ page }) => {
    await open(page, "setup", {
      permissions: { accessibility: false, input_monitoring: false, hotkey_running: false, fn_key_conflict: false, microphone: "denied" },
    });
    await expect(page.getByRole("button", { name: "Allow" })).toHaveCount(3);
    await expect(page.getByRole("button", { name: "Start listening" })).toBeVisible();
    await expectNoSeriousA11yIssues(page);
  });

  test("macOS fn conflict is explained with a way to fix it", async ({ page }) => {
    await open(page, "setup", {
      permissions: { accessibility: true, input_monitoring: true, hotkey_running: true, fn_key_conflict: true, microphone: "granted" },
    });
    await expect(page.getByText(/Press fn key to/)).toBeVisible();
    await expect(page.getByRole("button", { name: "Open Keyboard settings" })).toBeVisible();
  });

  test("all granted: ready message", async ({ page }) => {
    await open(page, "setup");
    await expect(page.getByText("Ready. Hold fn in any text field and talk.")).toBeVisible();
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
