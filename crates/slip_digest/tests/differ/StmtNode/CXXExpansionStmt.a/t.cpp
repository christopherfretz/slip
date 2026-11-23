// std: c++26
#include "side.h"
// §6 StmtNode `CXXExpansionStmt.a`.
#ifdef B
void f() { template for (int e : {1, 3}) ; }
#else
void f() { template for (int e : {1, 2}) ; }
#endif
