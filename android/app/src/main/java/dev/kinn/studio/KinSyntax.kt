package dev.kinn.studio

import android.text.Editable
import android.text.Spannable
import android.text.Spanned
import android.text.style.CharacterStyle
import android.text.style.UpdateAppearance

/**
 * Kin syntax highlighting for the Android editor — 0.4.1 fix.
 *
 * Highlights are applied IN PLACE to the live [Editable] of the editor
 * field via [Editable.setSpan] ([applyHighlight]), never by building a
 * new Spannable and replacing the field content: replacing the Editable
 * with setText() would reset the caret, break the selection and kill
 * the IME composition mid-word — the cause of the 0.3 "backspace
 * deletes the wrong character / visible text differs from compiled
 * text" symptoms.
 *
 * Spans are instances of our own [KinSpan] class, so removal only ever
 * touches OUR spans — composition/suggestion spans installed by the
 * IME are untouched.
 */

object KinSyntax {

    /** A colored run of code — our marker class for cleanup. */
    class KinSpan(val color: Int, val bold: Boolean = false) : CharacterStyle(), UpdateAppearance {
        override fun updateDrawState(ds: android.text.TextPaint) {
            ds.color = color
            if (bold) ds.typeface = android.graphics.Typeface.create(ds.typeface, android.graphics.Typeface.BOLD)
        }
    }

    private val KEYWORDS = setOf(
        "func", "fin", "var", "if", "else", "while", "for", "in",
        "return", "break", "continue", "and", "or", "not", "as", "import",
        "true", "false",
    )
    private val TYPES = setOf("int", "float", "bool", "string", "void")

    /**
     * Remove all of OUR spans, then add fresh ones in a single pass.
     * The text, the selection and the composition are never modified.
     */
    fun applyHighlight(editable: Editable, colors: SyntaxColors) {
        val length = editable.length
        // remove only KinSpans — never the IME's or the user's spans
        for (s in editable.getSpans(0, length, KinSpan::class.java)) {
            editable.removeSpan(s)
        }

        val text = editable.toString()
        var i = 0
        val n = text.length

        fun span(start: Int, end: Int, color: Int, bold: Boolean = false) {
            if (end <= start) return
            editable.setSpan(KinSpan(color, bold), start, end, Spannable.SPAN_EXCLUSIVE_EXCLUSIVE)
        }

        while (i < n) {
            val c = text[i]
            when {
                c == '/' && i + 1 < n && text[i + 1] == '/' -> {
                    val start = i
                    while (i < n && text[i] != '\n') i++
                    span(start, i, colors.comment)
                }
                c == '/' && i + 1 < n && text[i + 1] == '*' -> {
                    val start = i
                    i += 2
                    while (i + 1 < n && !(text[i] == '*' && text[i + 1] == '/')) i++
                    i = (i + 2).coerceAtMost(n)
                    span(start, i, colors.comment)
                }
                c == '"' -> {
                    val start = i
                    i++
                    while (i < n && text[i] != '"' && text[i] != '\n') {
                        if (text[i] == '\\' && i + 1 < n) i++
                        i++
                    }
                    if (i < n && text[i] == '"') i++
                    span(start, i, colors.string)
                }
                c.isDigit() -> {
                    val start = i
                    while (i < n && (text[i].isDigit() || text[i] == '.' && i + 1 < n && text[i + 1].isDigit())) i++
                    span(start, i, colors.number)
                }
                c.isLetter() || c == '_' -> {
                    val start = i
                    while (i < n && (text[i].isLetterOrDigit() || text[i] == '_')) i++
                    val word = text.substring(start, i)
                    when {
                        KEYWORDS.contains(word) -> span(start, i, colors.keyword, bold = true)
                        TYPES.contains(word) -> span(start, i, colors.type)
                    }
                }
                else -> i++
            }
        }
    }

    data class SyntaxColors(
        val keyword: Int,
        val type: Int,
        val string: Int,
        val number: Int,
        val comment: Int,
    )
}
