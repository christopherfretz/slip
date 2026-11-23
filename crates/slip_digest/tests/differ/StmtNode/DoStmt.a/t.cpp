#include "side.h"
// §6 StmtNode `DoStmt.a`.
#ifdef B
void f(int x) { do ; while (x == 2); }
#else
void f(int x) { do ; while (x == 1); }
#endif
