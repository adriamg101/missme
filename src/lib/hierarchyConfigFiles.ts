// Named hierarchy-config save slots, persisted as one JSON file per name
// under the app's config directory (not the browser's localStorage, since
// this data needs to be shareable/inspectable outside the app too).
import { BaseDirectory, mkdir, readDir, readTextFile, rename, writeTextFile } from "@tauri-apps/plugin-fs";

const CONFIGS_DIR = "hierarchy-configs";

async function ensureConfigsDir(): Promise<void> {
  await mkdir(CONFIGS_DIR, { baseDir: BaseDirectory.AppConfig, recursive: true });
}

export async function listSavedConfigNames(): Promise<string[]> {
  await ensureConfigsDir();
  const entries = await readDir(CONFIGS_DIR, { baseDir: BaseDirectory.AppConfig });
  return entries
    .filter((entry) => entry.isFile && entry.name?.toLowerCase().endsWith(".json"))
    .map((entry) => entry.name!.slice(0, -".json".length))
    .sort((a, b) => a.localeCompare(b));
}

export async function loadSavedConfig(name: string): Promise<unknown> {
  const text = await readTextFile(`${CONFIGS_DIR}/${name}.json`, { baseDir: BaseDirectory.AppConfig });
  return JSON.parse(text);
}

export async function saveConfig(name: string, state: unknown): Promise<void> {
  await ensureConfigsDir();
  await writeTextFile(`${CONFIGS_DIR}/${name}.json`, JSON.stringify(state, null, 2), { baseDir: BaseDirectory.AppConfig });
}

export async function renameConfig(oldName: string, newName: string): Promise<void> {
  await rename(`${CONFIGS_DIR}/${oldName}.json`, `${CONFIGS_DIR}/${newName}.json`, {
    oldPathBaseDir: BaseDirectory.AppConfig,
    newPathBaseDir: BaseDirectory.AppConfig,
  });
}
