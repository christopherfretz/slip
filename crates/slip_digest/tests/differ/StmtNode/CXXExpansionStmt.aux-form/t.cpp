// std: c++26
#include "side.h"
// §6 StmtNode `CXXExpansionStmt.aux-form`: an enumerating against an iterating statement.
#ifdef B
void f() { static constexpr int a[] = {1, 2}; template for (constexpr int e : a) ; }
#else
void f() { template for (int e : {1, 2}) ; }
#endif
