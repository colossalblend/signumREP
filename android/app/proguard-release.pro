# Release-only: drop logcat and debug dumps from the call graph.
-assumenosideeffects class android.util.Log {
    public static *** v(...);
    public static *** d(...);
    public static *** i(...);
    public static *** w(...);
    public static *** e(...);
    public static *** wtf(...);
    public static *** println(...);
}

-assumenosideeffects class pw.appbank.signum.data.api.DebugDump {
    public static void write(...);
    public static void writeText(...);
    public static void logFail(...);
    public static void pair(...);
}
