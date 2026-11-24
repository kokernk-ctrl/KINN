package dev.kinn.studio

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.net.Uri
import android.os.Bundle
import android.util.Base64
import android.util.Log
import android.view.View
import android.webkit.ConsoleMessage
import android.webkit.JavascriptInterface
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Button
import android.widget.TextView
import android.widget.Toast
import androidx.activity.OnBackPressedCallback
import androidx.webkit.WebViewAssetLoader
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity

/**
 * KINN Studio NEXT 0.4.0 — the default interface of the Android app.
 *
 * Hosts the built NEXT web app (studio/dist, bundled under APK assets
 * `studio-web/`) inside a WebView served through `WebViewAssetLoader`,
 * which gives the page a real `https://appassets.androidplatform.net`
 * origin instead of a `file://` URL. That origin is what makes the
 * features that matter work in a WebView:
 *
 *  - IndexedDB + localStorage  -> project persistence and settings
 *  - WebAssembly               -> the real Kin compiler and VM
 *    (studio-web/kinn.wasm, the same kinn-ffi the CLI is built from)
 *
 * Every byte is served from the APK through the request interceptor —
 * the app has no INTERNET permission and nothing can phone home.
 *
 * What a WebView cannot do itself goes through the `window.kinnAndroid`
 * bridge (feature-detected by the page; the page is unchanged in a
 * normal browser):
 *  - saveFile()          -> SAF create-document (exports, .zip, .kinnproj)
 *  - openClassicStudio() -> the native classic Studio 0.3 (fallback)
 * File *import* uses the standard <input type=file> flow, completed by
 * [onShowFileChooser] — no bridge needed.
 */
class NextActivity : AppCompatActivity() {

    private lateinit var webView: WebView
    private lateinit var errorView: View
    private lateinit var errorMessage: TextView

    /** Bytes waiting for the SAF create-document round-trip. */
    private var pendingSave: ByteArray? = null

    /** WebView file-chooser callback (import: <input type=file>). */
    private var filePathCallback: ValueCallback<Array<Uri>>? = null

    // SAF create-document for bridge saveFile(name, base64, mime).
    private val saveLauncher =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            val uri = result.data?.data
            val bytes = pendingSave
            pendingSave = null
            if (result.resultCode == Activity.RESULT_OK && uri != null && bytes != null) {
                try {
                    contentResolver.openOutputStream(uri)?.use { it.write(bytes) }
                    Toast.makeText(this, "saved", Toast.LENGTH_SHORT).show()
                } catch (e: Exception) {
                    Log.e(TAG, "save failed", e)
                    Toast.makeText(this, "save failed: ${e.message}", Toast.LENGTH_LONG).show()
                }
            }
        }

    // System picker for <input type=file> (project / .kin import).
    private val chooserLauncher =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            filePathCallback?.onReceiveValue(
                WebChromeClient.FileChooserParams.parseResult(result.resultCode, result.data),
            )
            filePathCallback = null
        }

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_next)
        webView = findViewById(R.id.next_webview)
        errorView = findViewById(R.id.next_error)
        errorMessage = findViewById(R.id.next_error_message)

        findViewById<Button>(R.id.next_error_retry).setOnClickListener {
            hideError()
            webView.loadUrl(ENTRY_URL)
        }
        findViewById<Button>(R.id.next_error_classic).setOnClickListener {
            startActivity(Intent(this, MainActivity::class.java))
        }

        // Dev aid: chrome://inspect on a debuggable build.
        if ((applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0) {
            WebView.setWebContentsDebuggingEnabled(true)
        }

        val assetLoader = WebViewAssetLoader.Builder()
            .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(this))
            .build()

        webView.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true // localStorage (settings/layout) + IndexedDB quota class
            // Everything loads from the APK via the asset loader; there
            // is no reason to touch file:// or content:// at all.
            allowFileAccess = false
            allowContentAccess = false
            cacheMode = WebSettings.LOAD_DEFAULT
            builtInZoomControls = true
            displayZoomControls = false
            // The app ships its own font-size settings; a system text
            // scale would break the dense grid instead of helping.
            textZoom = 100
        }
        webView.setBackgroundColor(0xFF0B0C0E.toInt()) // no white flash while booting
        webView.addJavascriptInterface(KinnAndroidBridge(), "kinnAndroid")

        webView.webViewClient = object : WebViewClient() {
            override fun shouldInterceptRequest(
                view: WebView,
                request: WebResourceRequest,
            ): WebResourceResponse? = assetLoader.intercept(request.url)

            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                // Only our own (local) origin may render; anything else
                // would be a network dependency this app does not have.
                val url = request.url
                return if (url.host == DOMAIN) {
                    false
                } else {
                    Log.w(TAG, "blocked navigation to $url (offline app)")
                    Toast.makeText(this@NextActivity, "blocked: $url (offline app)", Toast.LENGTH_SHORT).show()
                    true
                }
            }

            override fun onReceivedError(
                view: WebView,
                request: WebResourceRequest,
                error: android.webkit.WebResourceError,
            ) {
                if (request.isForMainFrame) {
                    showError("the NEXT interface failed to load (${error.description}). The bundled web app may be incomplete.")
                }
            }

            override fun onReceivedHttpError(
                view: WebView,
                request: WebResourceRequest,
                errorResponse: WebResourceResponse,
            ) {
                // The asset loader answers missing files with a real 404;
                // treat a main-frame 404 as "the bundle is incomplete".
                if (request.isForMainFrame) {
                    showError("the NEXT interface is not in this APK (HTTP ${errorResponse.statusCode} for ${request.url.path}). Build studio/ and rebuild the app.")
                }
            }
        }

        webView.webChromeClient = object : WebChromeClient() {
            override fun onShowFileChooser(
                view: WebView,
                callback: ValueCallback<Array<Uri>>,
                params: FileChooserParams,
            ): Boolean {
                // Complete the standard <input type=file> flow so the
                // page code (unchanged from the browser) works.
                filePathCallback?.onReceiveValue(null)
                filePathCallback = callback
                return try {
                    chooserLauncher.launch(params.createIntent())
                    true
                } catch (e: android.content.ActivityNotFoundException) {
                    filePathCallback = null
                    Toast.makeText(this@NextActivity, "no file picker on this device", Toast.LENGTH_SHORT).show()
                    false
                }
            }

            override fun onConsoleMessage(msg: ConsoleMessage): Boolean {
                Log.d(TAG, "[next] ${msg.message()} @ ${msg.sourceId()}:${msg.lineNumber()}")
                return true
            }
        }

        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                // In-app navigation (e.g. web classic -> NEXT) goes back
                // inside the WebView; at the root the app finishes.
                if (webView.canGoBack()) webView.goBack() else finish()
            }
        })

        // Honest guard: the bundle is assembled by the Gradle task
        // copyStudioWeb; if it is somehow incomplete, say so instead of
        // showing a blank screen.
        val entryPresent = try {
            assets.open(ENTRY_ASSET).use { true }
        } catch (e: Exception) {
            false
        }
        if (entryPresent) {
            webView.loadUrl(ENTRY_URL)
        } else {
            showError("the bundled NEXT web app is missing from this APK (assets/$ENTRY_ASSET). Build studio/ first, then rebuild the app.")
        }
    }

    /** The window.kinnAndroid bridge (see studio/src/host.ts). */
    private inner class KinnAndroidBridge {
        @JavascriptInterface
        fun platform(): String = "android"

        @JavascriptInterface
        fun hostVersion(): String = HOST_VERSION

        @JavascriptInterface
        fun saveFile(name: String, base64: String, mime: String) {
            val bytes = Base64.decode(base64, Base64.DEFAULT)
            runOnUiThread {
                pendingSave = bytes
                val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = mime
                    putExtra(Intent.EXTRA_TITLE, name)
                }
                try {
                    saveLauncher.launch(intent)
                } catch (e: android.content.ActivityNotFoundException) {
                    pendingSave = null
                    Toast.makeText(this@NextActivity, "no document picker on this device", Toast.LENGTH_SHORT).show()
                }
            }
        }

        @JavascriptInterface
        fun openClassicStudio() {
            runOnUiThread {
                startActivity(Intent(this@NextActivity, MainActivity::class.java))
            }
        }
    }

    private fun showError(message: String) {
        errorMessage.text = message
        errorView.visibility = View.VISIBLE
        webView.visibility = View.GONE
    }

    private fun hideError() {
        errorView.visibility = View.GONE
        webView.visibility = View.VISIBLE
    }

    override fun onDestroy() {
        if (this::webView.isInitialized) {
            webView.destroy()
        }
        super.onDestroy()
    }

    companion object {
        private const val TAG = "KinnNext"
        private const val HOST_VERSION = "0.4.0"
        private const val DOMAIN = "appassets.androidplatform.net"
        private const val ENTRY_ASSET = "studio-web/next.html"
        private const val ENTRY_URL = "https://$DOMAIN/assets/$ENTRY_ASSET"
    }
}
