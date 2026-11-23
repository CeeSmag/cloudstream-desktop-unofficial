package android.content.res;

import java.io.IOException;

/**
 * Where an {@link AssetManager} reads its files from. Desktop only (Android reads APK assets natively).
 *
 * <p>The host binds a plugin's own archive ({@code com.lagradost.runtime.assets.PluginArchiveAssets});
 * a plugin implementing this itself can only serve its own data. Paths arrive already checked by
 * {@link AssetManager#checkedPath}: relative, {@code /}-separated, no {@code .}/{@code ..}/empty parts.
 */
public interface DesktopAssetSource {

    /** The bytes of asset {@code path}, or {@code null} when there is no such file. */
    byte[] read(String path) throws IOException;

    /** Names of the direct children of directory {@code dir} ({@code ""} is the root), never {@code null}. */
    String[] list(String dir) throws IOException;
}
