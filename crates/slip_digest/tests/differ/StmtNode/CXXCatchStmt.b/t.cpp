#include "side.h"
// §6 StmtNode `CXXCatchStmt.b`.
#ifdef B
void f() { try {} catch (...) { ; } }
#else
void f() { try {} catch (...) {} }
#endif
