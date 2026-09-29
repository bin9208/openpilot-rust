use super::{
    device::{Device, Driver, Linux},
    graph::Call,
    Dispatch, ProgramImage, QcomBundle,
};
use crate::{
    graph::{Binding, View},
    Error,
};
use std::path::Path;

#[cfg(test)]
mod tests;

enum Step {
    Gpu(Vec<u32>),
    Copy { source: usize, destination: usize },
}

struct Model<D: Driver> {
    bundle: QcomBundle,
    device: Device<D>,
    allocations: Vec<usize>,
    steps: Vec<Step>,
}

pub struct QcomModel(Model<Linux>);

impl QcomModel {
    /// # Safety
    /// The bundle must contain trusted QCOMCL a630 kernels matching the declared
    /// argument order, image layout, launch sizes and buffer bounds. Kernel code
    /// can access CPU-mapped memory; checksums do not authenticate that code.
    pub unsafe fn load(directory: &Path, priority: u8) -> Result<Self, Error> {
        let bundle = QcomBundle::load(directory)?;
        Ok(Self(Model::new(bundle, Linux::open(priority)?)?))
    }

    pub fn write_input(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        self.0.write_input(name, bytes)
    }
    pub fn read_output(&self, name: &str) -> Result<&[u8], Error> {
        self.0.read_output(name)
    }
    pub fn run(&mut self) -> Result<(), Error> {
        self.0.run()
    }
}

impl<D: Driver> Model<D> {
    fn new(bundle: QcomBundle, driver: D) -> Result<Self, Error> {
        let mut device = Device::new(driver);
        let mut allocations = Vec::with_capacity(bundle.graph.allocations.len());
        for allocation in &bundle.graph.allocations {
            let buffer = device.allocate(allocation.bytes)?;
            if let Some(offset) = allocation.weight_offset {
                device.write(
                    buffer,
                    0,
                    &bundle.weights[offset..offset + allocation.bytes],
                )?;
            }
            allocations.push(buffer);
        }
        let stack_bytes = bundle
            .programs
            .iter()
            .map(|program| program.hw_stack_offset as usize * 4)
            .max()
            .unwrap_or(4096);
        let stack_buffer = device.allocate(stack_bytes)?;
        let border_buffer = device.allocate(4096)?;
        let dummy_buffer = device.allocate(4096)?;
        let stack = device.address(stack_buffer, 0, stack_bytes)?;
        let border = device.address(border_buffer, 0, 4096)?;
        let dummy = device.address(dummy_buffer, 0, 4096)?;
        let mut program_addresses = Vec::with_capacity(bundle.programs.len());
        for program in &bundle.programs {
            let buffer = device.allocate(program.image().len())?;
            device.write(buffer, 0, program.image())?;
            program_addresses.push(device.address(buffer, 0, program.image().len())?);
        }
        let mut steps = Vec::new();
        let mut commands = Vec::new();
        for call in &bundle.graph.calls {
            match call {
                Call::Copy {
                    source,
                    destination,
                } => {
                    if !commands.is_empty() {
                        steps.push(Step::Gpu(std::mem::take(&mut commands)));
                    }
                    steps.push(Step::Copy {
                        source: *source,
                        destination: *destination,
                    });
                }
                Call::Kernel {
                    kernel,
                    views,
                    scalars,
                    global,
                    local,
                } => {
                    let program = &bundle.programs[*kernel];
                    let mut arguments = Vec::new();
                    for (view, specs) in views.iter().zip(&bundle.graph.kernels[*kernel].arguments)
                    {
                        let view = &bundle.graph.views[*view];
                        let address = device.address(
                            allocations[view.allocation],
                            view.offset,
                            view.bytes,
                        )?;
                        arguments.extend(specs.iter().map(|spec| spec.bind(address)));
                    }
                    let bytes = program.arguments(&arguments, scalars)?;
                    let buffer = device.allocate(bytes.len())?;
                    device.write(buffer, 0, &bytes)?;
                    let args = device.address(buffer, 0, bytes.len())?;
                    commands.extend(ProgramImage::memory_barrier(dummy));
                    commands.extend(program.dispatch(&Dispatch {
                        program: program_addresses[*kernel],
                        stack,
                        border,
                        dummy,
                        args,
                        global: *global,
                        local: *local,
                    })?);
                    if commands.len() > 16 * 1024 * 1024 {
                        return Err(Error::Limit("QCOM frame command words"));
                    }
                }
            }
        }
        if !commands.is_empty() {
            steps.push(Step::Gpu(commands));
        }
        if let Some(words) = steps
            .iter()
            .filter_map(|step| match step {
                Step::Gpu(words) => Some(words.len()),
                _ => None,
            })
            .max()
        {
            device.reserve_commands(words)?;
        }
        Ok(Self {
            bundle,
            device,
            allocations,
            steps,
        })
    }

    fn binding<'a>(&'a self, bindings: &'a [Binding], name: &str) -> Result<&'a View, Error> {
        let binding = bindings
            .iter()
            .find(|binding| binding.name == name)
            .ok_or_else(|| Error::Binding(name.to_owned()))?;
        Ok(&self.bundle.graph.views[binding.view])
    }

    fn write_input(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        let view = self.binding(&self.bundle.graph.inputs, name)?;
        if bytes.len() != view.bytes {
            return Err(Error::Size {
                expected: view.bytes,
                actual: bytes.len(),
            });
        }
        self.device
            .write(self.allocations[view.allocation], view.offset, bytes)
    }

    fn read_output(&self, name: &str) -> Result<&[u8], Error> {
        let view = self.binding(&self.bundle.graph.outputs, name)?;
        self.device
            .read(self.allocations[view.allocation], view.offset, view.bytes)
    }

    fn run(&mut self) -> Result<(), Error> {
        for step in &self.steps {
            match step {
                Step::Gpu(commands) => self.device.execute(commands)?,
                Step::Copy {
                    source,
                    destination,
                } => {
                    let source = &self.bundle.graph.views[*source];
                    let destination = &self.bundle.graph.views[*destination];
                    self.device.copy(
                        self.allocations[source.allocation],
                        source.offset,
                        self.allocations[destination.allocation],
                        destination.offset,
                        source.bytes,
                    )?;
                }
            }
        }
        Ok(())
    }
}
