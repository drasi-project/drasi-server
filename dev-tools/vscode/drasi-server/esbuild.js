const { build } = require("esbuild");
const { glob } = require('glob');

//@ts-check
/** @typedef {import('esbuild').BuildOptions} BuildOptions **/

/** @type BuildOptions */
const baseConfig = {
  bundle: true,
  minify: process.env.NODE_ENV === "production",
  sourcemap: process.env.NODE_ENV !== "production",
};

/** @type BuildOptions */
const extensionConfig = {
  ...baseConfig,
  platform: "node",
  mainFields: ["module", "main"],
  format: "cjs",
  entryPoints: ["./src/extension.ts"],
  outfile: "./out/extension.js",
  external: ["vscode"],
};

/** @type BuildOptions */
const webviewConfig = {
  ...baseConfig,
  target: "es2020",
  format: "esm",
  entryPoints: ["./src/webview/main.ts"],
  outfile: "./out/webview.js",
};

/** @type BuildOptions */
const testConfig = {
  ...baseConfig,
  target: "es2022",
  format: "cjs",
  platform: "node",
  entryPoints: glob.sync('./src/test/**/*.ts'),
  outdir: "./out/test",
  external: ["vscode"],
};

/** @type BuildOptions */
const watchConfig = {
  watch: {
    onRebuild(error) {
      console.log("[watch] build started");
      if (error) {
        error.errors.forEach((error) =>
          console.error(
            `> ${error.location?.file}:${error.location?.line}:${error.location?.column}: error: ${error.text}`
          )
        );
      } else {
        console.log("[watch] build finished");
      }
    },
  },
};

(async () => {
  const args = process.argv.slice(2);
  try {
    if (args.includes("--watch")) {
      console.log("[watch] build started");
      await build({
        ...extensionConfig,
        ...watchConfig,
      });
      await build({
        ...webviewConfig,
        ...watchConfig,
      });
      console.log("[watch] build finished");
    } else {
      await build(extensionConfig);
      await build(webviewConfig);
      await build(testConfig);
      console.log("build complete");
    }
  } catch (err) {
    process.stderr.write(err.stderr);
    process.exit(1);
  }
})();
