#![cfg(feature = "native-skip-miri")]

use openpilot_model_runtime::catalog::{Catalog, Kind};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

fn descriptor() -> Value {
    json!({"version":1,"kind":"driver","camera":[1344,760],
        "nv12":{"stride":1408,"y_height":768,"uv_height":384,"bytes":2428928},
        "sources":{"model":"11".repeat(32)},
        "metadata":{"model_checkpoint":"fixture","input_shapes":{"calib":[1,3]},
            "output_shapes":{"outputs":[1,1]},"output_slices":{"result":[0,1]}},
        "inputs":[{"name":"frame","shape":[2428928],"dtype":"uint8"}],
        "outputs":[{"name":"model","shape":[1,1],"dtype":"float32"}]})
}

fn fixture(descriptor: &Value, directory_name: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let graph = serde_json::to_vec(&json!({
        "version":2,"backend":"cpu-llvm","arch":std::env::consts::ARCH,
        "weights_sha256":"00".repeat(32),"library_sha256":"11".repeat(32),
        "allocations":[{"bytes":2428932,"weight_offset":null}],
        "views":[{"allocation":0,"offset":0,"bytes":2428928},{"allocation":0,"offset":2428928,"bytes":4}],
        "inputs":[{"name":"frame","view":0}],"outputs":[{"name":"model","view":1}],
        "kernels":[],"calls":[],
        "entrypoints":[{"name":"prepare","start":0,"end":0},{"name":"model","start":0,"end":0}]
    })).unwrap();
    let pipeline = serde_json::to_vec(descriptor).unwrap();
    let index = serde_json::to_vec(&json!({"version":1,"bundles":[{
        "kind":"driver","camera":[1344,760],"directory":directory_name,
        "graph_sha256":format!("{:x}",Sha256::digest(&graph)),
        "pipeline_sha256":format!("{:x}",Sha256::digest(&pipeline))
    }]}))
    .unwrap();
    let generation = format!("{:x}", Sha256::digest(&index));
    let path = root.path().join(&generation).join("driver-1344x760");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("graph.json"), graph).unwrap();
    fs::write(path.join("pipeline.json"), pipeline).unwrap();
    fs::write(root.path().join(&generation).join("index.json"), index).unwrap();
    fs::write(
        root.path().join("current.json"),
        serde_json::to_vec(&json!({"version":1,"generation":generation})).unwrap(),
    )
    .unwrap();
    root
}

#[test]
fn selects_verified_metadata_without_loading_executable_kernels() {
    let root = fixture(&descriptor(), "driver-1344x760");
    let catalog = Catalog::load(root.path()).unwrap();
    let bundle = catalog.select(Kind::Driver, [1344, 760]).unwrap();
    assert_eq!(bundle.descriptor.metadata.output_slices["result"], [0, 1]);
    assert!(bundle.directory.ends_with("driver-1344x760"));
    assert!(catalog.select(Kind::Driving, [1344, 760]).is_err());
}

#[test]
fn rejects_metadata_that_disagrees_with_the_graph_or_camera() {
    for (pointer, replacement) in [
        ("/version", json!(2)),
        ("/kind", json!("driving")),
        ("/camera", json!([1928, 1208])),
        ("/nv12/bytes", json!(1)),
        ("/nv12/stride", json!(1409)),
        ("/inputs/0/shape", json!([2428927])),
        ("/inputs/0/name", json!("wrong")),
        ("/outputs/0/dtype", json!("float64")),
        ("/outputs/0/shape", json!([u64::MAX, 2])),
        ("/sources/model", json!("bad")),
        ("/metadata/output_slices/result", json!([0, 2])),
        ("/metadata/output_shapes/outputs", json!([1, 2])),
        ("/metadata/input_shapes/calib", json!([0, 3])),
    ] {
        let mut value = descriptor();
        *value.pointer_mut(pointer).unwrap() = replacement;
        let root = fixture(&value, "driver-1344x760");
        assert!(Catalog::load(root.path()).is_err(), "accepted {pointer}");
    }
}

#[test]
fn rejects_traversal_and_tampered_generation() {
    let root = fixture(&descriptor(), "../driver-1344x760");
    assert!(Catalog::load(root.path()).is_err());
    let root = fixture(&descriptor(), "driver-1344x760");
    let catalog = Catalog::load(root.path()).unwrap();
    let bundle = catalog.select(Kind::Driver, [1344, 760]).unwrap();
    fs::write(bundle.directory.join("pipeline.json"), b"{}").unwrap();
    assert!(Catalog::load(root.path()).is_err());
    fs::write(
        root.path().join("current.json"),
        br#"{"version":1,"generation":"../outside"}"#,
    )
    .unwrap();
    assert!(Catalog::load(root.path()).is_err());
}

#[test]
fn generation_cli_matches_original_metadata_read_and_revalidates_catalog() {
    let original: Value =
        serde_json::from_str(include_str!("fixtures/catalog-generation/original.json")).unwrap();
    let case = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["arch"] == std::env::consts::ARCH)
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    for (name, contents) in case["files"].as_object().unwrap() {
        let path = root.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents.as_str().unwrap()).unwrap();
    }
    let invoke = |flag| {
        std::process::Command::new(env!("CARGO_BIN_EXE_model-run"))
            .arg(flag)
            .arg(root.path())
            .env("PATH", "/nonexistent")
            .output()
            .unwrap()
    };
    let checked = invoke("--check-catalog");
    assert!(checked.status.success(), "{checked:?}");
    let result = invoke("--catalog-generation");
    assert!(result.status.success(), "{result:?}");
    assert_eq!(
        result.stdout,
        case["original_stdout"].as_str().unwrap().as_bytes()
    );
    assert_eq!(
        result.stderr,
        case["original_stderr"].as_str().unwrap().as_bytes()
    );
    let generation = case["original_stdout"].as_str().unwrap().trim_end();
    fs::write(root.path().join(generation).join("index.json"), b"{}").unwrap();
    for flag in ["--check-catalog", "--catalog-generation"] {
        let result = invoke(flag);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty(), "invalid catalog printed metadata");
    }
}

/// Hosted-only bridge proof; full-model output equality is a separate gate.
#[test]
#[ignore = "requires hosted Python/tinygrad/LLVM environment"]
fn guarded_add_bridge_retains_original_and_compiled_source_evidence() {
    use std::process::Command;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let python = std::env::var_os("MODEL_BRIDGE_PYTHON").expect("explicit hosted Python required");
    let helper = std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-guarded-add"));
    let helper_sha = format!("{:x}", Sha256::digest(fs::read(helper).unwrap()));
    let script = r#"
import hashlib,json,os,subprocess,tempfile,time
from pathlib import Path
from types import SimpleNamespace
from model_export.native import compile_kernel, guarded_add_binding, _guarded_add_helper
from model_export.schema import ExportError
from tinygrad.runtime.support.compiler_cpu import CPULLVMCompiler
sha=lambda b:hashlib.sha256(b).hexdigest()
source='define void @fixture(ptr %out, float %x, float %y, i1 %c) {\n %s = select i1 %c, float 0.0, float %y\n %r = fadd nsz contract float %s, %x\n store float %r, ptr %out\n ret void\n}\n'
op=SimpleNamespace(arg=SimpleNamespace(function_name='fixture',globals=[0],vars=[]),src=[None,None,None,SimpleNamespace(arg=source)])
compiler=CPULLVMCompiler(['x86_64','native'])
helper=os.environ['MODEL_LLVM_GUARDED_ADD_BINARY']; digest=os.environ['MODEL_LLVM_GUARDED_ADD_SHA256']
assert guarded_add_binding()['sha256']==digest
kernel=compile_kernel(op,7,compiler)
assert kernel.source==source
assert kernel.evidence is not None
body=kernel.evidence.compiled_source
receipt=json.loads(kernel.evidence.rewrite_receipt)
assert '@op_impl_7(' in body and 'diagnostic_guarded_sum' in body
assert receipt['rewrite_count']==1
assert receipt['original_source_sha256']==sha(source.replace('@fixture(', '@op_impl_7(').encode())
assert receipt['output_source_sha256']==sha(body.encode())
assert kernel.evidence.binding['object_sha256']==sha(kernel.object_code)
assert kernel.evidence.binding['original_source_sha256']==sha(source.encode())
assert kernel.evidence.binding['helper']['sha256']==digest
# Genuine helper failures must terminate export; no original-source compilation.
for bad in [source.replace(' %s =', ' %diagnostic_guarded_sum_0 = fadd float %x, %y\n %s ='), source+' '* (16*1024*1024)]:
 invalid=SimpleNamespace(arg=op.arg,src=[None,None,None,SimpleNamespace(arg=bad)])
 try:compile_kernel(invalid,7,compiler)
 except ExportError:pass
 else:raise AssertionError('failed helper fell back to compilation')
with tempfile.TemporaryDirectory() as td:
 held=Path(td)/'held';held.write_bytes(Path(helper).read_bytes());held.chmod(0o700)
 os.environ['MODEL_LLVM_GUARDED_ADD_BINARY']=str(held)
 try:
  with _guarded_add_helper():held.chmod(0o500)
 except ExportError:pass
 else:raise AssertionError('held helper metadata mutation silently accepted')
os.environ['MODEL_LLVM_GUARDED_ADD_BINARY']=helper
for configuration in [
 {'MODEL_LLVM_GUARDED_ADD_BINARY':helper},
 {'MODEL_LLVM_GUARDED_ADD_SHA256':digest},
 {'MODEL_LLVM_GUARDED_ADD_BINARY':helper,'MODEL_LLVM_GUARDED_ADD_SHA256':'0'*64},
 {'MODEL_LLVM_GUARDED_ADD_BINARY':str(Path('/bin/false').resolve()),'MODEL_LLVM_GUARDED_ADD_SHA256':sha(Path('/bin/false').read_bytes())},
]:
 for key in ['MODEL_LLVM_GUARDED_ADD_BINARY','MODEL_LLVM_GUARDED_ADD_SHA256']:os.environ.pop(key,None)
 os.environ.update(configuration)
 try:compile_kernel(op,7,compiler)
 except ExportError:pass
 else:raise AssertionError('invalid helper configuration silently accepted')
with tempfile.TemporaryDirectory() as td:
 fake=Path(td)/'script';fake.write_bytes(b'#!/bin/sh\nexit 0\n');fake.chmod(0o700)
 os.environ.update(MODEL_LLVM_GUARDED_ADD_BINARY=str(fake),MODEL_LLVM_GUARDED_ADD_SHA256=sha(fake.read_bytes()))
 try:compile_kernel(op,7,compiler)
 except ExportError:pass
 else:raise AssertionError('interpreter helper silently accepted')
# A real native peer leaves a descendant in its process group. Every bridge
# exit must kill that descendant while retaining the leader until cleanup.
peer_source=r'''
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
 if(argc!=3 || strcmp(argv[1],"--receipt")) return 2;
 pid_t descendant=fork(); if(descendant<0) return 2;
 if(!descendant) { close(0);close(1);close(2);for(;;) pause(); }
 FILE *pidfile=fopen(getenv("MODEL_BRIDGE_PEER_PIDFILE"),"w");
 if(!pidfile) return 2;
 fprintf(pidfile,"%ld",(long)descendant);fclose(pidfile);
 const char *mode=getenv("MODEL_BRIDGE_PEER_MODE");
 char input[65536];size_t bytes=0;ssize_t got;
 if(!strcmp(mode,"overrun")) { memset(input,'x',sizeof(input));for(;;) if(write(1,input,sizeof(input))<0) return 3; }
 while((got=read(0,input+bytes,sizeof(input)-bytes))>0) bytes+=(size_t)got;
 if(!strcmp(mode,"timeout")) for(;;) pause();
 if(!strcmp(mode,"nonzero")) return 3;
 FILE *receipt=fopen(argv[2],"w");if(!receipt) return 2;
 fputs(!strcmp(mode,"malformed") ? "{}" : getenv("MODEL_BRIDGE_PEER_RECEIPT"),receipt);fclose(receipt);
 return write(1,input,bytes)==(ssize_t)bytes ? 0 : 3;
}
'''
with tempfile.TemporaryDirectory() as td:
 peer=Path(td)/'peer'
 subprocess.run(['clang','-x','c','-','-o',str(peer)],input=peer_source.encode(),check=True,timeout=30)
 renamed=source.replace('@fixture(', '@op_impl_7(').encode()
 unchanged=dict(status='unchanged',original_source_sha256=sha(renamed),output_source_sha256=sha(renamed),input_bytes=len(renamed),output_bytes=len(renamed),rewrite_count=0,rewrites=[])
 os.environ.update(MODEL_LLVM_GUARDED_ADD_BINARY=str(peer),MODEL_LLVM_GUARDED_ADD_SHA256=sha(peer.read_bytes()),MODEL_BRIDGE_PEER_RECEIPT=json.dumps(unchanged))
 for mode in ['success','nonzero','malformed','overrun','timeout']:
  pidfile=Path(td)/(mode+'.pid')
  os.environ.update(MODEL_BRIDGE_PEER_MODE=mode,MODEL_BRIDGE_PEER_PIDFILE=str(pidfile))
  try:compile_kernel(op,7,compiler)
  except ExportError:
   if mode=='success':raise
  else:assert mode=='success', 'peer error did not terminate export: '+mode
  pid=int(pidfile.read_text())
  for _ in range(100):
   state=Path(f'/proc/{pid}/stat')
   try:observed=state.read_text().rsplit(')',1)[1].split()[0]
   except FileNotFoundError:break
   if observed=='Z':break
   time.sleep(0.01)
  else:raise AssertionError('owned helper descendant survived: '+mode)
 for key in ['MODEL_BRIDGE_PEER_MODE','MODEL_BRIDGE_PEER_PIDFILE','MODEL_BRIDGE_PEER_RECEIPT']:os.environ.pop(key,None)
# Real capture/export verifies per-kernel alignment, including host COPY entries.
os.environ.update(MODEL_LLVM_GUARDED_ADD_BINARY=helper,MODEL_LLVM_GUARDED_ADD_SHA256=digest)
from tinygrad import Tensor,TinyJit
import numpy as np
from model_export import Bindings,export_cpu
from model_export.capture import capture_cpu
# Match the established exporter host-COPY fixture: NPY directly wraps ndarray
# storage; Python-list construction would require unsupported NPY _copyin.
x=Tensor(np.arange(64,dtype=np.float32),device='NPY').realize()
assert x.uop.base.buffer.device=='NPY'
jit=TinyJit(lambda value:(value.to('CPU')*2+1).realize())
jit(x);y=jit(x)
np.testing.assert_array_equal(y.numpy(),np.arange(64,dtype=np.float32)*2+1)
bindings=Bindings({'x':x},{'y':y})
captured=capture_cpu(jit,bindings)
assert len(captured.compilation)==len(captured.sources)
assert any(source.startswith('copy:') for source in captured.sources)
with tempfile.TemporaryDirectory() as td:
 bundle=Path(td)/'bundle';export_cpu(jit,bindings,bundle)
 index=json.loads((bundle/'compilation-evidence.json').read_bytes())
 for i,source in enumerate(captured.sources):
  assert (bundle/f'kernel-{i}.txt').read_bytes()==source.encode()
  if source.startswith('copy:'):
   assert str(i) not in index['kernels'] and not (bundle/f'kernel-{i}.compiled.ll').exists()
  else:
   record=index['kernels'][str(i)]
   assert record['compiled_source_sha256']==sha((bundle/f'kernel-{i}.compiled.ll').read_bytes())
   assert record['rewrite_receipt_sha256']==sha((bundle/f'kernel-{i}.guarded-add.json').read_bytes())
   assert record['object_sha256']==sha((bundle/f'kernel-{i}.o').read_bytes())
for key in ['MODEL_LLVM_GUARDED_ADD_BINARY','MODEL_LLVM_GUARDED_ADD_SHA256']:os.environ.pop(key,None)
assert guarded_add_binding()=={'enabled':False}
assert compile_kernel(op,7,compiler).evidence is None
with tempfile.TemporaryDirectory() as td:
 disabled=Path(td)/'disabled';export_cpu(jit,bindings,disabled)
 assert not (disabled/'compilation-evidence.json').exists()
 assert not list(disabled.glob('*.compiled.ll')) and not list(disabled.glob('*.guarded-add.json'))
 assert set(json.loads((disabled/'graph.json').read_bytes()))=={'version','backend','arch','weights_sha256','library_sha256','allocations','views','inputs','outputs','kernels','calls'}
 for i,source in enumerate(captured.sources):assert (disabled/f'kernel-{i}.txt').read_bytes()==source.encode()
"#;
    let output = Command::new(python)
        .args(["-P", "-c", script])
        .current_dir(root)
        .env(
            "PYTHONPATH",
            format!(
                "{}:{}:{}",
                root.display(),
                root.join("tinygrad_repo").display(),
                root.join("rust/tools").display()
            ),
        )
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("DEV", "CPU:LLVM")
        .env("JIT_BATCH_SIZE", "0")
        .env("MODEL_LLVM_GUARDED_ADD_BINARY", helper)
        .env("MODEL_LLVM_GUARDED_ADD_SHA256", helper_sha)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "bridge failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Real publisher reuse must not retain sidecars from another helper policy.
#[test]
#[ignore = "requires hosted Python/tinygrad/LLVM environment"]
fn guarded_add_publish_reuse_requires_exact_evidence_roster() {
    use std::process::Command;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let python = std::env::var_os("MODEL_BRIDGE_PYTHON").expect("explicit hosted Python required");
    let helper = std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-guarded-add"));
    let helper_sha = format!("{:x}", Sha256::digest(fs::read(helper).unwrap()));
    let script = r#"
import json,os,tempfile
from pathlib import Path
import numpy as np
from tinygrad import Tensor,TinyJit
from model_export import Bindings,ExportError
from model_export.original import Metadata,OriginalPipeline
from model_export.pipeline import Stage,compose
from model_export.publish import BundleSpec,publish
helper=os.environ['MODEL_LLVM_GUARDED_ADD_BINARY'];digest=os.environ['MODEL_LLVM_GUARDED_ADD_SHA256']
inputs=Tensor(np.zeros(16,dtype=np.float32)).realize()
jit=TinyJit(lambda x:(x+2).realize())
jit(inputs);output=jit(inputs)
np.testing.assert_array_equal(output.numpy(),np.full(16,2,dtype=np.float32))
bindings=Bindings({'x':inputs},{'model':output})
stages=(Stage('prepare',jit,{'0':inputs}),)
metadata=Metadata('fixture',{'x':(16,)},{'outputs':(16,)},{'all':(0,16)})
original=OriginalPipeline(metadata,stages,bindings,compose(stages,bindings))
spec=BundleSpec('driving',(1344,760),original,{'model':'12'*32})
def enabled(value):
 for key in ['MODEL_LLVM_GUARDED_ADD_BINARY','MODEL_LLVM_GUARDED_ADD_SHA256']:os.environ.pop(key,None)
 if value:os.environ.update(MODEL_LLVM_GUARDED_ADD_BINARY=helper,MODEL_LLVM_GUARDED_ADD_SHA256=digest)
def snapshot(root):
 result={}
 for path in root.rglob('*'):
  meta=path.lstat()
  identity=(meta.st_dev,meta.st_ino,meta.st_mode,meta.st_size,meta.st_mtime_ns,meta.st_ctime_ns)
  content=('symlink',os.readlink(path)) if path.is_symlink() else ('directory',) if path.is_dir() else ('file',path.read_bytes())
  result[str(path.relative_to(root))]=(identity,content)
 return result
def rejected(root):
 before=snapshot(root)
 try:publish(root,(spec,),'cpu')
 except ExportError as error:assert 'existing generation' in str(error)
 else:raise AssertionError('different immutable evidence roster silently reused')
 assert snapshot(root)==before,'rejected publish changed generation or current pointer'
with tempfile.TemporaryDirectory() as td:
 on,off=Path(td)/'enabled',Path(td)/'disabled'
 publish(on,(spec,),'cpu')
 current=(on/'current.json').read_bytes();generation=json.loads(current)['generation']
 records=list((on/generation).rglob('compilation-evidence.json'))
 receipts=[r for p in records for r in p.parent.glob('*.guarded-add.json')]
 assert records and receipts and all(json.loads(r.read_bytes())['rewrite_count']==0 for r in receipts)
 enabled(False);publish(off,(spec,),'cpu')
 assert (off/'current.json').read_bytes()==current,'zero-rewrite helper unexpectedly changed generation identity'
 assert not list(off.rglob('*.guarded-add.json'))
 rejected(on)  # enabled generation has old evidence absent from new output.
 publish(off,(spec,),'cpu')  # exact disabled generation remains reusable.
 enabled(True);rejected(off)
 publish(on,(spec,),'cpu')  # exact enabled generation remains reusable.
 extra=on/generation/'unused-evidence.json';extra.write_text('{}')
 rejected(on);extra.unlink()
 empty=on/generation/'unused-directory';empty.mkdir()
 rejected(on);empty.rmdir()
 alias=on/generation/'evidence-alias';alias.symlink_to(records[0])
 rejected(on);alias.unlink()
 enabled(False)
"#;
    let output = Command::new(python)
        .args(["-P", "-c", script])
        .current_dir(root)
        .env(
            "PYTHONPATH",
            format!(
                "{}:{}:{}",
                root.display(),
                root.join("tinygrad_repo").display(),
                root.join("rust/tools").display()
            ),
        )
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("DEV", "CPU:LLVM")
        .env("JIT_BATCH_SIZE", "0")
        .env("MODEL_LLVM_GUARDED_ADD_BINARY", helper)
        .env("MODEL_LLVM_GUARDED_ADD_SHA256", helper_sha)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "publisher reuse proof failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
