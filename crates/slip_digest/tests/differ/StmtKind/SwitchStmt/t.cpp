#include "side.h"
// §6 StmtKind `SwitchStmt`.
#ifdef B
void f(int x) { switch (x) {} }
#else
void f(int x) { if (x) {} }
#endif
