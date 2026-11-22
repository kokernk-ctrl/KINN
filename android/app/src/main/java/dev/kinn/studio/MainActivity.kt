package dev.kinn.studio

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.text.Editable
import android.text.SpannableString
import android.text.Spanned
import android.text.TextWatcher
import android.text.style.ForegroundColorSpan
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.ImageButton
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import dev.kinn.studio.KinSyntax.SyntaxColors
import java.io.File

/**
 * KINN Studio for Android: projects, editor, problems, terminal and the
 * real Kin compiler (when libkinn_android.so is present on this device).
 *
 * Everything is local: projects live in app storage, the compiler runs
 * in-process, the documentation ships in the APK. No permissions.
 */
class MainActivity : AppCompatActivity() {

    private lateinit var store: ProjectStore

    private var project: String? = null
    private var activeFile: String? = null

    // views
    private lateinit var projectName: TextView
    private lateinit var projectsList: RecyclerView
    private lateinit var filesList: RecyclerView
    private lateinit var problemsList: RecyclerView
    private lateinit var problemsHeader: TextView
    private lateinit var editor: EditText
    private lateinit var gutter: TextView
    private lateinit var editorStatus: TextView
    private lateinit var terminalOut: TextView
    private lateinit var terminalScroll: ScrollView
    private lateinit var terminalInput: EditText
    private lateinit var activeFileLabel: TextView

    private val projectsAdapter =
        ProjectsAdapter(emptyList(), null, onOpen = { openProject(it) }, onDelete = { confirmDeleteProject(it) })
    private val filesAdapter =
        FilesAdapter(
            emptyList(), null,
            onOpen = { openFile(it) },
            onDelete = { confirmDeleteFile(it) },
            onRename = { promptRenameFile(it) },
        )
    private val problemsAdapter =
        ProblemsAdapter(emptyList(), onJump = { d ->
            if (d.file.isNotEmpty()) openFile(d.file)
            gotoLine(d.line)
        })

    private val sectionIds = listOf(
        R.id.section_files, R.id.section_editor, R.id.section_problems, R.id.section_terminal,
    )

    // SAF pickers
    private val importPicker =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            if (result.resultCode == Activity.RESULT_OK) {
                result.data?.data?.let { importUri(it) }
            }
        }
    private var pendingExportName: String? = null
    private val exportPicker =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            if (result.resultCode == Activity.RESULT_OK) {
                val uri = result.data?.data
                val name = pendingExportName
                if (uri != null && name != null) exportProjectTo(uri, name)
                pendingExportName = null
            }
        }
    private var pendingAndroidZip: List<BuildFile>? = null
    private val androidZipPicker =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            if (result.resultCode == Activity.RESULT_OK) {
                val uri = result.data?.data
                val files = pendingAndroidZip
                if (uri != null && files != null) exportAndroidZipTo(uri, files)
                pendingAndroidZip = null
            }
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        store = ProjectStore(this)
        bindViews()
        wireTabs()
        wireActions()
        wireTerminal()

        // Restore or bootstrap the first project.
        val saved = getPreferences(MODE_PRIVATE).getString("project", null)
        when {
            saved != null && store.hasProject(saved) -> openProject(saved)
            store.listProjects().isNotEmpty() -> openProject(store.listProjects().first())
            else -> {
                store.createProject("samples")
                openProject("samples")
            }
        }

        termPrint("KINN Studio 0.3.0 — Android shell", "accent")
        if (KinnBridge.available) {
            termPrint("compiler ${KinnBridge.version()} loaded (native, on-device, offline)", "ok")
        } else {
            termPrint("native compiler not available in this build/device — check/run stay disabled", "warn")
        }
        termPrint("type 'help' for commands", "dim")
    }

    // -- view wiring ----------------------------------------------------------

    private fun bindViews() {
        projectName = findViewById(R.id.project_name)
        projectsList = findViewById(R.id.projects_list)
        filesList = findViewById(R.id.files_list)
        problemsList = findViewById(R.id.problems_list)
        problemsHeader = findViewById(R.id.problems_header)
        editor = findViewById(R.id.editor)
        gutter = findViewById(R.id.gutter)
        editorStatus = findViewById(R.id.editor_status)
        terminalOut = findViewById(R.id.terminal_output)
        terminalScroll = findViewById(R.id.terminal_scroll)
        terminalInput = findViewById(R.id.terminal_input)
        activeFileLabel = findViewById(R.id.active_file)

        projectsList.layoutManager = LinearLayoutManager(this)
        projectsList.adapter = projectsAdapter
        filesList.layoutManager = LinearLayoutManager(this)
        filesList.adapter = filesAdapter
        problemsList.layoutManager = LinearLayoutManager(this)
        problemsList.adapter = problemsAdapter
    }

    private fun wireTabs() {
        val tabs = listOf(
            R.id.tab_files to R.id.section_files,
            R.id.tab_editor to R.id.section_editor,
            R.id.tab_problems to R.id.section_problems,
            R.id.tab_terminal to R.id.section_terminal,
        )
        tabs.forEach { (tabId, sectionId) ->
            findViewById<View>(tabId).setOnClickListener {
                sectionIds.forEach { findViewById<View>(it).visibility = View.GONE }
                findViewById<View>(sectionId).visibility = View.VISIBLE
            }
        }
        findViewById<View>(R.id.section_files).visibility = View.VISIBLE
    }

    private fun wireActions() {
        findViewById<View>(R.id.btn_run).setOnClickListener { runActive() }
        findViewById<View>(R.id.btn_build_web)?.setOnClickListener { buildActive("web") }
        findViewById<View>(R.id.btn_check).setOnClickListener { checkActive() }
        findViewById<View>(R.id.btn_docs).setOnClickListener {
            startActivity(Intent(this, DocsActivity::class.java))
        }
        findViewById<View>(R.id.btn_new_project).setOnClickListener { promptNewProject() }
        findViewById<View>(R.id.btn_new_file).setOnClickListener { promptNewFile() }
        findViewById<View>(R.id.btn_import).setOnClickListener { pickImport() }
        findViewById<View>(R.id.btn_export).setOnClickListener { exportActiveProject() }
    }

    private fun wireTerminal() {
        findViewById<ImageButton>(R.id.terminal_send).setOnClickListener { submitTerminal() }
        terminalInput.setOnEditorActionListener { _, actionId, _ ->
            if (actionId == EditorInfo.IME_ACTION_GO) {
                submitTerminal()
                true
            } else false
        }
    }

    private fun submitTerminal() {
        val cmd = terminalInput.text.toString()
        terminalInput.setText("")
        exec(cmd)
    }

    // -- editor -----------------------------------------------------------------

    private val editorWatcher = object : TextWatcher {
        override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) {}
        override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) {}
        override fun afterTextChanged(s: Editable?) {
            val p = project ?: return
            val f = activeFile ?: return
            val text = s?.toString() ?: return
            store.writeFile(p, f, text)
            editorStatus.text = "saved on device"
            updateGutter(text)
            scheduleRehighlight()
        }
    }

    private var highlightScheduled = false

    private fun scheduleRehighlight() {
        if (highlightScheduled) return
        highlightScheduled = true
        editor.postDelayed({
            highlightScheduled = false
            rehighlight()
        }, 250)
    }

    private fun rehighlight() {
        val text = editor.text.toString()
        if (text.length > 60_000) return // keep editing smooth on huge files
        val colors = SyntaxColors(
            keyword = 0xFFFF7A1A.toInt(),
            type = 0xFF79C0FF.toInt(),
            string = 0xFFA5D6FF.toInt(),
            number = 0xFFF2CC60.toInt(),
            comment = 0xFF6E7681.toInt(),
        )
        val spanned = KinSyntax.highlight(text, colors)
        val selStart = editor.selectionStart.coerceIn(0, spanned.length)
        val selEnd = editor.selectionEnd.coerceIn(0, spanned.length)
        editor.removeTextChangedListener(editorWatcher)
        editor.text = spanned
        editor.setSelection(selStart, selEnd)
        editor.addTextChangedListener(editorWatcher)
        updateGutter(text)
    }

    private fun updateGutter(text: String) {
        val count = text.count { it == '\n' } + 1
        val sb = StringBuilder()
        for (i in 1..count) sb.append(i).append('\n')
        gutter.text = sb.toString()
    }

    // -- project / file state ---------------------------------------------------

    private fun openProject(name: String) {
        project = name
        getPreferences(MODE_PRIVATE).edit().putString("project", name).apply()
        projectName.text = name
        refreshFiles()
        val files = store.listFiles(name)
        val target = if (activeFile != null && files.contains(activeFile)) activeFile else files.firstOrNull()
        if (target != null) openFile(target) else clearEditor()
        refreshProjects()
    }

    private fun refreshProjects() {
        projectsAdapter.update(store.listProjects(), project)
    }

    private fun refreshFiles() {
        val p = project ?: return
        filesAdapter.update(store.listFiles(p), activeFile)
    }

    private fun openFile(name: String) {
        val p = project ?: return
        val content = store.readFile(p, name) ?: return
        activeFile = name
        activeFileLabel.text = name
        editor.removeTextChangedListener(editorWatcher)
        editor.setText(content)
        editor.addTextChangedListener(editorWatcher)
        rehighlight()
        refreshFiles()
    }

    private fun clearEditor() {
        activeFile = null
        activeFileLabel.text = "(no file)"
        editor.removeTextChangedListener(editorWatcher)
        editor.setText("")
        updateGutter("")
    }

    private fun gotoLine(line: Int) {
        if (line <= 0) return
        var index = 0
        var current = 1
        val text = editor.text.toString()
        while (current < line && index < text.length) {
            if (text[index] == '\n') current++
            index++
        }
        editor.requestFocus()
        editor.setSelection(index.coerceIn(0, text.length))
        sectionIds.forEach { findViewById<View>(it).visibility = View.GONE }
        findViewById<View>(R.id.section_editor).visibility = View.VISIBLE
    }

    // -- compile + run ---------------------------------------------------------

    /** Entry file name and its source, or null when there is nothing to compile. */
    private fun entrySource(): Pair<String, String>? {
        val p = project ?: return null
        val entry = activeFile ?: store.listFiles(p).firstOrNull() ?: return null
        val src = store.readFile(p, entry) ?: return null
        return entry to src
    }

    /** Project files except the entry, as parallel arrays for the JNI call. */
    private fun moduleArrays(): Pair<Array<String>, Array<String>> {
        val p = project ?: return emptyArray<String>() to emptyArray<String>()
        val entry = activeFile
        val paths = ArrayList<String>()
        val contents = ArrayList<String>()
        for (n in store.listFiles(p)) {
            if (n == entry) continue
            paths.add(n)
            contents.add(store.readFile(p, n) ?: "")
        }
        return paths.toTypedArray() to contents.toTypedArray()
    }

    private fun checkActive() {
        val (entry, src) = entrySource() ?: run {
            toast("nothing to check")
            return
        }
        if (!KinnBridge.available) {
            termPrint("native compiler unavailable — full check needs libkinn_android.so (ARM64 build)", "warn")
            return
        }
        val modules = moduleArrays()
        val json = KinnBridge.nCheck(entry, src, modules.first, modules.second)
        val result = CompilerResults.parseCheck(json)
        showProblems(result.diagnostics)
        if (result.ok) {
            termPrint("OK: no errors in $entry (mode ${result.mode})", "ok")
        } else {
            result.diagnostics.filter { it.severity == "error" }.forEach {
                termPrint("${it.code}: ${it.message}  [${it.file}:${it.line}:${it.col}]", "err")
            }
        }
    }

    private fun runActive() {
        val (entry, src) = entrySource() ?: run {
            toast("nothing to run")
            return
        }
        if (!KinnBridge.available) {
            termPrint("running programs needs the native compiler (libkinn_android.so)", "warn")
            termPrint("official ARM64 builds ship it; this device/build does not have it", "dim")
            return
        }
        val modules = moduleArrays()
        val json = KinnBridge.nRun(entry, src, modules.first, modules.second)
        val result = CompilerResults.parseRun(json)
        showProblems(result.diagnostics)
        if (result.stdout.isNotEmpty()) {
            result.stdout.trimEnd('\n').split('\n').forEach { termPrint(it) }
        }
        when {
            result.ok -> termPrint("— exited 0 —", "ok")
            result.runtimeError != null -> termPrint("error: ${result.runtimeError}  [$entry]", "err")
            else -> termPrint("— compilation failed —", "err")
        }
    }

    private fun showProblems(diags: List<Diagnostic>) {
        problemsAdapter.update(diags)
        problemsHeader.text = "PROBLEMS ${diags.size}"
    }

    // -- build targets (0.3) -----------------------------------------------------

    /**
     * `build web` / `build android`: run the same backends as the CLI
     * through the native bridge and write the generated files into
     * app-private storage (`files/build/<target>/`).
     */
    private fun buildActive(target: String) {
        val (entry, src) = entrySource() ?: run {
            toast("nothing to build")
            return
        }
        if (!KinnBridge.available) {
            termPrint("building needs the native compiler (libkinn_android.so, ARM64 builds ship it)", "warn")
            return
        }
        if (target != "web" && target != "android") {
            termPrint("unknown target '$target' — supported: web, android", "err")
            return
        }
        termPrint("building $entry for $target (kinn ${KinnBridge.version()}, native)...", "dim")
        val modules = moduleArrays()
        val json = if (target == "web") {
            KinnBridge.nBuildWeb(entry, src, modules.first, modules.second)
        } else {
            KinnBridge.nBuildAndroid(entry, src, modules.first, modules.second)
        }
        val result = CompilerResults.parseBuild(json)
        showProblems(result.diagnostics)
        if (!result.ok) {
            result.diagnostics.filter { it.severity == "error" }.forEach {
                termPrint("${it.code}: ${it.message}  [${it.file}:${it.line}:${it.col}]", "err")
            }
            termPrint("— build failed —", "err")
            return
        }
        val root = File(filesDir, "build/$target")
        root.deleteRecursively()
        val written = BuildSupport.writeFiles(root, result.files)
        termPrint("built $target site \"${result.site}\" (${result.pages} page(s), ${result.files.size} files) → build/$target", "ok")
        written.forEach { termPrint("  ${it.relativeTo(root).path}") }
        if (target == "web") {
            termPrint("type 'preview' to open the site on this device", "dim")
        } else {
            termPrint("this is an exported Gradle project, not an APK — build it in Android Studio,", "dim")
            termPrint("with gradle assembleDebug, or through the remote CI workflow", "dim")
            termPrint("type 'export android' to share the project as a .zip", "dim")
        }
    }

    private fun previewBuild() {
        val index = File(filesDir, "build/web/index.html")
        if (!index.exists()) {
            termPrint("no web build yet — run 'build web' first", "warn")
            return
        }
        startActivity(
            Intent(this, PreviewActivity::class.java)
                .putExtra(PreviewActivity.EXTRA_ROOT, index.parentFile!!.absolutePath),
        )
    }

    /** Export the android Gradle project as a .zip through SAF. */
    private fun exportAndroidProject() {
        val (entry, src) = entrySource() ?: return toast("nothing to export")
        if (!KinnBridge.available) {
            termPrint("exporting needs the native compiler (libkinn_android.so)", "warn")
            return
        }
        val modules = moduleArrays()
        val json = KinnBridge.nBuildAndroid(entry, src, modules.first, modules.second)
        val result = CompilerResults.parseBuild(json)
        if (!result.ok) {
            termPrint("android export failed (diagnostics in PROBLEMS)", "err")
            return
        }
        pendingAndroidZip = result.files
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "application/zip"
            putExtra(Intent.EXTRA_TITLE, "${result.site.lowercase().ifEmpty { "kinn-app" }}-android-project.zip")
        }
        androidZipPicker.launch(intent)
    }

    private fun exportAndroidZipTo(uri: Uri, files: List<BuildFile>) {
        try {
            contentResolver.openOutputStream(uri)?.use { out ->
                BuildSupport.zipFiles(files, out)
            }
            termPrint("exported Android Gradle project (${files.size} files) as .zip", "ok")
            termPrint("an APK comes only from building this project (Studio/Gradle/CI)", "dim")
        } catch (e: Exception) {
            toast("export failed: ${e.message}")
        }
    }

    // -- terminal ---------------------------------------------------------------

    /** Append a colored line to the terminal scrollback. */
    private fun termPrint(text: String, cls: String = "out") {
        val color = when (cls) {
            "accent" -> 0xFFFF7A1A.toInt()
            "ok" -> 0xFF3FB950.toInt()
            "warn" -> 0xFFD29922.toInt()
            "err" -> 0xFFF85149.toInt()
            "dim" -> 0xFF8B949E.toInt()
            "cmd" -> 0xFFE6EDF3.toInt()
            else -> 0xFFE6EDF3.toInt()
        }
        val span = SpannableString(text + "\n")
        span.setSpan(
            ForegroundColorSpan(color),
            0,
            span.length,
            Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        terminalOut.append(span)
        terminalScroll.post { terminalScroll.fullScroll(ScrollView.FOCUS_DOWN) }
    }

    private fun exec(raw: String) {
        val cmd = raw.trim()
        termPrint("kinn> $cmd")
        if (cmd.isEmpty()) return
        val parts = cmd.split(Regex("\\s+"))
        when (parts[0]) {
            "help" -> help()
            "version" -> version()
            "about" -> about()
            "clear" -> terminalOut.text = ""
            "projects" -> store.listProjects().forEach {
                termPrint(if (it == project) "$it  (active)" else it)
            }
            "project" -> {
                val name = parts.getOrNull(1)
                if (name == null || !store.hasProject(name)) {
                    termPrint("usage: project <name> (see 'projects')")
                } else openProject(name)
            }
            "ls" -> {
                val p = project ?: return termPrint("no project open")
                store.listFiles(p).forEach {
                    termPrint(if (it == activeFile) "$it  (active)" else it)
                }
            }
            "cat" -> {
                val p = project ?: return termPrint("no project open")
                val name = parts.getOrNull(1) ?: activeFile ?: return termPrint("usage: cat <file>")
                val content = store.readFile(p, name) ?: return termPrint("no such file: $name")
                content.lines().forEachIndexed { i, l -> termPrint("${(i + 1).toString().padStart(3)} | $l") }
            }
            "open" -> {
                val name = parts.getOrNull(1) ?: return termPrint("usage: open <file>")
                openFile(name)
            }
            "new" -> {
                val name = parts.getOrNull(1) ?: return termPrint("usage: new <name.kin>")
                val p = project ?: return termPrint("no project open")
                if (store.writeFile(p, name, template(name))) {
                    openFile(name)
                    termPrint("created $name")
                } else termPrint("invalid name '$name' (must be name.kin)")
            }
            "rm" -> {
                val name = parts.getOrNull(1) ?: return termPrint("usage: rm <file>")
                val p = project ?: return termPrint("no project open")
                if (store.deleteFile(p, name)) {
                    if (activeFile == name) clearEditor()
                    refreshFiles()
                    termPrint("deleted $name")
                } else termPrint("no such file: $name")
            }
            "mv" -> {
                if (parts.size < 3) return termPrint("usage: mv <old.kin> <new.kin>")
                val p = project ?: return termPrint("no project open")
                if (store.renameFile(p, parts[1], parts[2])) {
                    if (activeFile == parts[1]) openFile(parts[2])
                    refreshFiles()
                    termPrint("renamed ${parts[1]} -> ${parts[2]}")
                } else termPrint("cannot rename (missing, invalid or exists)")
            }
            "check" -> checkActive()
            "run" -> runActive()
            "build" -> buildActive(parts.getOrNull(1) ?: "web")
            "preview" -> previewBuild()
            "syntax" -> syntax()
            "export" -> {
                if (parts.getOrNull(1) == "android") exportAndroidProject() else exportActiveProject()
            }
            "import" -> pickImport()
            "docs" -> startActivity(Intent(this, DocsActivity::class.java))
            else -> termPrint("unknown command '${parts[0]}' — type 'help'")
        }
    }

    private fun help() {
        listOf(
            "commands:",
            "  help            this list",
            "  version         shell and compiler versions",
            "  about           what KINN is",
            "  clear           clear the scrollback",
            "  projects        list projects on this device",
            "  project <name>  open a project",
            "  ls              list files in the project",
            "  cat <file>      print a file",
            "  open <file>     open a file in the editor",
            "  new <file>      create a .kin file",
            "  rm <file>       delete a file",
            "  mv <old> <new>  rename a file",
            "  check           full compiler check (native)",
            "  run             compile and execute (native)",
            "  build web       build the website (native, same backend as the CLI)",
            "  build android   export the Android Gradle project",
            "  preview         open the last web build in the offline preview",
            "  syntax          Kin cheat sheet",
            "  export          export the project (.kinnproj)",
            "  export android  export the Android project (.zip)",
            "  import          import .kinnproj or .kin",
            "  docs            bundled documentation",
        ).forEach { termPrint(it) }
    }

    private fun version() {
        val compiler = KinnBridge.version()?.let { "compiler kinn $it (native, on-device)" }
            ?: "native compiler NOT AVAILABLE"
        termPrint("KINN Studio 0.3.0 (Android) — $compiler")
    }

    private fun about() {
        termPrint("KINN is an offline-first programming language and environment.")
        termPrint("This app stores projects locally, compiles and runs Kin")
        termPrint("on-device when the native backend is present, and bundles")
        termPrint("the documentation. No accounts, no network, no tracking.")
    }

    private fun syntax() {
        listOf(
            "Kin 0.3 cheat sheet",
            "-------------------",
            "*mode = B;                 // B beginner, S standard, A advanced",
            "-- website module: build web / preview --",
            "website \"My Site\" {",
            "  page Home {",
            "    title \"Welcome\"        // document title + page heading",
            "    heading \"Section\"      // section heading",
            "    text \"A paragraph\"",
            "    button \"Go\" open About // prominent navigation",
            "    link \"More\" open About // inline navigation",
            "    image \"assets/p.png\" alt \"P\"  // alt required",
            "    divider",
            "  }",
            "  page About { text \"About page\" }",
            "}",
            "func main() { }",
            "import \"math.kin\" as math; // modules: math.add(1, 2)",
            "fin x = 1; var y = 2; y += 1;",
            "if x > 0 { } else { }  while x < 3 { }",
            "for i in 0..10 { }  for i in 0..=9 { }  for item in arr { }",
            "fin a = [1, 2, 3]; a[0] = 5; print(len(a));",
            "fin f = 2 as float;  fin s = 5 as string;",
        ).forEach { termPrint(it) }
    }

    private fun template(name: String): String =
        "// $name\n\n*mode = B;\n\nfunc main() {\n    print(\"new file\");\n}\n"

    // -- prompts -----------------------------------------------------------------

    private fun promptNewProject() {
        val input = EditText(this).apply { hint = getString(R.string.new_project_hint) }
        AlertDialog.Builder(this)
            .setTitle("new project")
            .setView(input)
            .setPositiveButton("create") { _, _ ->
                val raw = input.text.toString()
                val clean = ProjectStore.sanitizeProjectName(raw) ?: run {
                    toast("invalid project name")
                    return@setPositiveButton
                }
                if (store.createProject(clean)) {
                    openProject(clean)
                    termPrint("created project $clean")
                } else {
                    toast("project already exists")
                }
            }
            .setNegativeButton("cancel", null)
            .show()
    }

    private fun promptNewFile() {
        val input = EditText(this).apply { hint = getString(R.string.new_file_hint) }
        AlertDialog.Builder(this)
            .setTitle("new file")
            .setView(input)
            .setPositiveButton("create") { _, _ ->
                val p = project ?: return@setPositiveButton
                val name = input.text.toString().trim()
                if (store.writeFile(p, name, template(name))) {
                    openFile(name)
                    termPrint("created $name")
                } else {
                    toast("invalid name (must be name.kin)")
                }
            }
            .setNegativeButton("cancel", null)
            .show()
    }

    private fun promptRenameFile(old: String) {
        val input = EditText(this).apply { setText(old) }
        AlertDialog.Builder(this)
            .setTitle("rename $old")
            .setView(input)
            .setPositiveButton("rename") { _, _ ->
                val p = project ?: return@setPositiveButton
                val name = input.text.toString().trim()
                if (store.renameFile(p, old, name)) {
                    openFile(name)
                    termPrint("renamed $old -> $name")
                } else toast("cannot rename")
            }
            .setNegativeButton("cancel", null)
            .show()
    }

    private fun confirmDeleteFile(name: String) {
        val p = project ?: return
        AlertDialog.Builder(this)
            .setTitle("delete $name?")
            .setMessage("This cannot be undone.")
            .setPositiveButton("delete") { _, _ ->
                if (store.deleteFile(p, name)) {
                    if (activeFile == name) clearEditor()
                    refreshFiles()
                    termPrint("deleted $name")
                }
            }
            .setNegativeButton("cancel", null)
            .show()
    }

    private fun confirmDeleteProject(name: String) {
        AlertDialog.Builder(this)
            .setTitle("delete project '$name'?")
            .setMessage("All its files are deleted. This cannot be undone.")
            .setPositiveButton("delete") { _, _ ->
                if (store.deleteProject(name)) {
                    if (project == name) {
                        val next = store.listProjects().firstOrNull()
                        if (next != null) openProject(next) else {
                            project = null
                            clearEditor()
                            refreshFiles()
                            refreshProjects()
                        }
                    }
                    refreshProjects()
                    termPrint("deleted project $name")
                }
            }
            .setNegativeButton("cancel", null)
            .show()
    }

    // -- import / export -----------------------------------------------------------

    private fun pickImport() {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(Intent.EXTRA_MIME_TYPES, arrayOf("application/json", "text/*", "application/octet-stream"))
        }
        importPicker.launch(intent)
    }

    private fun importUri(uri: Uri) {
        try {
            val name = queryDisplayName(uri) ?: "imported.kin"
            val content = contentResolver.openInputStream(uri)?.use { stream ->
                stream.bufferedReader().readText()
            } ?: return toast("cannot read file")

            if (name.endsWith(".kin")) {
                val p = project ?: return toast("no project open")
                if (store.importFile(p, name, content)) {
                    openFile(name)
                    termPrint("imported $name")
                } else toast("invalid file name")
            } else {
                val imported = store.importProject(content)
                    ?: return toast("not a valid .kinnproj file")
                val created = store.createFromImport(imported)
                openProject(created)
                if (imported.active.isNotEmpty()) openFile(imported.active)
                termPrint("imported project '$created' (${imported.files.size} files)")
            }
        } catch (e: Exception) {
            toast("import failed: ${e.message}")
        }
    }

    private fun queryDisplayName(uri: Uri): String? {
        var name: String? = null
        contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) {
                val idx = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                if (idx >= 0) name = cursor.getString(idx)
            }
        }
        return name
    }

    private fun exportActiveProject() {
        val p = project ?: return toast("no project open")
        if (store.listFiles(p).isEmpty()) return toast("project is empty")
        pendingExportName = p
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "application/json"
            putExtra(Intent.EXTRA_TITLE, "$p.kinnproj")
        }
        exportPicker.launch(intent)
    }

    private fun exportProjectTo(uri: Uri, projectName: String) {
        try {
            val json = store.exportProject(projectName, activeFile ?: "")
                ?: return toast("project is empty")
            contentResolver.openOutputStream(uri)?.use { out ->
                out.write(json.toByteArray())
            }
            termPrint("exported project '$projectName'")
        } catch (e: Exception) {
            toast("export failed: ${e.message}")
        }
    }

    override fun onPause() {
        super.onPause()
        project?.let { getPreferences(MODE_PRIVATE).edit().putString("project", it).apply() }
    }

    private fun toast(message: String) {
        Toast.makeText(this, message, Toast.LENGTH_SHORT).show()
    }
}
