#include "side.h"
// §6 StmtKind `CaseStmt`.
#ifdef B
void f(int x) { switch (x) { case 0: ; } }
#else
void f(int x) { switch (x) { default: ; } }
#endif
