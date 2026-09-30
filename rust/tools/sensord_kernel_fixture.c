// Only explicitly selected owned regular files stand in for I2C/GPIO devices.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <linux/gpio.h>
#include <linux/i2c-dev.h>
#include <linux/i2c.h>
#include <pthread.h>
#include <sched.h>
#include <signal.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

static pthread_mutex_t guard = PTHREAD_MUTEX_INITIALIZER;
static unsigned char registers[256];
static unsigned long reads, writes, irq_events;
static int initialized;
static atomic_int event_fd = -1, writer_fd = -1, started;
static atomic_int running;
static pthread_t emitter;
static int selected(int fd, const char *key) {
  const char *path = getenv(key);
  struct stat expected, actual;
  return path && stat(path, &expected) == 0 && fstat(fd, &actual) == 0 && S_ISREG(actual.st_mode)
      && expected.st_dev == actual.st_dev && expected.st_ino == actual.st_ino;
}
static FILE *trace(void) { const char *path = getenv("SENSORD_TRACE"); return path ? fopen(path, "a") : NULL; }
static int continuous(void) { return getenv("SENSORD_CONTINUOUS") != NULL; }
static void initialize(void) {
  if (initialized) return;
  initialized = 1;
  registers[0x0f] = 0x6a;
  registers[0x0d] = 0x80;
  registers[0x1e] = 3;
}
static void *emit(void *unused) {
  (void)unused;
  sigset_t blocked;
  sigemptyset(&blocked); sigaddset(&blocked, SIGPIPE);
  pthread_sigmask(SIG_BLOCK, &blocked, NULL);
  struct timespec delay = {0, 9615384};
  while (atomic_load(&running)) {
    nanosleep(&delay, NULL);
    struct timespec now; clock_gettime(CLOCK_REALTIME, &now);
    struct gpioevent_data event;
    memset(&event, 0, sizeof(event));
    event.timestamp = (uint64_t)now.tv_sec * 1000000000ULL + (uint64_t)now.tv_nsec;
    event.id = GPIOEVENT_EVENT_FALLING_EDGE;
    pthread_mutex_lock(&guard); registers[0x1e] = 3; irq_events++; pthread_mutex_unlock(&guard);
    if (write(writer_fd, &event, sizeof(event)) != sizeof(event)) break;
  }
  return NULL;
}
static void vector(unsigned char *out, int gyro) {
  int16_t values[3] = {100, -200, 16384};
  if (gyro) { values[0] = -100; values[1] = 100; values[2] = 200; }
  const int active = registers[0x14] & (gyro ? 12 : 3);
  for (int i = 0; i < 3; ++i) {
    const int16_t value = (int16_t)(values[i] + (active ? (gyro ? 5000 : 2000) : 0));
    out[2*i] = (unsigned char)value;
    out[2*i+1] = (unsigned char)((uint16_t)value >> 8);
  }
}
int ioctl(int fd, unsigned long request, ...) {
  va_list args; va_start(args, request); unsigned long argument = va_arg(args, unsigned long); va_end(args);
  if (request == GPIO_GET_LINEEVENT_IOCTL) {
    if (!selected(fd, "SENSORD_GPIO")) { errno = EPERM; return -1; }
    struct gpioevent_request *rq = (void *)argument;
    if (!rq || rq->lineoffset != 84 || rq->handleflags != 1 || rq->eventflags != 3 || rq->consumer_label[31] != 0) abort();
    int pipes[2]; if (pipe2(pipes, O_CLOEXEC) != 0) return -1;
    event_fd = pipes[0]; writer_fd = pipes[1]; rq->fd = event_fd;
    FILE *out = trace(); if (out) { fprintf(out,"{\"kind\":\"gpio\",\"pin\":%u,\"flags\":%u,\"eventflags\":%u,\"label\":\"%s\",\"struct_size\":%zu}\n",rq->lineoffset,rq->handleflags,rq->eventflags,rq->consumer_label,sizeof(*rq)); fclose(out); }
    if (continuous()) { atomic_store(&running, 1); started = pthread_create(&emitter, NULL, emit, NULL) == 0; if (!started) abort(); }
    else {
      struct gpioevent_data events[2]; memset(events,0,sizeof(events));
      events[0].timestamp=123456789; events[0].id=2; events[1].timestamp=777; events[1].id=1;
      if (write(writer_fd,events,sizeof(events)) != sizeof(events)) abort();
    }
    return 0;
  }
  if (request != I2C_SLAVE && request != I2C_SLAVE_FORCE && request != I2C_SMBUS) {
    int (*next)(int,unsigned long,...)=dlsym(RTLD_NEXT,"ioctl"); return next(fd,request,argument);
  }
  if (!selected(fd,"SENSORD_I2C")) { errno=EPERM; return -1; }
  pthread_mutex_lock(&guard); initialize();
  FILE *out = !continuous() ? trace() : NULL;
  int error=0;
  if (request==I2C_SLAVE||request==I2C_SLAVE_FORCE) {
    if(argument!=0x6a)abort();
    if(out)fprintf(out,"{\"kind\":\"address\",\"request\":%lu,\"address\":%lu}\n",request,argument);
  } else {
    struct i2c_smbus_ioctl_data *rq=(void *)argument;
    if(!rq||!rq->data||rq->read_write>1)abort();
    if(rq->command==0xee){error=EINTR;}
    const char *fault=getenv("SENSORD_FAULT");
    if(continuous()&&rq->read_write==I2C_SMBUS_READ&&(rq->command==0x28||rq->command==0x22)&&fault&&access(fault,F_OK)==0)error=EIO;
    if(error&&continuous())out=trace();
    if(rq->read_write==I2C_SMBUS_READ)reads++;else writes++;
    if(rq->size==I2C_SMBUS_BYTE_DATA){
      if(rq->read_write==I2C_SMBUS_READ){if(!error)rq->data->byte=registers[rq->command];}
      else{if(!error)registers[rq->command]=rq->data->byte;if(continuous()){out=trace();}}
      if(out)fprintf(out,"{\"kind\":\"byte\",\"direction\":%u,\"reg\":%u,\"value\":%u,\"size\":%u,\"errno\":%d}\n",rq->read_write,rq->command,rq->data->byte,rq->size,error);
    } else if(rq->size==I2C_SMBUS_I2C_BLOCK_DATA){
      const unsigned length=rq->data->block[0];if(length>32||rq->read_write!=I2C_SMBUS_READ)abort();
      for(unsigned i=1;i<sizeof(rq->data->block);i++)if(rq->data->block[i]!=0)abort();
      unsigned char bytes[32]={0};
      if(rq->command==0x28||rq->command==0x22){vector(bytes,rq->command==0x22);if(continuous()&&started)registers[0x1e]&=(rq->command==0x28?~1:~2);}
      else if(rq->command==0x20){bytes[0]=0;bytes[1]=255;}
      else if(rq->command>=0x40&&rq->command<=0x44){for(unsigned i=0;i<32;i++)bytes[i]=(unsigned char)(i+1);}
      else{for(unsigned i=0;i<length;i++)bytes[i]=registers[(rq->command+i)%256];}
      if(!error){memcpy(rq->data->block+1,bytes,length);if(rq->command==0x40)rq->data->block[0]=0;else if(rq->command==0x41)rq->data->block[0]=34;else if(rq->command==0x42)rq->data->block[0]=2;}
      if(out)fprintf(out,"{\"kind\":\"block\",\"reg\":%u,\"length\":%u,\"returned\":%u,\"size\":%u,\"errno\":%d,\"struct_size\":%zu,\"union_size\":%zu}\n",rq->command,length,rq->data->block[0],rq->size,error,sizeof(*rq),sizeof(*rq->data));
    } else abort();
  }
  if(out)fclose(out);
  pthread_mutex_unlock(&guard);
  if(error){errno=error;return -1;}return 0;
}
int sched_setscheduler(pid_t pid,int policy,const struct sched_param *param){
  if(!getenv("SENSORD_I2C")||pid!=0||policy!=SCHED_FIFO||!param||param->sched_priority!=1){errno=EPERM;return -1;}
  FILE *out=trace();if(out){fprintf(out,"{\"kind\":\"scheduler\",\"policy\":%d,\"priority\":%d}\n",policy,param->sched_priority);fclose(out);}return 0;
}
int sched_setaffinity(pid_t pid,size_t size,const cpu_set_t *set){
  if(!getenv("SENSORD_I2C")||pid!=0||size<sizeof(unsigned long)||!set||!CPU_ISSET_S(1,size,set)||CPU_COUNT_S(size,set)!=1){errno=EPERM;return -1;}
  FILE *out=trace();if(out){fputs("{\"kind\":\"affinity\",\"core\":1}\n",out);fclose(out);}return 0;
}
int close(int fd){
  int(*next)(int)=dlsym(RTLD_NEXT,"close");
  const int bus=selected(fd,"SENSORD_I2C"),chip=selected(fd,"SENSORD_GPIO"),event=fd==event_fd;
  if(event){atomic_store(&running,0);if(started){pthread_join(emitter,NULL);started=0;}event_fd=-1;}
  const int status=next(fd);
  const int closed=fcntl(fd,F_GETFD)==-1&&errno==EBADF;
  if(bus||chip||event){FILE *out=trace();if(out){fprintf(out,"{\"kind\":\"close\",\"device\":\"%s\",\"closed\":%s}\n",bus?"i2c":chip?"gpiochip":"event",closed?"true":"false");fclose(out);}}
  return status;
}
__attribute__((destructor))static void finish(void){
  atomic_store(&running,0);if(started){pthread_join(emitter,NULL);started=0;}
  const char *path=getenv("SENSORD_STATE");if(!path)return;
  FILE *out=fopen(path,"w");if(out){fprintf(out,"{\"reads\":%lu,\"writes\":%lu,\"irq_events\":%lu,\"int1\":%u,\"ctrl1\":%u,\"ctrl2\":%u}\n",reads,writes,irq_events,registers[0x0d],registers[0x10],registers[0x11]);fclose(out);}
}
