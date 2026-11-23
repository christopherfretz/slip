#include "side.h"
// §6 StmtKind `DoStmt`.
#ifdef B
void f(int x) { do ; while (x); }
#else
void f(int x) { while (x) ; }
#endif
