# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Changed

- Update `prometheus` requirement to 0.11.
- Add `version=0.0.4` parameter to `Content-Type` header when returning metrics at the /metrics URL, as specified by the Prometheus [Exposition Formats specification](https://prometheus.io/docs/instrumenting/exposition_formats/#text-based-format).
- Use `Duration::as_secs_f64` instead of manually calculating nanoseconds when calculating request durations. This bumps the minimum supported Rust version to 1.38.0, which is unlikely to be a problem in practice, since Rocket still requires a nightly version of Rust.
- Impl `From<PrometheusMetrics> for Vec<Route>` instead of `Into<Vec<Route>> for PrometheusMetrics`, since the former gives us the later for free.

## [0.7.0] - 2020-06-19
### Changed

- Update `prometheus` requirement to 0.10.

## [0.6.0] - 2020-06-19
### Changed

- Don't require the default features of the `prometheus` or `rocket` dependencies. This should improve compile times for crates which also don't require those features. This is a breaking change since we re-export `prometheus` (which will now have a reduced public API) but is unlikely to affect many users, since the Protobuf format has not been supported by Prometheus for some time.

## [0.5.0] - 2020-05-18
### Changed

- Update `prometheus` requirement to 0.9.

## [0.4.0] - 2020-03-02
### Changed

- Update `prometheus` requirement to 0.8.

## [0.3.2] - 2020-01-02
### Changed

- Use `Instant` instead of `SystemTime` to track request durations. This should be more accurate and is infallible.

## [0.3.1] - 2020-01-02
### Added

- Add `PrometheusMetrics::with_registry` associated function to allow metrics to be tracked in a custom registry.

## [0.3.0] - 2019-06-25
### Changed

- Update `prometheus` requirement to 0.7.

## [0.2.0] - 2019-05-08
### Changed

- Re-export `prometheus` library. This re-export is the recommended way of accessing the `prometheus` crate to avoid any dependency version mismatches.
- Update `prometheus` requirement to 0.6.

## [0.1.1] - 2019-04-15

- First version of the crate released to crates.io.

[Unreleased]: https://github.com/sd2k/rocket_prometheus/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/sd2k/rocket_prometheus/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/sd2k/rocket_prometheus/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/sd2k/rocket_prometheus/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/sd2k/rocket_prometheus/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/sd2k/rocket_prometheus/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/sd2k/rocket_prometheus/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/sd2k/rocket_prometheus/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/sd2k/rocket_prometheus/compare/v0.1.1...v0.2.0
[0.2.1]: https://github.com/sd2k/rocket_prometheus/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/sd2k/rocket_prometheus/releases/tag/v0.2.0
