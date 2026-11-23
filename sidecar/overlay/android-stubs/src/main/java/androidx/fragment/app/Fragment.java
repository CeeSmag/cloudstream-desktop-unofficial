package androidx.fragment.app;

import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import com.lagradost.cloudstream3.desktop.shadowui.ShadowDialog;
import com.lagradost.cloudstream3.desktop.shadowui.ShadowUi;

/**
 * Overlay of the pinned reference {@code Fragment} stub, tracking real added/detached lifecycle
 * state ({@link #isAdded()}, {@link #getTag()}) and forwarding {@link #startActivity(Intent)}.
 */
@android.annotation.Implemented
public class Fragment {
    private static final FragmentActivity sActivity = new FragmentActivity();

    private View viewField = null;
    private Bundle arguments = null;
    private volatile boolean added = false;
    private volatile String tag = null;

    public void setArguments(Bundle args) {
        this.arguments = args;
    }

    public Bundle getArguments() {
        return this.arguments;
    }

    public final Bundle requireArguments() {
        Bundle args = getArguments();
        if (args == null) {
            throw new IllegalStateException("Fragment " + this + " does not have any arguments.");
        }
        return args;
    }

    public Context getContext() {
        return android.content.DesktopContextProvider.INSTANCE.getContext();
    }

    public final Context requireContext() {
        Context context = getContext();
        if (context == null) {
            throw new IllegalStateException("Fragment " + this + " not attached to a context.");
        }
        return context;
    }

    public FragmentActivity getActivity() {
        return sActivity;
    }

    public final FragmentActivity requireActivity() {
        return sActivity;
    }

    public final boolean isAdded() {
        return added;
    }

    public final String getTag() {
        return tag;
    }

    public final Fragment getParentFragment() {
        return null;
    }

    public LayoutInflater getLayoutInflater() {
        return LayoutInflater.from(requireContext());
    }

    public android.content.res.Resources getResources() {
        return requireContext().getResources();
    }

    public final String getString(int resId) {
        return requireContext().getResources().getString(resId);
    }

    public final String getString(int resId, Object... formatArgs) {
        return requireContext().getResources().getString(resId, formatArgs);
    }

    public void startActivity(Intent intent) {
        startActivity(intent, null);
    }

    public void startActivity(Intent intent, Bundle options) {
        if (intent == null) {
            return;
        }
        requireContext().startActivity(intent);
    }

    public View getView() {
        return viewField;
    }

    public View requireView() {
        return (viewField != null) ? viewField : new View();
    }

    public void onCreate(Bundle savedInstanceState) {}

    public View onCreateView(LayoutInflater inflater, ViewGroup container, Bundle savedInstanceState) {
        return null;
    }

    public void onViewCreated(View view, Bundle savedInstanceState) {}
    public void onStart() {}
    public void onResume() {}
    public void onPause() {}
    public void onStop() {}
    public void onSaveInstanceState(Bundle outState) {}
    public void onDestroyView() {}
    public void onDestroy() {}

    protected void markAdded(String fragmentTag) {
        this.tag = fragmentTag;
        this.added = true;
    }

    protected void markDetached() {
        this.added = false;
        this.viewField = null;
    }

    public void runLifecycle(boolean record, String tag) {
        markAdded(tag);
        try {
            onCreate(null);
            View view = onCreateView(LayoutInflater.from(requireContext()), null, null);
            this.viewField = view;
            onViewCreated(view, null);
            if (record && view != null) {
                ShadowUi.INSTANCE.push(
                    new ShadowDialog(this, view, true, tag, ShadowUi.INSTANCE.nextDialogId())
                );
            }
            onStart();
            onResume();
        } catch (Throwable t) {
            System.err.println("Fragment lifecycle error: " + t.getMessage());
        }
    }

    public FragmentManager getChildFragmentManager() {
        return new FragmentManager() {};
    }

    public FragmentManager getParentFragmentManager() {
        return new FragmentManager() {};
    }
}
