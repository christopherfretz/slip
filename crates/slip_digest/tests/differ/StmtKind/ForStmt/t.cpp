#include "side.h"
// §6 StmtKind `ForStmt`.
#ifdef B
void f(int x) { for (; x;) ; }
#else
void f(int x) { while (x) ; }
#endif
