import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, rm, stat, writeFile, copyFile } from "node:fs/promises";
import path from "node:path";

const repoRoot = process.cwd();
const packageJsonPath = path.join(repoRoot, "package.json");
const bundleRoot = path.join(repoRoot, "src-tauri", "target", "release", "bundle");

async function readPackageVersion() {
  const packageJson = JSON.parse(await readFile(packageJsonPath, "utf8"));
  const version = packageJson.version;

  if (typeof version !== "string" || !version.trim()) {
    throw new Error("package.json version is missing");
  }

  return version.trim();
}

async function listBundleFiles(relativeDir, extension) {
  const directory = path.join(bundleRoot, relativeDir);
  let entries = [];

  try {
    entries = await readdir(directory, { withFileTypes: true });
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && error.code === "ENOENT") {
      return [];
    }

    throw error;
  }

  return entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(extension))
    .map((entry) => path.join(directory, entry.name))
    .sort((left, right) => left.localeCompare(right));
}

async function sha256ForFile(filePath) {
  const contents = await readFile(filePath);
  return createHash("sha256").update(contents).digest("hex");
}

function formatFileSize(sizeBytes) {
  if (sizeBytes < 1024) {
    return `${sizeBytes} B`;
  }

  const sizeKiB = sizeBytes / 1024;
  if (sizeKiB < 1024) {
    return `${sizeKiB.toFixed(1)} KiB`;
  }

  const sizeMiB = sizeKiB / 1024;
  return `${sizeMiB.toFixed(2)} MiB`;
}

function buildReleaseNotes({ version, generatedAt, files }) {
  const artifactLines = files.map((file) => {
    return `- \`${file.name}\` · ${formatFileSize(file.sizeBytes)} · \`${file.sha256}\``;
  });

  const debFile = files.find((file) => file.name.endsWith(".deb"));
  const rpmFile = files.find((file) => file.name.endsWith(".rpm"));
  const installLines = [];

  if (debFile) {
    installLines.push(`- Debian / Ubuntu: \`sudo apt install ./${debFile.name}\``);
  }

  if (rpmFile) {
    installLines.push(`- Fedora / RHEL: \`sudo rpm -i ./${rpmFile.name}\``);
  }

  return `# P2P Chat ${version} Linux Release

生成时间: ${generatedAt}

## 包内容

${artifactLines.join("\n")}

## 校验

\`\`\`bash
sha256sum -c SHA256SUMS
\`\`\`

## 安装

${installLines.join("\n")}

## 文件

- \`manifest.json\`: 机器可读的发布清单。
- \`SHA256SUMS\`: 安装包 SHA-256 校验和。
`;
}

async function buildReleaseManifest() {
  const version = await readPackageVersion();
  const releaseRoot = path.join(repoRoot, "artifacts", "release", "linux", version);
  const files = [
    ...(await listBundleFiles("deb", ".deb")),
    ...(await listBundleFiles("rpm", ".rpm")),
  ];

  if (!files.length) {
    throw new Error(
      "no Linux bundle artifacts found under src-tauri/target/release/bundle; run `pnpm desktop:build` first",
    );
  }

  await rm(releaseRoot, { recursive: true, force: true });
  await mkdir(releaseRoot, { recursive: true });

  const manifestFiles = [];

  for (const sourcePath of files) {
    const fileName = path.basename(sourcePath);
    const targetPath = path.join(releaseRoot, fileName);
    const fileStats = await stat(sourcePath);
    const sha256 = await sha256ForFile(sourcePath);

    await copyFile(sourcePath, targetPath);

    manifestFiles.push({
      name: fileName,
      sizeBytes: fileStats.size,
      sha256,
      sourcePath: path.relative(repoRoot, sourcePath),
      artifactPath: path.relative(repoRoot, targetPath),
    });
  }

  const manifest = {
    platform: "linux",
    version,
    generatedAt: new Date().toISOString(),
    files: manifestFiles,
  };

  const shaSums = manifestFiles
    .map((file) => `${file.sha256}  ${file.name}`)
    .join("\n");

  await writeFile(
    path.join(releaseRoot, "manifest.json"),
    JSON.stringify(manifest, null, 2) + "\n",
    "utf8",
  );
  await writeFile(path.join(releaseRoot, "SHA256SUMS"), `${shaSums}\n`, "utf8");
  await writeFile(
    path.join(releaseRoot, "RELEASE_NOTES.md"),
    `${buildReleaseNotes(manifest)}\n`,
    "utf8",
  );

  return {
    releaseRoot,
    manifestFiles,
  };
}

const { releaseRoot, manifestFiles } = await buildReleaseManifest();

console.log(`Collected ${manifestFiles.length} Linux release artifact(s) into ${path.relative(repoRoot, releaseRoot)}`);
for (const file of manifestFiles) {
  console.log(`- ${file.name} (${file.sizeBytes} bytes)`);
}
