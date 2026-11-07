# Aggressive renaming + package flattening.
-allowaccessmodification
-repackageclasses 'o'
-dontusemixedcaseclassnames
-overloadaggressively

-obfuscationdictionary obfuscation-dictionary.txt
-classobfuscationdictionary obfuscation-dictionary.txt
-packageobfuscationdictionary obfuscation-dictionary.txt

# Keep what the runtime actually needs. Strip source maps.
-keepattributes RuntimeVisibleAnnotations,RuntimeInvisibleAnnotations,AnnotationDefault,InnerClasses,EnclosingMethod,Signature,Exceptions
-renamesourcefileattribute
-dontpreverify

# Entry points from the manifest.
-keep public class pw.appbank.signum.SignumApp { <init>(); }
-keep public class pw.appbank.signum.MainActivity { <init>(); }
-keep public class pw.appbank.signum.data.device.UsbPermissionReceiver { <init>(); }

# kotlinx.serialization generated serializers.
-keepattributes *Annotation*
-dontnote kotlinx.serialization.AnnotationsKt
-keep,includedescriptorclasses class pw.appbank.signum.**$$serializer { *; }
-keepclassmembers class pw.appbank.signum.** {
    *** Companion;
}
-keepclasseswithmembers class pw.appbank.signum.** {
    kotlinx.serialization.KSerializer serializer(...);
}
-if @kotlinx.serialization.Serializable class **
-keepclassmembers class <1> {
    static <1>$Companion Companion;
}
-if @kotlinx.serialization.Serializable class ** {
    static **$* *;
}
-keepclassmembers class <2>$<3> {
    kotlinx.serialization.KSerializer serializer(...);
}
-keepclassmembers @kotlinx.serialization.Serializable class ** {
    *** Companion;
    *** INSTANCE;
    kotlinx.serialization.KSerializer serializer(...);
}

# Crypto / plist — reflection and SPI.
-keep class org.bouncycastle.** { *; }
-dontwarn org.bouncycastle.**
-dontwarn org.bouncycastle.jsse.**
-keep class com.dd.plist.** { *; }
-dontwarn com.dd.plist.**

# OkHttp platform probes.
-dontwarn okhttp3.internal.platform.**
-dontwarn org.conscrypt.**
-dontwarn org.openjsse.**
-dontwarn javax.naming.**
-dontwarn org.slf4j.**
-dontwarn com.google.errorprone.annotations.**

# Compose / coroutines consumer rules are applied automatically.
-dontwarn androidx.compose.**
-dontwarn kotlin.reflect.jvm.internal.**

# Keep native method names if any JNI shows up later.
-keepclasseswithmembernames class * {
    native <methods>;
}
