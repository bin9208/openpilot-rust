"""Add a diagnostic-only bin copy in the exact checked-out locked workspace."""
from pathlib import Path
import sys

root = Path(sys.argv[1]).resolve()
destination = root / 'rust/crates/logmessaged/src/bin/panda-diag-logmessaged.rs'
original = root / 'rust/crates/logmessaged/src/main.rs'
source = original.read_text()
needle = '            let bytes = openpilot_logmessaged::record_bytes::join(parts);\n'
assert source.count(needle) == 1
capture = '''            if let Some(path) = std::env::var_os("PANDA_DIAG_INGRESS") {
                use std::io::Write;
                let mut capture = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
                capture.write_all(&(bytes.len() as u64).to_le_bytes())?;
                capture.write_all(&bytes)?;
            }
'''
destination.parent.mkdir(parents=True, exist_ok=True)
assert not destination.exists(), 'diagnostic collector copy must be new'
destination.write_text(source.replace(needle, needle + capture))
print(destination)
