#include "side.h"
// §6 StmtNode `kind`.
#ifdef B
void f() { for (;;) { continue; } }
#else
void f() { for (;;) { break; } }
#endif
