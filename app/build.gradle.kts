import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "com.ditzzy.nuvyro"
    compileSdk = 37
    defaultConfig {
        applicationId = "com.ditzzy.nuvyro"
        minSdk = 24
        targetSdk = 37
        versionCode = 1
        versionName = "1.0"
    }
    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        compose = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    implementation(libs.zxing.core)
    debugImplementation(libs.androidx.compose.ui.tooling)
    testImplementation(libs.junit)
}

val nativeWorkspace = rootProject.layout.projectDirectory.dir("native/nuvyro-transfer")
val nativeOutput = layout.buildDirectory.dir("generated/nuvyroTransfer/jniLibs")
val includeTransferNative = providers.gradleProperty("nuvyroNative").map { it.toBooleanStrict() }.getOrElse(false)
val nativeSdk = providers.environmentVariable("ANDROID_HOME")
    .orElse(providers.environmentVariable("ANDROID_SDK_ROOT"))
    .orElse(providers.provider {
        val properties = Properties()
        rootProject.file("local.properties").inputStream().use { properties.load(it) }
        properties.getProperty("sdk.dir") ?: error("Set ANDROID_HOME or sdk.dir for the native build")
    })

val buildTransferNative = tasks.register<Exec>("buildTransferNative") {
    group = "build"
    description = "Explicitly build and validate the arm64-v8a transfer JNI library"
    inputs.files(fileTree(nativeWorkspace.asFile) {
        include("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "crates/**", "tools/build_android.py", "tools/android_elf.py")
    })
    inputs.property("androidSdk", nativeSdk)
    outputs.dir(nativeOutput)
    // External packaged Rust/NDK toolchains are verified by the script on each requested build.
    outputs.upToDateWhen { false }
    doFirst {
        commandLine("python3", nativeWorkspace.file("tools/build_android.py").asFile.absolutePath,
            "--sdk", nativeSdk.get(), "--output", nativeOutput.get().asFile.absolutePath)
    }
}

if (includeTransferNative) {
    android {
        defaultConfig.ndk.abiFilters.add("arm64-v8a")
        sourceSets.getByName("main").jniLibs.srcDir(nativeOutput)
        packaging.jniLibs.useLegacyPackaging = false
    }
    tasks.named("preBuild").configure { dependsOn(buildTransferNative) }
}
