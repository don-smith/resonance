import { execFile } from "node:child_process";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { promisify } from "node:util";

export const execute = promisify(execFile);

export async function write(path: string, content: string): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, content);
}

export async function filesBelow(
  directory: string,
  prefix = "",
): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const files: string[] = [];
  for (const entry of entries.sort((left, right) =>
    left.name.localeCompare(right.name),
  )) {
    const path = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) {
      files.push(...(await filesBelow(resolve(directory, entry.name), path)));
    } else if (entry.isFile()) {
      files.push(path);
    }
  }
  return files;
}

export async function snapshotFiles(
  directory: string,
): Promise<Record<string, string>> {
  return Object.fromEntries(
    await Promise.all(
      (await filesBelow(directory)).map(async (path) => [
        path,
        await readFile(resolve(directory, path), "utf8"),
      ]),
    ),
  );
}
