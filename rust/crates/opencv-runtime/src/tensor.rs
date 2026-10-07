use crate::Error;

fn elements(shape: &[i32]) -> Result<usize, Error> {
    if shape.is_empty() || shape.len() > 32 {
        return Err(Error::Contract("tensor rank must be 1..=32"));
    }
    shape.iter().try_fold(1_usize, |total, &axis| {
        let size =
            usize::try_from(axis).map_err(|_| Error::Contract("tensor axes must be positive"))?;
        if size == 0 {
            return Err(Error::Contract("tensor axes must be positive"));
        }
        total.checked_mul(size).ok_or(Error::Contract(
            "tensor product exceeds addressable elements",
        ))
    })
}

#[derive(Debug, Clone, Copy)]
pub struct TensorView<'a> {
    shape: &'a [i32],
    values: &'a [f32],
}

impl<'a> TensorView<'a> {
    pub fn new(shape: &'a [i32], values: &'a [f32]) -> Result<Self, Error> {
        if elements(shape)? != values.len() {
            return Err(Error::Contract("tensor shape differs from value length"));
        }
        Ok(Self { shape, values })
    }
    pub const fn shape(self) -> &'a [i32] {
        self.shape
    }
    pub const fn values(self) -> &'a [f32] {
        self.values
    }
}

#[derive(Debug, Clone)]
pub struct Tensor {
    shape: Vec<i32>,
    values: Vec<f32>,
}

impl Tensor {
    pub fn from_parts(shape: Vec<i32>, values: Vec<f32>) -> Result<Self, Error> {
        TensorView::new(&shape, &values)?;
        Ok(Self { shape, values })
    }
    pub fn view(&self) -> TensorView<'_> {
        TensorView {
            shape: &self.shape,
            values: &self.values,
        }
    }
    pub fn into_parts(self) -> (Vec<i32>, Vec<f32>) {
        (self.shape, self.values)
    }
}
