package android.webkit;

import android.content.Context;
import android.widget.AbsoluteLayout;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.BiConsumer;

/**
 * Overlay of the pinned reference {@code WebView} stub following Android's class hierarchy
 * ({@code WebView -> AbsoluteLayout -> ViewGroup -> View}), keeping a persistent {@link WebSettings}
 * instance, load progress/URL state, {@link WebChromeClient}, and registered JS interfaces.
 */
@android.annotation.Stub
public class WebView extends AbsoluteLayout {
    public static volatile BiConsumer<WebView, String> loadUrlHandler = null;
    private static volatile boolean webContentsDebuggingEnabled = false;

    private final WebSettings settings = new WebSettings();
    private final ConcurrentHashMap<String, Object> javascriptInterfaces = new ConcurrentHashMap<>();
    private volatile WebViewClient webViewClient = null;
    private volatile WebChromeClient webChromeClient = null;
    private volatile String currentUrl = null;
    private volatile String currentTitle = null;
    private volatile int progress = 0;
    private volatile Map<String, String> lastAdditionalHttpHeaders = Collections.emptyMap();

    public WebView(Context context) {
        super(context);
    }

    public WebView(Context context, Object attrs) {
        super(context, attrs);
    }

    public WebView(Context context, Object attrs, int defStyleAttr) {
        super(context, attrs, defStyleAttr);
    }

    public WebSettings getSettings() {
        return settings;
    }

    public void loadUrl(String url) {
        loadUrl(url, null);
    }

    public void loadUrl(String url, Map<String, String> additionalHttpHeaders) {
        this.currentUrl = url;
        this.progress = 0;
        this.lastAdditionalHttpHeaders = copyHeaders(additionalHttpHeaders);
        BiConsumer<WebView, String> handler = loadUrlHandler;
        if (handler != null) {
            handler.accept(this, url);
        }
    }

    public void loadData(String data, String mimeType, String encoding) {
        if (data == null) {
            return;
        }
        String mime = mimeType != null ? mimeType : "text/html";
        String charset = encoding != null ? encoding : "utf-8";
        loadUrl("data:" + mime + ";charset=" + charset + "," + data);
    }

    public void loadDataWithBaseURL(
        String baseUrl,
        String data,
        String mimeType,
        String encoding,
        String historyUrl
    ) {
        String mime = mimeType != null ? mimeType : "text/html";
        String target = baseUrl != null
            ? baseUrl
            : (data != null ? "data:" + mime + "," + data : "about:blank");
        loadUrl(target);
    }

    public void postUrl(String url, byte[] postData) {
        loadUrl(url);
    }

    public int getProgress() {
        return progress;
    }

    public void clearCache(boolean includeDiskFiles) {}

    public void clearHistory() {}

    public boolean canGoBack() {
        return false;
    }

    public void goBack() {}

    public boolean canGoForward() {
        return false;
    }

    public void goForward() {}

    public String getUrl() {
        return currentUrl;
    }

    public String getTitle() {
        return currentTitle;
    }

    public void setWebViewClient(WebViewClient client) {
        this.webViewClient = client;
    }

    public WebViewClient getWebViewClient() {
        return this.webViewClient;
    }

    public void setWebChromeClient(WebChromeClient client) {
        this.webChromeClient = client;
    }

    public WebChromeClient getWebChromeClient() {
        return this.webChromeClient;
    }

    public void addJavascriptInterface(Object object, String name) {
        if (object != null && name != null) {
            javascriptInterfaces.put(name, object);
        }
    }

    public void removeJavascriptInterface(String name) {
        if (name != null) {
            javascriptInterfaces.remove(name);
        }
    }

    public void evaluateJavascript(String script, ValueCallback<String> resultCallback) {
        if (resultCallback != null) {
            resultCallback.onReceiveValue(null);
        }
    }

    public void stopLoading() {
        this.progress = 0;
    }

    public void reload() {
        String url = this.currentUrl;
        if (url != null) {
            loadUrl(url, this.lastAdditionalHttpHeaders);
        }
    }

    public void onPause() {}

    public void onResume() {}

    public void pauseTimers() {}

    public void resumeTimers() {}

    public void destroy() {
        this.progress = 0;
        this.currentUrl = null;
        this.currentTitle = null;
        this.lastAdditionalHttpHeaders = Collections.emptyMap();
        this.javascriptInterfaces.clear();
        removeAllViews();
    }

    public static void setWebContentsDebuggingEnabled(boolean enabled) {
        webContentsDebuggingEnabled = enabled;
    }

    @Override
    public void setLayerType(int layerType, android.graphics.Paint paint) {
        super.setLayerType(layerType, paint);
    }

    @Override
    public int getLayerType() {
        return super.getLayerType();
    }

    private static Map<String, String> copyHeaders(Map<String, String> headers) {
        if (headers == null || headers.isEmpty()) {
            return Collections.emptyMap();
        }
        return Collections.unmodifiableMap(new LinkedHashMap<>(headers));
    }
}
