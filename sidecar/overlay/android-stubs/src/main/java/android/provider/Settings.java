package android.provider;

import android.content.ContentResolver;

public class Settings {
    public static final class Secure {
        public static final String ANDROID_ID = "android_id";

        public static String getString(ContentResolver resolver, String name) {
            return "cloudstream_desktop_id";
        }
    }
}
