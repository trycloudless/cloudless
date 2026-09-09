/// <reference path="./node_modules/webdriverio/build/@types/async.d.ts" />
import type { Options } from "@wdio/types";
import { spawn, ChildProcess } from "child_process";
import { mkdirSync, mkdtempSync, rmSync } from "fs";
import { tmpdir } from "os";
import path from "path";

const WEBDRIVER_PORT = 4445;
const E2E_API_BASE_URL = process.env.E2E_API_BASE_URL || "http://localhost:9099";

// cargo build produces a plain binary (not a .app bundle) for debug builds
const appPath = path.resolve(__dirname, "../rust/target/debug/cloudless");

// Shared state: each worker gets its own app process and data dir.
// These are managed in onWorkerStart/onWorkerEnd so each spec group
// gets a fresh Tauri app with clean in-memory state.
let appProcess: ChildProcess | undefined;
let testDataDir: string | undefined;

async function startApp(): Promise<void> {
  testDataDir = mkdtempSync(path.join(tmpdir(), "cloudless-e2e-"));

  appProcess = spawn(appPath, [], {
    stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
      TAURI_WEBDRIVER_PORT: String(WEBDRIVER_PORT),
      CLOUDLESS_DATA_DIR: testDataDir,
      API_BASE_URL: E2E_API_BASE_URL,
    },
  });

  appProcess.on("error", (err) => {
    console.error("Failed to start Tauri app:", err.message);
  });

  appProcess.stderr?.on("data", (data: Buffer) => {
    const msg = data.toString();
    if (msg.trim()) {
      console.error("[tauri stderr]", msg.trim());
    }
  });

  // Poll the embedded WebDriver /status endpoint until it's ready
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(
        new Error(
          `Embedded WebDriver on port ${WEBDRIVER_PORT} did not become ready within 30s`,
        ),
      );
    }, 30000);

    const poll = setInterval(async () => {
      try {
        const res = await fetch(
          `http://127.0.0.1:${WEBDRIVER_PORT}/status`,
        );
        if (res.ok) {
          clearInterval(poll);
          clearTimeout(timeout);
          resolve();
        }
      } catch {
        // not ready yet
      }
    }, 500);

    appProcess!.on("exit", (code) => {
      clearInterval(poll);
      clearTimeout(timeout);
      if (code !== null && code !== 0) {
        reject(new Error(`Tauri app exited with code ${code}`));
      }
    });
  });
}

function stopApp(): void {
  if (appProcess) {
    appProcess.kill();
    appProcess = undefined;
  }
  if (testDataDir) {
    rmSync(testDataDir, { recursive: true, force: true });
    testDataDir = undefined;
  }
}

export const config: Options.Testrunner = {
  runner: "local",
  // @ts-expect-error autoCompileOpts is not in the type def but is required by wdio
  autoCompileOpts: {
    tsNodeOpts: {
      project: "./tsconfig.json",
    },
  },
  specs: [
    // Auth form tests (each gets its own fresh app)
    "./specs/login.e2e.ts",
    "./specs/signup.e2e.ts",
    // S3 wizard + all post-setup tests share one app session
    [
      "./specs/signup-wizard.e2e.ts",
      "./specs/dashboard.e2e.ts",
      "./specs/backup.e2e.ts",
      "./specs/files.e2e.ts",
      "./specs/restore.e2e.ts",
      "./specs/settings.e2e.ts",
      "./specs/backup-config-management.e2e.ts",
      "./specs/navigation.e2e.ts",
    ],
    // Local filesystem wizard — gets its own fresh app (no S3 credentials needed)
    "./specs/local-storage-wizard.e2e.ts",
  ],
  maxInstances: 1,
  capabilities: [
    {
      browserName: "wry",
    },
  ],
  logLevel: "warn",
  waitforTimeout: 60000,
  connectionRetryTimeout: 120000,
  connectionRetryCount: 3,
  hostname: "127.0.0.1",
  port: WEBDRIVER_PORT,
  path: "/",
  framework: "mocha",
  reporters: ["spec"],
  screenshotPath: "./screenshots/",
  mochaOpts: {
    ui: "bdd",
    timeout: 120000,
  },
  afterTest: async function (
    test: { title: string; fullTitle: string },
    _context: unknown,
    { error }: { error?: Error; passed: boolean },
  ) {
    const screenshotDir = path.resolve(__dirname, "screenshots");
    mkdirSync(screenshotDir, { recursive: true });
    const status = error ? "fail" : "pass";
    const safeName = (test.fullTitle || test.title || `test-${Date.now()}`)
      .replace(/[^a-z0-9]+/gi, "-")
      .toLowerCase();
    await browser.saveScreenshot(
      path.join(screenshotDir, `${status}-${safeName}.png`),
    );
  },
  // Start a fresh Tauri app for each worker (spec group).
  // This ensures each spec group gets clean in-memory state.
  onWorkerStart: async function () {
    // Ensure any previous app is fully stopped and port is released
    stopApp();
    await new Promise((r) => setTimeout(r, 1000));
    await startApp();
  },
  onWorkerEnd: function () {
    stopApp();
  },
  // Wait for the Leptos reactive system to be fully initialized before running
  // any test.  The App component sets data-app-ready="true" on <body> inside a
  // one-shot Effect that fires after the first render cycle completes — at that
  // point the login page is mounted, reactive bindings are active, and all form
  // elements are interactable.
  //
  // This replaces the old approach of polling for button=Sign Up clickability,
  // which was unreliable: the button became clickable in the DOM before Leptos
  // finished initializing, causing stale-element races and intermittent failures.
  //
  // Allow up to 9 minutes (6 attempts × 90 s) for the cold-WASM JIT case on
  // slow machines.
  before: async function () {
    for (let attempt = 0; attempt < 6; attempt++) {
      await browser.url("tauri://localhost");
      const ready = await browser
        .waitUntil(
          async () => {
            try {
              // data-app-ready is set by App's one-shot Effect after first render.
              // isExisting() is a DOM-presence check — no timing gap between
              // "attribute set" and "element found".
              return await $("body[data-app-ready]").isExisting();
            } catch {
              return false;
            }
          },
          { timeout: 90000, interval: 200 },
        )
        .then(() => true)
        .catch(() => false);
      if (ready) {
        return;
      }
    }
    throw new Error(
      "App did not become ready (data-app-ready attribute never set) after 6 navigation attempts",
    );
  },
};
