package androidx.fragment.app;

@android.annotation.Stub
public abstract class FragmentManager {
    public static abstract class FragmentLifecycleCallbacks {
        public FragmentLifecycleCallbacks() {}
    }

    public FragmentTransaction beginTransaction() {
        return new FragmentTransaction() {};
    }

    public Fragment findFragmentByTag(String tag) {
        return null;
    }

    public Fragment findFragmentById(int id) {
        return null;
    }

    public boolean executePendingTransactions() {
        return true;
    }

    public void popBackStack() {
    }

    public boolean isDestroyed() {
        return false;
    }

    public boolean isStateSaved() {
        return false;
    }

    public java.util.List<Fragment> getFragments() {
        return java.util.Collections.emptyList();
    }

    public void registerFragmentLifecycleCallbacks(FragmentLifecycleCallbacks cb, boolean recursive) {
    }

    public void unregisterFragmentLifecycleCallbacks(FragmentLifecycleCallbacks cb) {
    }
}
