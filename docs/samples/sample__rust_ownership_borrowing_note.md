# Rust Ownership and Borrowing

## Ownership Rules

1. Every value has exactly one owner.
2. When the owner goes out of scope, the value is dropped (`Drop` trait).
3. Ownership can be **moved** (transferred) or **cloned** (deep copy).

Move semantics are the default for heap-allocated types (`String`, `Vec<T>`, `Box<T>`). Stack-only types implementing `Copy` (integers, `bool`, `f64`, tuples of `Copy` types) are implicitly copied on assignment.

## Borrowing

References borrow a value without taking ownership. The borrow checker enforces at compile time:

- **Shared references** (`&T`): any number allowed simultaneously. Data is read-only through `&T`.
- **Mutable references** (`&mut T`): exactly one allowed, and no `&T` may coexist. This prevents data races at compile time.

Lifetimes annotate how long a reference is valid. The compiler infers most lifetimes via elision rules; explicit annotations (`'a`) are required when the compiler cannot determine the relationship between input and output lifetimes.

## Interior Mutability

When shared references need mutation, Rust provides runtime-checked wrappers:

| Type         | Check            | Thread-safe | Use case                          |
| ------------ | ---------------- | ----------- | --------------------------------- |
| `Cell<T>`    | None (Copy only) | No          | Single-threaded, small Copy types |
| `RefCell<T>` | Runtime borrow   | No          | Single-threaded, complex types    |
| `Mutex<T>`   | Lock             | Yes         | Multi-threaded shared state       |
| `RwLock<T>`  | Read/write lock  | Yes         | Multi-reader, single-writer       |

`RefCell` panics on double mutable borrow at runtime. `Mutex` blocks the thread until the lock is acquired.

## Common Patterns

- **RAII (Resource Acquisition Is Initialization)**: destructors run deterministically at scope exit. File handles, locks, and network connections clean up automatically.
- **Newtype pattern**: wrap a type in a single-field struct to add type safety without runtime cost. `struct UserId(u64)` prevents mixing user IDs with other `u64` values.
- **Builder pattern**: chain method calls that consume and return `self` (or `&mut self`) to construct complex objects incrementally.

## Example

```rust
fn append_label(label: &str, values: &mut Vec<String>) {
    values.push(format!("item:{label}"));
}

fn longest<'a>(left: &'a str, right: &'a str) -> &'a str {
    if left.len() >= right.len() {
        left
    } else {
        right
    }
}

fn main() {
    let title = String::from("AudioX");
    let borrowed = &title;

    let mut tags = vec![String::from("rust")];
    append_label(borrowed, &mut tags);

    let winner = longest(&tags[0], &tags[1]);
    println!("{winner}");
}
```
