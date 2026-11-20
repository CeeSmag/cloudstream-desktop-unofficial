//! Hot-path benchmarks for stream link filtering, pipe normalization, and quality sorting.
#![allow(missing_docs)]

use app_services::{link_rank, playable_links_best_first};
use core_model::PlayableLink;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn sample_links(count: usize) -> Vec<PlayableLink> {
    let qualities = [360, 480, 720, 1080, 1440, 2160];
    let kinds = ["VIDEO", "M3U8", "DASH", "TORRENT", "M3U8", "VIDEO"];
    (0..count)
        .map(|i| {
            let ext = if i % 9 == 0 { "mp3" } else { "m3u8" };
            PlayableLink {
                source: format!("Src{}", i % 7),
                name: format!("Mirror {i}"),
                url: format!("https://cdn{}.example/stream/{i}.{ext}", i % 5),
                quality: *qualities.get(i % qualities.len()).unwrap_or(&720),
                link_type: (*kinds.get(i % kinds.len()).unwrap_or(&"M3U8")).to_owned(),
                ..PlayableLink::default()
            }
        })
        .collect()
}

fn bench_link_ranking(c: &mut Criterion) {
    let small = sample_links(20);
    let medium = sample_links(200);
    let large = sample_links(1_000);

    c.bench_function("playable_links_best_first_20", |b| {
        b.iter(|| playable_links_best_first(black_box(small.clone())))
    });
    c.bench_function("playable_links_best_first_200", |b| {
        b.iter(|| playable_links_best_first(black_box(medium.clone())))
    });
    c.bench_function("playable_links_best_first_1000", |b| {
        b.iter(|| playable_links_best_first(black_box(large.clone())))
    });
    c.bench_function("link_rank_1000", |b| {
        b.iter(|| {
            large
                .iter()
                .map(|link| link_rank(black_box(link)))
                .sum::<i32>()
        })
    });
}

criterion_group!(benches, bench_link_ranking);
criterion_main!(benches);
