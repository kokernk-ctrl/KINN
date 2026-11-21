package dev.kinn.studio

import android.annotation.SuppressLint
import android.app.Activity
import android.os.Bundle
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Toast

/**
 * Offline preview of a generated web build (`build web` in the
 * terminal). The pages live in app-private storage; the WebView loads
 * them with file:// URLs and intercepts relative .html links so the
 * multi-page site navigates inside the app. No network access.
 */
class PreviewActivity : Activity() {

    private lateinit var webView: WebView
    private var buildRoot: String? = null

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        webView = WebView(this)
        setContentView(webView)

        buildRoot = intent.getStringExtra(EXTRA_ROOT)
        if (buildRoot == null) {
            Toast.makeText(this, "no build found — run `build web` first", Toast.LENGTH_SHORT).show()
            finish()
            return
        }

        webView.settings.allowFileAccess = true
        webView.settings.allowFileAccessFromFileURLs = true
        webView.settings.javaScriptEnabled = false
        webView.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val url = request.url
                // file:// URLs load inside the view; anything else is
                // blocked (the build contains no external references).
                if (url.scheme == "file") return false
                Toast.makeText(this@PreviewActivity, "blocked: $url (build is offline)", Toast.LENGTH_SHORT).show()
                return true
            }
        }
        webView.loadUrl("file://$buildRoot/index.html")
    }

    override fun onBackPressed() {
        if (this::webView.isInitialized && webView.canGoBack()) {
            webView.goBack()
        } else {
            super.onBackPressed()
        }
    }

    override fun onDestroy() {
        if (this::webView.isInitialized) {
            webView.destroy()
        }
        super.onDestroy()
    }

    companion object {
        const val EXTRA_ROOT = "build_root"
    }
}
