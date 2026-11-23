package android.content.res;

import java.io.ByteArrayInputStream;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.InputStream;

/**
 * Desktop {@code AssetManager}: the {@code assets/} folder of a plugin's own archive, as CloudStream's
 * PluginManager gives plugins that set {@code requiresResources} ({@code plugin.resources.assets}).
 *
 * <p>Names are checked before they reach the source: relative, {@code /}-separated, no empty,
 * {@code .} or {@code ..} parts, no backslash or NUL. So {@code open} can only reach files under
 * {@code assets/}, as on Android. Immutable and holds no open handle (nothing for {@link #close} to
 * release), so it is safe to share between threads.
 */
@android.annotation.Implemented
public final class AssetManager implements AutoCloseable {
    public static final int ACCESS_UNKNOWN = 0;
    public static final int ACCESS_RANDOM = 1;
    public static final int ACCESS_STREAMING = 2;
    public static final int ACCESS_BUFFER = 3;

    private static final DesktopAssetSource NONE = new DesktopAssetSource() {
        @Override
        public byte[] read(String path) {
            return null;
        }

        @Override
        public String[] list(String dir) {
            return new String[0];
        }
    };

    private final DesktopAssetSource source;

    /** No assets (Android's hidden constructor, which CloudStream calls reflectively). */
    public AssetManager() {
        this(NONE);
    }

    public AssetManager(DesktopAssetSource source) {
        this.source = source != null ? source : NONE;
    }

    public InputStream open(String fileName) throws IOException {
        return open(fileName, ACCESS_STREAMING);
    }

    public InputStream open(String fileName, int accessMode) throws IOException {
        String path = checkedPath(fileName, false);
        byte[] bytes = source.read(path);
        if (bytes == null) {
            throw new FileNotFoundException(fileName);
        }
        return new ByteArrayInputStream(bytes);
    }

    /** Direct children of directory {@code path}; {@code ""} lists the root. Empty when there is none. */
    public String[] list(String path) throws IOException {
        String dir = checkedPath(path, true);
        String[] names = source.list(dir);
        return names != null ? names : new String[0];
    }

    /** Nothing to release: every read opens and closes the archive itself. */
    @Override
    public void close() {
    }

    /**
     * The asset name in canonical form, or {@link FileNotFoundException} when it could point outside
     * {@code assets/}. A trailing {@code /} is accepted for directories; {@code ""} only when listing.
     */
    static String checkedPath(String name, boolean directory) throws FileNotFoundException {
        if (name == null) {
            throw new FileNotFoundException("null asset name");
        }
        // One trailing "/" names a directory ("scrapers/"); "/" alone is not the root, it is absolute.
        String path = directory && name.length() > 1 && name.endsWith("/")
            ? name.substring(0, name.length() - 1)
            : name;
        if (path.isEmpty()) {
            if (directory) {
                return "";
            }
            throw new FileNotFoundException("empty asset name");
        }
        if (path.startsWith("/") || path.indexOf('\\') >= 0 || path.indexOf('\0') >= 0) {
            throw new FileNotFoundException("invalid asset name: " + name);
        }
        for (String part : path.split("/", -1)) {
            if (part.isEmpty() || part.equals(".") || part.equals("..")) {
                throw new FileNotFoundException("invalid asset name: " + name);
            }
        }
        return path;
    }
}
