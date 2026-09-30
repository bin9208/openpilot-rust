// Owned PortAudio v19 ABI fixture: records samples without opening audio hardware.
#define _POSIX_C_SOURCE 200809L
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
typedef struct {
  int version; const char *name; int hostApi; int inputs; int outputs;
  double lowIn, lowOut, highIn, highOut, rate;
} PaDeviceInfo;
typedef struct { int device, channels; unsigned long format; double latency; void *hostInfo; } PaStreamParameters;
typedef int (*Callback)(const void *, void *, unsigned long, const void *, unsigned long, void *);
static PaDeviceInfo device = {2, "owned fixture", 0, 0, 1, .01, .02, .03, .25, 48000};
static Callback callback;
static void *userdata;
static pthread_t thread;
static _Atomic int active = 0;
static int launched, attempts, polls;
static pthread_mutex_t output_lock = PTHREAD_MUTEX_INITIALIZER;
static double now(void) { struct timespec value; clock_gettime(CLOCK_MONOTONIC, &value); return value.tv_sec + value.tv_nsec / 1e9; }
static void record(const char *event, int value) {
  pthread_mutex_lock(&output_lock);
  FILE *file = fopen(getenv("SOUND_FIXTURE_EVENTS"), "a");
  if (!file) abort();
  fprintf(file, "{\"event\":\"%s\",\"value\":%d,\"time\":%.9f,\"pid\":%d}\n", event, value, now(), getpid());
  fclose(file);
  pthread_mutex_unlock(&output_lock);
}
static int setting(const char *name) { const char *value = getenv(name); return value ? atoi(value) : 0; }
int Pa_Initialize(void) { record("initialize", 0); return 0; }
int Pa_Terminate(void) { record("terminate", 0); return 0; }
int Pa_GetDefaultOutputDevice(void) { return 7; }
const PaDeviceInfo *Pa_GetDeviceInfo(int index) { return index == 7 ? &device : NULL; }
int Pa_OpenStream(void **stream, const PaStreamParameters *input, const PaStreamParameters *output,
                  double rate, unsigned long frames, unsigned long flags, Callback function, void *user) {
  ++attempts;
  record("open", attempts);
  if (input || !output || output->device != 7 || output->channels != 1 || output->format != 1 ||
      output->latency != .25 || output->hostInfo || rate != 48000 || frames != 4096 || flags != 0) abort();
  if (attempts <= setting("SOUND_FIXTURE_FAIL_OPEN")) return -9985;
  callback = function; userdata = user; *stream = &device; return 0;
}
static void *run(void *unused) {
  (void)unused;
  unsigned count = 0;
  while (atomic_load(&active)) {
    float output[4096];
    int result = callback(NULL, output, 4096, NULL, count == 0 ? 4 : 0, userdata);
    pthread_mutex_lock(&output_lock);
    FILE *data = fopen(getenv("SOUND_FIXTURE_SAMPLES"), "ab");
    if (!data || fwrite(output, sizeof(float), 4096, data) != 4096) abort();
    fclose(data);
    pthread_mutex_unlock(&output_lock);
    record("callback", result);
    ++count;
    if (result != 0) { atomic_store(&active, 0); break; }
    struct timespec interval = {0, 85333333}; nanosleep(&interval, NULL);
  }
  return NULL;
}
int Pa_StartStream(void *stream) {
  if (stream != &device) abort();
  record("start", 0);
  if (setting("SOUND_FIXTURE_FAIL_START")) return -9985;
  atomic_store(&active, 1);
  if (pthread_create(&thread, NULL, run, NULL)) abort();
  launched = 1; return 0;
}
int Pa_IsStreamActive(void *stream) {
  if (stream != &device) abort();
  record("poll", ++polls);
  if (setting("SOUND_FIXTURE_INACTIVE") && polls >= 3) return 0;
  return atomic_load(&active);
}
int Pa_StopStream(void *stream) {
  if (stream != &device) abort();
  atomic_store(&active, 0);
  if (launched) { pthread_join(thread, NULL); launched = 0; }
  record("stop", 0); return 0;
}
int Pa_CloseStream(void *stream) { if (stream != &device) abort(); record("close", 0); return 0; }
