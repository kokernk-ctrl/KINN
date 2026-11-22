package dev.kinn.studio

import android.graphics.Typeface
import android.text.Editable
import android.text.Spannable
import android.text.SpannableStringBuilder
import android.text.style.ForegroundColorSpan
import android.text.style.StyleSpan

/**
 * Minimal Kin syntax highlighting for the Android editor. A small
 * tokenizer (same token set as the reference lexers) drives spannable
 * colors; it is intentionally simple — full diagnostics come from the
 * native compiler.
 */
object KinSyntax {

    private val KEYWORDS = setOf(
        "func", "fin", "var", "if", "else", "while", "for", "in",
        "return", "break", "continue", "and", "or", "not", "as", "import",
        "true", "false",
    )
    private val TYPES = setOf("int", "float", "bool", "string", "void")

    /** Apply Kin highlighting to `text` (single pass, allocations kept low). */
    fun highlight(text: String, colors: SyntaxColors): SpannableStringBuilder {
        val sb = SpannableStringBuilder(text)
        var i = 0
        val n = text.length

        fun span(start: Int, end: Int, color: Int, bold: Boolean = false) {
            if (end <= start) return
            sb.setSpan(ForegroundColorSpan(color), start, end, Spannable.SPAN_EXCLUSIVE_EXCLUSIVE)
            if (bold) sb.setSpan(StyleSpan(Typeface.BOLD), start, end, Spannable.SPAN_EXCLUSIVE_EXCLUSIVE)
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
        return sb
    }

    data class SyntaxColors(
        val keyword: Int,
        val type: Int,
        val string: Int,
        val number: Int,
        val comment: Int,
    )
}
