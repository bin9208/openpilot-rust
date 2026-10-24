// Mechanical Xlib/XTest ABI only. Lifecycle/assertions/timeouts stay in Rust.
// Xlib's public protocol types are stable LP64 ABI; no candidate UI policy.
struct ApplicationDisplay;
using ApplicationWindow = unsigned long;
struct ApplicationXError {
  int type;
  ApplicationDisplay *display;
  unsigned long resourceid, serial;
  unsigned char error_code, request_code, minor_code;
};
static int application_x_errors = 0;
static int application_x_error(ApplicationDisplay *, ApplicationXError *) {
  ++application_x_errors;
  return 0;
}
struct ApplicationX11 {
  void *x = nullptr, *xt = nullptr;
  ApplicationDisplay *display = nullptr;
  template <class T> T symbol(void *library, const char *name) {
    auto pointer = dlsym(library, name);
    if (!pointer) throw std::runtime_error(std::string("missing X11 symbol ") + name);
    return reinterpret_cast<T>(pointer);
  }
  template <class T> T get(const char *name) { return symbol<T>(x, name); }
  template <class T> T test(const char *name) { return symbol<T>(xt, name); }
  void open(const std::string &name) {
    if (display) throw std::runtime_error("duplicate X11 connection");
    x = dlopen("libX11.so.6", RTLD_NOW | RTLD_LOCAL);
    xt = dlopen("libXtst.so.6", RTLD_NOW | RTLD_LOCAL);
    if (!x || !xt) throw std::runtime_error("X11/XTest native library unavailable");
    display = get<ApplicationDisplay *(*)(const char *)>("XOpenDisplay")(name.c_str());
    if (!display) throw std::runtime_error("X11 display unavailable");
    using ErrorHandler = int (*)(ApplicationDisplay *, ApplicationXError *);
    get<ErrorHandler (*)(ErrorHandler)>("XSetErrorHandler")(application_x_error);
    application_x_errors = 0;
  }
  void ready() { if (!display) throw std::runtime_error("X11 display not open"); }
  ApplicationWindow root() { ready(); return get<ApplicationWindow (*)(ApplicationDisplay *)>("XDefaultRootWindow")(display); }
  void sync() { ready(); get<int (*)(ApplicationDisplay *, int)>("XSync")(display, 0); }
  ApplicationWindow find(const std::string &title) {
    ApplicationWindow root_window = 0, parent = 0, *children = nullptr;
    unsigned count = 0;
    if (!get<int (*)(ApplicationDisplay *, ApplicationWindow, ApplicationWindow *, ApplicationWindow *, ApplicationWindow **, unsigned *)>("XQueryTree")(display, root(), &root_window, &parent, &children, &count))
      throw std::runtime_error("X11 tree query failed");
    ApplicationWindow found = 0;
    for (unsigned index = 0; index < count; ++index) {
      char *name = nullptr;
      if (get<int (*)(ApplicationDisplay *, ApplicationWindow, char **)>("XFetchName")(display, children[index], &name) && name && title == name) found = children[index];
      if (name) get<int (*)(void *)>("XFree")(name);
      if (found) break;
    }
    if (children) get<int (*)(void *)>("XFree")(children);
    return found;
  }
  void focus(ApplicationWindow window) { ready(); get<int (*)(ApplicationDisplay *, ApplicationWindow, int, unsigned long)>("XSetInputFocus")(display, window, 2, 0); }
  void key(const std::string &name, bool down) {
    ready();
    auto keysym = get<unsigned long (*)(const char *)>("XStringToKeysym")(name.c_str());
    auto code = get<unsigned char (*)(ApplicationDisplay *, unsigned long)>("XKeysymToKeycode")(display, keysym);
    if (!test<int (*)(ApplicationDisplay *, unsigned, int, unsigned long)>("XTestFakeKeyEvent")(display, code, int(down), 0)) throw std::runtime_error("XTest key request failed");
  }
  void motion(ApplicationWindow window, int x, int y) {
    ready(); ApplicationWindow child = 0; int target_x = 0, target_y = 0;
    if (!get<int (*)(ApplicationDisplay *, ApplicationWindow, ApplicationWindow, int, int, int *, int *, ApplicationWindow *)>("XTranslateCoordinates")(display, window, root(), x, y, &target_x, &target_y, &child)) throw std::runtime_error("X11 coordinate translation failed");
    if (!test<int (*)(ApplicationDisplay *, int, int, int, unsigned long)>("XTestFakeMotionEvent")(display, -1, target_x, target_y, 0)) throw std::runtime_error("XTest motion request failed");
  }
  void button(bool down) { ready(); if (!test<int (*)(ApplicationDisplay *, unsigned, int, unsigned long)>("XTestFakeButtonEvent")(display, 1, int(down), 0)) throw std::runtime_error("XTest button request failed"); }
  void close() { if (display) { get<int (*)(ApplicationDisplay *)>("XCloseDisplay")(display); display = nullptr; } }
  ~ApplicationX11() { close(); if (xt) dlclose(xt); if (x) dlclose(x); }
};
