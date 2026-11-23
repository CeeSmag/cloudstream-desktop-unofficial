package com.google.android.material.bottomsheet;

import android.app.Dialog;
import android.content.Context;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.FrameLayout;
import com.google.android.material.R;

/**
 * Overlay of the pinned reference {@code BottomSheetDialog} stub. Hosts content inside a
 * {@link FrameLayout} identified by {@code R.id.design_bottom_sheet} bound to its
 * {@link BottomSheetBehavior} so {@link BottomSheetBehavior#from(View)} resolves to the dialog's
 * own behavior, matching Android Material Components.
 */
@android.annotation.Stub
public class BottomSheetDialog extends Dialog {

    private final BottomSheetBehavior<FrameLayout> behavior = new BottomSheetBehavior<>();
    private final FrameLayout bottomSheetContainer;
    private boolean dismissWithAnimation = false;

    public BottomSheetDialog(Context context) {
        this(context, 0);
    }

    public BottomSheetDialog(Context context, int theme) {
        super(context);
        this.bottomSheetContainer = new FrameLayout(getContext());
        this.bottomSheetContainer.setId(R.id.design_bottom_sheet);
        this.behavior.setHideable(this.cancelable);
        BottomSheetBehavior.attach(this.bottomSheetContainer, this.behavior);
    }

    public BottomSheetBehavior<FrameLayout> getBehavior() {
        return behavior;
    }

    public void setDismissWithAnimation(boolean dismissWithAnimation) {
        this.dismissWithAnimation = dismissWithAnimation;
    }

    public boolean getDismissWithAnimation() {
        return dismissWithAnimation;
    }

    @Override
    public void setCancelable(boolean cancelable) {
        super.setCancelable(cancelable);
        this.behavior.setHideable(cancelable);
    }

    @Override
    public void setContentView(View view) {
        wrapInBottomSheet(view, null);
    }

    @Override
    public void setContentView(int layoutResID) {
        View inflated = LayoutInflater.from(getContext()).inflate(layoutResID, bottomSheetContainer, false);
        wrapInBottomSheet(inflated, null);
    }

    public void setContentView(View view, ViewGroup.LayoutParams params) {
        wrapInBottomSheet(view, params);
    }

    @Override
    @SuppressWarnings("unchecked")
    public <T extends View> T findViewById(int id) {
        if (id == R.id.design_bottom_sheet) {
            return (T) bottomSheetContainer;
        }
        T found = super.findViewById(id);
        if (found != null) {
            return found;
        }
        return (T) bottomSheetContainer.findViewById(id);
    }

    public void onAttachedToWindow() {}

    public void onDetachedFromWindow() {}

    @Override
    public void show() {
        try {
            Class<?> interceptor = Class.forName(
                "com.lagradost.cloudstream3.desktop.network.DesktopCfDialogInterceptor"
            );
            java.lang.reflect.Method method = interceptor.getMethod("onShowCalled", Object.class, String.class);
            method.invoke(null, this, null);
        } catch (Exception ignored) {
        }
        super.show();
    }

    private void wrapInBottomSheet(View view, ViewGroup.LayoutParams params) {
        bottomSheetContainer.removeAllViews();
        if (view != null) {
            if (params != null) {
                bottomSheetContainer.addView(view, params);
            } else {
                bottomSheetContainer.addView(view);
            }
        }
        super.setContentView(bottomSheetContainer);
    }
}
