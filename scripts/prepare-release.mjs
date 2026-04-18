import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { spawn } from "node:child_process";

const repoRoot = process.cwd();
const packageJsonPath = path.join(repoRoot, "package.json");

function isValidVersion(value) {
  return /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(value);
}

async function readCurrentVersion() {
  const packageJson = JSON.parse(await readFile(packageJsonPath, "utf8"));
  const version = packageJson.version;

  if (typeof version !== "string" || !version.trim()) {
    throw new Error("package.json version is missing");
  }

  return version.trim();
}

function runCommand(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: repoRoot,
      stdio: "inherit",
      shell: false,
    });

    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
        return;
      }

      reject(new Error(`${command} ${args.join(" ")} exited with code ${code ?? "unknown"}`));
    });
  });
}

function buildChangelogTemplate(version) {
  return `# Changelog Draft ${version}

发布日期:

## Highlights

- 

## Fixes

- 

## Runtime And Transport

- 

## Packaging

- Linux artifacts collected under \`artifacts/release/linux/${version}/\`.
- Validate checksums with \`sha256sum -c SHA256SUMS\`.

## Verification

- \`pnpm verify\`
- \`pnpm desktop:build\`
- \`pnpm release:collect\`

## Known Issues

- AppImage is still behind \`pnpm desktop:build:full\` and may depend on local \`linuxdeploy\` behavior.
`;
}

async function writeReleaseTemplate(version) {
  const releaseRoot = path.join(repoRoot, "artifacts", "release", "linux", version);
  await mkdir(releaseRoot, { recursive: true });
  await writeFile(
    path.join(releaseRoot, "CHANGELOG_TEMPLATE.md"),
    `${buildChangelogTemplate(version)}\n`,
    "utf8",
  );
}

const targetVersion = process.argv[2];

if (!targetVersion || !isValidVersion(targetVersion)) {
  throw new Error("usage: node scripts/prepare-release.mjs <version>");
}

const currentVersion = await readCurrentVersion();
if (currentVersion !== targetVersion) {
  await runCommand("pnpm", ["version:set", targetVersion]);
}

await runCommand("pnpm", ["release:linux"]);
await writeReleaseTemplate(targetVersion);

console.log(`Prepared Linux release workspace for ${targetVersion}`);
console.log(`- artifacts/release/linux/${targetVersion}/CHANGELOG_TEMPLATE.md`);
