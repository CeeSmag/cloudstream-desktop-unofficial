package com.google.android.material.bottomsheet;

import android.app.Dialog;
import android.os.Bundle;
import androidx.fragment.app.DialogFragment;

/**
 * Overlay of the pinned reference {@code BottomSheetDialogFragment} stub returning a real
 * {@link BottomSheetDialog} from {@link #onCreateDialog(Bundle)} per Material Components.
 */
@android.annotation.Stub
public class BottomSheetDialogFragment extends DialogFragment {

    @Override
    public Dialog onCreateDialog(Bundle savedInstanceState) {
        return new BottomSheetDialog(getContext(), getTheme());
    }
}
