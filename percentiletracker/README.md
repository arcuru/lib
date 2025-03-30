# PercentileTracker

`PercentileTracker` is a data structure designed for efficiently tracking percentiles within a stream of numerical data. It is similar in concept to the problem of finding the median from a data stream (like LeetCode's [Find Median from Data Stream](https://leetcode.com/problems/find-median-from-data-stream/)), but offers a more generalized solution for any percentile.

Essentially, this data structure functions like an efficient "select nth unstable" operation that can dynamically incorporate new inputs. Adding a new number and retrieving any percentile are both O(1) operations on average, leading to an overall time complexity of O(N) for processing a stream of N inputs.

## How It Works

`PercentileTracker` uses a smart bucketing strategy to achieve its efficiency:

- Values are stored in multiple buckets, with each bucket containing values within a similar range
- Only the bucket that has the target percentile is kept sorted, and only when needed
- That bucket, the critical bucket, is also kept to a maximum size by splitting when it grows too large
- Sorting is performed lazily, only when needed and only on the specific bucket containing the target percentile
- Binary search is used to efficiently locate the appropriate bucket for new insertions
- The implementation maintains indices to track which bucket contains the target percentile, allowing for O(1) retrieval in most cases
- In practice the total number of buckets stays small, typically getting into the range of hundreds

This approach avoids the need to sort the entire dataset after each insertion, significantly improving performance for large data streams.

## Performance Characteristics

Benchmarks demonstrate that `PercentileTracker` maintains its performance characteristics across:

- Different data sizes (from 100K to 100M elements)
- Various data distributions (uniform, normal, skewed, ascending, descending)
- Different usage patterns (insert-heavy, balanced, get-heavy)

The data structure is particularly efficient for:

- Streaming data applications where values arrive continuously
- Scenarios requiring frequent percentile calculations on growing datasets

## Usage Example

```rust
use percentiletracker::PercentileTracker;

// Create a tracker for the 90th percentile
let mut tracker = PercentileTracker::<i64>::new(90);

// Insert values from a data stream
tracker.insert(42);
tracker.insert(17);
tracker.insert(99);
// ... more insertions ...

// Get the current 90th percentile value at any time
let p90 = tracker.get_percentile();
```

The implementation is generic over any type that implements `Clone + Ord`, making it usable for any sortable type.
