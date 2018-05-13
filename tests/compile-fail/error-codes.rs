//! You can match against numeric error codes instead of messages.

fn main() {
    let _r = {
        let local = 5;
        &local
    };
    //~^^ error[E0597]
    // does not live long enough
}
