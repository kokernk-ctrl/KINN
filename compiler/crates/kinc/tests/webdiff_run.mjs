// webdiff_run.mjs — differential harness driver (0.4.0 S5).
// Usage: node webdiff_run.mjs <kinnrt.js> <module.cjs>
// Executes the generated module (which auto-runs `main`) with the same
// observable behavior as `kinn run`: stdout lines, and KinRTError → the
// `[CODE] message (line N)` line on stderr with exit code 3.
// Paths are converted to file URLs so absolute Windows paths (drive
// letters + backslashes) import the same way as POSIX ones.
import { pathToFileURL } from "node:url";

const [, , kinnrtPath, modulePath] = process.argv;
try {
  await import(pathToFileURL(kinnrtPath).href);
  await import(pathToFileURL(modulePath).href);
  process.exit(0);
} catch (e) {
  if (e && e.kin) {
    process.stderr.write(`[${e.code}] ${e.message} (line ${e.line})\n`);
    process.exit(3);
  }
  console.error(e && e.stack ? e.stack : String(e));
  process.exit(4);
}
