// n0te: the notes app (plan ~/.claude/plans/n0te-notes-app.md). Separate package, installed
// through n0bridge like the Diktat app. The shared Rust core (n0te/crates/ffi) is built by
// n0te/scripts/build-android.sh into build/rust (native lib + generated Kotlin bindings).
// Builds as `:n0te` inside n0bridge (app-android) and on its own (n0te/android, CI).
plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// The n0te repo root (this module is n0te/android), the same whether built from n0bridge or alone.
val n0teDir: File = projectDir.parentFile

android {
    namespace = "dev.n0bridge.n0te"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.n0bridge.n0te"
        minSdk = 29
        targetSdk = 35
        versionCode = 70
        versionName = "0.34.1"
        // The tablet: arm64. Emulator tests add x86_64 (-Pn0te.abis="arm64-v8a x86_64").
        ndk { abiFilters += (project.findProperty("n0te.abis") as String? ?: "arm64-v8a").split(" ") }
    }

    signingConfigs {
        create("releaseDev") {
            enableV2Signing = true
            enableV3Signing = true
            // Release builds on GitHub Actions: the key from the repo's secrets (the workflow
            // writes it to a file and sets these). Updates only install over an app signed with
            // the same key, so that must be the key the installed app has. Locally: as before.
            val ciStore = System.getenv("N0TE_SIGNING_STORE_FILE")
            val ks = file("${System.getProperty("user.home")}/.android/debug.keystore")
            if (!ciStore.isNullOrBlank()) {
                storeFile = file(ciStore)
                storePassword = System.getenv("N0TE_SIGNING_STORE_PASSWORD")
                keyAlias = System.getenv("N0TE_SIGNING_KEY_ALIAS")
                keyPassword = System.getenv("N0TE_SIGNING_KEY_PASSWORD")
            } else if (ks.exists()) {
                storeFile = ks
                storePassword = "android"
                keyAlias = "androiddebugkey"
                keyPassword = "android"
            } else {
                initWith(signingConfigs.getByName("debug"))
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            signingConfig = signingConfigs.getByName("releaseDev")
            // JNA and the generated UniFFI bindings use reflection: keep rules in proguard-rules.pro.
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    sourceSets.getByName("main") {
        java.srcDir(layout.buildDirectory.dir("rust/kotlin"))
        jniLibs.srcDir(layout.buildDirectory.dir("rust/jniLibs"))
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures { compose = true }
    packaging {
        jniLibs { useLegacyPackaging = true }
        dex { useLegacyPackaging = true }
    }
    lint { checkReleaseBuilds = false }
    testOptions {
        unitTests.all {
            // JVM tests load a laptop build of the same Rust library (cargo build -p n0te-ffi).
            it.systemProperty("jna.library.path", n0teDir.resolve("target/release").absolutePath)
            // ...and the laptop pdfium from n0te/scripts/fetch-pdfium.sh (PdfBindingsTest).
            it.systemProperty("n0te.pdfium", n0teDir.resolve("third_party/pdfium/linux-x64/lib/libpdfium.so").absolutePath)
        }
    }
}

// Rebuild the Rust core when its sources change.
val rustCore by tasks.registering(Exec::class) {
    inputs.files(fileTree(n0teDir) { include("crates/**/*.rs", "crates/**/Cargo.toml", "Cargo.toml", "Cargo.lock", "scripts/*.sh") })
    outputs.dir(layout.buildDirectory.dir("rust"))
    commandLine(n0teDir.resolve("scripts/build-android.sh").absolutePath)
    environment("N0TE_ABIS", project.findProperty("n0te.abis") as String? ?: "arm64-v8a")
    inputs.property("abis", project.findProperty("n0te.abis") as String? ?: "arm64-v8a")
}
tasks.named("preBuild") { dependsOn(rustCore) }

dependencies {
    implementation(platform("androidx.compose:compose-bom:2024.10.01"))
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("net.java.dev.jna:jna:5.19.1@aar")
    implementation("androidx.graphics:graphics-core:1.0.4")
    implementation("androidx.input:input-motionprediction:1.0.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    // QR codes (pairing with the laptop, invites): Google's code scanner, no camera permission.
    implementation("com.google.android.gms:play-services-code-scanner:16.1.0")
    // Handwriting search: Google ML Kit's ink recogniser, on the device (the model for a
    // language, about 20 MB, is downloaded once; nothing written leaves the tablet).
    implementation("com.google.mlkit:digital-ink-recognition:19.0.0")
    // LaTeX formulas in text ($…$): JLaTeXMath (renders on a Canvas, offline).
    implementation("ru.noties:jlatexmath-android:0.2.0")
    testImplementation("junit:junit:4.13.2")
    testImplementation("net.java.dev.jna:jna:5.19.1")
}
