#include "side.h"
// §6 StmtNode `WhileStmt.a`.
#ifdef B
void f(int x) { while (x == 2) ; }
#else
void f(int x) { while (x == 1) ; }
#endif
