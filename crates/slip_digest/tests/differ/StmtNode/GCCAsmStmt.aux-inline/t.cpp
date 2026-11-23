#include "side.h"
// §6 StmtNode `GCCAsmStmt.aux-inline`.
#ifdef B
void f() { asm inline(""); }
#else
void f() { asm(""); }
#endif
