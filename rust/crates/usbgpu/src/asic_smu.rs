use crate::{
    amd_bus::Bus,
    asic::{Asic, SmuState},
    Error,
};
use std::{collections::BTreeMap, time::Duration};
impl<B: Bus> Asic<B> {
    pub(crate) fn init_smu_software(&mut self) -> Result<(), Error> {
        let mut target = self.hw.version(16)?;
        if target == [13, 0, 7] {
            target = [13, 0, 0];
        }
        let mut choices = Vec::new();
        for name in self.hw.catalog.constants.keys() {
            if let Some(suffix) = name.strip_prefix("smu_") {
                let values = suffix
                    .split('_')
                    .map(str::parse::<u8>)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| Error::Contract("SMU module version invalid"))?;
                if values.len() == 3
                    && values[0] == target[0]
                    && values.as_slice() <= target.as_slice()
                {
                    choices.push((values, name.clone()));
                }
            }
        }
        choices.sort();
        let module = choices
            .pop()
            .ok_or(Error::Contract("SMU module unavailable"))?
            .1;
        let driver_table = self
            .memory
            .palloc(&mut self.hw, 0x4000, 4096, false, true, false)?;
        self.smu = Some(SmuState {
            module,
            driver_table,
            clocks: BTreeMap::new(),
        });
        Ok(())
    }
    pub(crate) fn smu_constant(&self, name: &str) -> Result<u32, Error> {
        let state = self
            .smu
            .as_ref()
            .ok_or(Error::Contract("SMU software not initialized"))?;
        u32::try_from(self.hw.catalog.constant(&state.module, name)?)
            .map_err(|_| Error::Contract("SMU constant exceeds dword"))
    }
    pub(crate) fn smu_message(
        &mut self,
        message: u32,
        parameter: u32,
        read_back: bool,
        timeout: u64,
        debug: bool,
    ) -> Result<Option<u32>, Error> {
        let response = if debug {
            "mmMP1_SMN_C2PMSG_54"
        } else {
            "mmMP1_SMN_C2PMSG_90"
        };
        let argument = if debug {
            "mmMP1_SMN_C2PMSG_53"
        } else {
            "mmMP1_SMN_C2PMSG_82"
        };
        let request = if debug {
            "mmMP1_SMN_C2PMSG_75"
        } else {
            "mmMP1_SMN_C2PMSG_66"
        };
        self.hw.write(response, 0, 0, &[])?;
        self.hw.write(argument, 0, parameter, &[])?;
        self.hw.write(request, 0, message, &[])?;
        self.hw
            .wait(timeout, 1, &format!("SMU msg {message:#x} timeout"), |hw| {
                Ok(u64::from(hw.read(response, 0)?))
            })?;
        if read_back {
            Ok(Some(self.hw.read(argument, 0)?))
        } else {
            Ok(None)
        }
    }
    pub(crate) fn init_smu(&mut self) -> Result<(), Error> {
        let table = self
            .smu
            .as_ref()
            .ok_or(Error::Contract("SMU software not initialized"))?
            .driver_table;
        let address = self.hw.paddr_to_mc(table)?;
        self.smu_message(
            self.smu_constant("PPSMC_MSG_SetDriverDramAddrHigh")?,
            (address >> 32) as u32,
            false,
            10000,
            false,
        )?;
        self.smu_message(
            self.smu_constant("PPSMC_MSG_SetDriverDramAddrLow")?,
            address as u32,
            false,
            10000,
            false,
        )?;
        self.smu_message(
            self.smu_constant("PPSMC_MSG_EnableAllSmuFeatures")?,
            0,
            false,
            10000,
            false,
        )?;
        Ok(())
    }
    pub(crate) fn smu_alive(&mut self) -> Result<bool, Error> {
        let result = self.smu_message(
            self.smu_constant("PPSMC_MSG_GetSmuVersion")?,
            0,
            false,
            100,
            false,
        );
        if !matches!(result, Err(Error::Timeout { .. })) {
            result?;
        }
        Ok(self.hw.read("mmMP1_SMN_C2PMSG_90", 0)? != 0)
    }
    pub(crate) fn smu_reset(&mut self) -> Result<(), Error> {
        let version = self.hw.version(15)?;
        if version >= [14, 0, 0] {
            self.smu_message(2, 0, false, 10000, true)?;
        } else if [[13, 0, 6], [13, 0, 12]].contains(&version) {
            self.smu_message(
                self.smu_constant("PPSMC_MSG_GfxDriverReset")?,
                1,
                false,
                10000,
                false,
            )?;
        } else {
            self.smu_message(
                self.smu_constant("PPSMC_MSG_Mode1Reset")?,
                0,
                false,
                10000,
                false,
            )?;
        }
        if self.hw.gmc()?.xgmi_segment_size == 0 {
            self.hw.bus.sleep(Duration::from_millis(500));
        }
        Ok(())
    }
    pub(crate) fn set_clocks(&mut self, level: Option<isize>) -> Result<(), Error> {
        let mut clocks = vec![
            self.smu_constant("PPCLK_UCLK")?,
            self.smu_constant("PPCLK_FCLK")?,
            self.smu_constant("PPCLK_SOCCLK")?,
        ];
        if ![[13, 0, 6], [13, 0, 12]].contains(&self.hw.version(15)?) {
            clocks.push(self.smu_constant("PPCLK_GFXCLK")?);
        }
        let minimum = self.smu_constant("PPSMC_MSG_SetSoftMinByFreq")?;
        let maximum = self.smu_constant("PPSMC_MSG_SetSoftMaxByFreq")?;
        if level.is_some() {
            for &clock in &clocks {
                if !self
                    .smu
                    .as_ref()
                    .ok_or(Error::Contract("SMU state missing"))?
                    .clocks
                    .contains_key(&clock)
                {
                    let message = self.smu_constant("PPSMC_MSG_GetDpmFreqByIndex")?;
                    let count = self
                        .smu_message(message, (clock << 16) | 255, true, 10000, false)?
                        .ok_or(Error::Contract("SMU clock count missing"))?
                        & 0x7fffffff;
                    let mut values = Vec::new();
                    for index in 0..count {
                        values.push(
                            self.smu_message(message, (clock << 16) | index, true, 10000, false)?
                                .ok_or(Error::Contract("SMU clock frequency missing"))?
                                & 0x7fffffff,
                        );
                    }
                    self.smu.as_mut().unwrap().clocks.insert(clock, values);
                }
            }
        }
        for clock in clocks {
            let frequency = if let Some(level) = level {
                let values = &self.smu.as_ref().unwrap().clocks[&clock];
                if values.is_empty() {
                    continue;
                }
                let index = if level < 0 {
                    values.len().checked_sub(level.unsigned_abs())
                } else {
                    Some(level as usize)
                }
                .ok_or(Error::Contract("SMU clock level out of range"))?;
                *values
                    .get(index)
                    .ok_or(Error::Contract("SMU clock level out of range"))?
            } else {
                0
            };
            let result = self.smu_message(minimum, (clock << 16) | frequency, false, 20, false);
            if !matches!(result, Err(Error::Timeout { .. })) {
                result?;
            }
            if self.hw.gfx()? >= [10, 0, 0] {
                self.smu_message(
                    maximum,
                    (clock << 16) | if level.is_some() { frequency } else { 0xffff },
                    false,
                    10000,
                    false,
                )?;
            }
        }
        Ok(())
    }
    pub(crate) fn set_power_limit(&mut self, watts: f64) -> Result<(), Error> {
        if !watts.is_finite() {
            return Err(Error::Contract("GPU power limit is not finite"));
        }
        let value = watts.round_ties_even().max(1.);
        if value > f64::from(u32::MAX) {
            return Err(Error::Contract("GPU power limit exceeds dword"));
        }
        self.smu_message(
            self.smu_constant("PPSMC_MSG_SetPptLimit")?,
            value as u32,
            false,
            10000,
            false,
        )?;
        Ok(())
    }
    pub(crate) fn aca_banks(&mut self, uncorrectable: bool) -> Result<Vec<[u64; 16]>, Error> {
        if self.smu_constant("PPSMC_MSG_QueryValidMcaCount").is_err() {
            return Ok(Vec::new());
        }
        let count_message = self.smu_constant(if uncorrectable {
            "PPSMC_MSG_QueryValidMcaCount"
        } else {
            "PPSMC_MSG_QueryValidMcaCeCount"
        })?;
        let count = self
            .smu_message(count_message, 0, true, 10000, false)?
            .ok_or(Error::Contract("SMU ACA count missing"))?;
        let message = self.smu_constant(if uncorrectable {
            "PPSMC_MSG_McaBankDumpDW"
        } else {
            "PPSMC_MSG_McaBankCeDumpDW"
        })?;
        let mut banks = Vec::new();
        for bank in 0..count {
            let mut values = [0; 16];
            for (index, value) in values.iter_mut().enumerate() {
                let upper = self
                    .smu_message(
                        message,
                        (bank << 16) | (index as u32 * 8 + 4),
                        true,
                        10000,
                        false,
                    )?
                    .ok_or(Error::Contract("SMU ACA upper value missing"))?;
                let lower = self
                    .smu_message(
                        message,
                        (bank << 16) | (index as u32 * 8),
                        true,
                        10000,
                        false,
                    )?
                    .ok_or(Error::Contract("SMU ACA lower value missing"))?;
                *value = (u64::from(upper) << 32) | u64::from(lower);
            }
            banks.push(values);
        }
        Ok(banks)
    }
}
