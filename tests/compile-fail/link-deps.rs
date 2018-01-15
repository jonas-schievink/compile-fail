//! Compile-Fail tests can link against dependencies and dev-dependencies.

extern crate env_logger;
extern crate either;

fn main() {
    let () = 9;
    //~^ error: mismatched types
}
