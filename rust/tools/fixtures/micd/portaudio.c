#define _POSIX_C_SOURCE 200809L
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
typedef struct { int version; const char *name; int host, inputs, outputs; double low_in, low_out, high_in, high_out, rate; } Device;
typedef struct { int device, channels; unsigned long format; double latency; void *host; } Parameters;
typedef int (*Callback)(const void *, void *, unsigned long, const void *, unsigned long, void *);
static Device device = {2, "owned input", 0, 1, 0, .01, .02, .25, .03, 16000};
static Callback callback;
static void *user;
static pthread_t worker;
static atomic_int running;
static int launched, attempts;
static pthread_mutex_t log_lock = PTHREAD_MUTEX_INITIALIZER;
static void record(const char *event, int value) {
  pthread_mutex_lock(&log_lock);
  FILE *file = fopen(getenv("MIC_FIXTURE_EVENTS"), "a");
  if (!file) abort();
  struct timespec now; clock_gettime(CLOCK_MONOTONIC, &now);
  fprintf(file, "{\"event\":\"%s\",\"value\":%d,\"ns\":%lld}\n", event, value, (long long)now.tv_sec * 1000000000 + now.tv_nsec);
  fclose(file);
  pthread_mutex_unlock(&log_lock);
}
static int setting(const char *name) { const char *value = getenv(name); return value ? atoi(value) : 0; }
int Pa_Initialize(void) { record("initialize", 0); return 0; }
int Pa_Terminate(void) { record("terminate", 0); return 0; }
int Pa_GetDefaultInputDevice(void) { return 7; }
const Device *Pa_GetDeviceInfo(int index) { return index == 7 ? &device : NULL; }
int Pa_OpenStream(void **stream, const Parameters *input, const Parameters *output, double rate,
                  unsigned long frames, unsigned long flags, Callback function, void *context) {
  record("open", ++attempts);
  if (output || !input || input->device != 7 || input->channels != 1 || input->format != 1 ||
      input->latency != .25 || input->host || rate != 16000 || frames != 800 || flags) abort();
  if (attempts <= setting("MIC_FIXTURE_FAIL_OPEN")) return -9985;
  callback = function; user = context; *stream = &device; return 0;
}
static void *capture(void *unused) {
  (void)unused;
  float samples[800] = {0};
  if (callback(samples, NULL, 0, NULL, 0, user)) return NULL;
  record("ready", 0);
  while (atomic_load(&running) && access(getenv("MIC_FIXTURE_GO"), F_OK)) {
    struct timespec delay = {0, 1000000}; nanosleep(&delay, NULL);
  }
  for (int block = 0; block < 16 && atomic_load(&running); ++block) {
    for (int i = 0; i < 800; ++i) samples[i] = (float)((i + block) % 17 - 8) / 32;
    int result = callback(samples, NULL, 800, NULL, block == 0 ? 2 : 0, user);
    record("callback", block);
    if (result) break;
    struct timespec delay = {0, 50000000}; nanosleep(&delay, NULL);
  }
  record("complete", 0);
  return NULL;
}
int Pa_StartStream(void *stream) {
  if (stream != &device) abort();
  record("start", 0);
  if (setting("MIC_FIXTURE_FAIL_START")) return -9985;
  atomic_store(&running, 1);
  if (pthread_create(&worker, NULL, capture, NULL)) abort();
  launched = 1; return 0;
}
int Pa_IsStreamActive(void *stream) { (void)stream; abort(); }
int Pa_StopStream(void *stream) {
  if (stream != &device) abort();
  atomic_store(&running, 0);
  if (launched) { pthread_join(worker, NULL); launched = 0; }
  record("stop", 0); return 0;
}
int Pa_CloseStream(void *stream) { if (stream != &device) abort(); record("close", 0); return 0; }
