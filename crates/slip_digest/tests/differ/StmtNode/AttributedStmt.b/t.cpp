// std: c++20
#include "side.h"
// §6 StmtNode `AttributedStmt.b`.
#ifdef B
void f(int x) { if (x) [[likely]] return; }
#else
void f(int x) { if (x) [[likely]] ; }
#endif
