// SPDX-LICENSE-IDENTIFIER: GPL-3.0-or-later

use rug::{float::Special, Assign, Float};
use std::borrow::Cow;

use crate::{quadrature::Quadrature, utilities};

/// A configuration structure for specific thermal properties
#[derive(Clone, PartialEq, Debug)]
pub struct ThermalProperties<'a> {
    /// Units: g*cm^3
    pub rho: Cow<'a, Float>,

    /// Units: J*g^-1*K^-1
    pub c: Cow<'a, Float>,

    /// Units: W*cm^-1*K^-1
    pub k: Cow<'a, Float>,
}

/// A layer of tissue
#[derive(Clone, PartialEq, Debug)]
pub struct Layer<'a> {
    /// Units: cm
    pub d: Cow<'a, Float>,

    /// Units: cm
    pub z0: Cow<'a, Float>,

    /// Units: cm^-1
    pub mu_a: Cow<'a, Float>,

    /// Irradiance. Units: W*cm^-2
    pub e0: Cow<'a, Float>,
}

impl<'a> Layer<'a> {
    fn into_owned(self) -> Layer<'static> {
        Layer {
            d: Cow::Owned(self.d.into_owned()),
            z0: Cow::Owned(self.z0.into_owned()),
            mu_a: Cow::Owned(self.mu_a.into_owned()),
            e0: Cow::Owned(self.e0.into_owned()),
        }
    }
}

/// Multiple layers of tissue
#[derive(Clone, PartialEq, Debug)]
pub struct MultiLayer {
    /// The layers this [`struct@MultiLayer`] is composed of
    layers: Vec<Layer<'static>>,
}

impl MultiLayer {
    /// Creates a new [`struct@MultiLayer`] from multiple [`struct@Layer`]s
    ///
    /// If the input layers are not sorted in order of incidence, they are
    /// sorted. Irradiance is taken from the topmost layer and propagated
    /// downward according to Beer's Law
    ///
    /// If the input layers overlap in any way, [`None`] is returned
    pub fn new<'a>(input_layers: impl IntoIterator<Item = Layer<'a>>) -> Option<Self> {
        let input_layers = input_layers.into_iter();
        let mut layers = Vec::with_capacity(input_layers.size_hint().0);

        for layer in input_layers {
            layers.push(layer.into_owned());
        }

        layers.sort_by(|a, b| a.z0.total_cmp(b.z0.as_ref()));

        if let Some(layer) = layers.first() {
            let mut e0 = layer.e0.clone().into_owned();

            let mut z0 = layer.z0.clone().into_owned();
            z0 += layer.d.as_ref();

            let mut b = layer.d.clone().into_owned();
            b *= layer.mu_a.as_ref();
            b *= -1;
            b.exp_mut();
            e0 *= &b;

            for layer in layers.iter_mut().skip(1) {
                if layer.z0.as_ref() < &z0 {
                    return None;
                }

                layer.e0.to_mut().assign(&e0);

                z0.assign(layer.z0.as_ref());
                z0 += layer.d.as_ref();

                b.assign(layer.d.as_ref());
                b *= layer.mu_a.as_ref();
                b *= -1;
                b.exp_mut();
                e0 *= &b;
            }
        }

        Some(Self { layers })
    }

    //TODO: add a method for updating e0

    /// Runs the given [`trait@Beam`] over the contained [`struct@Layer`]s
    /// with the provided [`struct@ThermalProperties`]
    ///
    /// Not all implementations of [`trait@Beam`] will use all parameters
    pub fn evaluate_with(
        &self,
        precision: u64,
        beam: &impl Beam,
        thermal_properties: &ThermalProperties<'_>,
        z: &Float,
        r: &Float,
        tp: &Float,
    ) -> Float {
        let mut sum = Float::with_val_64(precision, Special::Zero);

        for layer in &self.layers {
            sum += beam.evaluate_with(precision, thermal_properties, layer, z, r, tp);
        }

        sum
    }

    /// Calculates the temperature rise over the interval a..b
    ///
    /// Similar to [`fn@temperature_rise`], this is really just a convenience
    /// wrapper over `Quadrature::integrate`
    pub fn temperature_rise(
        &self,
        precision: u64,
        quadrature: &impl Quadrature<Float>,
        beam: &impl Beam,
        thermal_properties: &ThermalProperties<'_>,
        z: &Float,
        r: &Float,
        epsilon: &Float,
        bounds: (&Float, &Float),
    ) -> (Float, Float) {
        quadrature.integrate(
            |t| self.evaluate_with(precision, beam, thermal_properties, z, r, &t),
            epsilon,
            bounds,
        )
    }
}

//TODO: we could probably swap the use of [`struct@Float`] for a generic
//      parameter that implements the operation traits in rug::ops in most
//      (if not all) places

/// An abstraction over the various `*Beam` structures
pub trait Beam {
    /// Run the beam over a given [`struct@Layer`] with the provided
    /// [`struct@ThermalProperties`]
    ///
    /// Not all implementations of [`trait@Beam`] will use all parameters
    fn evaluate_with<'a>(
        &self,
        precision: u64,
        thermal_properties: &ThermalProperties<'a>,
        layer: &Layer<'a>,
        z: &Float,
        r: &Float,
        tp: &Float,
    ) -> Float;
}

#[derive(Clone, PartialEq, Debug)]
pub struct LargeBeam;

impl Beam for LargeBeam {
    //TODO: it (might?) be worthwhile to have a specialized method that
    //      doesn't need to take r. however, this could also be addressed with
    //      the genericization of this method at the trait level. see above
    //      for more details
    fn evaluate_with<'a>(
        &self,
        precision: u64,
        thermal_properties: &ThermalProperties<'a>,
        layer: &Layer<'a>,
        z: &Float,
        _r: &Float,
        tp: &Float,
    ) -> Float {
        //TODO: make this less naive

        let mut alpha = Float::with_val_64(precision, thermal_properties.k.as_ref());
        alpha /= thermal_properties.rho.as_ref();
        alpha /= thermal_properties.c.as_ref();

        let mut term_1 = Float::with_val_64(precision, layer.mu_a.as_ref());
        term_1 *= layer.e0.as_ref();
        term_1 /= thermal_properties.rho.as_ref();
        term_1 /= thermal_properties.c.as_ref();
        term_1 /= 2.0;

        let mut term_2 = Float::with_val_64(precision, z);
        term_2 -= layer.z0.as_ref();
        term_2 *= layer.mu_a.as_ref();
        term_2 *= -1;
        term_2.exp_mut();

        if *tp == 0 {
            return term_1 * term_2;
        }

        let mut term_3 = Float::with_val_64(precision, layer.mu_a.as_ref());
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
        sqrt_mu_a *= layer.mu_a.as_ref();

        let mut argument_1 = Float::with_val_64(precision, layer.z0.as_ref());
        argument_1 += layer.d.as_ref();
        argument_1 -= z;
        argument_1 *= &reciprocal_sqrt;
        argument_1 += &sqrt_mu_a;
        argument_1.erf_mut();

        let mut argument_2 = Float::with_val_64(precision, layer.z0.as_ref());
        argument_2 -= z;
        argument_2 *= &reciprocal_sqrt;
        argument_2 += &sqrt_mu_a;
        argument_2.erf_mut();

        let mut term_4 = argument_1;
        term_4 -= argument_2;

        term_1 * term_2 * term_3 * term_4
    }
}

//TODO: same todo as above
#[derive(Clone, PartialEq, Debug)]
pub struct FlatTopBeam<'a> {
    /// Units: cm
    pub radius: Cow<'a, Float>,
}

impl<'a> Beam for FlatTopBeam<'a> {
    fn evaluate_with<'b>(
        &self,
        precision: u64,
        thermal_properties: &ThermalProperties<'b>,
        layer: &Layer<'b>,
        z: &Float,
        r: &Float,
        tp: &Float,
    ) -> Float {
        let radius = self.radius.as_ref();

        if *tp == 0 && r > radius {
            return Float::with_val_64(precision, Special::Zero);
        }

        let z_factor = LargeBeam.evaluate_with(precision, thermal_properties, layer, z, r, tp);

        if *tp == 0 {
            return z_factor;
        }

        //TODO: don't duplicate this between the code in LargeBeam and this
        //      function
        let mut alpha = Float::with_val_64(precision, thermal_properties.k.as_ref());
        alpha /= thermal_properties.rho.as_ref();
        alpha /= thermal_properties.c.as_ref();

        z_factor
            * if *r == 0 {
                let mut r_factor = Float::with_val_64(precision, radius);
                r_factor.square_mut();
                r_factor /= -4.0;
                r_factor /= alpha;
                r_factor /= tp;
                r_factor.exp_mut();
                r_factor = 1 - r_factor;
                r_factor
            } else {
                //TODO: this is not accurate at all. fix the marcum-q function
                //      implementation

                let mut a = Float::with_val_64(precision, 2.0);
                a *= alpha;
                a *= tp;
                a.recip_mut();

                let mut b = a.clone();
                b *= radius;
                a *= r;

                let mut r_factor = utilities::marcum_q(1, &a, &b, precision);
                r_factor = 1 - r_factor;
                r_factor
            }
    }
}

/// Calculates the temperature rise over the interval a..b
///
/// This is really just a convenience wrapper around `Quadrature::integrate`
#[inline]
pub fn temperature_rise(
    precision: u64,
    quadrature: &impl Quadrature<Float>,
    beam: &impl Beam,
    thermal_properties: &ThermalProperties<'_>,
    layer: &Layer<'_>,
    z: &Float,
    r: &Float,
    epsilon: &Float,
    bounds: (&Float, &Float),
) -> (Float, Float) {
    quadrature.integrate(
        |t| beam.evaluate_with(precision, thermal_properties, layer, z, r, &t),
        epsilon,
        bounds,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ctor::ctor]
    static ZERO: Float = Float::with_val_64(64, Special::Zero);

    #[ctor::ctor]
    static ONE: Float = Float::with_val_64(64, 1.0);

    #[ctor::ctor]
    static EPSILON: Float = Float::with_val_64(64, 1e-16);

    #[test]
    fn large_beam_sanity() {
        let thermal_properties = ThermalProperties {
            rho: Cow::Borrowed(&ONE),
            c: Cow::Borrowed(&ONE),
            k: Cow::Borrowed(&ONE),
        };
        let layer = Layer {
            d: Cow::Borrowed(&ONE),
            z0: Cow::Borrowed(&ZERO),
            mu_a: Cow::Borrowed(&ONE),
            e0: Cow::Borrowed(&ONE),
        };

        assert_eq!(
            LargeBeam.evaluate_with(64, &thermal_properties, &layer, &ZERO, &ZERO, &ZERO),
            5e-1
        );

        let mut result =
            LargeBeam.evaluate_with(64, &thermal_properties, &layer, &ONE, &ZERO, &ZERO);
        // reference result: 0.5 * e^-1
        result -= 1.8393972058572116080e-1;
        result.abs_mut();
        assert!(result < *EPSILON);

        let mut result =
            LargeBeam.evaluate_with(64, &thermal_properties, &layer, &ONE, &ZERO, &ONE);
        // reference result: 0.5 * e^-1 * e^1 * (erf(1) - erf(-1/sqrt(4) + 1))
        result -= 1.6110045756833416583e-1;
        result.abs_mut();
        println!("{}", result);
        assert!(result < *EPSILON);
    }

    #[test]
    fn flat_top_beam_sanity() {
        let thermal_properties = ThermalProperties {
            rho: Cow::Borrowed(&ONE),
            c: Cow::Borrowed(&ONE),
            k: Cow::Borrowed(&ONE),
        };
        let layer = Layer {
            d: Cow::Borrowed(&ONE),
            z0: Cow::Borrowed(&ZERO),
            mu_a: Cow::Borrowed(&ONE),
            e0: Cow::Borrowed(&ONE),
        };
        let beam = FlatTopBeam {
            radius: Cow::Borrowed(&ONE),
        };

        assert_eq!(
            beam.evaluate_with(64, &thermal_properties, &layer, &ZERO, &ZERO, &ZERO),
            5e-1
        );

        let mut result = beam.evaluate_with(64, &thermal_properties, &layer, &ONE, &ZERO, &ZERO);
        // reference result: 0.5 * e^-1 * (1 - 0)
        result -= 1.8393972058572116080e-1;
        result.abs_mut();
        assert!(result < *EPSILON);

        let mut result = beam.evaluate_with(64, &thermal_properties, &layer, &ONE, &ZERO, &ONE);
        // reference result: 0.5 * e^-1 * e^1 * (erf(1) - erf(-1/sqrt(4) + 1))
        //                       * (1 - e^(-1/4))
        result -= 3.5635295060953884529e-2;
        result.abs_mut();
        println!("{}", result);
        assert!(result < *EPSILON);
    }

    #[test]
    fn multi_layer_sanity() {
        let thermal_properties = ThermalProperties {
            rho: Cow::Borrowed(&ONE),
            c: Cow::Borrowed(&ONE),
            k: Cow::Borrowed(&ONE),
        };
        let layer = Layer {
            d: Cow::Borrowed(&ONE),
            z0: Cow::Borrowed(&ZERO),
            mu_a: Cow::Borrowed(&ONE),
            e0: Cow::Borrowed(&ONE),
        };
        let layers = MultiLayer::new([layer.clone()]).expect("Unable to construct a MultiLayer");

        let mut result =
            layers.evaluate_with(64, &LargeBeam, &thermal_properties, &ONE, &ZERO, &ONE);
        result -= LargeBeam.evaluate_with(64, &thermal_properties, &layer, &ONE, &ZERO, &ONE);
        assert!(result < *EPSILON);

        let layers = MultiLayer::new([
            Layer {
                d: Cow::Borrowed(&ONE),
                z0: Cow::Borrowed(&ZERO),
                mu_a: Cow::Borrowed(&ONE),
                e0: Cow::Borrowed(&ONE),
            },
            Layer {
                d: Cow::Borrowed(&ONE),
                z0: Cow::Borrowed(&ONE),
                mu_a: Cow::Borrowed(&ONE),
                e0: Cow::Borrowed(&ZERO),
            },
        ])
        .expect("Unable to construct a MultiLayer");

        let layer = Layer {
            d: Cow::Owned(Float::with_val_64(64, 2)),
            z0: Cow::Borrowed(&ZERO),
            mu_a: Cow::Borrowed(&ONE),
            e0: Cow::Borrowed(&ONE),
        };

        let beam = FlatTopBeam {
            radius: Cow::Borrowed(&ONE),
        };

        let small = Float::with_val_64(64, 1e-6);

        let mut result = layers.evaluate_with(64, &beam, &thermal_properties, &ZERO, &ZERO, &small);
        result -= beam.evaluate_with(64, &thermal_properties, &layer, &ZERO, &ZERO, &small);
        assert!(result < *EPSILON);
    }
}
