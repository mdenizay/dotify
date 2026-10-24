import { getVersion } from "@tauri-apps/api/app";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { engine, errorMessage } from "../engine/bridge";
import { toast } from "../state/editor";
import { controller } from "./controller";

const RECHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
let busy = false;

/**
 * Check GitHub Releases for a signed update (see `plugins.updater` in
 * tauri.conf.json). Background checks stay silent unless an update exists;
 * `interactive` checks also report "up to date" and errors.
 */
export async function checkForUpdates({ interactive }: { interactive: boolean }) {
  if (busy) return;
  busy = true;
  try {
    const update = await check();
    if (!update) {
      if (interactive) {
        await message(`Dotify ${await getVersion()} is the latest version.`, {
          title: "You’re up to date",
        });
      }
      return;
    }
    const notes = update.body?.trim() ? `\n\n${update.body.trim()}` : "";
    const install = await ask(
      `Dotify ${update.version} is available — you have ${update.currentVersion}.${notes}`,
      { title: "Update Available", okLabel: "Install and Restart", cancelLabel: "Later" },
    );
    if (!install || !(await controller.confirmDiscard())) return;

    let total = 0;
    let received = 0;
    await update.downloadAndInstall((e) => {
      if (e.event === "Started") total = e.data.contentLength ?? 0;
      if (e.event === "Progress") {
        received += e.data.chunkLength;
        if (total) toast(`Downloading update… ${Math.round((received / total) * 100)}%`);
      }
      if (e.event === "Finished") toast("Installing update…");
    });
    // Unsaved work was either saved or explicitly discarded above.
    await engine.recoveryDiscard();
    await relaunch();
  } catch (e) {
    if (interactive) {
      await message(`Couldn’t check for updates: ${errorMessage(e)}`, {
        title: "Update Check Failed",
        kind: "error",
      });
    } else {
      console.warn("update check failed", e);
    }
  } finally {
    busy = false;
  }
}

/** Background checks: shortly after launch, then periodically. Release builds only. */
export function scheduleUpdateChecks(): () => void {
  if (!import.meta.env.PROD) return () => undefined;
  const first = setTimeout(() => void checkForUpdates({ interactive: false }), 5000);
  const every = setInterval(
    () => void checkForUpdates({ interactive: false }),
    RECHECK_INTERVAL_MS,
  );
  return () => {
    clearTimeout(first);
    clearInterval(every);
  };
}
