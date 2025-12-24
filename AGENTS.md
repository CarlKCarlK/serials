# Coding Notes for Agents

- When loading data from flash (or any other storage) into a local variable, name the variable after the concrete type. Example: `DeviceConfig` data should live in variables like `device_config` and partitions like `device_config_flash`, not generic `config` or `flash0`.
- Avoid introducing `unsafe` blocks. If a change truly requires `unsafe`, call it out explicitly and explain the justification so the user can review it carefully.
- Avoid silent clamping; prefer asserts or typed ranges so out-of-range inputs fail fast.
- Prefer `no_run` doctests; use `ignore` only when absolutely necessary (and call out why).
- When adding docs for modules or public items, link readers to the primary struct and keep the single compilable example on that struct; other items should point back to it rather than duplicating examples.
- I often like `const` values defined in the local context (inside the function/example) rather than at module scope when they’re only used there.
- Always run `cargo check-all` before handing work back; xtask keeps doctests and examples in sync.

## Module Structure Convention

This project uses a specific module structure pattern. Do NOT create `mod.rs` files.

Correct pattern:

- `src/foo.rs` or `examples/foo.rs` (main module file)
- `src/foo/bar.rs` (submodule)
- `src/foo/baz.rs` (another submodule)

Incorrect pattern (never use):

- `src/foo/mod.rs` ❌
- `examples/foo/mod.rs` ❌

Example:

```rust
// File: src/wifi_auto.rs (main module)
pub mod fields;
pub mod portal;

// File: src/wifi_auto/fields.rs (submodule)
// File: src/wifi_auto/portal.rs (submodule)
```

## Variable Naming Conventions

Variables should generally match their type names converted to snake_case. This improves predictability and encourages better type names.

**Type-based naming:**

- `Led12x4` → `led_12x4` (preserves numeric separators)
- `WifiAuto` → `wifi_auto`
- `LedStrip` → `led_strip`
- `Led12x4ClockDisplay` → `led_12x4_clock_display`

**When to deviate:**

- Generic/contextual names are acceptable when the type is obvious and verbose naming would be redundant:
  - ✅ `button` (not `button_pico2`) when only one button exists
  - ✅ `clock` (not `clock_0`) when context is clear
  - ✅ `spawner` (not `embassy_spawner`) - universally understood

**Single-character variables:**

Avoid single-character variables; use descriptive names:

- ❌ `i`, `j`, `x`, `y`, `a`, `b`
- ✅ `read_index`, `write_index`, `first_pixel`, `second_pixel`

**Project-specific patterns:**

- `x_goal`/`y_goal`: Target image dimensions
- `x_stride`/`y_stride`: Sampling rates (must be `PowerOfTwo`)
- `step_index`: Current machine step number
- `tape_index`: Current head position (can be negative)
- `select`: Which symbol to visualize (`NonZeroU8`)
- For the board peripherals handle from `embassy_rp::init`, always use the shorthand `let p = embassy_rp::init(...)` so examples stay consistent.

**Reference variables:**

When capturing variables in closures or creating references, append `_ref`:

- `led_12x4` → `led_12x4_ref`
- `wifi_auto` → `wifi_auto_ref`

## Comment Conventions

Use `cmk00`/`cmk0` prefix for TODO items (author's initials + priority):

```rust
// cmk00 high priority task
// cmk0 lower priority consideration
// TODO standard todo for general items
```

Preserving comments: When changing code, generally don't remove TODO's and cmk's in comments. Just move the comments if needed. If you think they no longer apply, add `(may no longer apply)` to the comment rather than deleting it.

- **Debug code policy**: Do not remove debug/test code until the bug is proven fixed. Leave diagnostic code in place even after identifying issues so we can verify fixes work correctly.
- **Commit messages**: Always suggest a concise 1-2 line commit message when completing work (no bullet points, just 1-2 lines maximum).
- Preserve comments: keep `cmk00`/`cmk0`/`TODO` comments. If they seem obsolete, append `(may no longer apply)` rather than deleting.

## Documentation Conventions

- Start module docs with "A device abstraction ..." and have them point readers to the main struct docs.
- Put a single compilable example on the primary struct; other public docs should link back to that example instead of duplicating snippets.

- **Markdown formatting**: When creating or editing markdown files, follow these rules to avoid linter warnings:
  - Add blank lines before and after lists (both bulleted and numbered)
- Add blank lines before and after code blocks (fenced with triple backticks)
- Add blank lines before and after headings
- Ensure consistent list marker style within a file
- Example violations to avoid:
  - `**Title:**` followed immediately by a list (needs blank line)
  - Code block followed immediately by text (needs blank line)
  - Heading followed immediately by another heading (needs blank line or text between)

When adding new examples, also add the standard cargo aliases (run + check for pico1 and pico2 variants) in `.cargo/config.toml` so they stay discoverable.

### Documentation Spec (for device modules)

- Module-level docs must start with "A device abstraction ..." and immediately direct readers to the primary public struct for details.
- Each module should have exactly one full, compilable example placed on the primary struct; keep other docs free of extra examples.
- Other public items (constructors, helper methods, type aliases) should point back to the primary struct's example rather than adding new snippets.
- **API completeness**: Every public method must either (1) have its own doc test, OR (2) be used in the struct's main example AND have a link from its doc comment pointing to that example (e.g., `See the [struct-level example](Self) for usage.`). This ensures all functionality is documented and discoverable.
- Examples should use the module's real constructors (e.g., `new_static`, `new`) and follow the device/static pair pattern shown elsewhere in the repo.
- Avoid unnecessary public type aliases; prefer private or newtype wrappers when exposing resources so internal types stay hidden.
- In examples, prefer importing the types you need (`use crate::foo::{Device, DeviceStatic};`) instead of fully-qualified paths for statics.
- Keep example shape consistent: show an async function that receives `Peripherals`/`Spawner` (or other handles) and constructs the device with `new_static`/`new`; avoid mixing inline examples without that pattern next to function-based ones.
- Examples must show the actual `use` statements for the module being documented (bring types into scope explicitly rather than relying on hidden imports).
- In examples, keep `use` statements limited to `device_kit::...` items; refer to other crates/modules with fully qualified paths inline.

### Precision Over Future‑Proofing

- Prefer precise code that encodes current assumptions with `assert!`s and fails fast when violated.
- Do not write code that is “resilient to possible future changes” at the expense of clarity; instead, express today’s preconditions explicitly and let assertions catch regressions if behavior changes later.
- When control flow has expected invariants (e.g., counters must be equal, ranges nonnegative), use `assert!` rather than saturating math or silent fallbacks.
- If a `match` requires a catch‑all only for type completeness, use `unreachable!()`/`panic!()` rather than `_ => {}` to surface violations early.

- In Rust, generally custom Error enums should be named 'Error' rather than 'MyThingError'

- In Rust, move deconstruction into the arguments were possible.

- In Rust, I like using the same name when unwrapping, if let Some(max_steps) = max_steps {

- I like asserts and using asserts. So, if the difference between two values must always be nonnegative, I would NOT use saturating_sub, I would use assert!(a >= b); let diff = a - b; because I want to catch any violations. Likewise, if a match requires a catch all, I wold use unreachable or panic. I would not use_ => {}.

### Shadow Names (Rust)

Use shadowing to keep identifiers stable when unwrapping, narrowing types, or parsing values. This keeps code concise, avoids suffix noise (like `_opt`, `_u32`), and clearly communicates that the variable’s invariants/precision have been tightened at that point.

Patterns we prefer:

- Option unwrap (narrowing scope and invariants):

```rust
if let Some(max_steps) = max_steps {
    // use max_steps here (shadowed)
}
```

- Type narrowing with checked conversion (fail fast):

```rust
let count = u32::try_from(count).expect("count must fit in u32");
```

- Parsing into a stronger type:

```rust
let width = width.parse::<u32>()?;
```

Guidelines:

- Prefer shadowing at the smallest reasonable scope so the “new” meaning doesn’t leak too far.
- Use assertions or checked conversions before shadowing when truncation/overflow is possible.
- Don’t shadow across long spans if it could confuse readers—shadow near the point of use.

Spelling:

Use American over British spelling

When making up variable notes for examples and elsewhere, never use the prefix "My". I hate that.

Yes, in Rust the get_ prefix is generally discouraged for getters. The Rust API guidelines specifically recommend against it.

Rust convention:

Getters: offset_minutes(), text() (no prefix)
Setters: set_offset_minutes(), set_text() (with set_ prefix)

## Colors

For RGB colors, use the predefined constants from `smart_leds::colors` (re-exported from `led_strip::led_strip_shared::colors` and `led_strip::colors`) rather than creating RGB values manually:

✅ Good:

```rust
use device_kit::led_strip::led_strip_shared::colors;
let frame = [colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW];
```

❌ Bad:

```rust
use device_kit::led_strip::led_strip_shared::Rgb;
let red = Rgb::new(255, 0, 0);
let green = Rgb::new(0, 255, 0);
```

Common colors available: `RED`, `GREEN`, `BLUE`, `YELLOW`, `WHITE`, `BLACK`, `CYAN`, `MAGENTA`, `ORANGE`, `PURPLE`, etc.

## Device/Static Pair Pattern

Many drivers expose a `new_static` constructor for resources plus a `new` constructor for the runtime handle. We call this the **Device/Static Pair Pattern** and use it consistently across the repo.

- Always declare the static resources with `Type::new_static()` and name them `FOO_STATIC` when global.
- **Hardware singletons** (e.g., `WifiAuto` - one WiFi chip per device) hide the static inside `Type::new()` using a function-scoped static, so users never see `TypeStatic`.
- **Multi-instance devices** (e.g., `Led4` - can have multiple) require passing `&TypeStatic` as the **first** argument when implementing or calling `Type::new`, named `<type>_static` (e.g., `led4_static: &'static Led4Static`).
- If `Spawner` is needed, place it as the **final** argument so everything else reads naturally between those bookends.
- **Static placement**: Place the static constructor on the line directly before the struct constructor. Don't group all statics at the top and then all constructors below.

Examples:

Hardware singleton (static hidden inside `new()`):

```rust
// User code - no static needed!
let wifi_auto = WifiAuto::new(
    p.PIN_23,
    p.PIN_25,
    // ... more pins ...
    spawner,
)?;
```

Multi-instance device (static passed as first argument):

```rust
static LED4_STATIC: Led4Static = Led4::new_static();
let led4 = Led4::new(&LED4_STATIC, cells, segments, spawner)?;
```

Don't ignore errors by assigning results to an ignored variable. Don't do this:

```rust
let _ = something_that_returns_a_result()
```

## API Design Patterns

**Avoid the builder pattern.** Users find builder patterns hard to discover. Instead:

- Use direct constructors with named parameters
- Take slices instead of requiring users to construct collections
- Return arrays/fixed-size types when possible rather than requiring users to build them

❌ Bad (builder pattern):

```rust
let display = DisplayBuilder::new()
    .width(12)
    .height(4)
    .brightness(100)
    .build()?;
```

✅ Good (direct construction):

```rust
let display = Display::new(12, 4, brightness_percent(100))?;
```

❌ Bad (forcing users to build collections):

```rust
let mut frames = Vec::new();
frames.push(frame1);
frames.push(frame2);
led.animate_frames(frames);
```

✅ Good (accept slices):

```rust
let frames = [frame1, frame2];
led.animate(&frames);
```

## Async Coordination

**Never use delays/timers to "fix" async coordination issues.** Delays like `Timer::after(Duration::from_millis(1))` to "let something finish" are evil - they're unreliable, hide the real problem, and make code fragile.

If async operations need coordination:

- Use proper synchronization primitives (Signals, Channels, Mutexes)
- Make operations synchronous if they don't need to be async
- Restructure the design to avoid the race condition
- Use acknowledgment/completion signals

❌ Bad (hoping a delay is long enough):

```rust
send_command().await;
Timer::after(Duration::from_millis(1)).await; // Evil!
let result = read_state();
```

✅ Good (proper coordination):

```rust
send_command().await;
wait_for_completion().await;
let result = read_state();
```

or

```rust
// If read_state can be synchronous
let result = read_state_sync();
```

## Visibility and Documentation

When something shouldn't be in the public API docs, express that through visibility modifiers rather than doc attributes:

✅ Good:

```rust
pub(crate) struct InternalHelper { ... }  // Visible in crate, not in public docs
struct PrivateHelper { ... }              // Private, not in public docs
```

❌ Bad:

```rust
#[doc(hidden)]
pub struct InternalHelper { ... }  // Public but hidden - confusing!
```

If something truly shouldn't be in public docs, it shouldn't be `pub` either. Use `pub(crate)` for crate-internal APIs or omit `pub` entirely for private items. The `#[doc(hidden)]` attribute creates a mismatch between visibility and documentation that makes the API less clear.

**Exception: Macro helpers**

There is one legitimate use case for `#[doc(hidden)]` on `pub` items: functions called by public macros that expand at the call site. These must be `pub` (not `pub(crate)`) because macro-generated code in downstream crates needs to call them, but they're not part of the user-facing API.

```rust
#[doc(hidden)]
pub fn helper_for_macro() { ... }  // Called by macro expansion in user code
```

When using `#[doc(hidden)]` for this reason, always add a comment explaining why it must be public despite being an implementation detail.
