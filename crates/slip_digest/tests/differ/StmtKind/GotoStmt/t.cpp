#include "side.h"
// §6 StmtKind `GotoStmt`.
#ifdef B
void f() { l: goto l; }
#else
void f() { l: ; }
#endif
