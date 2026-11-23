package android.webkit;

import android.graphics.Bitmap;
import android.os.Message;

/**
 * Overlay of the pinned reference {@code WebChromeClient} stub with the standard Android callback
 * methods invoked by plugins (progress, title, icon, window lifecycle).
 */
@android.annotation.Stub
public class WebChromeClient {
    public WebChromeClient() {}

    public void onProgressChanged(WebView view, int newProgress) {}

    public void onReceivedTitle(WebView view, String title) {}

    public void onReceivedIcon(WebView view, Bitmap icon) {}

    public boolean onCreateWindow(
        WebView view,
        boolean isDialog,
        boolean isUserGesture,
        Message resultMsg
    ) {
        return false;
    }

    public void onCloseWindow(WebView window) {}

    public void onRequestFocus(WebView view) {}
}
