import java.io.File

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.kinn.studio"
    compileSdk = 34

    defaultConfig {
        applicationId = "dev.kinn.studio"
        minSdk = 26
        targetSdk = 34
        versionCode = 4
        versionName = "0.4.0"
    }

    // Release signing credentials come from the environment (CI encrypted
    // secrets). When they are absent or blank the release APK stays
    // unsigned — CI documents this clearly instead of faking a signing
    // identity.
    fun env(name: String): String? = System.getenv(name)?.takeIf { it.isNotBlank() }
    val storeFileEnv = env("KINN_STORE_FILE")
    val storePasswordEnv = env("KINN_STORE_PASSWORD")
    val keyAliasEnv = env("KINN_KEY_ALIAS")
    val keyPasswordEnv = env("KINN_KEY_PASSWORD")

    signingConfigs {
        if (storeFileEnv != null && storePasswordEnv != null && keyAliasEnv != null && keyPasswordEnv != null) {
            create("release") {
                storeFile = file(storeFileEnv)
                storePassword = storePasswordEnv
                keyAlias = keyAliasEnv
                keyPassword = keyPasswordEnv
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (storeFileEnv != null) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
        debug {
            applicationIdSuffix = ".debug"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    buildFeatures {
        viewBinding = false
    }

    // Bundled documentation: copied from docs/ by the copyDocs task below,
    // so the app always ships the same documents as the repository.
    // Bundled Studio web app: the built studio/dist (both entries:
    // next.html — the 0.4.0 default — plus the web classic) copied by
    // copyStudioWeb, served by NextActivity through WebViewAssetLoader.
    sourceSets {
        getByName("main") {
            assets.srcDir(layout.buildDirectory.dir("generated/docs"))
            assets.srcDir(layout.buildDirectory.dir("generated/studio-web"))
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.recyclerview:recyclerview:1.3.2")
    // WebViewAssetLoader: serves the bundled web app on a real https
    // origin so IndexedDB/localStorage/WASM all work — without any
    // network access (no INTERNET permission in the manifest).
    implementation("androidx.webkit:webkit:1.11.0")
}

// Copy the repository documentation into the APK assets.
val copyDocs by tasks.registering(Copy::class) {
    from(rootProject.file("../docs")) {
        include("*.md")
    }
    from(rootProject.file("../README.md"))
    into(layout.buildDirectory.dir("generated/docs"))
}

tasks.matching { it.name.startsWith("merge") && it.name.endsWith("Assets") }.configureEach {
    dependsOn(copyDocs)
}
tasks.matching { it.name.startsWith("preBuild") }.configureEach {
    dependsOn(copyDocs)
}

// ---------------------------------------------------------------------------
// Bundled Studio web app (KINN Studio NEXT 0.4.0 default interface).
//
// studio/dist is a build artifact (never committed): build it with
//   cd studio && npm ci && npm run build:wasm && npm run build
// The checks fail the build early and honestly when it is missing —
// the app must never ship a WebView with an empty entry page.
// ---------------------------------------------------------------------------

val studioWebDir = rootProject.file("../studio/dist")

val ensureStudioWeb by tasks.registering {
    doLast {
        val required = listOf("next.html", "index.html", "kinn.wasm")
        val missing = required.filterNot { File(studioWebDir, it).isFile }
        if (!studioWebDir.isDirectory || missing.isNotEmpty()) {
            throw GradleException(
                "studio/dist is missing or incomplete" +
                    (if (missing.isEmpty()) "" else " (missing: ${missing.joinToString()})") +
                    ". Build the web app first: cd studio && npm ci && npm run build:wasm && npm run build",
            )
        }
    }
}

val copyStudioWeb by tasks.registering(Copy::class) {
    dependsOn(ensureStudioWeb)
    from(studioWebDir)
    into(layout.buildDirectory.dir("generated/studio-web"))
}

// Wire like copyDocs (proven pattern): the copy itself runs at
// preBuild time, so lint-vital and every later consumer see a complete
// directory and Gradle's implicit-dependency validation is satisfied.
tasks.matching { it.name.startsWith("merge") && it.name.endsWith("Assets") }.configureEach {
    dependsOn(copyStudioWeb)
}
tasks.matching { it.name.startsWith("preBuild") }.configureEach {
    dependsOn(copyStudioWeb)
}
