#include "side.h"
// §6 StmtNode `ForStmt.a`.
#ifdef B
void f(int x) { for (; x == 2;) ; }
#else
void f(int x) { for (; x == 1;) ; }
#endif
