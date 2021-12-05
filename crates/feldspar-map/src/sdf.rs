use bytemuck::{Pod, Zeroable};

macro_rules! impl_fixed_precision {
    (name: $name:ident, doc: $docstr:expr, primitive: $primitive:ty, float: $float:ty, max: $max:literal) => {
        #[doc = $docstr]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name(pub $primitive);

        impl $name {
            pub const RESOLUTION: $float = <$primitive>::MAX as $float;
            pub const PRECISION: $float = $max / Self::RESOLUTION;
            pub const MIN: Self = Self(-<$primitive>::MAX);
            pub const MAX: Self = Self(<$primitive>::MAX);
            pub const ZERO: Self = Self(0);
        }

        impl From<$name> for $float {
            #[inline]
            fn from(x: $name) -> Self {
                x.0 as f32 * $name::PRECISION
            }
        }

        impl From<$float> for $name {
            #[inline]
            fn from(s: f32) -> Self {
                $name((Self::RESOLUTION * s.min($max).max(-$max)) as $primitive)
            }
        }
    };
}

impl_fixed_precision!(name: Sd8, doc: "An 8-bit value in the range `[-1.0, 1.0]`.", primitive: i8, float: f32, max: 1.0);

unsafe impl Zeroable for Sd8 {}
unsafe impl Pod for Sd8 {}

#[cfg(test)]
mod test {
    // An 8-bit value in the range [-1.0, 1.0].
    impl_fixed_precision!(name: F8, doc: "", primitive: i8, float: f32, max: 1.0);

    #[test]
    fn test_f8() {
        assert_eq!(f32::from(F8::MIN), -1.0);
        assert_eq!(f32::from(F8::ZERO), 0.0);
        assert_eq!(f32::from(F8::MAX), 1.0);

        assert_eq!(F8::from(-1.0), F8::MIN);
        assert_eq!(F8::from(0.0), F8::ZERO);
        assert_eq!(F8::from(1.0), F8::MAX);
    }

    // A 16-bit value in the range [-2.0, 2.0].
    impl_fixed_precision!(name: F16, doc: "", primitive: i16, float: f32, max: 2.0);

    #[test]
    fn test_f16() {
        assert_eq!(f32::from(F16::MIN), -2.0);
        assert_eq!(f32::from(F16::ZERO), 0.0);
        assert_eq!(f32::from(F16::MAX), 2.0);

        // Slight FP rounding error
        assert_eq!(F16::from(-2.0), F16(-32768));

        assert_eq!(F16::from(0.0), F16::ZERO);
        assert_eq!(F16::from(2.0), F16::MAX);
    }
}
