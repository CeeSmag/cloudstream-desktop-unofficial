package com.lagradost.runtime.stubs

import android.content.Context
import android.content.DialogInterface
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import com.google.android.material.R
import com.google.android.material.bottomsheet.BottomSheetBehavior
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.bottomsheet.BottomSheetDialogFragment
import org.junit.jupiter.api.Test
import java.util.concurrent.Callable
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertSame
import kotlin.test.assertTrue

/** Contract and concurrency tests for BottomSheetBehavior, BottomSheetDialog, and BottomSheetDialogFragment. */
class BottomSheetStubsTest {

    @Test
    fun `bottomSheetBehavior enforces material property and state transition contracts`() {
        val behavior = BottomSheetBehavior<View>()
        assertEquals(BottomSheetBehavior.STATE_COLLAPSED, behavior.state)
        assertFalse(behavior.skipCollapsed)
        assertTrue(behavior.isDraggable)
        assertFalse(behavior.isHideable)
        assertEquals(BottomSheetBehavior.PEEK_HEIGHT_AUTO, behavior.peekHeight)

        // Setting STATE_HIDDEN while not hideable is ignored per Material Components.
        behavior.state = BottomSheetBehavior.STATE_HIDDEN
        assertEquals(BottomSheetBehavior.STATE_COLLAPSED, behavior.state)

        behavior.isHideable = true
        behavior.state = BottomSheetBehavior.STATE_HIDDEN
        assertEquals(BottomSheetBehavior.STATE_HIDDEN, behavior.state)

        // Clearing hideable while hidden restores STATE_COLLAPSED.
        behavior.isHideable = false
        assertEquals(BottomSheetBehavior.STATE_COLLAPSED, behavior.state)

        behavior.skipCollapsed = true
        behavior.isDraggable = false
        behavior.setPeekHeight(-50)
        assertEquals(0, behavior.peekHeight)
        behavior.setPeekHeight(BottomSheetBehavior.PEEK_HEIGHT_AUTO)
        assertEquals(BottomSheetBehavior.PEEK_HEIGHT_AUTO, behavior.peekHeight)
        behavior.setPeekHeight(180)
        assertEquals(180, behavior.peekHeight)
        behavior.maxWidth = 640
        assertEquals(640, behavior.maxWidth)
        assertTrue(behavior.skipCollapsed)
        assertFalse(behavior.isDraggable)

        assertFailsWith<IllegalArgumentException> { behavior.state = BottomSheetBehavior.STATE_DRAGGING }
        assertFailsWith<IllegalArgumentException> { behavior.state = BottomSheetBehavior.STATE_SETTLING }
    }

    @Test
    fun `bottomSheetDialog binds design_bottom_sheet container to its behavior for from()`() {
        val dialog = BottomSheetDialog(Context())
        val content = View().apply { id = 42 }
        dialog.setContentView(content)

        val sheet = dialog.findViewById<View>(R.id.design_bottom_sheet)
        assertNotNull(sheet)
        assertSame(content, dialog.findViewById<View>(42))

        val resolved = BottomSheetBehavior.from(sheet)
        assertSame<BottomSheetBehavior<*>>(dialog.behavior, resolved)
        assertTrue(resolved.isHideable, "cancelable BottomSheetDialog defaults hideable=true")

        val transitions = mutableListOf<Int>()
        resolved.addBottomSheetCallback(object : BottomSheetBehavior.BottomSheetCallback() {
            override fun onStateChanged(bottomSheet: View, newState: Int) {
                assertSame(sheet, bottomSheet)
                transitions += newState
            }

            override fun onSlide(bottomSheet: View, slideOffset: Float) {}
        })
        resolved.state = BottomSheetBehavior.STATE_EXPANDED
        resolved.state = BottomSheetBehavior.STATE_HIDDEN
        assertEquals(listOf(BottomSheetBehavior.STATE_EXPANDED, BottomSheetBehavior.STATE_HIDDEN), transitions)

        assertFailsWith<IllegalArgumentException> { BottomSheetBehavior.from(View()) }
        assertFailsWith<IllegalArgumentException> { BottomSheetBehavior.from<View>(null) }
    }

    @Test
    fun `bottomSheetDialogFragment creates BottomSheetDialog tracks isAdded fires onDismiss once and supports startActivity`() {
        val dismissCount = AtomicInteger(0)
        val fragment = object : BottomSheetDialogFragment() {
            override fun onCreateDialog(savedInstanceState: Bundle?) =
                (super.onCreateDialog(savedInstanceState) as BottomSheetDialog).apply {
                    behavior.skipCollapsed = true
                    behavior.isDraggable = false
                    behavior.isHideable = true
                    behavior.state = BottomSheetBehavior.STATE_EXPANDED
                }

            override fun onCreateView(
                inflater: LayoutInflater,
                container: ViewGroup?,
                savedInstanceState: Bundle?,
            ): View = View()

            override fun onDismiss(dialog: DialogInterface) {
                super.onDismiss(dialog)
                dismissCount.incrementAndGet()
            }
        }

        assertFalse(fragment.isAdded)
        fragment.show(fragment.parentFragmentManager, "cf_sheet")
        assertTrue(fragment.isAdded)
        assertEquals("cf_sheet", fragment.tag)
        val dialog = fragment.dialog
        assertTrue(dialog is BottomSheetDialog)
        assertTrue(dialog.behavior.skipCollapsed)
        assertFalse(dialog.behavior.isDraggable)

        // Calling startActivity must not throw on desktop (headless safe).
        fragment.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://example.test")))

        fragment.dismissAllowingStateLoss()
        fragment.dismiss()
        assertFalse(fragment.isAdded)
        assertEquals(1, dismissCount.get(), "onDismiss must fire exactly once")
    }

    @Test
    fun `concurrent BottomSheetDialog creation and BottomSheetBehavior from lookups are race-free`() {
        val threads = 8
        val iterations = 200
        val start = CountDownLatch(1)
        val pool = Executors.newFixedThreadPool(threads)
        try {
            val futures = (0 until threads).map {
                pool.submit(
                    Callable {
                        start.await(5, TimeUnit.SECONDS)
                        for (i in 0 until iterations) {
                            val dialog = BottomSheetDialog(Context())
                            val sheet = dialog.findViewById<View>(R.id.design_bottom_sheet)
                            val behavior = BottomSheetBehavior.from(sheet)
                            assertSame<BottomSheetBehavior<*>>(dialog.behavior, behavior)
                            behavior.skipCollapsed = (i % 2 == 0)
                            behavior.state = BottomSheetBehavior.STATE_EXPANDED
                        }
                        true
                    },
                )
            }
            start.countDown()
            for (f in futures) {
                assertTrue(f.get(15, TimeUnit.SECONDS))
            }
        } finally {
            pool.shutdownNow()
        }
    }
}
