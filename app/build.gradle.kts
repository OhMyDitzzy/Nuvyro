import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import java.util.Properties
import javax.inject.Inject
import org.gradle.api.DefaultTask
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.Internal
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import org.gradle.process.ExecOperations

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

abstract class BuildTransferNativeTask : DefaultTask() {
    @get:Internal
    abstract val workspaceDirectory: DirectoryProperty

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val nativeSources: ConfigurableFileCollection

    @get:Input
    abstract val androidSdk: Property<String>

    @get:OutputDirectory
    abstract val outputDirectory: DirectoryProperty

    @get:Inject
    abstract val execOperations: ExecOperations

    @TaskAction
    fun buildNative() {
        execOperations.exec {
            commandLine(
                "python3", workspaceDirectory.file("tools/build_android.py").get().asFile.absolutePath,
                "--sdk", androidSdk.get(), "--output", outputDirectory.get().asFile.absolutePath
            )
        }
    }
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
    .orElse(providers.fileContents(rootProject.layout.projectDirectory.file("local.properties")).asText.map { text ->
        Properties().apply { load(text.reader()) }.getProperty("sdk.dir")
            ?: error("Set ANDROID_HOME or sdk.dir for the native build")
    })

val buildTransferNative = tasks.register<BuildTransferNativeTask>("buildTransferNative") {
    group = "build"
    description = "Explicitly build and validate the arm64-v8a transfer JNI library"
    workspaceDirectory.set(nativeWorkspace)
    nativeSources.from(fileTree(nativeWorkspace.asFile) {
        include("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "crates/**", "tools/build_android.py", "tools/android_elf.py")
    })
    androidSdk.set(nativeSdk)
    outputDirectory.set(nativeOutput)
    // External packaged Rust/NDK toolchains are verified by the script on each requested build.
    outputs.upToDateWhen { false }
}

if (includeTransferNative) {
    android {
        defaultConfig.ndk.abiFilters.add("arm64-v8a")
        packaging.jniLibs.useLegacyPackaging = false
    }
    androidComponents.onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(buildTransferNative) { it.outputDirectory }
    }
    tasks.named("preBuild").configure { dependsOn(buildTransferNative) }
}
