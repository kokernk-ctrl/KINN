package dev.kinn.studio

import org.json.JSONArray
import org.json.JSONObject

/**
 * Results from the native compiler bridge — the same JSON the CLI and
 * the web Studio produce.
 */
data class Diagnostic(
    val severity: String,
    val code: String,
    val message: String,
    val file: String,
    val line: Int,
    val col: Int,
)

data class CheckResult(
    val ok: Boolean,
    val mode: String,
    val diagnostics: List<Diagnostic>,
)

data class RunResult(
    val ok: Boolean,
    val exitCode: Int,
    val stdout: String,
    val runtimeError: String?,
    val diagnostics: List<Diagnostic>,
)

/** One generated artifact (path relative to the build dir + UTF-8 content). */
data class BuildFile(val path: String, val content: String)

/** Result of nBuildWeb / nBuildAndroid — a real project export, never an APK. */
data class BuildResult(
    val ok: Boolean,
    val site: String,
    val pages: Int,
    val files: List<BuildFile>,
    val diagnostics: List<Diagnostic>,
)

object CompilerResults {
    fun parseCheck(json: String): CheckResult {
        return try {
            val obj = JSONObject(json)
            CheckResult(
                ok = obj.optBoolean("ok", false),
                mode = obj.optString("mode", "S"),
                diagnostics = parseDiags(obj.optJSONArray("diagnostics")),
            )
        } catch (e: Exception) {
            CheckResult(false, "S", listOf(Diagnostic("error", "E5003", "bad result from compiler: ${e.message}", "", 0, 0)))
        }
    }

    fun parseRun(json: String): RunResult {
        return try {
            val obj = JSONObject(json)
            val rt = obj.optJSONObject("runtimeError")
            RunResult(
                ok = obj.optBoolean("ok", false),
                exitCode = obj.optInt("exitCode", 1),
                stdout = obj.optString("stdout", ""),
                runtimeError = if (rt != null) {
                    "[${rt.optString("code")}] ${rt.optString("message")} (line ${rt.optInt("line")})"
                } else null,
                diagnostics = parseDiags(obj.optJSONArray("diagnostics")),
            )
        } catch (e: Exception) {
            RunResult(false, 1, "", null, listOf(Diagnostic("error", "E5003", "bad result from compiler: ${e.message}", "", 0, 0)))
        }
    }

    fun parseBuild(json: String): BuildResult {
        return try {
            val obj = JSONObject(json)
            val files = ArrayList<BuildFile>()
            val arr = obj.optJSONArray("files")
            if (arr != null) {
                for (i in 0 until arr.length()) {
                    val f = arr.optJSONObject(i) ?: continue
                    files.add(BuildFile(f.optString("path"), f.optString("content")))
                }
            }
            BuildResult(
                ok = obj.optBoolean("ok", false),
                site = obj.optString("site", ""),
                pages = obj.optInt("pages", 0),
                files = files,
                diagnostics = parseDiags(obj.optJSONArray("diagnostics")),
            )
        } catch (e: Exception) {
            BuildResult(false, "", 0, emptyList(), listOf(Diagnostic("error", "E5003", "bad result from compiler: ${e.message}", "", 0, 0)))
        }
    }

    private fun parseDiags(arr: JSONArray?): List<Diagnostic> {
        if (arr == null) return emptyList()
        val out = ArrayList<Diagnostic>(arr.length())
        for (i in 0 until arr.length()) {
            val d = arr.optJSONObject(i) ?: continue
            out.add(
                Diagnostic(
                    severity = d.optString("severity", "error"),
                    code = d.optString("code", ""),
                    message = d.optString("message", ""),
                    file = d.optString("file", ""),
                    line = d.optInt("line", 0),
                    col = d.optInt("col", 0),
                )
            )
        }
        return out
    }
}
