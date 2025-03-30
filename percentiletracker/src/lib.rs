use std::cell::RefCell;
use std::cmp::Ord;

// This was handtuned over a few timing runs. It's not perfect, but it's good enough.
// Also confusingly, this number seems to not have much impact if it isn't pathological.
// I haven't tested but I suspect it's because other operations dominate the runtime.
const MAX_BUCKET_SIZE: usize = 64;

/// A container for a subset of values with a common property - all values are greater than or equal to min_value.
///
/// The bucket structure enables efficient percentile calculation by:
/// - Grouping values with similar magnitudes together
/// - Lazily sorting values only when needed
/// - Tracking minimum values to enable binary search across buckets
///
/// Buckets store their values in a vector and track whether the values are sorted.
/// They also cache the minimum value for efficient bucket location.
struct Bucket<T>
where
    T: Clone + Ord,
{
    /// The minimum value in this bucket, cached for efficient comparisons.
    min_value: T,

    /// The collection of values stored in this bucket.
    values: Vec<T>,

    /// Flag indicating whether the values are currently sorted.
    /// This allows us to avoid unnecessary sorting operations.
    sorted: bool,
}

impl<T> Bucket<T>
where
    T: Clone + Ord,
{
    /// Creates a new bucket containing a single value.
    ///
    /// The bucket is initialized with the value as both its minimum value and its only content.
    /// The bucket is marked as sorted since it contains only one element.
    ///
    /// # Parameters
    /// * `value` - The initial value to store in the bucket
    fn new(value: T) -> Self {
        Bucket {
            min_value: value.clone(),
            values: vec![value],
            sorted: true,
        }
    }

    /// Returns the minimum value stored in this bucket.
    ///
    /// This is an O(1) operation as the minimum value is cached.
    fn min(&self) -> &T {
        &self.min_value
    }

    /// Returns the number of values stored in this bucket.
    fn len(&self) -> usize {
        self.values.len()
    }

    /// Adds a new value to this bucket.
    ///
    /// After pushing a new value, the bucket is marked as unsorted.
    /// Note that this does not update the minimum value of the bucket, which must be
    /// done separately if needed.
    ///
    /// # Parameters
    /// * `num` - The value to add to the bucket
    fn push(&mut self, num: T) {
        self.values.push(num);
        self.sorted = false;
    }

    /// Updates the minimum value of this bucket.
    ///
    /// This method only updates the cached minimum value and does not check if the
    /// provided value is actually the minimum value in the bucket.
    ///
    /// # Parameters
    /// * `new_min` - The new minimum value to set
    fn update_min_value(&mut self, new_min: T) {
        self.min_value = new_min;
    }

    /// Ensures that the values in this bucket are sorted.
    ///
    /// If the bucket is already marked as sorted, this is a no-op. Otherwise,
    /// it sorts the values in the bucket and marks it as sorted.
    fn ensure_sorted(&mut self) {
        if !self.sorted {
            self.values.sort_unstable();
            self.sorted = true;
        }
    }

    /// Retrieves the value at the specified index.
    ///
    /// # Parameters
    /// * `index` - The index of the value to retrieve
    ///
    /// # Panics
    /// Panics if the index is out of bounds.
    fn get_value_at(&self, index: usize) -> &T {
        &self.values[index]
    }

    /// Splits this bucket at its median value, returning a new bucket containing the upper half.
    ///
    /// This method uses the `select_nth_unstable` algorithm to efficiently find the median
    /// without fully sorting the bucket. After splitting, both this bucket and the new bucket
    /// are marked as unsorted.
    ///
    /// The split is done at the middle index, so if the bucket has an odd number of elements,
    /// the new bucket will have one fewer element than this bucket.
    ///
    /// # Returns
    /// A new bucket containing the upper half of the values from this bucket.
    fn split_at_median(&mut self) -> Bucket<T> {
        // Use select_nth_unstable to partition around the middle element to split the bucket in half
        let mid_idx = self.values.len() / 2;
        self.values.select_nth_unstable(mid_idx);

        // Get the pivot value (the element at the middle position)
        let pivot_value = self.values[mid_idx].clone();

        // Split at the pivot position
        let upper_values = self.values.split_off(mid_idx);

        // Mark this bucket as unsorted
        self.sorted = false;

        // Create and return the new bucket
        Bucket {
            min_value: pivot_value,
            values: upper_values,
            sorted: false,
        }
    }
}

/// A data structure for efficiently tracking percentiles of a stream of values.
///
/// PercentileTracker maintains a collection of buckets that partition the data space,
/// allowing it to compute percentiles without keeping the entire dataset sorted.
/// The key insight is that we only need to sort the bucket that contains the percentile
/// we're interested in, rather than sorting the entire dataset.
///
/// The tracker maintains indices to efficiently locate the bucket containing the specified percentile,
/// which allows for O(1) insertion and retrieval in the common case.
///
/// When buckets grow too large, they are split to maintain performance characteristics.
pub struct PercentileTracker<T>
where
    T: Clone + Ord,
{
    /// Collection of buckets that store the values in partitioned ranges.
    buckets: RefCell<Vec<Bucket<T>>>,

    /// Total number of values inserted into the tracker.
    total_count: usize,

    /// Index of the bucket that currently contains the percentile value.
    percentile_bucket_idx: RefCell<usize>,

    /// Number of values in all buckets before the percentile bucket.
    /// This is used to calculate the offset into the percentile bucket.
    percentile_bucket_offset: RefCell<usize>,

    /// The percentile to track (0-100)
    percentile: usize,

    /// Flag to track if rebalancing is needed
    needs_rebalancing: RefCell<bool>,
}

impl<T> PercentileTracker<T>
where
    T: Clone + Ord,
{
    /// Creates a new, empty PercentileTracker.
    ///
    /// The tracker is initialized with no buckets and all counters set to zero.
    ///
    /// # Parameters
    /// * `percentile` - The percentile to track (0-100)
    pub fn new(percentile: usize) -> Self {
        if !(1..=99).contains(&percentile) {
            panic!(
                "Percentile must be between 1 and 99 inclusive, got {}",
                percentile
            );
        }
        PercentileTracker {
            buckets: RefCell::new(Vec::new()),
            total_count: 0,
            percentile_bucket_idx: RefCell::new(0),
            percentile_bucket_offset: RefCell::new(0),
            percentile,
            needs_rebalancing: RefCell::new(false),
        }
    }

    /// Inserts a new value into the tracker.
    ///
    /// This method only handles the insertion of the value into the appropriate bucket
    /// without rebalancing or sorting. Rebalancing will happen lazily when get_percentile is called.
    ///
    /// # Parameters
    /// * `num` - The value to insert
    ///
    /// # Edge Cases
    /// - If this is the first value inserted, it becomes the target percentile
    pub fn insert(&mut self, num: T) {
        let mut buckets = self.buckets.borrow_mut();
        if buckets.is_empty() {
            buckets.push(Bucket::new(num));
            self.total_count += 1;
            return;
        }

        let bucket_idx = match buckets.binary_search_by(|bucket| bucket.min().cmp(&num)) {
            Ok(idx) => idx,
            Err(idx) => idx,
        };
        self.total_count += 1;

        // Handle insertion
        let inserted_into;
        if bucket_idx >= buckets.len() {
            if let Some(last_bucket) = buckets.last_mut() {
                last_bucket.push(num);
                inserted_into = buckets.len() - 1;
            } else {
                buckets.push(Bucket::new(num));
                inserted_into = 0;
            }
        } else if bucket_idx == 0 && buckets[bucket_idx].min() > &num {
            // Lower than the first bucket, so we need to add to the first bucket and update the min value
            inserted_into = 0;
            buckets[inserted_into].push(num.clone());
            buckets[inserted_into].update_min_value(num);
        } else if &num == buckets[bucket_idx].min() {
            inserted_into = bucket_idx;
            buckets[inserted_into].push(num);
        } else if bucket_idx == 0 {
            // This scenario should be captured by the above conditions
            panic!();
        } else {
            inserted_into = bucket_idx - 1;
            buckets[inserted_into].push(num);
        }

        let current_percentile_bucket_idx = *self.percentile_bucket_idx.borrow();
        if inserted_into < current_percentile_bucket_idx {
            *self.percentile_bucket_offset.borrow_mut() += 1;
        }

        // Mark that rebalancing is needed
        *self.needs_rebalancing.borrow_mut() = true;
    }

    /// Performs all necessary rebalancing operations to ensure the percentile can be computed correctly.
    ///
    /// This method:
    /// 1. Updates the indices to point to the bucket containing the target percentile
    /// 2. Splits buckets that have grown too large
    /// 3. Ensures the bucket containing the percentile is sorted
    ///
    /// This is called lazily by get_percentile() when needed.
    fn rebalance(&self) {
        if !*self.needs_rebalancing.borrow() {
            return;
        }

        let mut buckets = self.buckets.borrow_mut();

        // Update indices to point to new percentile position
        let target_pos = self.get_target_pos();
        let mut percentile_bucket_idx = *self.percentile_bucket_idx.borrow();
        let mut percentile_bucket_offset = *self.percentile_bucket_offset.borrow();

        if target_pos >= percentile_bucket_offset {
            let mut offset_into_bucket = target_pos - percentile_bucket_offset;
            while offset_into_bucket >= buckets[percentile_bucket_idx].len() {
                percentile_bucket_offset += buckets[percentile_bucket_idx].len();
                percentile_bucket_idx += 1;
                offset_into_bucket = target_pos - percentile_bucket_offset;
            }
        } else {
            while target_pos < percentile_bucket_offset {
                percentile_bucket_idx -= 1;
                percentile_bucket_offset -= buckets[percentile_bucket_idx].len();
            }
        }

        // Store updated indices
        *self.percentile_bucket_idx.borrow_mut() = percentile_bucket_idx;
        *self.percentile_bucket_offset.borrow_mut() = percentile_bucket_offset;

        // Handle bucket splitting if necessary
        while buckets[percentile_bucket_idx].len() > MAX_BUCKET_SIZE {
            // Split the bucket
            let new_bucket = buckets[percentile_bucket_idx].split_at_median();
            buckets.insert(percentile_bucket_idx + 1, new_bucket);

            // Update indices after split if needed
            let target_pos = self.get_target_pos();
            let offset_into_bucket = target_pos - percentile_bucket_offset;
            if offset_into_bucket >= buckets[percentile_bucket_idx].len() {
                percentile_bucket_offset += buckets[percentile_bucket_idx].len();
                percentile_bucket_idx += 1;

                // Update stored indices
                *self.percentile_bucket_idx.borrow_mut() = percentile_bucket_idx;
                *self.percentile_bucket_offset.borrow_mut() = percentile_bucket_offset;
            }
        }

        // Ensure the critical bucket is sorted
        buckets[percentile_bucket_idx].ensure_sorted();

        // Mark rebalancing as complete
        *self.needs_rebalancing.borrow_mut() = false;
    }

    /// Calculates the position of the target percentile in the overall dataset.
    ///
    /// This method computes the array index that would correspond to the target percentile
    /// if all values were stored in a single sorted array.
    ///
    /// # Returns
    /// The zero-based index of the target percentile value
    fn get_target_pos(&self) -> usize {
        (self.percentile * self.total_count) / 100
    }

    /// Retrieves the current target percentile value.
    ///
    /// This method calculates the position of the target percentile within the overall dataset,
    /// determines which bucket contains that position, and returns the value at the appropriate
    /// offset within that bucket.
    ///
    /// Before returning the value, it ensures that all necessary rebalancing operations
    /// have been performed.
    ///
    /// # Returns
    /// The value at the target percentile position
    pub fn get_percentile(&self) -> T
    where
        T: Clone,
    {
        // First ensure proper rebalancing
        self.rebalance();

        let target_pos = self.get_target_pos();
        let percentile_bucket_idx = *self.percentile_bucket_idx.borrow();
        let percentile_bucket_offset = *self.percentile_bucket_offset.borrow();
        let offset_into_bucket = target_pos - percentile_bucket_offset;

        self.buckets.borrow()[percentile_bucket_idx]
            .get_value_at(offset_into_bucket)
            .clone()
    }

    /// Prints debug statistics about the current state of the tracker.
    ///
    /// This method outputs information including:
    /// - Total count of values inserted
    /// - Current percentile bucket index
    /// - Current percentile bucket offset
    /// - Current target percentile value
    /// - Number of buckets
    /// - Whether the bucket offset is correct
    ///
    /// This is useful for debugging and performance analysis.
    #[allow(dead_code)]
    pub fn print_stats(&self)
    where
        T: Clone + std::fmt::Display,
    {
        // Ensure rebalancing is done before printing stats
        self.rebalance();

        eprintln!("Total count: {}", self.total_count);
        eprintln!("Percentile tracked: {}", self.percentile);
        eprintln!(
            "Percentile bucket idx: {}",
            *self.percentile_bucket_idx.borrow()
        );
        eprintln!(
            "Percentile bucket offset: {}",
            *self.percentile_bucket_offset.borrow()
        );
        eprintln!("Percentile: {}", self.get_percentile());
        eprintln!("Buckets: {:?}", self.buckets.borrow().len());
        if self.verify_bucket_offset() {
            eprintln!("Bucket offset is correct");
        } else {
            eprintln!("Bucket offset is incorrect");
        }
    }

    /// Verify that the bucket offset is correct
    ///
    /// This is a debug function that verifies that the bucket offset is correct.
    /// It is used to verify the correctness of the implementation.
    ///
    /// Because of how the buckets are chunked, this is effectively O(1). Even with a tiny MAX_BUCKET_SIZE
    /// And a huge input, there are only a couple hundred buckets.
    #[allow(dead_code)]
    pub fn verify_bucket_offset(&self) -> bool {
        // Ensure rebalancing is done before verification
        self.rebalance();

        let sum: usize = self
            .buckets
            .borrow()
            .iter()
            .take(*self.percentile_bucket_idx.borrow())
            .map(|bucket| bucket.len())
            .sum();
        sum == *self.percentile_bucket_offset.borrow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Debug;

    /// Helper function to calculate the target percentile from a sorted vector.
    ///
    /// This is used in tests to verify that the PercentileTracker is calculating
    /// the correct target percentile.
    ///
    /// # Parameters
    /// * `values` - A sorted slice of values
    /// * `percentile` - The percentile to calculate (0-100)
    ///
    /// # Returns
    /// The value at the target percentile position
    fn calculate_percentile<T: Clone>(values: &[T], percentile: usize) -> T {
        let index = (values.len() * percentile) / 100;
        values[index].clone()
    }

    /// Test helper that inserts a sequence of values into a PercentileTracker and
    /// verifies that the calculated percentile matches the expected value at each step.
    ///
    /// # Parameters
    /// * `values` - A slice of values to insert
    /// * `percentile` - The percentile to track (0-100)
    fn insert_and_verify<T>(values: &[T], percentile: usize)
    where
        T: Clone + Ord + Debug + PartialEq,
    {
        let mut tracker = PercentileTracker::new(percentile);
        let mut test_values = Vec::new();
        for value in values {
            tracker.insert(value.clone());
            test_values.push(value.clone());
            test_values.sort_unstable();
            let expected = calculate_percentile(&test_values, percentile);
            assert_eq!(
                tracker.get_percentile(),
                expected,
                "Failed at insertion {:?}",
                value
            );
        }
    }

    /// Test helper that inserts a sequence of values into a PercentileTracker and
    /// verifies that the calculated percentile matches the expected value after all insertions.
    ///
    /// This is used to test the lazy rebalancing, that it will work even after a large number of insertions.
    ///
    /// # Parameters
    /// * `values` - A slice of values to insert
    /// * `percentile` - The percentile to track (0-100)
    fn insert_all_and_verify<T>(values: &[T], percentile: usize)
    where
        T: Clone + Ord + Debug + PartialEq,
    {
        let mut tracker = PercentileTracker::new(percentile);
        let mut test_values = Vec::new();
        for value in values {
            tracker.insert(value.clone());
            test_values.push(value.clone());
        }
        test_values.sort_unstable();
        let expected = calculate_percentile(&test_values, percentile);
        assert_eq!(
            tracker.get_percentile(),
            expected,
            "Failed to insert all values"
        );
    }

    /// This is the explicit case from the problem statement.
    #[test]
    fn test_explicit_case() {
        let values = [
            -37, -37, -5, -48, -15, 42, -3, -43, 7, 14, -41, 6, 29, -28, -8, -25, -21, 31, -3, -23,
        ];
        let expected = [
            -37, -37, -5, -5, -5, 42, 42, 42, 42, 42, 14, 14, 29, 29, 29, 29, 29, 31, 31, 31,
        ];
        let mut tracker = PercentileTracker::<i64>::new(90);
        for (i, value) in values.iter().enumerate() {
            tracker.insert(*value);
            let percentile = tracker.get_percentile();
            assert_eq!(
                percentile, expected[i],
                "Failed at insertion {}: {}",
                i, value
            );
        }
    }

    #[test]
    fn test_empty_tracker() {
        insert_and_verify(&[42], 90);
    }

    #[test]
    fn test_small_dataset() {
        insert_and_verify(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 90);
    }

    #[test]
    fn test_reverse_order() {
        insert_and_verify(&[10, 9, 8, 7, 6, 5, 4, 3, 2, 1], 90);
    }

    #[test]
    fn test_random_values() {
        insert_and_verify(&[42, 17, 99, 7, 23, 56, 31, 84, 12, 63, 5, 27, 91], 90);
    }

    #[test]
    fn test_duplicate_values() {
        insert_and_verify(&[5, 5, 10, 10, 15, 15, 20, 20, 25, 25], 90);
    }

    #[test]
    fn test_negative_values() {
        insert_and_verify(&[-10, -5, 0, 5, 10, -15, -20, 15, 20, -25], 90);
    }

    #[test]
    fn test_edge_cases() {
        // Test with i64 min and max values
        insert_and_verify(
            &[i64::MIN, i64::MAX, 0, 42, -42, i64::MIN + 1, i64::MAX - 1],
            90,
        );
    }

    #[test]
    fn test_large_dataset() {
        // Use a seed to make the test deterministic
        use rand::prelude::*;
        use rand_chacha::ChaCha8Rng;
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        let values = {
            let mut v = Vec::new();
            // Insert enough values to trigger bucket splitting
            for _ in 0..(MAX_BUCKET_SIZE * 4) {
                v.push(rng.random::<i64>());
            }
            v
        };

        // Test at multiple percentiles
        for percentile in [1, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 99] {
            insert_and_verify(&values, percentile);
        }
    }

    #[test]
    fn test_different_percentile() {
        // Test with 50th percentile (median)
        let values = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];
        let mut tracker = PercentileTracker::new(50);
        let mut test_values = Vec::new();
        for value in values {
            tracker.insert(value);
            test_values.push(value);
            test_values.sort_unstable();
            let expected = calculate_percentile(&test_values, 50);
            assert_eq!(
                tracker.get_percentile(),
                expected,
                "Failed at insertion {}",
                value
            );
        }
    }

    #[test]
    fn test_large_dataset_all_values() {
        // Use a seed to make the test deterministic
        use rand::prelude::*;
        use rand_chacha::ChaCha8Rng;
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        let values = {
            let mut v = Vec::new();
            // Insert enough values to trigger bucket splitting
            for _ in 0..(MAX_BUCKET_SIZE * 4) {
                v.push(rng.random::<i64>());
            }
            v
        };

        // Test at multiple percentiles
        for percentile in [1, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 99] {
            insert_all_and_verify(&values, percentile);
        }
    }

    // New test for using with different numeric types
    #[test]
    fn test_different_numeric_types() {
        // Test with u32
        insert_and_verify(&[1u32, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50], 90);

        // Note: Floating point types like f32 and f64 don't implement Ord in Rust
        // because of NaN values which break total ordering requirements.
        // For example, NaN != NaN and NaN is neither less than nor greater than any value.
        // To use with floating point, you would need a wrapper type with a custom Ord implementation.
    }
}
