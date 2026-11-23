package android.webkit;

import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Desktop implementation of Android's {@code WebStorage} singleton for managing per-origin web
 * storage records. Thread-safe via {@link ConcurrentHashMap}.
 */
@android.annotation.Implemented
public class WebStorage {

    private static final WebStorage INSTANCE = new WebStorage();

    private final ConcurrentHashMap<String, Origin> origins = new ConcurrentHashMap<>();

    public static class Origin {
        private final String origin;
        private final long quota;
        private final long usage;

        protected Origin(String origin, long quota, long usage) {
            this.origin = origin;
            this.quota = quota;
            this.usage = usage;
        }

        protected Origin(String origin, long quota) {
            this(origin, quota, 0L);
        }

        protected Origin(String origin) {
            this(origin, 0L, 0L);
        }

        public String getOrigin() {
            return origin;
        }

        public long getQuota() {
            return quota;
        }

        public long getUsage() {
            return usage;
        }
    }

    public WebStorage() {}

    public static WebStorage getInstance() {
        return INSTANCE;
    }

    @SuppressWarnings("rawtypes")
    public void getOrigins(ValueCallback<Map> callback) {
        if (callback != null) {
            callback.onReceiveValue(new HashMap<>(origins));
        }
    }

    public void getUsageForOrigin(String origin, ValueCallback<Long> callback) {
        if (callback == null) {
            return;
        }
        Origin entry = origin != null ? origins.get(origin) : null;
        callback.onReceiveValue(entry != null ? entry.getUsage() : 0L);
    }

    public void getQuotaForOrigin(String origin, ValueCallback<Long> callback) {
        if (callback == null) {
            return;
        }
        Origin entry = origin != null ? origins.get(origin) : null;
        callback.onReceiveValue(entry != null ? entry.getQuota() : 0L);
    }

    public void setQuotaForOrigin(String origin, long quota) {
        if (origin == null || origin.isEmpty()) {
            return;
        }
        long clamped = Math.max(0L, quota);
        origins.compute(origin, (key, prev) -> {
            long usage = prev != null ? prev.getUsage() : 0L;
            return new Origin(key, clamped, usage);
        });
    }

    public void deleteOrigin(String origin) {
        if (origin != null) {
            origins.remove(origin);
        }
    }

    public void deleteAllData() {
        origins.clear();
    }
}
