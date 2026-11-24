// Standalone build of the n0te Android app (CI, the n0te repo on its own):
//   cd n0te/android && gradle assembleRelease
// Inside n0bridge the same module is `:n0te` of app-android, and this file is not read.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
    // Versions for the plugins the module asks for without one (n0bridge's root build sets the same).
    plugins {
        id("com.android.application") version "8.7.2"
        id("org.jetbrains.kotlin.android") version "2.0.21"
        id("org.jetbrains.kotlin.plugin.compose") version "2.0.21"
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "n0te"
