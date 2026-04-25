# Presets

Packaged fan presets are read-only planning data in the current scaffold. They document the intended 10-point fan curve shape, are validated in CI, and can be previewed through dry-run planning, but no daemon method applies them yet.

Schema:

- `schema_version = 1`
- `id` must match the file name without `.toml`
- `label`, `description`, and `safety_note` are required strings
- `target_profiles` lists platform profile names the preset is intended for
- `[[points]]` must contain exactly 10 ascending `temperature_c` values
- `pwm` values must be integers from 0 to 255 and non-decreasing

These presets are not hardware promises. Dry-run planning validates the packaged shape and detected fan curve capability before previewing a future write. Actual write support must still clamp values, write full curves only, read back results, and keep rollback state.
