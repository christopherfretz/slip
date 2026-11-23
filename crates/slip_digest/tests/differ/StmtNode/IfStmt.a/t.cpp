#include "side.h"
// §6 StmtNode `IfStmt.a`.
#ifdef B
void f(int x) { if (x == 2) ; }
#else
void f(int x) { if (x == 1) ; }
#endif
