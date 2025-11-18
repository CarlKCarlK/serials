# Coding Notes for Agents

- When loading data from flash (or any other storage) into a local variable, name the variable after the concrete type. Example: `DeviceConfig` data should live in variables like `device_config` and partitions like `device_config_flash`, not generic `config` or `flash0`.
- Avoid introducing `unsafe` blocks. If a change truly requires `unsafe`, call it out explicitly and explain the justification so the user can review it carefully.

## Variable Naming Conventions

Avoid single-character variables; use descriptive names:

- ❌ `i`, `j`, `x`, `y`, `a`, `b`
- ✅ `read_index`, `write_index`, `first_pixel`, `second_pixel`

Project patterns:

- `x_goal`/`y_goal`: Target image dimensions
- `x_stride`/`y_stride`: Sampling rates (must be `PowerOfTwo`)
- `step_index`: Current machine step number
- `tape_index`: Current head position (can be negative)
- `select`: Which symbol to visualize (`NonZeroU8`)

## Comment Conventions

Use `cmk00`/`cmk0` prefix for TODO items (author's initials + priority):

```rust
// cmk00 high priority task
// cmk0 lower priority consideration
// TODO standard todo for general items
```

Preserving comments: When changing code, generally don't remove TODO's and cmk's in comments. Just move the comments if needed. If you think they no longer apply, add `(may no longer apply)` to the comment rather than deleting it.

- **Commit messages**: Always suggest a concise 1-2 line commit message when completing work (no bullet points, just 1-2 lines maximum).
- Preserve comments: keep `cmk00`/`cmk0`/`TODO` comments. If they seem obsolete, append `(may no longer apply)` rather than deleting.

- **Markdown formatting**: When creating or editing markdown files, follow these rules to avoid linter warnings:
  - Add blank lines before and after lists (both bulleted and numbered)
  - Add blank lines before and after code blocks (fenced with triple backticks)
  - Add blank lines before and after headings
  - Ensure consistent list marker style within a file
  - Example violations to avoid:
    - `**Title:**` followed immediately by a list (needs blank line)
    - Code block followed immediately by text (needs blank line)
    - Heading followed immediately by another heading (needs blank line or text between)

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

## Static Device Pattern

Many drivers expose a `new_static` constructor for resources plus a `new` constructor for the runtime handle. We call this the **Static Device Pattern** and use it consistently across the repo.

- Always declare the static resources with `Type::new_static()` and name them `FOO_STATIC` when global.
- When implementing or calling `Type::new`, pass `&TypeStatic` (or equivalent) as the **first** argument.
- If `Spawner` is needed, place it as the **final** argument so everything else reads naturally between those bookends.

Example:

```rust
static LED4_STATIC: Led4Static = Led4::new_static();
let led4 = Led4::new(&LED4_STATIC, cells, segments, spawner)?;
```
