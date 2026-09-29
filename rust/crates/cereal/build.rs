use std::{env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("../../..");
    let staging = PathBuf::from(env::var("OUT_DIR")?).join("schema");
    fs::create_dir_all(staging.join("include"))?;
    let mut compiler = capnpc::CompilerCommand::new();
    compiler.src_prefix(&staging).import_path(&staging);
    for (name, source) in [
        ("log.capnp", "openpilot/cereal/log.capnp"),
        ("deprecated.capnp", "openpilot/cereal/deprecated.capnp"),
        ("custom.capnp", "openpilot/cereal/custom.capnp"),
        ("car.capnp", "opendbc_repo/opendbc/car/car.capnp"),
    ] {
        let source = root.join(source);
        println!("cargo:rerun-if-changed={}", source.display());
        let destination = staging.join(name);
        fs::copy(source, &destination)?;
        compiler.file(destination);
    }
    let annotation = root.join("openpilot/cereal/include/c++.capnp");
    println!("cargo:rerun-if-changed={}", annotation.display());
    fs::copy(annotation, staging.join("include/c++.capnp"))?;
    compiler.run()?;
    remove_unused_annotation_generics()?;
    Ok(())
}

fn remove_unused_annotation_generics() -> Result<(), Box<dyn Error>> {
    let path = PathBuf::from(env::var("OUT_DIR")?).join("log_capnp.rs");
    let generated = fs::read_to_string(&path)?;
    let signature = "get_annotation_types<Key,Value>(child_index: Option<u16>, index: u32) -> ::capnp::introspect::Type where Key: ::capnp::traits::Owned + 'static, Value: ::capnp::traits::Owned + 'static  {";
    let bodies = generated.split(signature).skip(1);
    if generated.matches(signature).count() != 2
        || bodies.into_iter().any(|body| {
            !body.trim_start().starts_with(
                "::capnp::introspect::panic_invalid_annotation_indices(child_index, index)",
            )
        })
    {
        return Err("capnpc annotation output changed; review the generic normalization".into());
    }
    let normalized = generated
        .replace(signature, "get_annotation_types(child_index: Option<u16>, index: u32) -> ::capnp::introspect::Type {")
        .replace("get_annotation_types::<Key,Value>", "get_annotation_types");
    fs::write(path, normalized)?;
    Ok(())
}
