use ark_bn254::Fr;

mod sharing;
use crate::sharing::{shamir_share, shamir_reconstruct};

fn main() {
    let secret = Fr::from(1234u64);
    let threshold = 2;
    let num_shares = 5;

    let shares = shamir_share(secret, threshold, num_shares);
    println!("Shares:");
    for (x, fx) in &shares {
        println!("x = {}, fx = {}", x, fx);
    }

    let recovered = shamir_reconstruct(&shares[0..=threshold]);
    println!("Recovered Secret: {}", recovered);
    assert_eq!(recovered, secret);
}
