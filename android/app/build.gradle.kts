import java.net.URI
import java.security.MessageDigest

plugins {
    id("com.android.application")
}

// The release version: the tag's, which the release pipeline passes as -PyogurtVersion, else the workspace's in Cargo.toml.
// The in-app update check compares it with the APK names on the GitHub releases.
val appVersion: String = providers.gradleProperty("yogurtVersion").orNull
    ?: Regex("""(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"""")
        .find(rootDir.resolve("../Cargo.toml").readText())!!.groupValues[1]

// sherpa-onnx runs the on-device speech models. Its AAR is on GitHub releases only, so it is fetched once and pinned by hash.
// The static-link build carries onnxruntime inside its one native library.
val sherpaVersion = "2.0.1"
val sherpaSha256 = "07e042d97f3a6bd5b7fe74d4eef0513dc01a714b16c15a903250095aba43a575"
// Kept in Gradle's home, so a clean build does not fetch it again.
val sherpaAar: File = gradle.gradleUserHomeDir.resolve("caches/yogurt/sherpa-onnx-static-link-onnxruntime-$sherpaVersion.aar")

fun sha256(f: File): String = MessageDigest.getInstance("SHA-256").let { md ->
    f.inputStream().use { input -> val buf = ByteArray(1 shl 16); while (true) { val n = input.read(buf); if (n < 0) break; md.update(buf, 0, n) } }
    md.digest().joinToString("") { "%02x".format(it) }
}

if (!sherpaAar.exists() || sha256(sherpaAar) != sherpaSha256) {
    sherpaAar.parentFile.mkdirs()
    val part = File(sherpaAar.path + ".part")
    val conn = URI("https://github.com/k2-fsa/sherpa-onnx/releases/download/v$sherpaVersion/${sherpaAar.name}").toURL().openConnection().apply {
        connectTimeout = 30_000
        readTimeout = 60_000
    }
    conn.getInputStream().use { input ->
        part.outputStream().use { input.copyTo(it) }
    }
    check(sha256(part) == sherpaSha256) { "${sherpaAar.name} does not match its pinned SHA-256" }
    part.renameTo(sherpaAar)
}

val keystore: String? = System.getenv("YOGURT_ANDROID_KEYSTORE")
val keystorePassword: String? = System.getenv("YOGURT_ANDROID_KEYSTORE_PASSWORD")

android {
    namespace = "com.jarvisrchen.yogurt"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.jarvisrchen.yogurt"
        minSdk = 26
        targetSdk = 36
        versionName = appVersion
        // 0.1.0 -> 100, 2.3.4 -> 20304, so every release is also an upgrade Android accepts.
        versionCode = appVersion.split(".").fold(0) { acc, part -> acc * 100 + part.toInt() }
        // Every phone the app targets is 64-bit ARM, and each ABI adds about 10 MB of speech runtime.
        // Elsewhere Local reports that the phone can't run it, and Cloud still works.
        ndk { abiFilters += "arm64-v8a" }
    }

    signingConfigs {
        if (keystore != null && keystorePassword != null) {
            create("release") {
                storeFile = file(keystore)
                storePassword = keystorePassword
                keyAlias = "yogurt"
                keyPassword = keystorePassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release")
        }
    }

    buildFeatures {
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("com.squareup.okhttp3:okhttp:5.4.0")
    implementation("androidx.core:core:1.17.0")
    implementation(files(sherpaAar))
    testImplementation("junit:junit:4.13.2")
    // android.jar's org.json is a stub in JVM tests.
    testImplementation("org.json:json:20250517")
}
