package android.webkit;

import android.content.Context;

/**
 * Overlay of the pinned reference {@code WebSettings} stub, adding {@link #getDefaultUserAgent(Context)},
 * standard cache/mixed-content/force-dark constants, and remaining Android settings accessors.
 */
@android.annotation.Stub
public class WebSettings {
    public static final int LOAD_DEFAULT = -1;
    public static final int LOAD_NORMAL = 0;
    public static final int LOAD_CACHE_ELSE_NETWORK = 1;
    public static final int LOAD_NO_CACHE = 2;
    public static final int LOAD_CACHE_ONLY = 3;

    public static final int MIXED_CONTENT_ALWAYS_ALLOW = 0;
    public static final int MIXED_CONTENT_NEVER_ALLOW = 1;
    public static final int MIXED_CONTENT_COMPATIBILITY_MODE = 2;

    public static final int FORCE_DARK_OFF = 0;
    public static final int FORCE_DARK_AUTO = 1;
    public static final int FORCE_DARK_ON = 2;

    public static final String DEFAULT_USER_AGENT =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0";

    private String userAgent = DEFAULT_USER_AGENT;
    private boolean javaScriptEnabled = true;
    private boolean domStorageEnabled = true;
    private boolean mediaPlaybackRequiresUserGesture = false;
    private boolean blockNetworkImage = false;
    private boolean allowFileAccess = true;
    private boolean allowContentAccess = true;
    private boolean appCacheEnabled = true;
    private boolean databaseEnabled = true;
    private boolean loadsImagesAutomatically = true;
    private boolean useWideViewPort = true;
    private boolean loadWithOverviewMode = true;
    private boolean supportZoom = true;
    private boolean builtInZoomControls = false;
    private boolean displayZoomControls = false;
    private int cacheMode = LOAD_DEFAULT;
    private int mixedContentMode = MIXED_CONTENT_ALWAYS_ALLOW;
    private boolean supportMultipleWindows = false;
    private boolean javaScriptCanOpenWindowsAutomatically = false;
    private String defaultTextEncodingName = "UTF-8";
    private String standardFontFamily = "sans-serif";
    private PluginState pluginState = PluginState.OFF;
    private boolean safeBrowsingEnabled = true;
    private boolean offscreenPreRaster = false;
    private int forceDark = FORCE_DARK_AUTO;
    private int disabledActionModeMenuItems = 0;

    public static String getDefaultUserAgent(Context context) {
        return DEFAULT_USER_AGENT;
    }

    public void setJavaScriptEnabled(boolean flag) { this.javaScriptEnabled = flag; }
    public boolean getJavaScriptEnabled() { return this.javaScriptEnabled; }

    public void setDomStorageEnabled(boolean flag) { this.domStorageEnabled = flag; }
    public boolean getDomStorageEnabled() { return this.domStorageEnabled; }

    public void setMediaPlaybackRequiresUserGesture(boolean flag) { this.mediaPlaybackRequiresUserGesture = flag; }
    public boolean getMediaPlaybackRequiresUserGesture() { return this.mediaPlaybackRequiresUserGesture; }

    public void setBlockNetworkImage(boolean flag) { this.blockNetworkImage = flag; }
    public boolean getBlockNetworkImage() { return this.blockNetworkImage; }

    public void setUserAgentString(String ua) {
        this.userAgent = (ua == null || ua.isEmpty()) ? DEFAULT_USER_AGENT : ua;
    }

    public String getUserAgentString() { return this.userAgent; }

    public void setAllowFileAccess(boolean flag) { this.allowFileAccess = flag; }
    public boolean getAllowFileAccess() { return this.allowFileAccess; }

    public void setAllowContentAccess(boolean flag) { this.allowContentAccess = flag; }
    public boolean getAllowContentAccess() { return this.allowContentAccess; }

    public void setAppCacheEnabled(boolean flag) { this.appCacheEnabled = flag; }
    public boolean getAppCacheEnabled() { return this.appCacheEnabled; }

    public void setDatabaseEnabled(boolean flag) { this.databaseEnabled = flag; }
    public boolean getDatabaseEnabled() { return this.databaseEnabled; }

    public void setLoadsImagesAutomatically(boolean flag) { this.loadsImagesAutomatically = flag; }
    public boolean getLoadsImagesAutomatically() { return this.loadsImagesAutomatically; }

    public void setUseWideViewPort(boolean flag) { this.useWideViewPort = flag; }
    public boolean getUseWideViewPort() { return this.useWideViewPort; }

    public void setLoadWithOverviewMode(boolean flag) { this.loadWithOverviewMode = flag; }
    public boolean getLoadWithOverviewMode() { return this.loadWithOverviewMode; }

    public void setSupportZoom(boolean flag) { this.supportZoom = flag; }
    public boolean getSupportZoom() { return this.supportZoom; }

    public void setBuiltInZoomControls(boolean flag) { this.builtInZoomControls = flag; }
    public boolean getBuiltInZoomControls() { return this.builtInZoomControls; }

    public void setDisplayZoomControls(boolean flag) { this.displayZoomControls = flag; }
    public boolean getDisplayZoomControls() { return this.displayZoomControls; }

    public void setCacheMode(int mode) { this.cacheMode = mode; }
    public int getCacheMode() { return this.cacheMode; }

    public void setMixedContentMode(int mode) { this.mixedContentMode = mode; }
    public int getMixedContentMode() { return this.mixedContentMode; }

    public void setSupportMultipleWindows(boolean flag) { this.supportMultipleWindows = flag; }
    public boolean getSupportMultipleWindows() { return this.supportMultipleWindows; }

    public void setJavaScriptCanOpenWindowsAutomatically(boolean flag) { this.javaScriptCanOpenWindowsAutomatically = flag; }
    public boolean getJavaScriptCanOpenWindowsAutomatically() { return this.javaScriptCanOpenWindowsAutomatically; }

    public void setDefaultTextEncodingName(String name) { this.defaultTextEncodingName = name; }
    public String getDefaultTextEncodingName() { return this.defaultTextEncodingName; }

    public void setStandardFontFamily(String font) { this.standardFontFamily = font; }
    public String getStandardFontFamily() { return this.standardFontFamily; }

    public void setPluginState(PluginState state) { this.pluginState = state; }
    public PluginState getPluginState() { return this.pluginState; }

    public void setSafeBrowsingEnabled(boolean enabled) { this.safeBrowsingEnabled = enabled; }
    public boolean getSafeBrowsingEnabled() { return this.safeBrowsingEnabled; }

    public void setOffscreenPreRaster(boolean enabled) { this.offscreenPreRaster = enabled; }
    public boolean getOffscreenPreRaster() { return this.offscreenPreRaster; }

    public void setForceDark(int forceDark) { this.forceDark = forceDark; }
    public int getForceDark() { return this.forceDark; }

    public void setDisabledActionModeMenuItems(int menuItems) { this.disabledActionModeMenuItems = menuItems; }
    public int getDisabledActionModeMenuItems() { return this.disabledActionModeMenuItems; }

    public enum PluginState {
        ON, ON_DEMAND, OFF
    }
}
