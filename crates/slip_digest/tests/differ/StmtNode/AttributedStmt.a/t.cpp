// std: c++20
#include "side.h"
// §6 StmtNode `AttributedStmt.a`.
#ifdef B
void f(int x) { if (x) [[unlikely]] ; }
#else
void f(int x) { if (x) [[likely]] ; }
#endif
