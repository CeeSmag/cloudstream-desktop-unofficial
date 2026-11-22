//! The values each limit may take. The settings screen offers exactly these, and a stored value
//! outside them is ignored.

/// Video cache sizes (MiB).
pub const VIDEO_CACHE_CHOICES_MIB: [u32; 7] = [64, 128, 256, 512, 1024, 2048, 4096];
/// Shares (percent) of the video cache read ahead of the playing position.
pub const READ_AHEAD_CHOICES_PERCENT: [u8; 4] = [25, 50, 75, 90];
/// Limits (GB) of the app's data folder.
pub const STORAGE_LIMIT_CHOICES_GB: [u32; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

/// Whether `mib` is an offered video cache size.
pub fn is_offered_video_cache(mib: u32) -> bool {
    VIDEO_CACHE_CHOICES_MIB.contains(&mib)
}

/// Whether `percent` is an offered read-ahead share.
pub fn is_offered_read_ahead(percent: u8) -> bool {
    READ_AHEAD_CHOICES_PERCENT.contains(&percent)
}

/// Whether `gb` is an offered storage limit.
pub fn is_offered_storage_gb(gb: u32) -> bool {
    STORAGE_LIMIT_CHOICES_GB.contains(&gb)
}

#[cfg(test)]
#[path = "budget_choices_tests.rs"]
mod tests;
