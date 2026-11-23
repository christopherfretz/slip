#include "side.h"
// §6 StmtNode `DefaultStmt.a`.
#ifdef B
void f(int x) { switch (x) { default: break; } }
#else
void f(int x) { switch (x) { default: ; } }
#endif
