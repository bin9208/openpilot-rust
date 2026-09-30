// Only external fixture paths are redirected; original file readers execute unchanged.
#include <cstdlib>
#include <map>
#include <string>

using Text = std::string;
using Files = std::map<Text, Text>;
Text original_read(const Text &) asm("__real__ZN4util9read_fileERKNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEE");
Text fixture_read(const Text &) asm("__wrap__ZN4util9read_fileERKNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEE");
Files original_directory(const Text &) asm("__real__ZN4util17read_files_in_dirERKNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEE");
Files fixture_directory(const Text &) asm("__wrap__ZN4util17read_files_in_dirERKNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEE");
Text fixture_read(const Text &path) {
  const char *fixture = std::getenv("BOOTLOG_LAUNCH_PATH");
  return original_read(path == "/tmp/launch_log" && fixture ? Text(fixture) : path);
}
Files fixture_directory(const Text &path) {
  const char *fixture = std::getenv("BOOTLOG_PSTORE_PATH");
  return original_directory(path == "/sys/fs/pstore" && fixture ? Text(fixture) : path);
}
