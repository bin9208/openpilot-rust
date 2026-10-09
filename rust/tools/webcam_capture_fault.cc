// External getter-failure fixture; never linked into the production runtime.
#include <opencv2/videoio.hpp>
#include <atomic>
#include <dlfcn.h>
#include <cstdlib>
#include <cstring>
#include <stdexcept>
#include <unistd.h>

static std::atomic<bool> info_observed{false};

static bool fault(const char *name) {
  const char *selected = std::getenv("WEBCAM_CAPTURE_FAULT");
  const char *root = std::getenv("WEBCAM_OWNED_ROOT");
  const char *prefix = std::getenv("OPENPILOT_PREFIX");
  return root && prefix && root[0] == '/' && prefix[0] && selected &&
         std::strcmp(selected, name) == 0;
}

double cv::VideoCapture::get(int property) const {
  if (fault("get")) throw std::runtime_error("owned capture get failure");
  using Function = double (*)(const cv::VideoCapture *, int);
  auto actual = reinterpret_cast<Function>(dlsym(RTLD_NEXT, "_ZNK2cv12VideoCapture3getEi"));
  if (!actual) _exit(123);
  const double value = actual(this, property);
  if (property == cv::CAP_PROP_FPS) info_observed.store(true);
  return value;
}

bool cv::VideoCapture::isOpened() const {
  if (fault("closed") && info_observed.load() && cap.empty() && icap.empty())
    throw std::runtime_error("owned capture closed-state failure");
  using Function = bool (*)(const cv::VideoCapture *);
  auto actual = reinterpret_cast<Function>(dlsym(RTLD_NEXT, "_ZNK2cv12VideoCapture8isOpenedEv"));
  if (!actual) _exit(124);
  return actual(this);
}
