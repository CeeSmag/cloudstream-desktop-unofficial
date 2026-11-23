package com.google.android.material.bottomsheet;

import android.view.View;
import java.lang.ref.WeakReference;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.WeakHashMap;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * Overlay of the pinned reference {@code BottomSheetBehavior} stub implementing the Material
 * Components contract: state machine, hideable/draggable/skipCollapsed/peekHeight/maxWidth
 * properties, {@link BottomSheetCallback} notifications, and thread-safe {@link #from(View)}.
 */
@android.annotation.Stub
public class BottomSheetBehavior<V extends View> {
    public static final int STATE_DRAGGING = 1;
    public static final int STATE_SETTLING = 2;
    public static final int STATE_EXPANDED = 3;
    public static final int STATE_COLLAPSED = 4;
    public static final int STATE_HIDDEN = 5;
    public static final int STATE_HALF_EXPANDED = 6;

    public static final int PEEK_HEIGHT_AUTO = -1;

    public static final int SAVE_PEEK_HEIGHT = 0x1;
    public static final int SAVE_FIT_TO_CONTENTS = 1 << 1;
    public static final int SAVE_HIDEABLE = 1 << 2;
    public static final int SAVE_SKIP_COLLAPSED = 1 << 3;
    public static final int SAVE_ALL = -1;
    public static final int SAVE_NONE = 0;

    private static final Map<View, BottomSheetBehavior<?>> ATTACHED_BEHAVIORS =
        Collections.synchronizedMap(new WeakHashMap<>());

    public abstract static class BottomSheetCallback {
        public abstract void onStateChanged(View bottomSheet, int newState);
        public abstract void onSlide(View bottomSheet, float slideOffset);
    }

    private final List<BottomSheetCallback> callbacks = new CopyOnWriteArrayList<>();
    private volatile WeakReference<V> viewRef = null;
    private volatile int state = STATE_COLLAPSED;
    private volatile boolean skipCollapsed = false;
    private volatile boolean draggable = true;
    private volatile boolean hideable = false;
    private volatile boolean fitToContents = true;
    private volatile int peekHeight = PEEK_HEIGHT_AUTO;
    private volatile int maxWidth = -1;
    private volatile float halfExpandedRatio = 0.5f;

    public BottomSheetBehavior() {}

    public int getState() {
        return state;
    }

    public void setState(int newState) {
        if (newState == STATE_DRAGGING || newState == STATE_SETTLING) {
            String label = newState == STATE_DRAGGING ? "DRAGGING" : "SETTLING";
            throw new IllegalArgumentException("STATE_" + label + " should not be set externally.");
        }
        if (!hideable && newState == STATE_HIDDEN) {
            return;
        }
        if (this.state == newState) {
            return;
        }
        this.state = newState;
        notifyStateChanged(newState);
    }

    public void setSkipCollapsed(boolean skipCollapsed) {
        this.skipCollapsed = skipCollapsed;
    }

    public boolean getSkipCollapsed() {
        return skipCollapsed;
    }

    public void setDraggable(boolean draggable) {
        this.draggable = draggable;
    }

    public boolean isDraggable() {
        return draggable;
    }

    public void setHideable(boolean hideable) {
        if (this.hideable != hideable) {
            this.hideable = hideable;
            if (!hideable && this.state == STATE_HIDDEN) {
                setState(STATE_COLLAPSED);
            }
        }
    }

    public boolean isHideable() {
        return hideable;
    }

    public void setPeekHeight(int peekHeight) {
        setPeekHeight(peekHeight, false);
    }

    public void setPeekHeight(int peekHeight, boolean animate) {
        this.peekHeight = peekHeight == PEEK_HEIGHT_AUTO
            ? PEEK_HEIGHT_AUTO
            : Math.max(0, peekHeight);
    }

    public int getPeekHeight() {
        return peekHeight;
    }

    public void setMaxWidth(int maxWidth) {
        this.maxWidth = maxWidth;
    }

    public int getMaxWidth() {
        return maxWidth;
    }

    public void setFitToContents(boolean fitToContents) {
        this.fitToContents = fitToContents;
    }

    public boolean isFitToContents() {
        return fitToContents;
    }

    public void setHalfExpandedRatio(float ratio) {
        if (ratio <= 0f || ratio >= 1f) {
            throw new IllegalArgumentException("ratio must be a float value between 0 and 1");
        }
        this.halfExpandedRatio = ratio;
    }

    public float getHalfExpandedRatio() {
        return halfExpandedRatio;
    }

    public void addBottomSheetCallback(BottomSheetCallback callback) {
        if (callback != null && !callbacks.contains(callback)) {
            callbacks.add(callback);
        }
    }

    public void removeBottomSheetCallback(BottomSheetCallback callback) {
        callbacks.remove(callback);
    }

    @SuppressWarnings("unchecked")
    public static <V extends View> BottomSheetBehavior<V> from(V view) {
        if (view == null) {
            throw new IllegalArgumentException("The view must not be null");
        }
        BottomSheetBehavior<?> found = ATTACHED_BEHAVIORS.get(view);
        if (found == null) {
            throw new IllegalArgumentException("The view is not associated with BottomSheetBehavior");
        }
        return (BottomSheetBehavior<V>) found;
    }

    static <V extends View> void attach(V view, BottomSheetBehavior<V> behavior) {
        if (view == null || behavior == null) {
            return;
        }
        behavior.viewRef = new WeakReference<>(view);
        ATTACHED_BEHAVIORS.put(view, behavior);
    }

    private void notifyStateChanged(int newState) {
        WeakReference<V> ref = viewRef;
        View sheet = ref != null ? ref.get() : null;
        if (sheet == null) {
            return;
        }
        for (BottomSheetCallback callback : callbacks) {
            callback.onStateChanged(sheet, newState);
        }
    }
}
