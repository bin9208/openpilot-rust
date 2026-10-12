#include "openpilot-sdl2/src/gui/bridge.rs.h"
#include "sdl2/abi.hpp"
#include <dlfcn.h>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>
namespace openpilot::sdlgui {
class Library {
    void *handle;
public:
    explicit Library(rust::Str path, bool persistent=false) : handle(dlopen(std::string(path).c_str(),RTLD_NOW|RTLD_LOCAL|(persistent?RTLD_NODELETE:0))) {
        if (!handle) throw std::runtime_error(dlerror());
    }
    ~Library() { dlclose(handle); }
    Library(const Library&)=delete;
    template<typename T> T symbol(const char *name) {
        dlerror(); auto value=dlsym(handle,name); auto error=dlerror();
        if(error) throw std::runtime_error(error);
        return reinterpret_cast<T>(value);
    }
};
struct Api {
    Library sdl,ttf;
#define SDL_FN(name,signature) using name##Type=signature; name##Type name=sdl.symbol<name##Type>(#name)
    SDL_FN(SDL_InitSubSystem,int(*)(Uint32)); SDL_FN(SDL_QuitSubSystem,void(*)(Uint32));
    SDL_FN(SDL_GetError,const char*(*)());
    SDL_FN(SDL_CreateWindow,SDL_Window*(*)(const char*,int,int,int,int,Uint32));
    SDL_FN(SDL_DestroyWindow,void(*)(SDL_Window*)); SDL_FN(SDL_GetWindowSize,void(*)(SDL_Window*,int*,int*));
    SDL_FN(SDL_GetWindowSurface,SDL_Surface*(*)(SDL_Window*)); SDL_FN(SDL_UpdateWindowSurface,int(*)(SDL_Window*));
    SDL_FN(SDL_PollEvent,int(*)(SDL_Event*));
    SDL_FN(SDL_CreateRGBSurfaceWithFormatFrom,SDL_Surface*(*)(void*,int,int,int,int,Uint32));
    SDL_FN(SDL_SetSurfaceBlendMode,int(*)(SDL_Surface*,int));
    SDL_FN(SDL_UpperBlit,int(*)(SDL_Surface*,const SDL_Rect*,SDL_Surface*,SDL_Rect*));
    SDL_FN(SDL_FreeSurface,void(*)(SDL_Surface*)); SDL_FN(SDL_GetClipRect,void(*)(SDL_Surface*,SDL_Rect*));
    SDL_FN(SDL_CreateSoftwareRenderer,SDL_Renderer*(*)(SDL_Surface*));
    SDL_FN(SDL_DestroyRenderer,void(*)(SDL_Renderer*));
    SDL_FN(SDL_RenderReadPixels,int(*)(SDL_Renderer*,const SDL_Rect*,Uint32,void*,int));
#undef SDL_FN
#define TTF_FN(name,signature) using name##Type=signature; name##Type name=ttf.symbol<name##Type>(#name)
    TTF_FN(TTF_Init,int(*)()); TTF_FN(TTF_Quit,void(*)());
    TTF_FN(TTF_OpenFont,TTF_Font*(*)(const char*,int)); TTF_FN(TTF_CloseFont,void(*)(TTF_Font*));
    TTF_FN(TTF_SetFontStyle,void(*)(TTF_Font*,int));
    TTF_FN(TTF_RenderUTF8_Blended,SDL_Surface*(*)(TTF_Font*,const char*,SDL_Color));
    TTF_FN(TTF_FontHeight,int(*)(const TTF_Font*));
#undef TTF_FN
    Api(rust::Str s, rust::Str t) : sdl(s),ttf(t) {
        if(SDL_InitSubSystem(0x20)!=0) fail();
        if(TTF_Init()!=0) { SDL_QuitSubSystem(0x20); fail(); }
    }
    ~Api(){ TTF_Quit(); SDL_QuitSubSystem(0x20); }
    [[noreturn]] void fail() const { throw std::runtime_error(SDL_GetError()); }
    Image image(SDL_Surface *surface) const {
        if(!surface) fail();
        SDL_Rect bounds{}; SDL_GetClipRect(surface,&bounds);
        if(bounds.w<0 || bounds.h<0 || bounds.w>std::numeric_limits<int>::max()/4) throw std::runtime_error("SDL image dimensions invalid");
        Image result; result.width=bounds.w; result.height=bounds.h;
        auto length=uint64_t(bounds.w)*uint64_t(bounds.h)*4;
        if(length>std::numeric_limits<size_t>::max()) throw std::runtime_error("SDL image size overflow");
        result.rgba.reserve(size_t(length)); for(uint64_t i=0;i<length;++i) result.rgba.push_back(0);
        if(!length) return result;
        auto renderer=SDL_CreateSoftwareRenderer(surface); if(!renderer) fail();
        auto status=SDL_RenderReadPixels(renderer,nullptr,0x16762004,result.rgba.data(),bounds.w*4);
        SDL_DestroyRenderer(renderer); if(status!=0) fail(); return result;
    }
};
Context::Context(std::shared_ptr<Api> a):api(std::move(a)){} Context::~Context()=default;
std::unique_ptr<Context> load(rust::Str s, rust::Str t){return std::make_unique<Context>(std::make_shared<Api>(s,t));}
struct Window::Impl { std::shared_ptr<Api> api; SDL_Window *window=nullptr; ~Impl(){if(window)api->SDL_DestroyWindow(window);} };
Window::Window(std::shared_ptr<Api>a,rust::Str title,int w,int h,int x,int y,bool resize):impl(std::make_unique<Impl>()){
    impl->api=std::move(a);
    if(w<=0 || h<=0) throw std::runtime_error("SDL window dimensions invalid");
    impl->window=impl->api->SDL_CreateWindow(std::string(title).c_str(),x,y,w,h,4|(resize?0x20:0));
    if(!impl->window) impl->api->fail();
}
Window::~Window()=default;
std::unique_ptr<Window> Context::window(rust::Str t,int w,int h,int x,int y,bool r)const{return std::make_unique<Window>(api,t,w,h,x,y,r);}
Size Window::size()const{Size result{};impl->api->SDL_GetWindowSize(impl->window,&result.width,&result.height);return result;}
rust::Vec<Event> Window::events(){
    rust::Vec<Event> result; SDL_Event event{};
    while(impl->api->SDL_PollEvent(&event)){
        Event e{};e.kind=event.type;
        switch(event.type){
        case 0x200:e.button=event.window.event;e.x=event.window.data1;e.y=event.window.data2;break;
        case 0x300:e.key=event.key.keysym.sym;e.repeat=event.key.repeat!=0;break;
        case 0x400:e.x=event.motion.x;e.y=event.motion.y;break;
        case 0x401:case 0x402:e.x=event.button.x;e.y=event.button.y;e.button=event.button.button;break;
        default:break;
        } result.push_back(e);
    }return result;
}
void Window::present(rust::Slice<const uint8_t> bytes,int w,int h){
    if(w<=0 || h<=0 || w>std::numeric_limits<int>::max()/4 || uint64_t(w)*uint64_t(h)*4!=bytes.size()) throw std::runtime_error("SDL present buffer invalid");
    auto surface=impl->api->SDL_CreateRGBSurfaceWithFormatFrom(const_cast<uint8_t*>(bytes.data()),w,h,32,w*4,0x16762004);
    if(!surface) impl->api->fail();
    impl->api->SDL_SetSurfaceBlendMode(surface,0);
    auto target=impl->api->SDL_GetWindowSurface(impl->window);
    auto status=target?impl->api->SDL_UpperBlit(surface,nullptr,target,nullptr):-1;
    impl->api->SDL_FreeSurface(surface);
    if(status || impl->api->SDL_UpdateWindowSurface(impl->window)) impl->api->fail();
}
Image Window::capture()const{return impl->api->image(impl->api->SDL_GetWindowSurface(impl->window));}
struct Font::Impl{std::shared_ptr<Api>api;TTF_Font*font;~Impl(){if(font)api->TTF_CloseFont(font);}};
Font::Font(std::shared_ptr<Api>a,rust::Str path,int size,bool bold):impl(std::make_unique<Impl>()){
    impl->api=std::move(a);if(size<=0)throw std::runtime_error("font size invalid");
    impl->font=impl->api->TTF_OpenFont(std::string(path).c_str(),size);if(!impl->font)impl->api->fail();
    impl->api->TTF_SetFontStyle(impl->font,bold?1:0);
}
Font::~Font()=default;
std::unique_ptr<Font>Context::font(rust::Str p,int s,bool b)const{return std::make_unique<Font>(api,p,s,b);}
Image Font::render(rust::Str text,uint8_t r,uint8_t g,uint8_t b)const{
    if(text.empty()){Image i{};i.height=impl->api->TTF_FontHeight(impl->font);return i;}
    auto surface=impl->api->TTF_RenderUTF8_Blended(impl->font,std::string(text).c_str(),SDL_Color{r,g,b,255});
    if(!surface)impl->api->fail();
    try{auto image=impl->api->image(surface);impl->api->SDL_FreeSurface(surface);return image;}
    catch(...){impl->api->SDL_FreeSurface(surface);throw;}
}
struct Tk::Impl{
    Library tcl,tk;Tcl_Interp*interp=nullptr;int major=0;
    using Init=int(*)(Tcl_Interp*);Init init=tcl.symbol<Init>("Tcl_Init"),tk_init=tk.symbol<Init>("Tk_Init");
    using Create=Tcl_Interp*(*)();Create create=tcl.symbol<Create>("Tcl_CreateInterp");
    using Delete=void(*)(Tcl_Interp*);Delete destroy=tcl.symbol<Delete>("Tcl_DeleteInterp");
    using ObjResult=Tcl_Obj*(*)(Tcl_Interp*);ObjResult obj_result=tcl.symbol<ObjResult>("Tcl_GetObjResult");
    using String=char*(*)(Tcl_Obj*);String string=tcl.symbol<String>("Tcl_GetString");
    const char* result(Tcl_Interp* i)const{return string(obj_result(i));}
    using Ref=void(*)(Tcl_Obj*,const char*,int);Ref incr=tcl.symbol<Ref>("Tcl_DbIncrRefCount"),decr=tcl.symbol<Ref>("Tcl_DbDecrRefCount");
    using New8=Tcl_Obj*(*)(const char*,int);using New9=Tcl_Obj*(*)(const char*,intptr_t);
    using Eval8=int(*)(Tcl_Interp*,int,Tcl_Obj*const*,int);using Eval9=int(*)(Tcl_Interp*,intptr_t,Tcl_Obj*const*,int);
    New8 new8=nullptr;New9 new9=nullptr;Eval8 eval8=nullptr;Eval9 eval9=nullptr;
    Impl(rust::Str t,rust::Str k,rust::Str executable):tcl(t,true),tk(k,true){
        using Version=void(*)(int*,int*,int*,int*);int minor=0,patch=0,type=0;tcl.symbol<Version>("Tcl_GetVersion")(&major,&minor,&patch,&type);
        if(major==8){tcl.symbol<void(*)(const char*)>("Tcl_FindExecutable")(std::string(executable).c_str());new8=tcl.symbol<New8>("Tcl_NewStringObj");eval8=tcl.symbol<Eval8>("Tcl_EvalObjv");}
        else if(major==9){tcl.symbol<const char*(*)(const char*)>("Tcl_FindExecutable")(std::string(executable).c_str());new9=tcl.symbol<New9>("Tcl_NewStringObj");eval9=tcl.symbol<Eval9>("Tcl_EvalObjv");}
        else throw std::runtime_error("unsupported Tcl ABI");
        interp=create();if(!interp)throw std::runtime_error("Tcl interpreter allocation failed");
        if(init(interp)!=0 || tk_init(interp)!=0){std::string message=result(interp);destroy(interp);interp=nullptr;throw std::runtime_error(message);}
    }
    ~Impl(){if(interp)destroy(interp);}
};
Tk::Tk(rust::Str t,rust::Str k,rust::Str e):impl(std::make_unique<Impl>(t,k,e)){}Tk::~Tk()=default;
std::unique_ptr<Tk>load_tk(rust::Str t,rust::Str k,rust::Str e){return std::make_unique<Tk>(t,k,e);}
rust::String Tk::eval(rust::Slice<const rust::String> words){
    if(words.size()>size_t(std::numeric_limits<int>::max()))throw std::runtime_error("Tcl argument count overflow");
    std::vector<Tcl_Obj*>objects;objects.reserve(words.size());
    for(const auto &word:words)if(word.size()>size_t(std::numeric_limits<int>::max()))throw std::runtime_error("Tcl argument length overflow");
    for(auto &word:words){auto object=impl->major==8?impl->new8(word.data(),int(word.size())):impl->new9(word.data(),intptr_t(word.size()));impl->incr(object,__FILE__,__LINE__);objects.push_back(object);}
    auto status=impl->major==8?impl->eval8(impl->interp,int(objects.size()),objects.data(),0):impl->eval9(impl->interp,intptr_t(objects.size()),objects.data(),0);
    std::string result=impl->result(impl->interp);for(auto object:objects)impl->decr(object,__FILE__,__LINE__);
    if(status!=0)throw std::runtime_error(result);
    return rust::String(result);
}
}
