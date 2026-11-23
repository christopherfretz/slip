// std: c++26
#include "side.h"
// §6 StmtNode `CXXExpansionStmt.b`: the inner statement is one template level deeper: the depth follows the nesting, so no pair has the row alone.
#ifdef B
template <class T> void f(T) { template for (int e : {1}) ; template for (int g : {2}) ; }
#else
template <class T> void f(T) { template for (int e : {1}) { template for (int g : {2}) ; } }
#endif
