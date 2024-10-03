// SPDX-LICENSE-IDENTIFIER: GPL-3.0-or-later

use rug::Float;

/// Configuration structure for [`fn@large_beam_absorbing_layer`]
//TODO: document these parameters better
#[derive(Clone, PartialEq)]
pub struct LargeBeamAbsorbingLayerConfig<'a> {
    /// Units: cm^-1
    pub mu_a: &'a Float,

    /// Units: g*cm^3
    pub rho: &'a Float,

    /// Units: J*g^-1*K^-1
    pub c: &'a Float,

    /// Units: W*cm^-1*K^-1
    pub k: &'a Float,

    /// Units: cm
    pub d: &'a Float,

    /// Units: cm
    pub z0: &'a Float,

    /// Units: W*cm^-2
    pub e0: &'a Float,

    /// Precision (in bits) of the arbitrary-precision floating point values
    /// used in intermediate calculations (and in the output)
    pub precision: u64,
}

pub fn large_beam_absorbing_layer(
    LargeBeamAbsorbingLayerConfig {
        mu_a,
        rho,
        c,
        k,
        d,
        z0,
        e0,
        precision,
    }: LargeBeamAbsorbingLayerConfig,
    z: &Float,
    tp: &Float,
) -> Float {
    //TODO: make this less naive

    let mut alpha = Float::with_val_64(precision, k);
    alpha /= rho;
    alpha /= c;

    let mut term_1 = Float::with_val_64(precision, mu_a);
    term_1 *= e0;
    term_1 /= rho;
    term_1 /= c;
    term_1 /= 2.0;

    let mut term_2 = Float::with_val_64(precision, z);
    term_2 -= z0;
    term_2 *= mu_a;
    term_2 *= -1;
    term_2.exp_mut();

    if *tp == 0 {
        return term_1 * term_2;
    }

    let mut term_3 = Float::with_val_64(precision, mu_a);
    term_3.square_mut();
    term_3 *= tp;
    term_3 *= &alpha;
    term_3.exp_mut();

    let mut reciprocal_sqrt = Float::with_val_64(precision, &alpha);
    reciprocal_sqrt *= tp;
    reciprocal_sqrt *= 4.0;
    reciprocal_sqrt.sqrt_mut();
    reciprocal_sqrt.recip_mut();

    let mut sqrt_mu_a = alpha;
    sqrt_mu_a *= tp;
    sqrt_mu_a.sqrt_mut();
    sqrt_mu_a *= mu_a;

    let mut argument_1 = Float::with_val_64(precision, z0);
    argument_1 += d;
    argument_1 -= z;
    argument_1 *= &reciprocal_sqrt;
    argument_1 += &sqrt_mu_a;
    argument_1.erf_mut();

    let mut argument_2 = Float::with_val_64(precision, z0);
    argument_2 -= z;
    argument_2 *= &reciprocal_sqrt;
    argument_2 += &sqrt_mu_a;
    argument_2.erf_mut();

    let mut term_4 = argument_1;
    term_4 -= argument_2;

    term_1 * term_2 * term_3 * term_4
}

#[cfg(test)]
mod tests {
    use super::*;

    use rug::float::Special;

    #[ctor::ctor]
    static ZERO: Float = Float::with_val_64(64, Special::Zero);

    #[ctor::ctor]
    static ONE: Float = Float::with_val_64(64, 1.0);

    #[ctor::ctor]
    static EPSILON: Float = Float::with_val_64(64, 1e-10);

    #[test]
    fn large_beam_sanity() {
        let config = LargeBeamAbsorbingLayerConfig {
            mu_a: &ONE,
            rho: &ONE,
            c: &ONE,
            k: &ONE,
            d: &ONE,
            z0: &ZERO,
            e0: &ONE,
            precision: 256,
        };

        assert_eq!(
            large_beam_absorbing_layer(config.clone(), &ZERO, &ZERO),
            5e-1
        );

        let mut result = large_beam_absorbing_layer(config.clone(), &ONE, &ZERO);
        // reference result: 0.5 * e^-1
        result -= 1.83939720586e-1;
        result.abs_mut();
        assert!(result < *EPSILON);

        let mut result = large_beam_absorbing_layer(config.clone(), &ONE, &ONE);
        // reference result: 0.5 * e^-1 * e^1 * (erf(1) - erf(-1/sqrt(4) + 1))
        result -= 1.6110045756833416583e-1;
        result.abs_mut();
        println!("{}", result);
        assert!(result < *EPSILON);
    }
}
