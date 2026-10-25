// Actions 전용 작은 leaf. GUI/decoder/주행/원문 traffic을 실행하지 않는다.
#include "openpilot/tools/jotpluggler/dbc_paths.h"

#include <iostream>
#include <iterator>
#include <vector>

namespace fs = std::filesystem;

static void require(bool value, const char *reason) {
  if (!value) throw std::runtime_error(reason);
}

static std::string read(const fs::path &path) {
  require(fs::file_size(path) <= 1024 * 1024, "leaf input exceeds bound");
  std::ifstream stream(path);
  require(stream.good(), "leaf input cannot be opened");
  return {std::istreambuf_iterator<char>(stream), std::istreambuf_iterator<char>()};
}

template <typename Function>
static void refused(Function function, std::string_view reason) {
  bool caught = false;
  try {
    function();
  } catch (const std::runtime_error &error) {
    caught = std::string_view(error.what()).find(reason) != std::string_view::npos;
  }
  require(caught, "required staging refusal was not observed");
}

int main(int argc, char **argv) {
  try {
    require(argc == 4, "usage: dbc-paths-557 SOURCE STAGED_DBC NEW_CONTROL_DIRECTORY");
    const fs::path source = fs::canonical(argv[1]);
    const fs::path staged(argv[2]);
    const fs::path control(argv[3]);
    require(control.is_absolute() && fs::weakly_canonical(control) == control && !fs::exists(control),
            "control directory must be a new canonical absolute path");
    require(fs::create_directory(control), "cannot create control directory");
    fs::permissions(control, fs::perms::owner_all, fs::perm_options::replace);
    const auto edited = control / "edited";
    fs::create_directory(edited);
    std::ofstream(edited / "foreign.dbc") << "foreign-preserved\n";
    const auto factory = jot_dbc_paths::factory_root(source, staged);
    for (const std::string name : {"hyundai_canfd_corner_radar_180_generated", "hyundai_canfd_corner_radar_235_generated"}) {
      const auto selected = jot_dbc_paths::resolve(factory, edited, name, true);
      require(selected == staged / (name + ".dbc"), "fresh factory catalog was not selected");
      require(read(selected) != read(source / "opendbc_repo/opendbc/dbc" / (name + ".dbc")),
              "card and fresh factory variants must remain distinct");
      std::cout << selected.string() << '\n';
    }
    require(jot_dbc_paths::resolve(factory, edited, "foreign", true) == edited / "foreign.dbc",
            "foreign edited file was not preserved");
    require(read(edited / "foreign.dbc") == "foreign-preserved\n", "foreign edited bytes changed");
    refused([&] { jot_dbc_paths::resolve(factory, edited, "../foreign", true); }, "invalid DBC name");
    const auto alias = control / "alias";
    fs::create_directory_symlink(staged, alias);
    refused([&] { jot_dbc_paths::factory_root(source, alias); }, "canonical");
    refused([&] { jot_dbc_paths::factory_root(source, source / "opendbc_repo/opendbc/dbc"); }, "canonical");
    const auto wrong = control / "wrong";
    fs::create_directory(wrong);
    fs::permissions(wrong, fs::perms::owner_all, fs::perm_options::replace);
    std::ofstream(wrong / ".ready") << "openpilot-scons-dbc-v0\n";
    refused([&] { jot_dbc_paths::factory_root(source, wrong); }, "wrong role");
    std::cout << "JOT_DBC_PATHS_557 controls=4 actual_variants=2 foreign_preserved=true\n";
    // 증거/foreign 파일은 proof 아래 남긴다. 이 leaf에는 재귀 삭제가 없다.
    return 0;
  } catch (const std::exception &error) {
    std::cerr << "dbc-paths-557: " << error.what() << '\n';
    return 1;
  }
}
