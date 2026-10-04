use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Rgb,
    Gray,
}

impl Color {
    pub const fn components(self) -> u8 {
        match self {
            Self::Rgb => 3,
            Self::Gray => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractError {
    Dimensions,
    Allocation,
    Length,
    Quality,
}

impl fmt::Display for ContractError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(match self {
            Self::Dimensions => "JPEG dimensions must be 1..=65500",
            Self::Allocation => "JPEG pixel product exceeds addressable bytes",
            Self::Length => "JPEG packed pixel length differs from layout",
            Self::Quality => "JPEG quality must be 1..=100",
        })
    }
}
impl Error for ContractError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    width: u32,
    height: u32,
    color: Color,
    len: usize,
}

impl Layout {
    pub fn new(width: u32, height: u32, color: Color) -> Result<Self, ContractError> {
        if width == 0 || height == 0 || width > 65500 || height > 65500 {
            return Err(ContractError::Dimensions);
        }
        let count = u64::from(width) * u64::from(height) * u64::from(color.components());
        let len = usize::try_from(count).map_err(|_| ContractError::Allocation)?;
        Ok(Self {
            width,
            height,
            color,
            len,
        })
    }
    pub const fn width(self) -> u32 {
        self.width
    }
    pub const fn height(self) -> u32 {
        self.height
    }
    pub const fn color(self) -> Color {
        self.color
    }
    pub const fn len(self) -> usize {
        self.len
    }
    pub const fn is_empty(self) -> bool {
        false
    }
    pub fn check_pixels(self, pixels: &[u8]) -> Result<(), ContractError> {
        if pixels.len() != self.len {
            return Err(ContractError::Length);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quality(u8);

impl Quality {
    pub const fn new(value: u8) -> Result<Self, ContractError> {
        if value == 0 || value > 100 {
            return Err(ContractError::Quality);
        }
        Ok(Self(value))
    }
    pub const fn value(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    quality: Quality,
}

impl Options {
    pub const fn new(quality: Quality) -> Self {
        Self { quality }
    }
    pub const fn quality(self) -> Quality {
        self.quality
    }
}
