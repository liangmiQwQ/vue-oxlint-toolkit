use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use oxc_allocator::Allocator;
use vue_oxlint_jsx::VueOxcParser;

fn bench(c: &mut Criterion) {
  let mut group = c.benchmark_group("vue_parse_by_size");

  let samples = [
    ("small", include_str!("../small.vue")),
    ("medium", include_str!("../medium.vue")),
    ("large", include_str!("../large.vue")),
  ];

  for (name, html) in samples {
    // Measure memory usage once before the timing benchmark
    #[allow(clippy::cast_precision_loss)]
    {
      let allocator = Allocator::default();
      let _ = black_box(VueOxcParser::new(&allocator, html).parse());
      let used = allocator.used_bytes();

      println!(
        "\nBenchmark: {:<10} | Memory: {:>10} bytes ({:>8.2} KB)",
        name,
        used,
        used as f64 / 1024.0
      );
    }

    group.bench_function(name, |b| {
      b.iter(|| {
        let allocator = Allocator::new();
        black_box(VueOxcParser::new(&allocator, html).parse());
      });
    });
  }

  group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
