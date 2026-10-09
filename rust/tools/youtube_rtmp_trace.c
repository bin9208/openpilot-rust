/* Owned validation-only ABI proxy: forward all original symbols while capturing
 * SetupURL before its mutation and tcUrl while its AVal is borrowed. */
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { char *value; int length; } AVal;
static void *provider;
static unsigned write_calls;
static void *symbol(const char *name) {
  if (!provider) {
    const char *path = getenv("OWNED_RTMP_PROVIDER");
    if (!path || path[0] != '/') abort();
    provider = dlopen(path, RTLD_NOW | RTLD_LOCAL);
  }
  if (!provider) { fputs(dlerror(), stderr); abort(); }
  void *result = dlsym(provider, name);
  if (!result) { fputs(dlerror(), stderr); abort(); }
  return result;
}
static void capture(const char *name, const char *value, size_t length) {
  const char *path = getenv("OWNED_RTMP_TRACE");
  if (!path) return;
  FILE *file = fopen(path, "ab");
  if (!file) abort();
  fprintf(file, "%s %zu ", name, length);
  for (size_t i = 0; i < length; ++i) fprintf(file, "%02x", (unsigned char)value[i]);
  fputc('\n', file);
  fclose(file);
}
void *RTMP_Alloc(void) { return ((void *(*)(void))symbol("RTMP_Alloc"))(); }
void RTMP_Init(void *handle) { ((void (*)(void *))symbol("RTMP_Init"))(handle); }
int RTMP_SetupURL(void *handle, char *url) {
  capture("SetupURL", url, strlen(url));
  return ((int (*)(void *, char *))symbol("RTMP_SetupURL"))(handle, url);
}
int RTMP_SetOpt(void *handle, const AVal *name, const AVal *value) {
  if (name->length == 5 && memcmp(name->value, "tcUrl", 5) == 0) capture("tcUrl", value->value, (size_t)value->length);
  return ((int (*)(void *, const AVal *, const AVal *))symbol("RTMP_SetOpt"))(handle, name, value);
}
void RTMP_EnableWrite(void *handle) { ((void (*)(void *))symbol("RTMP_EnableWrite"))(handle); }
int RTMP_Connect(void *handle, void *packet) { return ((int (*)(void *, void *))symbol("RTMP_Connect"))(handle, packet); }
int RTMP_ConnectStream(void *handle, int index) { return ((int (*)(void *, int))symbol("RTMP_ConnectStream"))(handle, index); }
int RTMP_Write(void *handle, const char *data, int length) {
  int forwarded = length;
  const char *limit = getenv("OWNED_RTMP_FIRST_PREFIX");
  if (write_calls++ == 0 && limit) {
    char *end;
    long prefix = strtol(limit, &end, 10);
    if (*end || prefix < 16384 || prefix >= length) abort();
    forwarded = (int)prefix;
  }
  int actual = ((int (*)(void *, const char *, int))symbol("RTMP_Write"))(handle, data, forwarded);
  const char *path = getenv("OWNED_RTMP_WRITE_TRACE");
  if (path) {
    FILE *file = fopen(path, "ab");
    if (!file) abort();
    fprintf(file, "%d %d %d\n", length, forwarded, actual);
    fclose(file);
  }
  return actual;
}
int RTMP_IsConnected(void *handle) { return ((int (*)(void *))symbol("RTMP_IsConnected"))(handle); }
void RTMP_Close(void *handle) { ((void (*)(void *))symbol("RTMP_Close"))(handle); }
void RTMP_Free(void *handle) { ((void (*)(void *))symbol("RTMP_Free"))(handle); }
