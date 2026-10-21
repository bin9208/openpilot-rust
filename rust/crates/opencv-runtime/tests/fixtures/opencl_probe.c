// Test-only no-platform OpenCL loader. No vendor runtime or device is accessed.
#include <stdio.h>
#include <stdlib.h>
__attribute__((constructor)) static void loaded(void) {
  const char *path = getenv("OPENCV_POLICY_MARKER");
  if (path) { FILE *f = fopen(path, "w"); if (f) { fputs("loaded\n", f); fclose(f); } }
}
int clGetPlatformIDs(unsigned int count, void **platforms, unsigned int *found) {
  (void)count; (void)platforms;
  if (found) *found = 0;
  return -1001; // CL_PLATFORM_NOT_FOUND_KHR
}
