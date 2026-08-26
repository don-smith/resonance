import type {
  MountedRecord,
  PackageHostErrorStage,
  PackageHostOptions,
} from "./records.js";

export function reportPackageError(
  record: MountedRecord,
  stage: PackageHostErrorStage,
  error: unknown,
  reporter: NonNullable<PackageHostOptions["onError"]>,
): void {
  try {
    reporter(error, record.entry.manifest, stage);
  } catch {
    // Error reporting must not disable the host queue or hide the safe region.
  }

  const document = record.root.ownerDocument;
  const region = document.createElement("div");
  region.setAttribute("role", "alert");
  region.setAttribute("data-package-error", stage);
  const heading = document.createElement("h2");
  heading.textContent = record.entry.manifest.name;
  const message = document.createElement("p");
  message.textContent = `${record.entry.manifest.name} could not ${
    stage === "import" ? "load" : stage
  }.`;
  region.append(heading, message);
  record.root.replaceChildren(region);
}
