package dev.kinn.studio

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Typeface
import android.text.Editable
import android.text.Layout
import android.text.TextWatcher
import android.util.AttributeSet
import androidx.appcompat.widget.AppCompatEditText

/**
 * Code editor field for the classic native Studio — 0.4.1 fix.
 *
 * One component owns the whole editing surface, so text, caret,
 * highlighting and line numbers CANNOT drift apart:
 *
 *  - Line numbers are drawn in [onDraw] from the REAL text layout
 *    ([Layout.getLineBaseline], [Layout.getLineTop]) — no separate
 *    TextView with estimated metrics, no shared-scroll bookkeeping.
 *    The numbers follow the vertical scroll (mScrollY) exactly and stay
 *    fixed horizontally, like every code editor.
 *  - Horizontal scrolling is provided by the TextView itself
 *    ([setHorizontallyScrolling] disables soft wrap: one horizontal
 *    scroll, no wrapping), so no HorizontalScrollView and no scrollbars
 *    cutting through the middle of the code.
 *  - Syntax highlighting is applied to the LIVE Editable via setSpan
 *    (see [KinSyntax.applyHighlight], called from the debounced
 *    [scheduleRehighlight]) — never via setText(), so the caret, the
 *    selection and the IME composition are never interrupted.
 *  - Input is configured for code: no suggestions, no autocorrect
 *    (textVisiblePassword variation — the strongest Gboard hint), no
 *    capitalization (XML).
 */
class CodeEditorView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = androidx.appcompat.R.attr.editTextStyle,
) : AppCompatEditText(context, attrs, defStyleAttr) {

    private val gutterPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = Typeface.MONOSPACE
        color = context.getColor(R.color.fg_dim)
    }

    /** Debounced rehighlight state (keeps typing smooth). */
    private var highlightScheduled = false

    /** Left inset of the code text (dp-converted once). */
    private val textLeftPad = dp(6)

    /** Gap between the numbers column and the code text. */
    private val gutterGap = dp(10)

    /** Default colors — same palette the previous rehighlight() used. */
    private val syntaxColors = KinSyntax.SyntaxColors(
        keyword = 0xFFFF7A1A.toInt(),
        type = 0xFF79C0FF.toInt(),
        string = 0xFFA5D6FF.toInt(),
        number = 0xFFF2CC60.toInt(),
        comment = 0xFF6E7681.toInt(),
    )

    init {
        // code editor behaviour: no soft wrap, the field scrolls itself
        setHorizontallyScrolling(true)
        setTypeface(Typeface.MONOSPACE)
        addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) {}
            override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) {}
            override fun afterTextChanged(s: Editable?) {
                updateGutterPadding()
                scheduleRehighlight()
            }
        })
        // the height of the view can change (keyboard, layout) — the
        // gutter strip just paints, so only a repaint is needed
        addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> invalidate() }
    }

    // -- gutter ------------------------------------------------------------------

    /**
     * Reserve left padding for the line-number column (width from the
     * digit count). Runs on text changes, never inside onDraw, so
     * requestLayout stays legal.
     */
    private fun updateGutterPadding() {
        val lines = (text?.count { it == '\n' } ?: 0) + 1
        val digits = lines.toString().length.coerceAtLeast(2)
        gutterPaint.textSize = textSize
        val numbersWidth = gutterPaint.measureText("0".repeat(digits)) + gutterGap * 2
        val wanted = (numbersWidth + textLeftPad).toInt()
        if (paddingLeft != wanted) {
            setPadding(wanted, paddingTop, paddingEnd, paddingBottom)
        }
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        updateGutterPadding()
    }

    /**
     * Draw the line numbers from the real layout. Baselines come from
     * [Layout.getLineBaseline] (the same metrics the text renderer
     * uses), offset by the view's scroll — aligned by construction.
     */
    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val layout: Layout = layout ?: return
        val lineCount = layout.lineCount
        if (lineCount <= 0) return

        gutterPaint.textSize = textSize
        val numbersRight = paddingLeft - textLeftPad - gutterGap / 2
        val extraTop = extendedPaddingTop
        val scroll = scrollY
        val h = height

        for (i in 0 until lineCount) {
            val lineTop = extraTop + layout.getLineTop(i) - scroll
            if (lineTop + lineHeight < 0 || lineTop > h) continue // culling
            val baseline = extraTop + layout.getLineBaseline(i) - scroll
            val num = (i + 1).toString()
            canvas.drawText(num, numbersRight - gutterPaint.measureText(num), baseline.toFloat(), gutterPaint)
        }
    }

    // -- highlighting --------------------------------------------------------------

    /** Debounce rehighlighting (250 ms) — typing stays smooth. */
    fun scheduleRehighlight() {
        if (highlightScheduled) return
        highlightScheduled = true
        postDelayed({
            highlightScheduled = false
            rehighlight()
        }, 250)
    }

    /**
     * Apply syntax highlighting to the LIVE Editable. Only our own
     * spans are touched: no setText, no selection change, no IME
     * interruption (requirement: never replace the content to
     * highlight it).
     */
    fun rehighlight() {
        val editable: Editable = editableText ?: return
        val text = editable.toString()
        if (text.length > MAX_HIGHLIGHT_CHARS) return // keep editing smooth on huge files
        KinSyntax.applyHighlight(editable, syntaxColors)
        invalidate()
    }

    /** Bring the line of a text offset into view (gotoLine support). */
    fun showOffset(offset: Int) {
        bringPointIntoView(offset.coerceIn(0, length()))
    }

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()

    companion object {
        private const val MAX_HIGHLIGHT_CHARS = 60_000
    }
}
