//! One authenticated original-archive pass, followed by independently verified
//! small-component reuse. Cache receipts never supply expected member hashes.
use super::*;
use std::time::Instant;

const FORMAT: &str = "openpilot-provider-batch-v1";

#[derive(Debug, Serialize)]
pub struct BatchExport {
    pub schema: u32,
    pub source_archive_sha256: String,
    pub source_archive_passes: u32,
    pub source_archive_bytes: u64,
    pub transferred_bytes: u64,
    pub elapsed_ms: u128,
    /// These retain Export's existing shape. Their prepared origin prefix is
    /// authenticated from the pinned small component, including its original
    /// manifest/provenance, rather than claiming a new raw-origin transfer.
    pub exports: BTreeMap<String, Export>,
}

pub fn export_batch(
    plans: &[Plan],
    cache: &Path,
    store: &Path,
    output: &Path,
    download: &impl assets::Download,
) -> Result<BatchExport> {
    for plan in plans {
        plan.validate()?;
    }
    export_batch_inner(plans, cache, store, output, download)
}

fn ordered(plans: &[Plan]) -> Result<Vec<&Plan>> {
    if plans.is_empty() || plans.len() > 15 {
        return Err("provider batch requires 1..15 independently pinned profiles".into());
    }
    let mut plans: Vec<_> = plans.iter().collect();
    plans.sort_by(|a, b| a.profile.cmp(&b.profile));
    for (index, plan) in plans.iter().enumerate() {
        component_names(&plan.profile)?;
        plan.source_binding()?;
        if index > 0 && plans[index - 1].profile == plan.profile {
            return Err("duplicate provider batch profile".into());
        }
        if plan.source_archive.sha256 != plans[0].source_archive.sha256
            || plan.source_archive.bytes != plans[0].source_archive.bytes
            || plan.source_archive.url != plans[0].source_archive.url
            || plan.target != plans[0].target
        {
            return Err(
                "provider batch profiles must share the exact original archive and target".into(),
            );
        }
    }
    Ok(plans)
}

fn directory_names(path: &Path) -> Result<BTreeSet<std::ffi::OsString>> {
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err("provider batch generation is not a real directory".into());
    }
    Ok(fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<_>>()?)
}

fn component_descriptor(plan: &Plan, archive: &Path) -> Result<Descriptor> {
    let sha = digest_file(archive)?;
    let bytes = regular(archive)?.metadata()?.len();
    if bytes == 0 || bytes > COMPONENT_LIMIT {
        return Err("provider batch component exceeds its bounded archive size".into());
    }
    Ok(Descriptor {
        schema: 1,
        role: plan.role().into(),
        profile: plan.profile.clone(),
        target: plan.target.clone(),
        agnos_version: plan.agnos_version.clone(),
        provider_source_commit: plan.source_commit.clone(),
        tree_sha256: hash(&json(&expected(plan)?)?),
        asset: assets::Asset {
            url: component_url(&plan.profile, &sha)?,
            sha256: sha,
            bytes,
        },
    })
}

fn verify_generation(plans: &[&Plan], generation: &Path, contents: bool) -> Result<()> {
    let names: BTreeSet<std::ffi::OsString> =
        plans.iter().map(|p| p.profile.clone().into()).collect();
    if directory_names(generation)? != names {
        return Err("provider batch generation has missing or unexpected profiles".into());
    }
    for plan in plans {
        let root = generation.join(&plan.profile);
        let (_, filename) = component_names(&plan.profile)?;
        if directory_names(&root)? != ["provider.json".into(), filename.into()].into() {
            return Err("provider batch component has missing or unexpected files".into());
        }
        let expected_descriptor = json(&component_descriptor(plan, &root.join(filename))?)?;
        let mut actual = Vec::new();
        regular(&root.join("provider.json"))?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut actual)?;
        if actual != expected_descriptor {
            return Err(
                "provider batch descriptor differs from the current independent profile".into(),
            );
        }
        if contents {
            let verify = tempfile::Builder::new()
                .prefix(".verify-provider-component-")
                .tempdir_in(generation.parent().ok_or("missing provider batch parent")?)?;
            directory(&verify.path().join("tree"))?;
            unpack(
                plan,
                &root.join(filename),
                &verify.path().join("tree"),
                true,
            )?;
            // The archive hash inside a cached descriptor is not an independent
            // pin. Recreate the legacy canonical bytes from the verified small
            // tree too, rejecting re-packed archives with forged descriptors.
            let canonical = verify.path().join("canonical.tar.gz");
            compress(&verify.path().join("tree"), &expected(plan)?, &canonical)?;
            if digest_file(&canonical)? != digest_file(&root.join(filename))? {
                return Err("provider batch cache archive is not the canonical export".into());
            }
        }
    }
    Ok(())
}

fn export_batch_inner(
    plans: &[Plan],
    cache: &Path,
    store: &Path,
    output: &Path,
    download: &impl assets::Download,
) -> Result<BatchExport> {
    let started = Instant::now();
    let plans = ordered(plans)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    match fs::symlink_metadata(output) {
        Ok(_) => return Err("provider batch output already exists".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    directory(store)?;
    let store = store.canonicalize()?;
    if parent.starts_with(&store)
        || store.starts_with(parent.join(output.file_name().ok_or("missing batch output name")?))
    {
        return Err("provider batch output must be outside its immutable cache".into());
    }
    let identity = hash(&json(&(FORMAT, &plans))?);
    let object = store.join(&identity);
    directory(&object)?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(object.join("lock"))?;
    if !lock.metadata()?.is_file() {
        return Err("provider batch lock is not a regular file".into());
    }
    let _lock = FileLock::exclusive(lock)?;
    let generation = object.join("generation");
    let mut passes = 0;
    let mut transferred = 0;
    match fs::symlink_metadata(&generation) {
        Ok(_) => verify_generation(&plans, &generation, true)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let payload = plans
                .iter()
                .flat_map(|plan| plan.members.values())
                .try_fold(0u64, |total, member| {
                    total
                        .checked_add(member.bytes)
                        .ok_or("provider batch payload overflow")
                })?;
            let growth = payload
                .checked_mul(6)
                .and_then(|n| n.checked_add(plans[0].source_archive.bytes))
                .and_then(|n| n.checked_add(64 * 1024 * 1024))
                .ok_or("provider batch growth overflow")?;
            room(&object, growth)?;
            let assets = assets::Plan {
                schema: 1,
                reserve_bytes: 0,
                assets: BTreeMap::from([(
                    "original-provider".into(),
                    assets::Asset {
                        url: plans[0].source_archive.url.clone(),
                        sha256: plans[0].source_archive.sha256.clone(),
                        bytes: plans[0].source_archive.bytes,
                    },
                )]),
            };
            let source = assets::prepare(cache, &assets, download)?
                .remove("original-provider")
                .ok_or("missing authenticated provider batch source")?;
            transferred = source.transferred_bytes;
            let pending = tempfile::Builder::new()
                .prefix(".pending-provider-batch-")
                .tempdir_in(&object)?;
            unpack_original(&plans, &source.path, pending.path())?;
            passes = 1;
            for plan in &plans {
                let root = pending.path().join(&plan.profile);
                let tree = root.join("tree");
                let (_, filename) = component_names(&plan.profile)?;
                compress(&tree, &expected(plan)?, &root.join(filename))?;
                write(
                    &root.join("provider.json"),
                    &json(&component_descriptor(plan, &root.join(filename))?)?,
                )?;
                fs::remove_dir_all(&tree)?;
                File::open(&root)?.sync_all()?;
                plan.source_binding()?;
            }
            if digest_file(&source.path)? != plans[0].source_archive.sha256 {
                return Err("provider batch source changed during extraction".into());
            }
            verify_generation(&plans, pending.path(), true)?;
            File::open(pending.path())?.sync_all()?;
            rustix::fs::renameat_with(
                rustix::fs::CWD,
                pending.path(),
                rustix::fs::CWD,
                &generation,
                rustix::fs::RenameFlags::NOREPLACE,
            )?;
            File::open(&object)?.sync_all()?;
        }
        Err(e) => return Err(e.into()),
    }
    // The original archive is deliberately not needed for a verified component
    // hit. Existing preparation verifies every small archive and every prefix
    // against the current plan, including original manifest and source binding.
    let components = object.join("components");
    directory(&components)?;
    let mut exports = BTreeMap::new();
    for plan in &plans {
        let root = generation.join(&plan.profile);
        let (_, filename) = component_names(&plan.profile)?;
        let descriptor_path = root.join("provider.json");
        let pin = digest_file(&descriptor_path)?;
        let descriptor = descriptor(plan, &descriptor_path, &pin)?;
        let component = components.join(&plan.profile);
        directory(&component)?;
        let prepared = prepare_inner(
            plan,
            &descriptor_path,
            &pin,
            &component.join("assets"),
            &component.join("prefixes"),
            Some(&root.join(filename)),
            download,
        )?;
        exports.insert(
            plan.profile.clone(),
            Export {
                descriptor: descriptor_path,
                descriptor_sha256: pin,
                archive: root.join(filename),
                component_sha256: descriptor.asset.sha256,
                component_bytes: descriptor.asset.bytes,
                provider_source_commit: plan.source_commit.clone(),
                tree_sha256: prepared.prepared.tree_sha256.clone(),
                origin: prepared.prepared,
                published_release: false,
                provider_executed: false,
                source_binding: plan.source_binding()?,
            },
        );
    }
    for plan in &plans {
        plan.source_binding()?;
    }
    verify_generation(&plans, &generation, false)?;
    for export in exports.values() {
        if digest_file(&export.descriptor)? != export.descriptor_sha256
            || digest_file(&export.archive)? != export.component_sha256
        {
            return Err("provider batch component changed during preparation".into());
        }
    }
    let result = BatchExport {
        schema: 1,
        source_archive_sha256: plans[0].source_archive.sha256.clone(),
        source_archive_passes: passes,
        source_archive_bytes: if passes == 1 {
            plans[0].source_archive.bytes
        } else {
            0
        },
        transferred_bytes: transferred,
        elapsed_ms: started.elapsed().as_millis(),
        exports,
    };
    let pending = tempfile::Builder::new()
        .prefix(".provider-batch-output-")
        .tempdir_in(&parent)?;
    write(&pending.path().join("batch.json"), &json(&result)?)?;
    File::open(pending.path())?.sync_all()?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        pending.path(),
        rustix::fs::CWD,
        output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    File::open(parent)?.sync_all()?;
    Ok(result)
}

/// All headers and the complete gzip tail are consumed, including unselected
/// old project files. Only independently pinned selected members are written.
fn unpack_original(plans: &[&Plan], archive: &Path, root: &Path) -> Result<()> {
    let trees: Vec<_> = plans
        .iter()
        .map(|plan| expected(plan))
        .collect::<Result<_>>()?;
    let mut selected: BTreeMap<PathBuf, Vec<(usize, PathBuf)>> = BTreeMap::new();
    let mut written: Vec<BTreeSet<PathBuf>> = vec![BTreeSet::new(); plans.len()];
    for (index, plan) in plans.iter().enumerate() {
        directory(&root.join(&plan.profile))?;
        directory(&root.join(&plan.profile).join("tree"))?;
        for (destination, member) in &plan.members {
            selected
                .entry(member.source.clone())
                .or_default()
                .push((index, destination.clone()));
        }
    }
    let bound = 8 * 1024 * 1024 * 1024u64;
    let decoder = flate2::read::MultiGzDecoder::new(regular(archive)?);
    let mut tar = tar::Archive::new(decoder.take(bound + 1));
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for item in tar.entries()?.raw(true) {
        let mut item = item?;
        let raw = item.path()?.into_owned();
        let path = if item.header().entry_type().is_dir() {
            PathBuf::from(
                raw.to_str()
                    .ok_or("non-UTF8 provider directory")?
                    .trim_end_matches('/'),
            )
        } else {
            raw
        };
        safe(&path)?;
        if !seen.insert(path.clone()) || seen.len() > 100_000 {
            return Err("duplicate or excessive provider archive members".into());
        }
        total = total
            .checked_add(item.size())
            .ok_or("provider archive size overflow")?;
        if total > bound {
            return Err("provider archive exceeds expansion bound".into());
        }
        let Some(destinations) = selected.get(&path) else {
            std::io::copy(&mut item, &mut std::io::sink())?;
            continue;
        };
        if item.header().mode()? & 0o6000 != 0 {
            return Err("set-id provider member".into());
        }
        let kind = item.header().entry_type();
        let mut outputs = Vec::new();
        for (index, destination) in destinations {
            let expected = trees[*index]
                .entries
                .get(destination)
                .ok_or("unknown provider destination")?;
            match expected {
                Entry::Symlink { target } if kind.is_symlink() && item.size() == 0 => {
                    if item.link_name()?.as_deref() != Some(target.as_path()) {
                        return Err("provider alias target differs from the fixed release".into());
                    }
                    written[*index].insert(destination.clone());
                }
                Entry::File {
                    bytes,
                    mode: expected_mode,
                    ..
                } if kind.is_file()
                    && item.size() == *bytes
                    && mode(item.header().mode()?) == *expected_mode =>
                {
                    let target = root
                        .join(&plans[*index].profile)
                        .join("tree")
                        .join(destination);
                    fs::create_dir_all(target.parent().ok_or("missing provider parent")?)?;
                    let file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                        .open(target)?;
                    outputs.push((*index, destination, file, expected));
                }
                _ => return Err("provider member kind/size/mode differs from pinned table".into()),
            }
        }
        if outputs.is_empty() {
            continue;
        }
        let mut digest = Sha256::new();
        let mut count = 0u64;
        let mut metadata = Vec::new();
        let mut buffer = [0; 65536];
        let capture_manifest = outputs
            .iter()
            .any(|(_, destination, _, _)| destination.as_path() == Path::new(MANIFEST_PATH));
        loop {
            let n = item.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > item.size() {
                return Err("provider member exceeds its fixed size".into());
            }
            digest.update(&buffer[..n]);
            for (_, _, file, _) in &mut outputs {
                file.write_all(&buffer[..n])?;
            }
            if capture_manifest {
                if count > 4 * 1024 * 1024 {
                    return Err("provider manifest exceeds 4 MiB".into());
                }
                metadata.extend_from_slice(&buffer[..n]);
            }
        }
        let actual = format!("{:x}", digest.finalize());
        for (index, destination, file, expected) in outputs {
            let Entry::File {
                bytes,
                sha256,
                mode,
            } = expected
            else {
                unreachable!()
            };
            if count != *bytes || actual != *sha256 {
                return Err("provider member differs from original pinned bytes".into());
            }
            if destination == Path::new(MANIFEST_PATH) {
                manifest(plans[index], &metadata)?;
            }
            file.set_permissions(fs::Permissions::from_mode(*mode))?;
            file.sync_all()?;
            written[index].insert(destination.clone());
        }
    }
    let mut decoder = tar.into_inner();
    let mut tail = [0; 10240];
    let mut padding = 0;
    loop {
        let n = decoder.read(&mut tail)?;
        if n == 0 {
            break;
        }
        padding += n;
        if padding > 10240 || tail[..n].iter().any(|byte| *byte != 0) {
            return Err("unexpected provider archive tail".into());
        }
    }
    if decoder.limit() == 0 {
        return Err("provider archive exhausted expansion bound".into());
    }
    for (index, plan) in plans.iter().enumerate() {
        let tree = root.join(&plan.profile).join("tree");
        fs::create_dir_all(tree.join("provenance"))?;
        write(
            &tree.join(PROVENANCE),
            &json(&(plan.provenance_kind(), *plan))?,
        )?;
        written[index].insert(PROVENANCE.into());
        if let Some((_, artifact, bytes, _)) = plan.contract() {
            write(&tree.join(artifact), bytes)?;
            written[index].insert(artifact.into());
        }
        for (path, entry) in &trees[index].entries {
            if !matches!(entry, Entry::Directory) && !written[index].contains(path) {
                return Err("required provider member missing from archive".into());
            }
            if let Entry::Symlink { target } = entry {
                let path = tree.join(path);
                fs::create_dir_all(path.parent().ok_or("missing provider alias parent")?)?;
                symlink(target, path)?;
            }
        }
        trees[index].verify(&tree)?;
        plan.source_binding()?;
    }
    Ok(())
}

pub(super) fn cli(args: &[String]) -> Result<()> {
    let mut values = BTreeMap::new();
    let mut paths = Vec::new();
    let mut mode = false;
    let mut args = args.iter();
    while let Some(key) = args.next() {
        if key == "--export-released-provider-batch" && !mode {
            mode = true;
            continue;
        }
        let value = args.next().ok_or("missing provider batch argument")?;
        if key == "--plan" {
            paths.push(value);
        } else if !["--cache", "--store", "--output", "--source-root"].contains(&key.as_str())
            || values.insert(key.as_str(), value).is_some()
        {
            return Err("mixed/duplicate/unknown provider batch arguments".into());
        }
    }
    if !mode {
        return Err("choose provider batch export".into());
    }
    let mut plans = Vec::new();
    for path in paths {
        let mut plan = Plan::read(Path::new(path))?;
        if plan.source_contract_sha256.is_some() {
            plan.bind_source(Path::new(
                values
                    .get("--source-root")
                    .ok_or("source-bound batch requires --source-root")?,
            ))?;
        }
        plans.push(plan);
    }
    let result = export_batch(
        &plans,
        Path::new(values.get("--cache").ok_or("--cache required")?),
        Path::new(values.get("--store").ok_or("--store required")?),
        Path::new(values.get("--output").ok_or("--output required")?),
        &assets::Curl,
    )?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_pass_matches_legacy_archives_and_warm_reuse_needs_no_original() {
        let temp = tempfile::tempdir().unwrap();
        let (first, download) = super::super::tests::fixture();
        let mut second: Plan =
            serde_json::from_slice(&serde_json::to_vec(&first).unwrap()).unwrap();
        second.profile = "raylib600-shared".into();
        let plans = [first, second];
        let cold = export_batch_inner(
            &plans,
            &temp.path().join("raw"),
            &temp.path().join("batch"),
            &temp.path().join("cold"),
            &download,
        )
        .unwrap();
        assert_eq!(cold.source_archive_passes, 1);
        assert_eq!(download.calls.get(), 1);
        for (index, plan) in plans.iter().enumerate() {
            let legacy = export_inner(
                plan,
                &temp.path().join(format!("raw-{index}")),
                &temp.path().join(format!("origin-{index}")),
                &temp.path().join(format!("legacy-{index}")),
                &download,
            )
            .unwrap();
            let current = &cold.exports[&plan.profile];
            assert_eq!(
                fs::read(&legacy.archive).unwrap(),
                fs::read(&current.archive).unwrap()
            );
            assert_eq!(
                fs::read(&legacy.descriptor).unwrap(),
                fs::read(&current.descriptor).unwrap()
            );
            assert_eq!(legacy.tree_sha256, current.tree_sha256);
        }
        fs::remove_dir_all(temp.path().join("raw")).unwrap();
        let calls = download.calls.get();
        let warm = export_batch_inner(
            &plans,
            &temp.path().join("missing-raw"),
            &temp.path().join("batch"),
            &temp.path().join("warm"),
            &download,
        )
        .unwrap();
        assert_eq!(warm.source_archive_passes, 0);
        assert_eq!(warm.transferred_bytes, 0);
        assert_eq!(download.calls.get(), calls);
        for value in warm.exports.values() {
            assert_eq!(value.origin.copied_bytes, 0);
            assert_eq!(
                digest_file(&value.origin.asset.path).unwrap(),
                value.origin.asset.sha256
            );
        }
    }

    #[test]
    fn damaged_component_is_rejected_without_redownload_or_output() {
        let temp = tempfile::tempdir().unwrap();
        let (plan, download) = super::super::tests::fixture();
        let plans = [plan];
        let cold = export_batch_inner(
            &plans,
            &temp.path().join("raw"),
            &temp.path().join("batch"),
            &temp.path().join("cold"),
            &download,
        )
        .unwrap();
        let archive = &cold.exports[&plans[0].profile].archive;
        fs::set_permissions(archive, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(archive, b"tampered").unwrap();
        let calls = download.calls.get();
        assert!(export_batch_inner(
            &plans,
            &temp.path().join("raw"),
            &temp.path().join("batch"),
            &temp.path().join("failed"),
            &download
        )
        .is_err());
        assert_eq!(download.calls.get(), calls);
        assert!(!temp.path().join("failed").exists());
        assert!(temp.path().join("cold/batch.json").is_file());
    }

    #[test]
    fn missing_member_publishes_neither_generation_nor_output() {
        let temp = tempfile::tempdir().unwrap();
        let (mut plan, download) = super::super::tests::fixture();
        plan.members
            .get_mut(Path::new("lib/libfixture.so.1"))
            .unwrap()
            .source = "missing.so".into();
        assert!(export_batch_inner(
            &[plan],
            &temp.path().join("raw"),
            &temp.path().join("batch"),
            &temp.path().join("failed"),
            &download
        )
        .is_err());
        assert!(!temp.path().join("failed").exists());
        for object in fs::read_dir(temp.path().join("batch")).unwrap() {
            assert!(!object.unwrap().path().join("generation").exists());
        }
    }

    #[test]
    fn forged_descriptor_cannot_authorize_a_different_cached_payload() {
        let temp = tempfile::tempdir().unwrap();
        let (plan, download) = super::super::tests::fixture();
        let plans = [plan];
        let cold = export_batch_inner(
            &plans,
            &temp.path().join("raw"),
            &temp.path().join("batch"),
            &temp.path().join("cold"),
            &download,
        )
        .unwrap();
        let export = &cold.exports[&plans[0].profile];
        fs::set_permissions(&export.archive, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(&export.archive, b"forged component").unwrap();
        fs::set_permissions(&export.descriptor, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(
            &export.descriptor,
            json(&component_descriptor(&plans[0], &export.archive).unwrap()).unwrap(),
        )
        .unwrap();
        assert!(export_batch_inner(
            &plans,
            &temp.path().join("raw"),
            &temp.path().join("batch"),
            &temp.path().join("forged"),
            &download,
        )
        .is_err());
        assert!(!temp.path().join("forged").exists());
    }

    #[test]
    fn forged_descriptor_cannot_authorize_repacked_identical_members() {
        let temp = tempfile::tempdir().unwrap();
        let (plan, download) = super::super::tests::fixture();
        let plans = [plan];
        let cold = export_batch_inner(
            &plans,
            &temp.path().join("raw"),
            &temp.path().join("batch"),
            &temp.path().join("cold"),
            &download,
        )
        .unwrap();
        let export = &cold.exports[&plans[0].profile];
        let mut bytes = fs::read(&export.archive).unwrap();
        bytes[4] ^= 1; // gzip timestamp; the validated members remain identical.
        fs::set_permissions(&export.archive, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(&export.archive, bytes).unwrap();
        fs::set_permissions(&export.descriptor, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(
            &export.descriptor,
            json(&component_descriptor(&plans[0], &export.archive).unwrap()).unwrap(),
        )
        .unwrap();
        let calls = download.calls.get();
        assert!(export_batch_inner(
            &plans,
            &temp.path().join("absent-raw"),
            &temp.path().join("batch"),
            &temp.path().join("repacked"),
            &download,
        )
        .unwrap_err()
        .to_string()
        .contains("canonical export"));
        assert_eq!(download.calls.get(), calls);
        assert!(!temp.path().join("repacked").exists());
    }
}
