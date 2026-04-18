import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const repoRoot = process.cwd();
const packageJsonPath = path.join(repoRoot, "package.json");
const cargoTomlPath = path.join(repoRoot, "src-tauri", "Cargo.toml");
const tauriConfigPath = path.join(repoRoot, "src-tauri", "tauri.conf.json");

function isValidVersion(value) {
  return /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(value);
}

async function readPackageJsonVersion() {
  const raw = await readFile(packageJsonPath, "utf8");
  const parsed = JSON.parse(raw);

  if (typeof parsed.version !== "string") {
    throw new Error("package.json version is missing");
  }

  return {
    version: parsed.version,
    raw,
    parsed,
  };
}

async function readCargoVersion() {
  const raw = await readFile(cargoTomlPath, "utf8");
  const match = raw.match(/^version = "([^"]+)"$/m);

  if (!match) {
    throw new Error("Cargo.toml version is missing");
  }

  return {
    version: match[1],
    raw,
  };
}

async function readTauriConfigVersion() {
  const raw = await readFile(tauriConfigPath, "utf8");
  const parsed = JSON.parse(raw);

  if (typeof parsed.version !== "string") {
    throw new Error("tauri.conf.json version is missing");
  }

  return {
    version: parsed.version,
    raw,
    parsed,
  };
}

async function readVersions() {
  const [packageJson, cargoToml, tauriConfig] = await Promise.all([
    readPackageJsonVersion(),
    readCargoVersion(),
    readTauriConfigVersion(),
  ]);

  return {
    packageJson,
    cargoToml,
    tauriConfig,
  };
}

function versionMismatchMessage(versions) {
  return [
    "version mismatch detected:",
    `- package.json: ${versions.packageJson.version}`,
    `- src-tauri/Cargo.toml: ${versions.cargoToml.version}`,
    `- src-tauri/tauri.conf.json: ${versions.tauriConfig.version}`,
  ].join("\n");
}

async function checkVersions() {
  const versions = await readVersions();
  const uniqueVersions = new Set([
    versions.packageJson.version,
    versions.cargoToml.version,
    versions.tauriConfig.version,
  ]);

  if (uniqueVersions.size !== 1) {
    throw new Error(versionMismatchMessage(versions));
  }

  console.log(`Version check passed: ${versions.packageJson.version}`);
}

async function setVersion(nextVersion) {
  if (!nextVersion || !isValidVersion(nextVersion)) {
    throw new Error(
      "invalid version; use a semver-like value such as `0.1.1` or `0.2.0-beta.1`",
    );
  }

  const versions = await readVersions();
  const currentVersion = versions.packageJson.version;

  versions.packageJson.parsed.version = nextVersion;
  versions.tauriConfig.parsed.version = nextVersion;

  const nextCargoRaw = versions.cargoToml.raw.replace(
    /^version = "([^"]+)"$/m,
    `version = "${nextVersion}"`,
  );

  await Promise.all([
    writeFile(packageJsonPath, JSON.stringify(versions.packageJson.parsed, null, 2) + "\n", "utf8"),
    writeFile(tauriConfigPath, JSON.stringify(versions.tauriConfig.parsed, null, 2) + "\n", "utf8"),
    writeFile(cargoTomlPath, nextCargoRaw, "utf8"),
  ]);

  console.log(`Updated version: ${currentVersion} -> ${nextVersion}`);
}

const command = process.argv[2] ?? "check";

if (command === "check") {
  await checkVersions();
} else if (command === "set") {
  await setVersion(process.argv[3]);
} else {
  throw new Error("unknown command; use `check` or `set <version>`");
}
