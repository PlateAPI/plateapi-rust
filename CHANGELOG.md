# Changelog

## 0.2.0 -- 2026-10-18

Existing error variants keep their shapes; see "Upgrading from 0.1.0" in the README for the two
option structs that gained a field and the behaviour changes.

### Added

- `Vehicle.vehicle_id`: PlateAPI's stable ID for the exact variant, on the primary vehicle and
  on every entry in `alternatives`.
- `LookupResult.match_` (`exact`, `multiple`, `ambiguous`) and `LookupResult::candidate_ids()`.
- `PlateAPI::vehicle_by_id(id)` -> `VehicleRecord` for `GET /api/v1/vehicles/{vehicle_id}`.
- `VehiclesOptions.vehicle_type` (`"motorcycle"`) for the motorcycle cascade.
- `VehiclesResult.vehicles` and `VehiclesResult::vehicle_ids()` for the last cascade level.
- `PlateAPI::walk_vehicles(make, model, WalkOptions)` -> `VehicleWalker`: walks the cascade for
  one make and model (and its trims) and returns each `vehicle_id` once from `next().await`,
  with `year_from` / `year_to`, `engine_contains`, `per_year`, `known` (resume) and `delay_ms`.
- `RateLimit.topup_remaining` from the `X-Topup-Remaining` header.
- Error variants `BadRequest` (400), `PermissionDenied` (403) and `NotFound` (404), and the
  accessors `status_code()`, `retry_after()`, `code()`, `request_id()`, `is_rate_limited()`.
- `ClientOptions.max_wait_secs`: the longest `Retry-After` the SDK will sleep (default 60 s).
- `plateapi::VERSION` and `User-Agent: plateapi-rust/<version>`.
- Offline test suite (`cargo test`); live checks behind `--ignored`.

### Fixed

- A monthly-quota 429 returns `QuotaExceeded` at once instead of sleeping for an hour and
  retrying.
- A `Retry-After` longer than `max_wait_secs` returns the error instead of sleeping.
- `health()` makes a single attempt.
- A lookup that returns `code: "internal_error"` (not billed) is retried before it is returned.
- `request_id` falls back to the `X-Request-ID` header.

### Changed

- `VehiclesOptions` and `ClientOptions` gained one field each (use `..Default::default()`).
- 400, 403 and 404 map to their own variants instead of `Api`.

## 0.1.0 -- 2026-08-16

- First release: `lookup`, `vehicles`, `usage`, `logs`, `health`, retries with backoff.
