package android.content.pm;

@android.annotation.Implemented
public class SigningInfo {
    private final Signature[] signatures = new Signature[] { new Signature("desktop_signature") };

    public SigningInfo() {}

    public boolean hasMultipleSigners() {
        return false;
    }

    public boolean hasPastSigningCertificates() {
        return false;
    }

    public Signature[] getSigningCertificateHistory() {
        return signatures;
    }

    public Signature[] getApkContentsSigners() {
        return signatures;
    }
}
