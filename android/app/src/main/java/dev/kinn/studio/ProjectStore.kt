package dev.kinn.studio

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/**
 * Project storage on the device: every project is a directory under
 * `filesDir/projects` containing `.kin` files. Fully local — no
 * permissions needed, no network, survives app restarts.
 *
 * The `.kinnproj` JSON format is identical to the web Studio's export
 * format, so projects move between Studio on the web and Android.
 */
class ProjectStore(context: Context) {
    private val root: File = File(context.filesDir, "projects").apply { mkdirs() }

    fun listProjects(): List<String> =
        root.listFiles()?.filter { it.isDirectory }?.map { it.name }?.sorted() ?: emptyList()

    fun createProject(name: String): Boolean {
        val clean = sanitizeProjectName(name) ?: return false
        val dir = File(root, clean)
        if (dir.exists()) return false
        dir.mkdirs()
        // Every project starts with a main.kin so the Run button always
        // has something honest to run.
        File(dir, "main.kin").writeText(
            "// ${clean} — a Kin project\n\n*mode = B;\n\nfunc main() {\n    print(\"hello from KINN\");\n}\n"
        )
        return true
    }

    fun deleteProject(name: String): Boolean {
        val dir = File(root, name)
        if (!dir.isDirectory) return false
        return dir.deleteRecursively()
    }

    fun hasProject(name: String): Boolean = File(root, name).isDirectory

    fun listFiles(project: String): List<String> =
        File(root, project).listFiles()
            ?.filter { it.isFile && it.name.endsWith(".kin") }
            ?.map { it.name }
            ?.sorted()
            ?: emptyList()

    fun readFile(project: String, name: String): String? {
        if (!isValidFileName(name)) return null
        val f = File(File(root, project), name)
        return if (f.isFile) f.readText() else null
    }

    fun writeFile(project: String, name: String, content: String): Boolean {
        if (!isValidFileName(name)) return false
        val dir = File(root, project)
        if (!dir.isDirectory) return false
        File(dir, name).writeText(content)
        return true
    }

    fun deleteFile(project: String, name: String): Boolean {
        if (!isValidFileName(name)) return false
        return File(File(root, project), name).delete()
    }

    fun renameFile(project: String, oldName: String, newName: String): Boolean {
        if (!isValidFileName(oldName) || !isValidFileName(newName)) return false
        val dir = File(root, project)
        val from = File(dir, oldName)
        val to = File(dir, newName)
        if (!from.isFile || to.exists()) return false
        return from.renameTo(to)
    }

    /** Add or replace a file (import). */
    fun importFile(project: String, name: String, content: String): Boolean {
        if (!isValidFileName(name)) return false
        val dir = File(root, project)
        if (!dir.isDirectory) return false
        File(dir, name).writeText(content)
        return true
    }

    /** Serialize the project in the .kinnproj JSON format. */
    fun exportProject(project: String, active: String): String? {
        val files = listFiles(project)
        if (files.isEmpty()) return null
        val arr = JSONArray()
        for (name in files) {
            arr.put(JSONObject().put("name", name).put("content", readFile(project, name) ?: ""))
        }
        return JSONObject()
            .put("format", "kinnproj")
            .put("version", 1)
            .put("name", project)
            .put("active", if (files.contains(active)) active else files[0])
            .put("files", arr)
            .toString(2)
    }

    data class ImportedProject(val name: String, val active: String, val files: List<Pair<String, String>>)

    /** Parse a .kinnproj JSON payload (same validation as the web Studio). */
    fun importProject(json: String): ImportedProject? {
        return try {
            val obj = JSONObject(json)
            if (obj.optString("format") != "kinnproj") return null
            if (obj.optInt("version", 99) > 1) return null
            val name = sanitizeProjectName(obj.optString("name")) ?: return null
            val arr = obj.optJSONArray("files") ?: return null
            if (arr.length() == 0) return null
            val files = ArrayList<Pair<String, String>>(arr.length())
            for (i in 0 until arr.length()) {
                val f = arr.optJSONObject(i) ?: return null
                val fname = f.optString("name")
                if (!isValidFileName(fname)) return null
                files.add(fname to f.optString("content"))
            }
            val wantedActive = obj.optString("active")
            val active = files.firstOrNull { it.first == wantedActive }?.first ?: files[0].first
            ImportedProject(name, active, files)
        } catch (_: Exception) {
            null
        }
    }

    /** Create a project from imported data (name collision -> suffix). */
    fun createFromImport(p: ImportedProject): String {
        var name = p.name
        var n = 1
        while (hasProject(name)) {
            name = "${p.name}-${++n}"
        }
        val dir = File(root, name)
        dir.mkdirs()
        for ((fname, content) in p.files) {
            File(dir, fname).writeText(content)
        }
        return name
    }

    companion object {
        fun isValidFileName(name: String): Boolean =
            Regex("^[\\w.-]+\\.kin$").matches(name)

        fun sanitizeProjectName(name: String): String? {
            val clean = name.trim().replace(Regex("[^\\w-]"), "-").trim('-')
            return if (clean.isEmpty() || clean.length > 40) null else clean
        }
    }
}
