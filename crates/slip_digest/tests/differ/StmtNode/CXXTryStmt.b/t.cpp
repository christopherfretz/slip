#include "side.h"
// §6 StmtNode `CXXTryStmt.b`: the order of two handlers (WP-034 extra test).
#ifdef B
void f() { try {} catch (long) { ; } catch (int) {} }
#else
void f() { try {} catch (int) {} catch (long) { ; } }
#endif
