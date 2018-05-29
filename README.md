# Ensure that unsafe code doesn't compile

(**Work in progress, don't use yet.**)

The `compile-fail` crate provides a mechanism to write tests that expect
compilation to fail with a specific error. This is useful for crates that use
lots of `unsafe` but try to provide a safe abstraction.

Currently, `compile-fail` requires **nightly Rust**, because Cargo's
[`--build-plan`](https://github.com/rust-lang/cargo/pull/5301) option is
unstable. It uses no other unstable features, so as soon as that's stabilized,
`compile-fail` can work on stable, too.

## What's the difference between this and `compiletest-rs`?

TL;DR: This is a more robust solution using Cargo, focused solely on
`compile-fail` tests, but requires nightly Rust.

The [compiletest-rs](https://github.com/laumann/compiletest-rs) crate provides
the same kind of compile-fail test, among other things.

It was originally written to test rustc itself and was later extracted as a
standalone crate. This means that it comes with a lot of baggage that isn't
needed by most users and was written with rustc's build system in mind. It also
tends to break when doing anything non-standard (like using dependencies in a
weird way, resulting in non-obivous errors like "multiple matching crates
found").

`compile-fail` incorporates a few useful parts of `compiletest` (like the
parsing of rustc errors), but is otherwise a complete overhaul. Instead of
generating the `rustc` invocation itself, it resorts to Cargo (since it really
ought to know this best!). All `compile-fail` tests are compiled in the same
manner as other tests, meaning that they can link against the main crate, its
dependencies, and all dev-dependencies without any weird hacks.

However, the Cargo integration required is only available on nightly right now,
so **`compile-fail` requires nightly Rust**, while compiletest-rs (finally!)
[works on stable Rust](https://github.com/laumann/compiletest-rs/pull/107).

Since `compile-fail` was written with simplicity in mind, it is extremely easy
to use: No confusing Cargo features you have to enable, no complex
configuration. Using `compile-fail` just takes 2 lines of code (see the example
below).

## Why should I write `compile-fail` tests?

If you're writing bindings to a native library, there's no borrow checker
preventing memory unsafety. *You* have to take its place by writing the correct
lifetimes. The only way to see if you did it right is to ensure that the
compiler rejects invalid uses of your proclaimed safe API.

Testing that something doesn't compile is also useful if you're using the type
system in a cool way (ie. to make illegal states irrepresentable). You can write
a `compile-fail` test to ensure that anything that *would* result in an illegal
state does not compile.


# Usage

Add this to your `Cargo.toml` (since `compile-fail` is only used by tests, it
can be a `dev-dependency`):

```toml
[dev-dependencies]
compile-fail = "0.1.0"
```

Create the test entry point in `tests/compile-fail.rs`:

```rust
#[macro_use] extern crate compile_fail;

run_compile_fail_tests!();
```

Create your `compile-fail` tests in `tests/compile-fail/`. An example can look
like this:

```rust
fn main() {
    let () = 9;     //~ error: mismatched types
}
```


# Syntax

In a `compile-fail` test, you need to define at least one *error pattern* that
specifies how the error you expect to see should look. An error pattern is a
comment starting with `//~`. The pattern has to mention the error message or the
error code emitted by rustc. The following 2 patterns should match the same
errors since `E0308` is the error code for mismatched types:

```rust
let () = 9;     //~ error: mismatched types
let () = 9;     //~ error[E0308]
```

The position of the pattern is also taken into account when matching errors,
since a matching error occurring at an unrelated place may mean something
completely different. By default, the error is expected on the same line where
the `//~` pattern is found. A pattern can point at the line above it by using
`//~^`:

```rust
let () = 9;
//~^ error: mismatched types
```

The `^` can be repeated to refer to lines higher up:

```rust
let () = 9
        + 10
        + 11;
//~^^^ error: mismatched types
// refers to the `let () = 9` line (3 lines up)
```
