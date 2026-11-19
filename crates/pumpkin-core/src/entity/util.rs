//! Shared helpers for entities

/// Extra sampling on top of [`rand::RngExt`].
pub trait RandomExt: rand::RngExt {
    /// Triangular distribution centred on `center`.
    fn triangle(&mut self, center: f64, spread: f64) -> f64 {
        spread.mul_add(self.random::<f64>() - self.random::<f64>(), center)
    }

    /// Standard normal sample, vanilla `RandomSource.nextGaussian` (Marsaglia polar method).
    fn gaussian(&mut self) -> f64 {
        loop {
            let a = 2.0f64.mul_add(self.random::<f64>(), -1.0);
            let b = 2.0f64.mul_add(self.random::<f64>(), -1.0);
            let s = a.mul_add(a, b * b);
            if s < 1.0 && s != 0.0 {
                return a * (-2.0 * s.ln() / s).sqrt();
            }
        }
    }
}

impl<T: rand::RngExt + ?Sized> RandomExt for T {}
