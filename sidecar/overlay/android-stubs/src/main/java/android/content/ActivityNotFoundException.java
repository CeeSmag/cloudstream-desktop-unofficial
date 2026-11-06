package android.content;

@android.annotation.Implemented
public class ActivityNotFoundException extends RuntimeException {
    public ActivityNotFoundException() {
        super();
    }

    public ActivityNotFoundException(String name) {
        super(name);
    }
}
