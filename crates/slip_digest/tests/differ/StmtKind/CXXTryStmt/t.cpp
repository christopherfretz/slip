#include "side.h"
// §6 StmtKind `CXXTryStmt`.
#ifdef B
void f() { try {} catch (...) {} }
#else
void f() { {} }
#endif
