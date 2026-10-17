#pragma once
#include <stddef.h>
#include <stdint.h>
struct diagnostic_api;
struct diagnostic_api *diagnostic_open(const unsigned char *, size_t, char *,
                                       size_t);
void diagnostic_destroy(struct diagnostic_api *);
int diagnostic_flags(struct diagnostic_api *, unsigned, char *, size_t);
int diagnostic_window(struct diagnostic_api *, int, int, const char *, char *,
                      size_t);
int diagnostic_begin(struct diagnostic_api *, char *, size_t);
int diagnostic_clear(struct diagnostic_api *, uint32_t, char *, size_t);
int diagnostic_rectangle(struct diagnostic_api *, float, float, float, float,
                         uint32_t, char *, size_t);
int diagnostic_line(struct diagnostic_api *, int, int, int, int, uint32_t,
                    char *, size_t);
int diagnostic_circle(struct diagnostic_api *, int, int, float, uint32_t,
                      char *, size_t);
int diagnostic_font(struct diagnostic_api *, const char *, int, int,
                    const int *, int, char *, size_t);
int diagnostic_text(struct diagnostic_api *, const char *, float, float, float,
                    uint32_t, char *, size_t);
int diagnostic_target(struct diagnostic_api *, int, int, char *, size_t);
int diagnostic_target_begin(struct diagnostic_api *, char *, size_t);
int diagnostic_target_end(struct diagnostic_api *, char *, size_t);
int diagnostic_shader(struct diagnostic_api *, const char *, const char *,
                      char *, size_t);
int diagnostic_shader_begin(struct diagnostic_api *, char *, size_t);
int diagnostic_shader_end(struct diagnostic_api *, char *, size_t);
int diagnostic_target_draw(struct diagnostic_api *, char *, size_t);
int diagnostic_export(struct diagnostic_api *, const char *, char *, size_t);
int diagnostic_end(struct diagnostic_api *, char *, size_t);
