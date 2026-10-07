use super::Error;

#[derive(Clone)]
pub struct Model {
    pub name: String,
    pub ev: bool,
    pub cc: bool,
    pub camera: bool,
    pub sdgm: bool,
}
impl Model {
    pub fn new(name: &str) -> Result<Self, Error> {
        if crate::vehicle_params::platform(name)?.brand != "gm" {
            return Err(Error::Platform(name.into()));
        }
        let ev = matches!(
            name,
            "CHEVROLET_VOLT"
                | "CHEVROLET_VOLT_2019"
                | "CHEVROLET_BOLT_EUV"
                | "CHEVROLET_VOLT_CC"
                | "CHEVROLET_BOLT_CC"
        );
        let cc = matches!(
            name,
            "CHEVROLET_VOLT_CC"
                | "CHEVROLET_BOLT_CC"
                | "CHEVROLET_EQUINOX_CC"
                | "CHEVROLET_SUBURBAN_CC"
                | "GMC_YUKON_CC"
                | "CADILLAC_CT6_CC"
                | "CHEVROLET_TRAILBLAZER_CC"
                | "CADILLAC_XT5_CC"
                | "CHEVROLET_MALIBU_CC"
        );
        let camera = matches!(
            name,
            "CHEVROLET_BOLT_EUV"
                | "CHEVROLET_SILVERADO"
                | "CHEVROLET_EQUINOX"
                | "CHEVROLET_TRAILBLAZER"
                | "CHEVROLET_TRAX"
        ) || (cc && name != "CHEVROLET_SUBURBAN_CC");
        let sdgm = matches!(
            name,
            "CADILLAC_XT4" | "CHEVROLET_TRAVERSE" | "BUICK_BABYENCLAVE" | "CHEVROLET_VOLT_2019"
        );
        Ok(Self {
            name: name.into(),
            ev,
            cc,
            camera,
            sdgm,
        })
    }
    pub fn volt(&self) -> bool {
        self.name == "CHEVROLET_VOLT"
    }
    pub fn bolt(&self) -> bool {
        self.name == "CHEVROLET_BOLT_EUV"
    }
    pub fn cluster_ratio(&self) -> bool {
        matches!(
            self.name.as_str(),
            "CHEVROLET_TRAX" | "CHEVROLET_TRAILBLAZER" | "CHEVROLET_TRAILBLAZER_CC"
        )
    }
}
