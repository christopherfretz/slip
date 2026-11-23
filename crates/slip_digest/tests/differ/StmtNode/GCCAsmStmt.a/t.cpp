#include "side.h"
// §6 StmtNode `GCCAsmStmt.a`.
#ifdef B
void f() { asm("nop"); }
#else
void f() { asm(""); }
#endif
