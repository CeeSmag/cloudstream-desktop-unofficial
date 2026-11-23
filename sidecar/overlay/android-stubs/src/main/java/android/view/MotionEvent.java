package android.view;

/**
 * Minimal Android {@code MotionEvent} implementation for touch dispatch on desktop stubs.
 *
 * <p>Carries the standard action, coordinate, timing, meta-state and source fields expected by
 * {@link View#dispatchTouchEvent(MotionEvent)} and synthetic pointer helpers.
 */
@android.annotation.Implemented
public final class MotionEvent {
    public static final int ACTION_MASK = 0xff;
    public static final int ACTION_DOWN = 0;
    public static final int ACTION_UP = 1;
    public static final int ACTION_MOVE = 2;
    public static final int ACTION_CANCEL = 3;
    public static final int ACTION_OUTSIDE = 4;
    public static final int ACTION_POINTER_DOWN = 5;
    public static final int ACTION_POINTER_UP = 6;
    public static final int ACTION_POINTER_INDEX_MASK = 0xff00;
    public static final int ACTION_POINTER_INDEX_SHIFT = 8;

    private final long downTime;
    private final long eventTime;
    private final int action;
    private final float x;
    private final float y;
    private final int metaState;
    private int source;
    private boolean recycled;

    private MotionEvent(
        long downTime,
        long eventTime,
        int action,
        float x,
        float y,
        int metaState
    ) {
        this.downTime = downTime;
        this.eventTime = eventTime;
        this.action = action;
        this.x = x;
        this.y = y;
        this.metaState = metaState;
    }

    /** Android platform factory matching {@code MotionEvent.obtain(long, long, int, float, float, int)}. */
    public static MotionEvent obtain(
        long downTime,
        long eventTime,
        int action,
        float x,
        float y,
        int metaState
    ) {
        return new MotionEvent(downTime, eventTime, action, x, y, metaState);
    }

    public static MotionEvent obtain(MotionEvent other) {
        if (other == null) {
            throw new IllegalArgumentException("other must not be null");
        }
        MotionEvent copy = new MotionEvent(
            other.downTime,
            other.eventTime,
            other.action,
            other.x,
            other.y,
            other.metaState
        );
        copy.source = other.source;
        return copy;
    }

    public long getDownTime() {
        return downTime;
    }

    public long getEventTime() {
        return eventTime;
    }

    public int getAction() {
        return action;
    }

    public int getActionMasked() {
        return action & ACTION_MASK;
    }

    public int getActionIndex() {
        return (action & ACTION_POINTER_INDEX_MASK) >> ACTION_POINTER_INDEX_SHIFT;
    }

    public float getX() {
        return x;
    }

    public float getY() {
        return y;
    }

    public float getRawX() {
        return x;
    }

    public float getRawY() {
        return y;
    }

    public int getMetaState() {
        return metaState;
    }

    public int getPointerCount() {
        return 1;
    }

    public int getSource() {
        return source;
    }

    public void setSource(int source) {
        this.source = source;
    }

    public boolean isRecycled() {
        return recycled;
    }

    public void recycle() {
        this.recycled = true;
    }
}
