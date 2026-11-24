# JNA (used by the UniFFI Kotlin bindings) reflects on these.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.Structure { *; }
-dontwarn java.awt.**
# Generated bindings of the Rust core.
-keep class uniffi.** { *; }

# JLaTeXMath loads its macro packages and atoms by class name (reflection).
-keep class org.scilab.forge.jlatexmath.** { *; }
-keep class ru.noties.jlatexmath.** { *; }
-dontwarn org.scilab.forge.jlatexmath.**
