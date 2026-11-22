// std: c++26
#include "side.h"
// §6 StmtKind `CXXExpansionStmt`.
#ifdef B
void f() { template for (int e : {1, 2}) ; }
#else
void f() { ; }
#endif
