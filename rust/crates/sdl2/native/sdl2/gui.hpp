#pragma once
#include "rust/cxx.h"
#include <memory>
namespace openpilot::sdlgui {
struct Event; struct Size; struct Image; struct Api;
class Window; class Font;
class Context {
    std::shared_ptr<Api> api;
public:
    explicit Context(std::shared_ptr<Api> api);
    ~Context();
    std::unique_ptr<Window> window(rust::Str,int,int,int,int,bool) const;
    std::unique_ptr<Font> font(rust::Str,int,bool) const;
};
class Window {
    struct Impl; std::unique_ptr<Impl> impl;
public:
    Window(std::shared_ptr<Api>,rust::Str,int,int,int,int,bool);
    ~Window();
    Size size() const; rust::Vec<Event> events();
    void present(rust::Slice<const uint8_t>,int,int); Image capture() const;
};
class Font {
    struct Impl; std::unique_ptr<Impl> impl;
public:
    Font(std::shared_ptr<Api>,rust::Str,int,bool); ~Font();
    Image render(rust::Str,uint8_t,uint8_t,uint8_t) const;
};
class Tk {
    struct Impl; std::unique_ptr<Impl> impl;
public:
    Tk(rust::Str,rust::Str,rust::Str); ~Tk(); rust::String eval(rust::Slice<const rust::String>);
};
std::unique_ptr<Context> load(rust::Str,rust::Str);
std::unique_ptr<Tk> load_tk(rust::Str,rust::Str,rust::Str);
}
