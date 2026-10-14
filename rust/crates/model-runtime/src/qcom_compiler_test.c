/* Test-owned ABI oracle. No vendor code or model source is embedded. */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
typedef struct {uint64_t chip; uint32_t mode; void* bitcode; uint64_t size; char* log; uint32_t log_len,error;} Compiled;
typedef struct {int32_t kernels; void* props; uint32_t error; char* log; char unknown[32]; uint64_t chip; uint32_t mode;} Executable;
typedef struct {uint32_t kind; void* data;} Handle;
static int counters[4], mode, null_next;
void null_next_create(void){null_next=1;}
int* counts(void){return counters;}
void* cl_compiler_create_llvm_instance(void){++counters[2];if(null_next){null_next=0;return 0;}return (void*)0x1234;}
void cl_compiler_destroy_llvm_instance(void* instance){assert(instance==(void*)0x1234);++counters[3];}
Handle* cl_compiler_compile_source(void* instance,uint64_t chip,int bits,const char* options,int p5,uint64_t p6,uint64_t p7,const char* source,uint64_t length,uint64_t type,void* p11){
 assert(instance==(void*)0x1234 && chip==0x6030001 && bits==1 && options && !*options && !p5 && !p6 && !p7 && !length && !type && !p11);
 mode=!strcmp(source,"compile_error")?1:!strcmp(source,"link_error")?2:!strcmp(source,"empty")?3:!strcmp(source,"invalid_tag")?4:!strcmp(source,"null_handle")?5:0;
 if(mode==5)return 0;
 Compiled* data=calloc(1,sizeof(*data));data->error=mode==1;data->log="source failed";
 Handle* handle=calloc(1,sizeof(*handle));handle->kind=mode==4?2:1;handle->data=data;return handle;
}
Handle* cl_compiler_link_program(void* instance,uint64_t chip,int bits,const char* options,int count,Handle** handles){
 assert(instance==(void*)0x1234 && chip==0x6030001 && bits==1 && !options && count==1 && handles && (*handles)->kind==1);
 Executable* data=calloc(1,sizeof(*data));data->error=mode==2;data->log="link failed";
 Handle* handle=calloc(1,sizeof(*handle));handle->kind=3;handle->data=data;return handle;
}
void cl_compiler_handle_create_binary(Handle* handle,void** bytes,uint64_t* size){assert(handle->kind==3);*bytes=malloc(4);memcpy(*bytes,"\x00\xff\x11\x00",4);*size=mode==3?0:4;}
void cl_compiler_free_handle(Handle* handle){++counters[0];free(handle->data);free(handle);}
void cl_compiler_free_assembly(void* bytes){++counters[1];free(bytes);}
