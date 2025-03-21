use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use percentiletracker::PercentileTracker;
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;

// Benchmark the full throughput of the percentile tracker
fn bench_tracker_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("tracker_throughput");

    // Test with different batch sizes
    for batch_size in [100_000, 1_000_000, 10_000_000, 100_000_000].iter() {
        // Test with different percentiles
        for percentile in [10, 50, 90].iter() {
            // Set throughput to report in bytes processed
            // Each i64 is 8 bytes
            group.throughput(Throughput::Bytes((*batch_size as u64) * 8));

            // Configure sample count based on batch size
            let sample_count = match batch_size {
                100_000 => 50,
                1_000_000 => 50,
                10_000_000 => 20,
                100_000_000 => 10,
                _ => 100,
            };
            group.sample_size(sample_count);

            group.bench_with_input(
                BenchmarkId::new(format!("size_{}", batch_size), percentile),
                &(*batch_size, *percentile),
                |b, &(size, percentile)| {
                    // Generate random values outside the benchmark loop
                    let mut rng = ChaCha8Rng::seed_from_u64(42);
                    let values: Vec<i64> = (0..size).map(|_| rng.random::<i64>()).collect();

                    b.iter(|| {
                        // Create a new tracker for each iteration
                        let mut tracker = PercentileTracker::<i64>::new(percentile);

                        // Insert all pre-generated values
                        for &value in &values {
                            tracker.insert(black_box(value));
                            black_box(tracker.get_percentile());
                        }
                    });
                },
            );
        }
    }

    group.finish();
}

// Benchmark with different data distributions
fn bench_data_distributions(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_distributions");

    // Number of values to insert
    let data_size = 1000000;
    let percentile = 90;

    // Set throughput to report in bytes processed
    // Each i64 is 8 bytes
    group.throughput(Throughput::Bytes((data_size as u64) * 8));

    // Define a type for our distribution functions
    type DistributionFn = Box<dyn Fn(&mut ChaCha8Rng, usize) -> Vec<i64>>;

    // Define different data distributions
    let distributions: Vec<(&str, DistributionFn)> = vec![
        (
            "uniform",
            Box::new(|rng: &mut ChaCha8Rng, n: usize| {
                (0..n).map(|_| rng.random::<i64>()).collect::<Vec<_>>()
            }),
        ),
        (
            "normal",
            Box::new(|rng: &mut ChaCha8Rng, n: usize| {
                (0..n)
                    .map(|_| {
                        // Approximate normal distribution using sum of uniform distributions
                        let sum: i32 = (0..12).map(|_| rng.random::<i32>() % 1000).sum();
                        (sum - 6000) as i64
                    })
                    .collect::<Vec<_>>()
            }),
        ),
        (
            "skewed",
            Box::new(|rng: &mut ChaCha8Rng, n: usize| {
                (0..n)
                    .map(|_| {
                        // Create a right-skewed distribution
                        let x = rng.random::<f64>();
                        (x * x * 1000.0) as i64
                    })
                    .collect::<Vec<_>>()
            }),
        ),
        (
            "ascending",
            Box::new(|_: &mut ChaCha8Rng, n: usize| (0..n as i64).collect::<Vec<_>>()),
        ),
        (
            "descending",
            Box::new(|_: &mut ChaCha8Rng, n: usize| (0..n as i64).rev().collect::<Vec<_>>()),
        ),
    ];

    for (name, dist_fn) in distributions.iter() {
        group.bench_function(*name, |b| {
            // Generate values outside the benchmark loop
            let mut rng = ChaCha8Rng::seed_from_u64(42);
            let values = dist_fn(&mut rng, data_size);

            b.iter(|| {
                // Create a new tracker for each iteration
                let mut tracker = PercentileTracker::<i64>::new(percentile);

                // Insert all pre-generated values
                for &value in &values {
                    tracker.insert(black_box(value));
                    black_box(tracker.get_percentile());
                }
            });
        });
    }

    group.finish();
}

// Benchmark realistic usage patterns
fn bench_realistic_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("realistic_usage");

    // Total operations for each benchmark
    let total_ops = 1000000;

    // Define different usage patterns
    for &(pattern_name, insert_count, get_count) in &[
        ("insert_heavy", 10, 1), // 10 inserts per get
        ("balanced", 1, 1),      // Equal inserts and gets
        ("get_heavy", 1, 10),    // 1 insert per 10 gets
    ] {
        // Calculate estimated number of inserts based on the pattern
        let total_cycle_ops = insert_count + get_count;
        let cycles = total_ops / total_cycle_ops;
        let estimated_inserts = cycles * insert_count;

        // Set throughput to report in bytes processed
        // Each i64 is 8 bytes - only count inserts for throughput
        group.throughput(Throughput::Bytes((estimated_inserts as u64) * 8));

        group.bench_function(pattern_name, |b| {
            // Generate values outside the benchmark loop
            let mut rng = ChaCha8Rng::seed_from_u64(42);
            let values: Vec<i64> = (0..1000000).map(|_| rng.random::<i64>()).collect();

            b.iter(|| {
                let mut tracker = PercentileTracker::<i64>::new(90);
                let mut value_index = 0;

                // Simulate the pattern for a fixed number of operations
                let total_ops = 1000000;
                let mut op_count = 0;

                while op_count < total_ops {
                    // Do inserts
                    for _ in 0..insert_count {
                        if value_index < values.len() {
                            tracker.insert(black_box(values[value_index]));
                            value_index += 1;
                            op_count += 1;
                            if op_count >= total_ops {
                                break;
                            }
                        }
                    }

                    if op_count >= total_ops {
                        break;
                    }

                    // Do gets
                    for _ in 0..get_count {
                        if value_index > 0 {
                            // Only get if we've inserted something
                            black_box(tracker.get_percentile());
                            op_count += 1;
                            if op_count >= total_ops {
                                break;
                            }
                        }
                    }
                }

                // Final get to ensure full operation
                black_box(tracker.get_percentile())
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_tracker_throughput,
    bench_data_distributions,
    bench_realistic_usage
);
criterion_main!(benches);
