//! Pattern exists and matches, but additional errors not caught by any pattern.

fn main() {
    let () = 0;  //~ error: mismatched types
    let () = 0;
}
