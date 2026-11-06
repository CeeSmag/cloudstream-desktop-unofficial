package android.content.pm;

@android.annotation.Implemented
public class ApplicationInfo {
    public String packageName;
    public String dataDir;
    public String sourceDir;
    public int flags;
    public int targetSdkVersion = 34;

    public ApplicationInfo() {}

    public CharSequence loadLabel(PackageManager pm) {
        return packageName != null ? packageName : "CloudStream";
    }
}
