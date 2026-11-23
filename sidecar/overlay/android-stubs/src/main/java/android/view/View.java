package android.view;

import android.content.Context;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.Drawable;
import android.os.Handler;
import com.lagradost.cloudstream3.desktop.shadowui.ShadowUi;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Overlay of the pinned reference {@code View} stub, adding standard base-class contracts:
 * touch event dispatch, per-view {@link Handler}, scrolling, layout bounds, window visibility,
 * and scrollbar flags (inherited by {@link ViewGroup} and {@link android.webkit.WebView}).
 */
@android.annotation.Stub
public class View {
    public static final int VISIBLE = 0;
    public static final int INVISIBLE = 4;
    public static final int GONE = 8;

    public static final int LAYER_TYPE_NONE = 0;
    public static final int LAYER_TYPE_SOFTWARE = 1;
    public static final int LAYER_TYPE_HARDWARE = 2;
    private int layerType = LAYER_TYPE_NONE;

    private static final AtomicInteger NEXT_GENERATED_ID = new AtomicInteger(0x100000);

    public int id = 0;
    public boolean isEnabled = true;
    private int visibility = VISIBLE;
    private boolean clickable = false;
    private boolean focusable = false;
    private boolean focusableInTouchMode = false;
    private boolean focused = false;
    private boolean selected = false;
    private boolean verticalScrollBarEnabled = false;
    private boolean horizontalScrollBarEnabled = false;

    public ViewGroup.LayoutParams layoutParams;

    public Drawable background;
    public int paddingLeft = 0;
    public int paddingTop = 0;
    public int paddingRight = 0;
    public int paddingBottom = 0;

    public float alpha = 1.0f;
    public float translationX = 0f;
    public float translationY = 0f;
    public int minHeight = 0;
    public int minWidth = 0;

    private int left = 0;
    private int top = 0;
    private int right = 0;
    private int bottom = 0;
    private int measuredWidth = 0;
    private int measuredHeight = 0;
    private int scrollX = 0;
    private int scrollY = 0;

    public OnClickListener clickListener;
    public OnLongClickListener longClickListener;
    public OnFocusChangeListener focusChangeListener;
    public OnKeyListener keyListener;
    private OnTouchListener touchListener;

    private Object tag;
    private ViewParent parent;
    private final ViewTreeObserver viewTreeObserver = new ViewTreeObserver();
    private final AtomicReference<Handler> handlerRef = new AtomicReference<>();

    protected Context context;

    public View() {
        this.context = android.content.DesktopContextProvider.INSTANCE.getContext();
    }

    public View(Context context) {
        this.context = (context != null) ? context : android.content.DesktopContextProvider.INSTANCE.getContext();
    }

    public View(Context context, Object attrs) {
        this(context);
    }

    public View(Context context, Object attrs, int defStyleAttr) {
        this(context);
    }

    public Context getContext() {
        return (context != null) ? context : android.content.DesktopContextProvider.INSTANCE.getContext();
    }

    public android.content.res.Resources getResources() {
        return getContext().getResources();
    }

    public interface OnClickListener {
        void onClick(View v);
    }

    public interface OnLongClickListener {
        boolean onLongClick(View v);
    }

    public interface OnFocusChangeListener {
        void onFocusChange(View v, boolean hasFocus);
    }

    public interface OnKeyListener {
        boolean onKey(View v, int keyCode, KeyEvent event);
    }

    public interface OnTouchListener {
        boolean onTouch(View v, Object event);
    }

    public int getId() { return id; }
    public void setId(int id) { this.id = id; }

    public int getVisibility() { return visibility; }
    public void setVisibility(int visibility) {
        this.visibility = visibility;
        ShadowUi.INSTANCE.bump();
    }

    public boolean isEnabled() { return isEnabled; }
    public void setEnabled(boolean enabled) { this.isEnabled = enabled; }

    public ViewGroup.LayoutParams getLayoutParams() { return layoutParams; }
    public void setLayoutParams(ViewGroup.LayoutParams params) {
        this.layoutParams = params;
        ShadowUi.INSTANCE.bump();
    }

    public void setPadding(int left, int top, int right, int bottom) {
        this.paddingLeft = left;
        this.paddingTop = top;
        this.paddingRight = right;
        this.paddingBottom = bottom;
    }

    public int getPaddingLeft() { return paddingLeft; }
    public int getPaddingTop() { return paddingTop; }
    public int getPaddingRight() { return paddingRight; }
    public int getPaddingBottom() { return paddingBottom; }

    public Drawable getBackground() { return background; }
    public void setBackground(Drawable background) { this.background = background; }
    public void setBackgroundDrawable(Drawable background) { this.background = background; }
    public void setBackgroundColor(int color) { this.background = new ColorDrawable(color); }

    public float getAlpha() { return alpha; }
    public void setAlpha(float alpha) { this.alpha = alpha; }

    public float getTranslationX() { return translationX; }
    public void setTranslationX(float translationX) { this.translationX = translationX; }

    public float getTranslationY() { return translationY; }
    public void setTranslationY(float translationY) { this.translationY = translationY; }

    public void setMinHeight(int minHeight) { this.minHeight = minHeight; }
    public void setMinWidth(int minWidth) { this.minWidth = minWidth; }

    public void setOnClickListener(OnClickListener listener) {
        this.clickListener = listener;
        if (listener != null) {
            this.clickable = true;
        }
    }

    public OnClickListener getOnClickListener() { return clickListener; }

    public void setOnLongClickListener(OnLongClickListener listener) {
        this.longClickListener = listener;
    }

    public OnLongClickListener getOnLongClickListener() { return longClickListener; }

    public void setOnFocusChangeListener(OnFocusChangeListener l) {
        this.focusChangeListener = l;
    }

    public OnFocusChangeListener getOnFocusChangeListener() { return focusChangeListener; }

    public void setOnKeyListener(OnKeyListener l) {
        this.keyListener = l;
    }

    public void setOnTouchListener(OnTouchListener l) {
        this.touchListener = l;
    }

    public boolean dispatchTouchEvent(MotionEvent event) {
        if (event == null) {
            return false;
        }
        if (isEnabled && touchListener != null && touchListener.onTouch(this, event)) {
            return true;
        }
        return onTouchEvent(event);
    }

    public boolean onTouchEvent(MotionEvent event) {
        if (event == null || !isEnabled) {
            return false;
        }
        if (!clickable) {
            return false;
        }
        if (event.getActionMasked() == MotionEvent.ACTION_UP) {
            performClick();
        }
        return true;
    }

    public View findViewById(int targetId) {
        if (this.id == targetId) return this;
        if (this instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) this;
            for (int i = 0; i < group.getChildCount(); i++) {
                View child = group.getChildAt(i);
                if (child != null) {
                    View found = child.findViewById(targetId);
                    if (found != null) return found;
                }
            }
        }
        return null;
    }

    public boolean performClick() {
        if (clickListener != null) {
            clickListener.onClick(this);
            return true;
        }
        return false;
    }

    public boolean performLongClick() {
        if (longClickListener != null) {
            return longClickListener.onLongClick(this);
        }
        return false;
    }

    public ViewPropertyAnimator animate() {
        return new ViewPropertyAnimator(this);
    }

    public Handler getHandler() {
        Handler existing = handlerRef.get();
        if (existing != null) {
            return existing;
        }
        Handler created = new Handler();
        return handlerRef.compareAndSet(null, created) ? created : handlerRef.get();
    }

    public boolean post(Runnable action) {
        if (action == null) return false;
        return getHandler().post(action);
    }

    public boolean postDelayed(Runnable action, long delayMillis) {
        if (action == null) return false;
        return getHandler().postDelayed(action, delayMillis);
    }

    public void removeCallbacks(Runnable action) {
        if (action == null) return;
        Handler existing = handlerRef.get();
        if (existing != null) {
            existing.removeCallbacks(action);
        }
    }

    public void setLayerType(int layerType, android.graphics.Paint paint) {
        this.layerType = layerType;
    }

    public int getLayerType() {
        return layerType;
    }

    public void invalidate() {}
    public void requestLayout() {}
    public void bringToFront() {}

    public boolean requestFocus() {
        this.focused = true;
        return true;
    }

    public void clearFocus() {
        this.focused = false;
    }

    public boolean isFocused() { return focused; }
    public void setClickable(boolean clickable) { this.clickable = clickable; }
    public boolean isClickable() { return clickable; }
    public void setFocusable(boolean focusable) { this.focusable = focusable; }
    public boolean isFocusable() { return focusable; }
    public void setFocusableInTouchMode(boolean focusable) { this.focusableInTouchMode = focusable; }
    public boolean isFocusableInTouchMode() { return focusableInTouchMode; }
    public void setSelected(boolean selected) { this.selected = selected; }
    public boolean isSelected() { return selected; }

    public void setVerticalScrollBarEnabled(boolean enabled) {
        this.verticalScrollBarEnabled = enabled;
    }

    public boolean isVerticalScrollBarEnabled() {
        return verticalScrollBarEnabled;
    }

    public void setHorizontalScrollBarEnabled(boolean enabled) {
        this.horizontalScrollBarEnabled = enabled;
    }

    public boolean isHorizontalScrollBarEnabled() {
        return horizontalScrollBarEnabled;
    }

    public int getScrollX() { return scrollX; }
    public int getScrollY() { return scrollY; }

    public void scrollTo(int x, int y) {
        if (this.scrollX != x || this.scrollY != y) {
            int oldX = this.scrollX;
            int oldY = this.scrollY;
            this.scrollX = x;
            this.scrollY = y;
            onScrollChanged(x, y, oldX, oldY);
        }
    }

    public void scrollBy(int x, int y) {
        scrollTo(this.scrollX + x, this.scrollY + y);
    }

    protected void onScrollChanged(int l, int t, int oldl, int oldt) {}

    public int getLeft() { return left; }
    public int getTop() { return top; }
    public int getRight() { return right; }
    public int getBottom() { return bottom; }
    public int getWidth() { return right - left; }
    public int getHeight() { return bottom - top; }

    public void layout(int l, int t, int r, int b) {
        int oldWidth = getWidth();
        int oldHeight = getHeight();
        boolean changed = l != this.left || t != this.top || r != this.right || b != this.bottom;
        this.left = l;
        this.top = t;
        this.right = r;
        this.bottom = b;
        int newWidth = getWidth();
        int newHeight = getHeight();
        if (newWidth != oldWidth || newHeight != oldHeight) {
            onSizeChanged(newWidth, newHeight, oldWidth, oldHeight);
        }
        onLayout(changed, l, t, r, b);
    }

    public final void measure(int widthMeasureSpec, int heightMeasureSpec) {
        onMeasure(widthMeasureSpec, heightMeasureSpec);
    }

    public Object getTag() { return tag; }
    public void setTag(Object tag) { this.tag = tag; }

    public ViewParent getParent() { return parent; }
    public void setParent(ViewParent parent) { this.parent = parent; }

    public ViewTreeObserver getViewTreeObserver() { return viewTreeObserver; }

    private boolean willNotDraw = false;

    public void setWillNotDraw(boolean willNotDraw) {
        this.willNotDraw = willNotDraw;
    }

    public boolean willNotDraw() {
        return willNotDraw;
    }

    public void draw(android.graphics.Canvas canvas) {
        onDraw(canvas);
    }

    protected void onDraw(android.graphics.Canvas canvas) {}

    protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
        setMeasuredDimension(
            getDefaultSize(getSuggestedMinimumWidth(), widthMeasureSpec),
            getDefaultSize(getSuggestedMinimumHeight(), heightMeasureSpec)
        );
    }

    protected void onLayout(boolean changed, int left, int top, int right, int bottom) {}

    protected void onSizeChanged(int w, int h, int oldw, int oldh) {}

    protected void onWindowVisibilityChanged(int visibility) {}

    public int getMeasuredWidth() { return measuredWidth; }
    public int getMeasuredHeight() { return measuredHeight; }

    protected final void setMeasuredDimension(int measuredWidth, int measuredHeight) {
        this.measuredWidth = measuredWidth;
        this.measuredHeight = measuredHeight;
    }

    public static int getDefaultSize(int size, int measureSpec) { return size; }
    public int getSuggestedMinimumWidth() { return minWidth; }
    public int getSuggestedMinimumHeight() { return minHeight; }

    public static int generateViewId() {
        return NEXT_GENERATED_ID.incrementAndGet();
    }
}
