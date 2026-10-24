//! Development corpus inputs from the pinned NumPy PCG64 stream, evaluated by
//! the explicitly supplied, original C++ oracle. No Rust model equations are
//! used to produce expected function values.
#[allow(unsafe_code)]
mod provider;

use crate::{
    argparse_scan::{scan_os, ArgumentError, OptionSpec},
    lagd_random::Generator,
    seal::Result,
};
use openpilot_runtime_core::python_float::write_float;
use provider::Provider;
use std::{
    ffi::OsString,
    fmt::Write as _,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Mutex,
};

// Car setters are C++ process globals. Keep each load, six-setter sequence,
// every function call, final pin verification, and Library drop exclusive.
static PROVIDER_MUTEX: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Model {
    Pose,
    Car,
}

impl Model {
    pub fn name(self) -> &'static str {
        match self {
            Self::Pose => "pose",
            Self::Car => "car",
        }
    }

    fn row_count(self) -> usize {
        match self {
            Self::Pose => 16,
            Self::Car => 24,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Input {
    pub state: Vec<f64>,
    pub globals: Option<Vec<f64>>,
    pub dt: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Function {
    pub name: &'static str,
    pub width: usize,
    pub timed: bool,
}

pub fn functions(model: Model) -> Vec<Function> {
    let state = match model {
        Model::Pose => 18,
        Model::Car => 9,
    };
    let mut names = vec![
        Function {
            name: "f_fun",
            width: state,
            timed: true,
        },
        Function {
            name: "F_fun",
            width: state * state,
            timed: true,
        },
    ];
    let kinds: &[usize] = match model {
        Model::Pose => &[4, 10, 13, 14],
        Model::Car => &[24, 25, 26, 27, 28, 29, 30, 31],
    };
    for &kind in kinds {
        let dimension = match model {
            Model::Pose => 3,
            Model::Car if kind == 24 => 2,
            Model::Car => 1,
        };
        // The function names are a closed source roster, not user input.
        let h = match kind {
            4 => "h_4",
            10 => "h_10",
            13 => "h_13",
            14 => "h_14",
            24 => "h_24",
            25 => "h_25",
            26 => "h_26",
            27 => "h_27",
            28 => "h_28",
            29 => "h_29",
            30 => "h_30",
            31 => "h_31",
            _ => unreachable!(),
        };
        let jacobian = match kind {
            4 => "H_4",
            10 => "H_10",
            13 => "H_13",
            14 => "H_14",
            24 => "H_24",
            25 => "H_25",
            26 => "H_26",
            27 => "H_27",
            28 => "H_28",
            29 => "H_29",
            30 => "H_30",
            31 => "H_31",
            _ => unreachable!(),
        };
        names.extend([
            Function {
                name: h,
                width: dimension,
                timed: false,
            },
            Function {
                name: jacobian,
                width: dimension * state,
                timed: false,
            },
        ]);
    }
    names
}

/// Exact original draws, including the 18 draws discarded when Pose row zero
/// is overwritten. Car scalar uniforms follow all nine normal draws per row.
pub fn input_rows(model: Model) -> Vec<Input> {
    let mut random = Generator::new(match model {
        Model::Pose => 138,
        Model::Car => 143,
    });
    let mut rows = Vec::with_capacity(model.row_count());
    for index in 0..model.row_count() {
        let dt = [0.0, 0.001, 0.05, 0.8][index % 4];
        match model {
            Model::Pose => {
                let mut state = (0..18).map(|_| random.normal()).collect::<Vec<_>>();
                for value in &mut state[..3] {
                    *value *= 0.3;
                }
                for value in &mut state[3..6] {
                    *value *= 20.0;
                }
                for value in &mut state[9..12] {
                    *value *= 0.1;
                }
                for value in &mut state[12..15] {
                    *value *= 10.0;
                }
                if index == 0 {
                    state.fill(0.0);
                }
                rows.push(Input {
                    state,
                    globals: None,
                    dt,
                });
            }
            Model::Car => {
                let mut state = (0..9).map(|_| random.normal() * 0.1).collect::<Vec<_>>();
                state[0] = random.uniform(0.2, 5.0);
                state[1] = random.uniform(7.0, 30.0);
                state[4] = random.uniform(1.0, 40.0);
                let globals = [
                    random.uniform(1200.0, 3200.0),
                    random.uniform(2000.0, 6500.0),
                    random.uniform(0.9, 1.5),
                    random.uniform(1.2, 1.8),
                    random.uniform(60000.0, 170000.0),
                    random.uniform(60000.0, 170000.0),
                ];
                rows.push(Input {
                    state,
                    globals: Some(globals.to_vec()),
                    dt,
                });
            }
        }
    }
    rows
}

pub struct Options {
    pub oracle: PathBuf,
    pub output: PathBuf,
}

/// `None` is the scanner's help action. Both paths retain raw Unix bytes.
pub fn parse_options(
    arguments: impl IntoIterator<Item = OsString>,
    default_output: &str,
) -> std::result::Result<Option<Options>, ArgumentError> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let parsed = scan_os(
        &arguments,
        &[
            OptionSpec::value("--oracle", true),
            OptionSpec::value("--output", false),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    Ok(Some(Options {
        oracle: PathBuf::from(parsed.values["--oracle"].clone()),
        output: PathBuf::from(
            parsed
                .values
                .get("--output")
                .cloned()
                .unwrap_or_else(|| default_output.into()),
        ),
    }))
}

pub struct Row {
    pub input: Input,
    pub functions: Vec<(&'static str, Vec<f64>)>,
}

pub fn evaluate(model: Model, oracle: &Path) -> Result<Vec<Row>> {
    let guard = PROVIDER_MUTEX
        .lock()
        .map_err(|_| "oracle provider mutex poisoned")?;
    let provider = Provider::load(model, oracle)?;
    let roster = functions(model);
    let mut output = Vec::with_capacity(model.row_count());
    for input in input_rows(model) {
        if let Some(globals) = &input.globals {
            provider.set_globals(globals)?;
        }
        let mut values = Vec::with_capacity(roster.len());
        for (index, function) in roster.iter().enumerate() {
            let mut result = vec![0.0; function.width];
            provider.call(index, &input.state, input.dt, &mut result)?;
            values.push((function.name, result));
        }
        output.push(Row {
            input,
            functions: values,
        });
    }
    provider.verify()?;
    drop(provider);
    drop(guard);
    Ok(output)
}

fn indent(output: &mut String, depth: usize) {
    output.extend(std::iter::repeat_n(' ', depth * 2));
}

fn array(output: &mut String, values: &[f64], depth: usize) -> Result<()> {
    output.push('[');
    if !values.is_empty() {
        output.push('\n');
        for (index, value) in values.iter().enumerate() {
            indent(output, depth + 1);
            write_float(*value, output)?;
            if index + 1 < values.len() {
                output.push(',');
            }
            output.push('\n');
        }
        indent(output, depth);
    }
    output.push(']');
    Ok(())
}

/// Python's json.dumps(indent=2) layout, declared key order and shortest
/// float tokens. Hosted fresh-original comparison remains the byte-level gate.
pub fn serialize(model: Model, rows: &[Row]) -> Result<String> {
    if rows.len() != model.row_count() {
        return Err("wrong model corpus row count".into());
    }
    let roster = functions(model);
    let mut output = String::new();
    output.push_str("[\n");
    for (row_index, row) in rows.iter().enumerate() {
        if row.functions.len() != roster.len() {
            return Err("wrong model function roster".into());
        }
        output.push_str("  {\n    \"state\": ");
        array(&mut output, &row.input.state, 2)?;
        output.push_str(",\n");
        if model == Model::Car {
            output.push_str("    \"globals\": ");
            array(
                &mut output,
                row.input.globals.as_ref().ok_or("missing car globals")?,
                2,
            )?;
            output.push_str(",\n");
        }
        output.push_str("    \"dt\": ");
        write_float(row.input.dt, &mut output)?;
        output.push_str(",\n    \"functions\": {\n");
        for (index, ((name, values), expected)) in row.functions.iter().zip(&roster).enumerate() {
            if name != &expected.name || values.len() != expected.width {
                return Err(format!("invalid function roster: {name}").into());
            }
            write!(output, "      \"{name}\": ")?;
            array(&mut output, values, 3)?;
            if index + 1 < roster.len() {
                output.push(',');
            }
            output.push('\n');
        }
        output.push_str("    }\n  }");
        if row_index + 1 < rows.len() {
            output.push(',');
        }
        output.push('\n');
    }
    output.push_str("]\n");
    Ok(output)
}

/// Keep the final output untouched if provider evaluation or serialization
/// fails. On success, write through the selected path like Path.write_text:
/// existing links, mode, and inode retain their original behavior.
pub fn generate(model: Model, options: &Options) -> Result<()> {
    let rows = evaluate(model, &options.oracle)?;
    let bytes = serialize(model, &rows)?;
    let parent = options
        .output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    fs::write(&options.output, bytes)?;
    Ok(())
}

/// Python's surrogateescape print emits raw path bytes on Unix stdout.
pub fn success_line(model: Model, output: &Path) -> Vec<u8> {
    let mut line = match model {
        Model::Pose => b"Generated 16 actual-source model/Jacobian samples: ".to_vec(),
        Model::Car => b"Generated 24 actual-source car model/Jacobian samples: ".to_vec(),
    };
    line.extend_from_slice(output.as_os_str().as_bytes());
    line.push(b'\n');
    line
}
