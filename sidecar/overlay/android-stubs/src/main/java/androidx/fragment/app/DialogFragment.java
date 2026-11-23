package androidx.fragment.app;

import android.app.Dialog;
import android.content.Context;
import android.content.DialogInterface;
import android.os.Bundle;
import com.lagradost.cloudstream3.desktop.shadowui.ShadowUi;

/**
 * Overlay of the pinned reference {@code DialogFragment} stub implementing AndroidX's
 * {@link DialogInterface.OnCancelListener} / {@link DialogInterface.OnDismissListener} contract,
 * single-fire {@link #onDismiss(DialogInterface)}, theme/style accessors, and attached-state
 * transitions.
 */
@android.annotation.Stub
public class DialogFragment extends Fragment
    implements DialogInterface.OnCancelListener, DialogInterface.OnDismissListener {

    public static final int STYLE_NORMAL = 0;
    public static final int STYLE_NO_TITLE = 1;
    public static final int STYLE_NO_FRAME = 2;
    public static final int STYLE_NO_INPUT = 3;

    private Dialog dialogField;
    private int style = STYLE_NORMAL;
    private int theme = 0;
    private boolean cancelable = true;
    private boolean showsDialog = true;
    private boolean dismissed = false;

    public void setStyle(int style, int theme) {
        this.style = style;
        if (theme != 0) {
            this.theme = theme;
        }
    }

    public int getTheme() {
        return theme;
    }

    public void setShowsDialog(boolean showsDialog) {
        this.showsDialog = showsDialog;
    }

    public boolean getShowsDialog() {
        return showsDialog;
    }

    public Dialog onCreateDialog(Bundle savedInstanceState) {
        return new Dialog(getContext());
    }

    public void show(FragmentManager manager, String tag) {
        dismissed = false;
        markAdded(tag);
        prepareDialog();
        notifyCfInterceptor(tag);
        runLifecycle(true, tag);
        if (showsDialog && getView() == null && dialogField != null) {
            dialogField.show();
        }
    }

    public void show(FragmentTransaction transaction, String tag) {
        show((FragmentManager) null, tag);
    }

    public void dismiss() {
        dismissInternal();
    }

    public void dismissAllowingStateLoss() {
        dismissInternal();
    }

    @Override
    public void onCancel(DialogInterface dialog) {}

    @Override
    public void onDismiss(DialogInterface dialog) {
        if (!dismissed) {
            dismissed = true;
            ShadowUi.INSTANCE.pop(this);
            runTeardown();
            markDetached();
        }
        if (dialogField == dialog) {
            dialogField = null;
        }
    }

    public void setCancelable(boolean cancelable) {
        this.cancelable = cancelable;
        if (dialogField != null) {
            dialogField.setCancelable(cancelable);
        }
    }

    public boolean isCancelable() {
        return cancelable;
    }

    public Dialog getDialog() {
        if (dialogField == null) {
            dialogField = new Dialog(getContext());
        }
        return dialogField;
    }

    @Override
    public Context getContext() {
        return android.content.DesktopContextProvider.INSTANCE.getContext();
    }

    private void prepareDialog() {
        Dialog created = null;
        try {
            created = onCreateDialog(null);
        } catch (Throwable ignored) {}

        if (created != null) {
            dialogField = created;
        } else if (dialogField == null) {
            dialogField = new Dialog(getContext());
        }
        dialogField.setCancelable(cancelable);
        DialogInterface.OnDismissListener previousDismiss = dialogField.dismissListener;
        dialogField.setOnDismissListener(d -> {
            if (previousDismiss != null && previousDismiss != this) {
                previousDismiss.onDismiss(d);
            }
            onDismiss(d);
        });
        DialogInterface.OnCancelListener previousCancel = dialogField.cancelListener;
        dialogField.setOnCancelListener(d -> {
            if (previousCancel != null && previousCancel != this) {
                previousCancel.onCancel(d);
            }
            onCancel(d);
        });
    }

    private void dismissInternal() {
        if (dismissed) {
            return;
        }
        Dialog current = dialogField;
        if (current != null && current.isShowing()) {
            current.dismiss();
            if (!dismissed) {
                onDismiss(current);
            }
            return;
        }
        onDismiss(current);
    }

    private void runTeardown() {
        try {
            onPause();
            onStop();
            onDestroyView();
            onDestroy();
        } catch (Throwable ignored) {}
    }

    private void notifyCfInterceptor(String tag) {
        try {
            Class<?> interceptor = Class.forName(
                "com.lagradost.cloudstream3.desktop.network.DesktopCfDialogInterceptor"
            );
            java.lang.reflect.Method method = interceptor.getMethod("onShowCalled", Object.class, String.class);
            method.invoke(null, this, tag);
        } catch (Exception ignored) {}
    }
}
