package android.widget;

import android.content.Context;
import android.view.ViewGroup;

/**
 * Standard Android {@code AbsoluteLayout} base class for {@link android.webkit.WebView}.
 * Gives {@code WebView} the real {@code ViewGroup} inheritance chain ({@code removeAllViews}, etc.).
 */
@android.annotation.Stub
public class AbsoluteLayout extends ViewGroup {

    public AbsoluteLayout(Context context) {
        super(context);
    }

    public AbsoluteLayout(Context context, Object attrs) {
        super(context, attrs);
    }

    public AbsoluteLayout(Context context, Object attrs, int defStyleAttr) {
        super(context, attrs);
    }

    public static class LayoutParams extends ViewGroup.LayoutParams {
        public int x;
        public int y;

        public LayoutParams(int width, int height, int x, int y) {
            super(width, height);
            this.x = x;
            this.y = y;
        }

        public LayoutParams(Context c, Object attrs) {
            super(c, attrs);
        }

        public LayoutParams(ViewGroup.LayoutParams source) {
            super(source);
        }
    }
}
