#include "side.h"
// §6 StmtKind `CXXForRangeStmt`.
#ifdef B
void f(int (&a)[2]) { for (int e : a) ; }
#else
void f(int (&a)[2]) { ; }
#endif
