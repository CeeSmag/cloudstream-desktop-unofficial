package android.content.pm;

@android.annotation.Implemented
public class Signature {
    private final byte[] signature;

    public Signature(byte[] signature) {
        this.signature = signature != null ? signature : new byte[0];
    }

    public Signature(String text) {
        this.signature = text != null ? text.getBytes() : new byte[0];
    }

    public byte[] toByteArray() {
        return signature;
    }

    public String toCharsString() {
        return "desktop_signature";
    }
}
