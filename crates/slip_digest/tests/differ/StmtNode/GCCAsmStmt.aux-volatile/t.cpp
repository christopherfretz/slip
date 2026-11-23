#include "side.h"
// §6 StmtNode `GCCAsmStmt.aux-volatile`.
#ifdef B
void f(int x) { asm volatile("" : : "r"(x)); }
#else
void f(int x) { asm("" : : "r"(x)); }
#endif
