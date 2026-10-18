// Mechanical additions to the shared raw raylib dispatcher; no widget policy.
} else if (op == "fps-limit") {
  int fps = 0;
  in >> fps;
  done();
  ready();
  a->get<void (*)(int)>("SetTargetFPS")(fps);
} else if (op == "window-close") {
  done();
  ready();
  result = a->get<bool (*)()>("WindowShouldClose")() ? " 1" : " 0";
} else if (op == "poll-input") {
  done();
  ready();
  a->get<void (*)()>("PollInputEvents")();
} else if (op == "clock") {
  done();
  ready();
  char value[64];
  std::snprintf(value, sizeof(value), " %.17g",
                a->get<double (*)()>("GetTime")());
  result = value;
} else if (op == "get-fps") {
  done();
  ready();
  result = " " + std::to_string(a->get<int (*)()>("GetFPS")());
} else if (op == "mouse-input") {
  done();
  ready();
  auto down = a->get<bool (*)(int)>("IsMouseButtonDown");
  auto touch = a->get<Vector2 (*)(int)>("GetTouchPosition");
  for (int slot = 0; slot < 2; slot++) {
    bool pressed = down(slot);
    Vector2 point = pressed ? touch(slot) : Vector2{};
    char value[120];
    std::snprintf(value, sizeof(value), " %d %.17g %.17g",
                  int(pressed), double(point.x), double(point.y));
    result += value;
  }
} else if (op == "keyboard-input") {
  done();
  ready();
  result = " " + std::to_string(a->get<int (*)()>("GetKeyPressed")()) +
           " " + std::to_string(a->get<int (*)()>("GetCharPressed")());
  for (int key : {262, 263, 259, 261}) {
    result += a->get<bool (*)(int)>("IsKeyDown")(key) ? " 1" : " 0";
  }
}
 else if (op == "app-window") {
  int width = 0, height = 0;
  std::string title;
  in >> width >> height >> title;
  done();
  if (a->window) throw std::runtime_error("duplicate application window");
  a->get<void (*)(int, int, const char *)>("InitWindow")(width, height, unhex(title).c_str());
  a->window = true;
} else if (op == "app-font") {
  size_t id = 0;
  std::string kind, path;
  int size = 0, count = 0;
  in >> id >> kind >> path >> size >> count;
  if (count < 0 || count > 0x110000) throw std::runtime_error("application font codepoint count");
  std::vector<int> points(static_cast<size_t>(count));
  for (int &point : points) in >> point;
  done(); ready();
  if (id != a->fonts.size()) throw std::runtime_error("application font index");
  Font font = kind == "atlas"
    ? a->get<Font (*)(const char *)>("LoadFont")(unhex(path).c_str())
    : a->get<Font (*)(const char *, int, int *, int)>("LoadFontEx")(unhex(path).c_str(), size, points.data(), count);
  a->fonts.push_back(font);
  a->font = font;
  result = font.texture.id > 0 ? " 1" : " 0";
} else if (op == "app-unload") {
  std::string kind;
  size_t id = 0;
  in >> kind >> id;
  done(); ready();
  if (kind == "font") {
    if (id >= a->fonts.size()) throw std::runtime_error("application font missing");
    a->get<void (*)(Font)>("UnloadFont")(a->fonts[id]);
    a->fonts[id] = Font{};
  } else if (kind == "texture") {
    if (id >= a->textures.size()) throw std::runtime_error("application texture missing");
    a->get<void (*)(Texture)>("UnloadTexture")(a->textures[id]);
    a->textures[id] = Texture{};
  } else if (kind == "target") {
    a->get<void (*)(RenderTexture)>("UnloadRenderTexture")(a->target);
    a->target = RenderTexture{};
  } else if (kind == "shader") {
    a->get<void (*)(Shader)>("UnloadShader")(a->shader);
    a->shader = Shader{};
  } else throw std::runtime_error("application unload kind");
} else if (op == "app-close-window") {
  done(); ready();
  a->get<void (*)()>("CloseWindow")();
  a->window = a->frame = a->texture_frame = a->matrix = a->clip = false;
} else if (op == "app-record-read") {
  std::string path;
  in >> path;
  done(); ready();
  if (a->application_image.data) throw std::runtime_error("application recording image already retained");
  a->application_image = a->get<Image (*)(Texture)>("LoadImageFromTexture")(a->target.texture);
  Image &image = a->application_image;
  FILE *file = std::fopen(unhex(path).c_str(), "wb");
  const size_t pixels = static_cast<size_t>(image.width) * static_cast<size_t>(image.height);
  bool ok = file && std::fwrite(image.data, 4, pixels, file) == pixels;
  if (file && std::fclose(file)) ok = false;
  if (!ok) throw std::runtime_error("application recording readback write");
} else if (op == "app-record-unload") {
  done(); ready();
  a->get<void (*)(Image)>("UnloadImage")(a->application_image);
  a->application_image = Image{};
}
 else if (op == "app-grid-line") {
  int x1 = 0, y1 = 0, x2 = 0, y2 = 0;
  uint32_t rgba = 0;
  in >> x1 >> y1 >> x2 >> y2 >> rgba;
  done(); ready();
  a->get<void (*)(int, int, int, int, Color)>("DrawLine")(x1, y1, x2, y2, color(rgba));
}

 else if (op == "x11-open") {
  std::string name; in >> name; done(); application_x11.open(unhex(name));
} else if (op == "x11-window") {
  std::string name; in >> name; done(); result = " " + std::to_string(application_x11.find(unhex(name)));
} else if (op == "x11-focus") {
  ApplicationWindow window = 0; in >> window; done(); application_x11.focus(window);
} else if (op == "x11-sync") {
  done(); application_x11.sync();
} else if (op == "x11-key") {
  std::string name; int down = 0; in >> name >> down; done(); application_x11.key(unhex(name), down != 0);
} else if (op == "x11-motion") {
  ApplicationWindow window = 0; int x = 0, y = 0; in >> window >> x >> y; done(); application_x11.motion(window, x, y);
} else if (op == "x11-button") {
  int down = 0; in >> down; done(); application_x11.button(down != 0);
} else if (op == "x11-errors") {
  done(); result = " " + std::to_string(application_x_errors);
} else if (op == "x11-close") {
  done(); application_x11.close();
}
