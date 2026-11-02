plugins {
    id("com.android.application")
}

// Bump this to ship an app update: the release pipeline names the APK after it, and the in-app update check compares it.
val appVersion = "0.1.0"

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
        // 0.1.0 -> 100, 2.3.4 -> 20304, so every version bump is also an upgrade Android accepts.
        versionCode = appVersion.split(".").fold(0) { acc, part -> acc * 100 + part.toInt() }
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
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
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
    testImplementation("junit:junit:4.13.2")
    // android.jar's org.json is a stub in JVM tests.
    testImplementation("org.json:json:20250517")
}
