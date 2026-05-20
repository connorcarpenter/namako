use criterion::criterion_main;

mod parsing;

criterion_main!(parsing::parsing_benchmarks);
