//! Compile-Fail tests can link against the containing crate.

extern crate compile_fail;

fn main() {
    let () = 0;
    //~^ error: mismatched types
}
