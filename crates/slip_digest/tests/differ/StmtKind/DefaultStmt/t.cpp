#include "side.h"
// §6 StmtKind `DefaultStmt`.
#ifdef B
void f(int x) { switch (x) { default: ; } }
#else
void f(int x) { switch (x) { case 0: ; } }
#endif
