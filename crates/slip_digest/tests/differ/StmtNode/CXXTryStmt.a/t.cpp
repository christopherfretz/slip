#include "side.h"
// §6 StmtNode `CXXTryStmt.a`.
#ifdef B
void f() { try { ; } catch (...) {} }
#else
void f() { try { } catch (...) {} }
#endif
