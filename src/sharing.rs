use ark_bn254::Fr;
use ark_ff::{Field, UniformRand, One, Zero};
use rand::{thread_rng};

// shamir secret sharing reference: https://evervault.com/blog/shamir-secret-sharing
// polynomial interpolation reference: https://vitalik.eth.limo/general/2016/12/10/qap.html
// curve reference: https://docs.rs/ark-bn254/latest/ark_bn254/ (bn256)

/// generate t-degree polynomial f(x) with f(0) = secret
pub fn shamir_share(secret: Fr, t: usize, n: usize) -> Vec<(Fr, Fr)> {
    let mut rng = thread_rng();

    // 1. builds a random polynomial: f(x) = a_0 + a_1x + ... + a_tx^t

    // a_0 = secret
    let mut coefficients = vec![secret];

    // a_1, ..., a_t are random coefficients
    for _ in 0..t-1 {
        coefficients.push(Fr::rand(&mut rng))
    }


    // 2. evaluates the polynomial f(x) at x = 1, ..., n to generate n shares
    let mut shares = Vec::new();
    for i in 1..=n {

        // x_i
        let x = Fr::from(i as u64);

        // f(x_i)
        let mut fx = Fr::zero();
        for (j, coef) in coefficients.iter().enumerate() {
            fx += *coef * x.pow([j as u64]);
        }
        // each share is a point (x_i, f(x_i))
        shares.push((x, fx)); 
    }
    shares
}

/// lagrange interpolation at x=0
pub fn shamir_reconstruct(shares: &[(Fr, Fr)]) -> Fr {
    let mut secret = Fr::zero();

    for (i, &(xi, yi)) in shares.iter().enumerate() {

        let mut num = Fr::one();
        let mut den = Fr::one();

        // lagrange basis polynomial evaluated at 0: ℓ_i(0) = \prod_{j=1, j != i}^k x_j / (x_j - x_i)
        for (j, &(xj, _)) in shares.iter().enumerate() {
            if i != j {
                num *= xj;
                den *= xj - xi;
            }
        }
        let lagrange_basis_polynomial = num * den.inverse().unwrap();

        // secret: f(0) = \sum y_i * ℓ_i(0)
        secret += yi * lagrange_basis_polynomial;
    }
    secret
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_bn254::Fr;
    use ark_ff::UniformRand;

    #[test]
    fn test_shamir_reconstruction_correctness() {
        let secret = Fr::rand(&mut rand::thread_rng());
        let shares = shamir_share(secret, 3, 5);
        let recovered = shamir_reconstruct(&shares[..3]); 
        assert_eq!(secret, recovered);
    }

    #[test]
    fn test_reconstruction_with_all_shares() {
        let secret = Fr::rand(&mut rand::thread_rng());
        let shares = shamir_share(secret, 3, 7);
        let recovered = shamir_reconstruct(&shares);
        assert_eq!(secret, recovered);
    }

    #[test]
    fn test_reconstruction_fails_with_too_few_shares() {
        let secret = Fr::rand(&mut rand::thread_rng());
        let shares = shamir_share(secret, 3, 5);
        // note: using 2 < t+1 shares will still return a value
        // but it should not be equal to the original secret (in general)
        let recovered = shamir_reconstruct(&shares[..2]);
        assert_ne!(secret, recovered); // not guaranteed but likely
    }
}

