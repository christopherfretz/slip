#include "side.h"
// §6 StmtNode `CXXCatchStmt.a`: `catch (...)` against `catch (int)` (WP-034 extra test).
#ifdef B
void f() { try {} catch (int) {} }
#else
void f() { try {} catch (...) {} }
#endif
