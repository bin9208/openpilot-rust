#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <limits.h>
int main(void) {
  const char *root=getenv("CHECK_FIXTURE_ROOT");
  if (!root) return 2;
  char path[PATH_MAX], mode[64]={0};
  snprintf(path,sizeof(path),"%s/mode",root);
  FILE *file=fopen(path,"r");
  if (!file || fscanf(file,"%63s",mode)!=1) return 2;
  fclose(file);
  int count=0;
  snprintf(path,sizeof(path),"%s/count",root);
  file=fopen(path,"r");
  if (file) { if(fscanf(file,"%d",&count)!=1) return 2; fclose(file); }
  file=fopen(path,"w"); if (!file) return 2;
  fprintf(file,"%d",++count);fclose(file);
  snprintf(path,sizeof(path),"%s/pid",root);
  file=fopen(path,"w");if(!file)return 2;fprintf(file,"%ld",(long)getpid());fclose(file);
  if (!getenv("DEV") || strcmp(getenv("DEV"),"USB+AMD:LLVM") || !getenv("GMMU") || strcmp(getenv("GMMU"),"0")) return 2;
  if (!strcmp(mode,"sleep")) sleep(10);
  if (!strcmp(mode,"pcie") || (!strcmp(mode,"pcie_once") && count==1)) { fputs("PCIE LINK NOT UP\n",stderr);return 1; }
  if (!strcmp(mode,"read")) { fputs("read(0xB450, 1) failed\n",stdout);return 1; }
  if (!strcmp(mode,"bad")) { fputs("incompatible GPU\n",stderr);return 1; }
  if (!strcmp(mode,"power")) { fputs("PCIe power on failed\n",stderr);return 1; }
  if (!strcmp(mode,"link")) {
    snprintf(path,sizeof(path),"%s/controller.ssusb/portli",root);file=fopen(path,"w");if(!file)return 2;fputs("4",file);fclose(file);
  }
  if (!strcmp(mode,"gone")) {snprintf(path,sizeof(path),"%s/controller.ssusb/usb1/1-1/idVendor",root);if(unlink(path))return 2;}
  return 0;
}
