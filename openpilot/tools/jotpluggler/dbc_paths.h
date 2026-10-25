#pragma once

// SCons factory catalog와 사용자 편집 디렉터리를 구분한다. decoder는 바꾸지 않는다.
#include <filesystem>
#include <fstream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <sys/stat.h>
#include <unistd.h>

namespace jot_dbc_paths {
namespace fs = std::filesystem;

inline fs::path factory_root(const fs::path &repository, const fs::path &staged) {
  if (staged.empty()) return repository / "opendbc_repo/opendbc/dbc";
  const fs::path canonical = fs::canonical(staged);
  const fs::path source = fs::canonical(repository);
  const auto inside = [](const fs::path &child, const fs::path &parent) {
    const auto relative = child.lexically_relative(parent);
    return !relative.empty() && *relative.begin() != "..";
  };
  if (!staged.is_absolute() || canonical != staged || inside(canonical, source) || inside(source, canonical) ||
      fs::equivalent(canonical, source / "opendbc_repo/opendbc/dbc")) {
    throw std::runtime_error("Jotpluggler DBC stage is not an external canonical directory");
  }
  struct stat metadata {};
  if (::lstat(staged.c_str(), &metadata) != 0 || !S_ISDIR(metadata.st_mode) ||
      metadata.st_uid != ::getuid() || (metadata.st_mode & 0777) != 0700) {
    throw std::runtime_error("Jotpluggler DBC stage is not an owned private directory");
  }
  const auto ready = staged / ".ready";
  constexpr std::string_view expected = "openpilot-scons-dbc-v1\n";
  if (!fs::is_regular_file(fs::symlink_status(ready)) || fs::file_size(ready) != expected.size()) {
    throw std::runtime_error("Jotpluggler DBC stage is not ready");
  }
  std::ifstream stream(ready);
  std::string marker(expected.size(), '\0');
  stream.read(marker.data(), static_cast<std::streamsize>(marker.size()));
  if (!stream || std::string_view(marker) != expected) {
    throw std::runtime_error("Jotpluggler DBC stage has the wrong role");
  }
  return canonical;
}

inline fs::path resolve(const fs::path &factory, const fs::path &edited, const std::string &name, bool staged = false) {
  if (staged && (name.empty() || fs::path(name).filename() != name || name == "." || name == "..")) {
    throw std::runtime_error("invalid DBC name");
  }
  for (const auto &root : {factory, edited}) {
    const auto path = root / (name + ".dbc");
    if (fs::exists(path)) {
      if (staged && root == factory && !fs::is_regular_file(fs::symlink_status(path))) {
        throw std::runtime_error("DBC is not a regular file");
      }
      return path;
    }
  }
  throw std::runtime_error("DBC not found: " + name);
}
}  // namespace jot_dbc_paths
