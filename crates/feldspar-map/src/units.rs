macro_rules! def_units {
    ($t:ident, $docstr:expr) => {
        #[doc = $docstr]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $t<T>(pub T);

        impl<T> $t<T> {
            pub fn into_inner(self) -> T {
                self.0
            }

            pub fn map<S>(self, mut f: impl FnMut(T) -> S) -> $t<S> {
                $t(f(self.0))
            }

            pub fn map2<S, R>(u1: Self, u2: $t<S>, mut f: impl FnMut(T, S) -> R) -> $t<R> {
                $t(f(u1.into_inner(), u2.into_inner()))
            }
        }
    };
}

def_units!(VoxelUnits, "Denotes that the inner `T` is given in units of voxels.");
def_units!(ChunkUnits, "Denotes that the inner `T` is given in units of chunks.");
