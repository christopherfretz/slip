#include "side.h"
// §6 StmtKind `ReturnStmt`.
#ifdef B
void f() { return; }
#else
void f() { ; }
#endif
