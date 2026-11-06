package android.content.pm;

import android.content.Intent;

@android.annotation.Implemented
public class PackageManager {
    public Intent getLaunchIntentForPackage(String packageName) {
        return null;
    }

    public PackageInfo getPackageInfo(String packageName, int flags) {
        PackageInfo info = new PackageInfo();
        info.packageName = packageName;
        return info;
    }
}
