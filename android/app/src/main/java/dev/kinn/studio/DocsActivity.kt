package dev.kinn.studio

import android.os.Bundle
import android.view.View
import android.widget.ArrayAdapter
import android.widget.ListView
import android.widget.ScrollView
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

/**
 * Offline documentation browser: the .md documents from the repository
 * are bundled into the APK assets (no network). Markdown is shown as
 * plain monospace text — honest and readable; a fancy renderer is not
 * claimed.
 */
class DocsActivity : AppCompatActivity() {

    private var openDoc: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_docs)

        val list = findViewById<ListView>(R.id.docs_list)
        val scroll = findViewById<ScrollView>(R.id.docs_scroll)
        val content = findViewById<TextView>(R.id.docs_content)
        val title = findViewById<TextView>(R.id.docs_title)

        val docs = assets.list("docs")?.filter { it.endsWith(".md") }?.sorted() ?: emptyList()

        findViewById<View>(R.id.docs_back).setOnClickListener {
            if (openDoc != null) {
                openDoc = null
                title.text = "docs"
                list.visibility = View.VISIBLE
                scroll.visibility = View.GONE
            } else {
                finish()
            }
        }

        list.adapter = ArrayAdapter(this, android.R.layout.simple_list_item_1, docs)
        list.setOnItemClickListener { _, _, position, _ ->
            val name = docs[position]
            openDoc = name
            title.text = name
            content.text = assets.open("docs/$name").bufferedReader().use { it.readText() }
            list.visibility = View.GONE
            scroll.visibility = View.VISIBLE
            scroll.scrollTo(0, 0)
        }
    }
}
