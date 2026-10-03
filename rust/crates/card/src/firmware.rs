mod codes;
mod custom;
mod matcher;
mod types;
pub use types::*;

pub fn platform_codes(
    brand: Brand,
    versions: &[Vec<u8>],
) -> std::collections::BTreeSet<(Vec<u8>, Option<Vec<u8>>)> {
    codes::extract(brand, versions.iter())
}

impl Catalog {
    pub fn load() -> Result<Self, serde_json::Error> {
        serde_json::from_str(include_str!("../data/firmware.json"))
    }

    pub fn selected_platform(&self, name: &str) -> Option<&str> {
        self.selected
            .iter()
            .find(|(choice, _)| choice == name)
            .map(|(_, platform)| platform.as_str())
    }

    pub fn match_car(
        &self,
        versions: &[Firmware],
        vin: &str,
        options: MatchOptions,
    ) -> Result<CarMatch, Error> {
        matcher::match_car(self, versions, vin, options)
    }
}
