# Arcuru's Library

[![build](https://img.shields.io/github/actions/workflow/status/arcuru/lib/rust.yml?style=flat-square)](https://github.com/arcuru/lib/actions?query=workflow%3ANix)
[![coverage](https://img.shields.io/codecov/c/github/arcuru/lib)](https://codecov.io/gh/arcuru/lib)
![license](https://img.shields.io/github/license/arcuru/lib)

This repository contains a collection of libraries and tools that I wrote.

## PercentileTracker

`PercentileTracker` is an efficient data structure for tracking percentiles within a stream of numerical data. It generalizes the concept of finding a median from a data stream (like LeetCode's [Find Median from Data Stream](https://leetcode.com/problems/find-median-from-data-stream/)) to work with any percentile. Using a smart bucketing strategy with lazy sorting, it achieves O(1) amortized time complexity for both insertions and retrievals, resulting in an overall O(N) performance for processing N inputs. The implementation is highly efficient across various data distributions and scales well with large datasets (tested up to 100M elements). It's particularly valuable for streaming applications, monitoring systems, and analytics tools that need to continuously track percentiles of growing datasets.
