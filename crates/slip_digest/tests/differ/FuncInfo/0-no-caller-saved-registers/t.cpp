#include "side.h"
// §4.5 FuncInfo `0-no-caller-saved-registers`.
#ifdef B
void (*v)();
#else
void (*v)() __attribute__((no_caller_saved_registers));
#endif
