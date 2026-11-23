#include "side.h"
// §6 StmtKind `ContinueStmt`.
#ifdef B
void f() { for (;;) { continue; } }
#else
void f() { for (;;) { break; } }
#endif
