const https = require("https");
const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

const { version } = require("./package.json");

const REPO = "circlesac/oneup";

const PLATFORMS = {
  "darwin-x64": { artifact: "oneup-x86_64-apple-darwin", ext: ".tar.gz" },
  "darwin-arm64": { artifact: "oneup-aarch64-apple-darwin", ext: ".tar.gz" },
  "linux-x64": { artifact: "oneup-x86_64-unknown-linux-gnu", ext: ".tar.gz" },
  "linux-arm64": { artifact: "oneup-aarch64-unknown-linux-gnu", ext: ".tar.gz" },
  "win32-x64": { artifact: "oneup-x86_64-pc-windows-msvc", ext: ".zip" },
};

async function download(url) {
  return new Promise((resolve, reject) => {
    https.get(url, (res) => {
      if (res.statusCode === 302 || res.statusCode === 301) {
        download(res.headers.location).then(resolve).catch(reject);
        return;
      }
      if (res.statusCode !== 200) {
        reject(new Error(`HTTP ${res.statusCode}`));
        return;
      }
      const chunks = [];
      res.on("data", (chunk) => chunks.push(chunk));
      res.on("end", () => resolve(Buffer.concat(chunks)));
      res.on("error", reject);
    });
  });
}

async function main() {
  const platform = `${process.platform}-${process.arch}`;
  const info = PLATFORMS[platform];

  if (!info) {
    console.error(`Unsupported platform: ${platform}`);
    console.error(`Supported: ${Object.keys(PLATFORMS).join(", ")}`);
    process.exit(1);
  }

  const { artifact, ext } = info;
  const url = `https://github.com/${REPO}/releases/download/v${version}/${artifact}${ext}`;
  console.log(`Downloading ${artifact}...`);

  try {
    const data = await download(url);
    const nativeDir = path.join(__dirname, "bin", "native");

    if (!fs.existsSync(nativeDir)) {
      fs.mkdirSync(nativeDir, { recursive: true });
    }

    const tmpFile = path.join(nativeDir, `tmp${ext}`);
    fs.writeFileSync(tmpFile, data);

    if (ext === ".zip") {
      // Windows: use PowerShell to extract zip
      execSync(
        `powershell -Command "Expand-Archive -Force '${tmpFile}' '${nativeDir}'"`,
        { cwd: nativeDir }
      );
    } else {
      // Unix: use tar
      execSync(`tar xzf "${tmpFile}"`, { cwd: nativeDir });
    }
    fs.unlinkSync(tmpFile);

    // Make executable (Unix only)
    if (process.platform !== "win32") {
      const binPath = path.join(nativeDir, "oneup");
      fs.chmodSync(binPath, 0o755);
    }

    console.log(`Installed oneup v${version}`);
  } catch (err) {
    console.error(`Failed to install: ${err.message}`);
    process.exit(1);
  }
}

module.exports = main();
