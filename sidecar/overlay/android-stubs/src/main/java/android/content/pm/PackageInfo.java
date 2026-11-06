package android.content.pm;

@android.annotation.Implemented
public class PackageInfo {
    public String packageName;
    public int versionCode = 1;
    public String versionName = "1.0.0";
    public ApplicationInfo applicationInfo = new ApplicationInfo();
    public SigningInfo signingInfo = new SigningInfo();
    public Signature[] signatures = new Signature[] { new Signature("desktop_signature") };

    public PackageInfo() {}

    public CharSequence loadLabel(PackageManager pm) {
        return packageName != null ? packageName : "CloudStream";
    }
}
