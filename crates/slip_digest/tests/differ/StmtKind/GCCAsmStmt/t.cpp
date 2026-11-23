#include "side.h"
// §6 StmtKind `GCCAsmStmt`.
#ifdef B
void f() { asm(""); }
#else
void f() { ; }
#endif
