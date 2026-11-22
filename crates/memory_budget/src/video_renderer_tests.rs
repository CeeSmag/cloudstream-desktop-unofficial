use super::VideoRenderer;

#[test]
fn the_two_choices_are_cpu_then_gpu() {
    assert_eq!(
        VideoRenderer::CHOICES,
        [VideoRenderer::Cpu, VideoRenderer::Gpu]
    );
}

#[test]
fn text_round_trips_for_storage() {
    for choice in VideoRenderer::CHOICES {
        assert_eq!(VideoRenderer::parse(choice.to_text()), Some(choice));
    }
    assert_eq!(VideoRenderer::parse(" GPU "), Some(VideoRenderer::Gpu));
}

#[test]
fn an_unknown_word_is_not_accepted() {
    assert_eq!(VideoRenderer::parse("turbo"), None);
    assert_eq!(VideoRenderer::parse(""), None);
}

#[test]
fn only_gpu_asks_for_zero_copy_video() {
    assert!(VideoRenderer::Gpu.draws_into_the_window());
    assert!(!VideoRenderer::Cpu.draws_into_the_window());
}
