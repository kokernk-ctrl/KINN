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
        versionCode = 2
        versionName = "0.2.0"
    }

    // Release signing credentials come from the environment (CI encrypted
    // secrets). When they are absent the release APK stays unsigned —
    // CI documents this clearly instead of faking a signing identity.
    val storeFileEnv = providers.environmentVariable("KINN_STORE_FILE").orNull
    val storePasswordEnv = providers.environmentVariable("KINN_STORE_PASSWORD").orNull
    val keyAliasEnv = providers.environmentVariable("KINN_KEY_ALIAS").orNull
    val keyPasswordEnv = providers.environmentVariable("KINN_KEY_PASSWORD").orNull

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
    sourceSets {
        getByName("main") {
            assets.srcDir(layout.buildDirectory.dir("generated/docs"))
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.recyclerview:recyclerview:1.3.2")
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
