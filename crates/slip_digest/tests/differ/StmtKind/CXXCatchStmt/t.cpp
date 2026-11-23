#include "side.h"
// §6 StmtKind `CXXCatchStmt`.
#ifdef B
void f() { try {} catch (int) {} catch (...) {} }
#else
void f() { try {} catch (...) {} }
#endif
