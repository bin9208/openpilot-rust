"""One-off scratch graft into a detached diagnostic checkout; never maintained."""
import hashlib
import sys
from pathlib import Path

path = Path(sys.argv[1])
raw = path.read_bytes()
assert hashlib.sha256(raw).hexdigest() == '4ff1fecc57a413a3846fcd9ba1ab8348f5bd33b489c7490671083f643a582374'
text = raw.decode()
before_loop = '    for capture in captures.as_array().unwrap() {\n'
assert text.count(before_loop) == 1
text = text.replace(before_loop, before_loop + '''        if capture["name"].as_str() != Some("strict") {
            continue;
        }
''')
after_run = '        let result = command.output().unwrap();\n'
# One earlier test also uses command.output(); patch only the matrix body.
assert text[text.index(before_loop):].count(after_run) == 1
position = text.index(after_run, text.index(before_loop)) + len(after_run)
graft = '''        let destination = PathBuf::from(std::env::var_os("UI_APPLICATION_DIAGNOSTIC_OUTPUT").expect("diagnostic output"));
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("native.stdout"), &result.stdout).unwrap();
        fs::write(destination.join("native.stderr"), &result.stderr).unwrap();
        fs::write(destination.join("native.exit"), format!("{:?}\\n", result.status.code())).unwrap();
        if output.exists() { fs::copy(&output, destination.join("native.png")).unwrap(); }
        if output.with_extension("json").exists() {
            fs::copy(output.with_extension("json"), destination.join("native.json")).unwrap();
        }
        eprintln!("UI_STRICT_DIAGNOSTIC exit={:?}\\n{}", result.status.code(), String::from_utf8_lossy(&result.stderr));
'''
text = text[:position] + graft + text[position:]
path.write_text(text)
