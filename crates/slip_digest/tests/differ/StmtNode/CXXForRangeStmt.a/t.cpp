#include "side.h"
// §6 StmtNode `CXXForRangeStmt.a`.
#ifdef B
void f(int (&a)[2]) { for (long e : a) ; }
#else
void f(int (&a)[2]) { for (int e : a) ; }
#endif
