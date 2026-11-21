package dev.kinn.studio

import java.io.File
import java.io.OutputStream
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/**
 * Support for the 0.3 build targets: writing generated files into
 * app-private storage and zipping a build result for SAF export.
 *
 * Status vocabulary (honesty rule): the web build is a real dist; the
 * android build is an exported Gradle project — an APK comes only from
 * building that project (Android Studio, Gradle, or remote CI).
 */
object BuildSupport {

    /** Write generated files under `<root>/`, creating directories. */
    fun writeFiles(root: File, files: List<BuildFile>): List<File> {
        val written = ArrayList<File>(files.size)
        for (f in files) {
            val out = File(root, f.path)
            out.parentFile?.mkdirs()
            out.writeText(f.content)
            written.add(out)
        }
        return written
    }

    /** Zip a build result into `out` (deflate; contents are text). */
    fun zipFiles(files: List<BuildFile>, out: OutputStream) {
        ZipOutputStream(out).use { zip ->
            for (f in files) {
                zip.putNextEntry(ZipEntry(f.path))
                zip.write(f.content.toByteArray(Charsets.UTF_8))
                zip.closeEntry()
            }
        }
    }
}
