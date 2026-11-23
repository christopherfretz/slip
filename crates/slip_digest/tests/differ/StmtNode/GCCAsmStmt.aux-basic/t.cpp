#include "side.h"
// §6 StmtNode `GCCAsmStmt.aux-basic`: a basic statement against an extended one with no operands.
#ifdef B
void f() { asm("" : ); }
#else
void f() { asm(""); }
#endif
