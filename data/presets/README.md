# Presets

Packaged fan presets are inert data in the current read-only scaffold. They document the intended 10-point fan curve shape and are validated in CI, but no daemon method applies them yet.

Schema:

- `schema_version = 1`
- `id` must match the file name without `.toml`
- `label`, `description`, and `safety_note` are required strings
- `target_profiles` lists platform profile names the preset is intended for
- `[[points]]` must contain exactly 10 ascending `temperature_c` values
- `pwm` values must be integers from 0 to 255 and non-decreasing

These presets are not hardware promises. Future write support must still validate detected fan curve capabilities, clamp values, write full curves only, read back results, and keep rollback state.
