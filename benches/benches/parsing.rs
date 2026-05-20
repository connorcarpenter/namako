use criterion::{BenchmarkId, Criterion, Throughput, criterion_group};

fn bench_gherkin_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser/gherkin_parse");
    group.warm_up_time(std::time::Duration::from_millis(500));
    group.measurement_time(std::time::Duration::from_secs(5));

    let cases = [
        ("small_3s", namako_benches::FEATURE_SMALL),
        ("medium_15s", namako_benches::FEATURE_MEDIUM),
        ("large_40s", namako_benches::FEATURE_LARGE),
    ];

    for (name, content) in &cases {
        group.throughput(Throughput::Bytes(content.len() as u64));
        group.bench_with_input(BenchmarkId::new("feature", name), content, |b, &content| {
            b.iter(|| {
                namako_engine::gherkin::Feature::parse(
                    criterion::black_box(content),
                    namako_engine::gherkin::GherkinEnv::default(),
                )
            });
        });
    }

    group.finish();
}

/// Benchmark the cost of parsing N feature files sequentially — mirrors what
/// `namako lint` does during the discovery phase.
fn bench_multi_file_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser/multi_file");
    group.warm_up_time(std::time::Duration::from_millis(500));
    group.measurement_time(std::time::Duration::from_secs(5));

    let files = [
        namako_benches::FEATURE_SMALL,
        namako_benches::FEATURE_MEDIUM,
        namako_benches::FEATURE_LARGE,
    ];

    for count in [1usize, 5, 10] {
        let batch: Vec<&str> = files.iter().cycle().take(count).copied().collect();
        let total_bytes: u64 = batch.iter().map(|s| s.len() as u64).sum();

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("files", count),
            &batch,
            |b, batch| {
                b.iter(|| {
                    batch.iter().map(|content| {
                        namako_engine::gherkin::Feature::parse(
                            criterion::black_box(*content),
                            namako_engine::gherkin::GherkinEnv::default(),
                        )
                    }).collect::<Vec<_>>()
                });
            },
        );
    }

    group.finish();
}

criterion_group!(parsing_benchmarks, bench_gherkin_parse, bench_multi_file_parse);
