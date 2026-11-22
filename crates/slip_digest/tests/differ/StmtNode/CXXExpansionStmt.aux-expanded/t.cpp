// std: c++26
#include "side.h"
// §6 StmtNode `CXXExpansionStmt.aux-expanded`: the pattern's statement is not expanded, the explicit instantiation's is: Kiln expands a statement exactly where it is not in a template, so no pair has the bit alone.
#ifdef B
template <class T> void f(T) { template for (int e : {1, 2}) ; }
template void f<int>(int);
#else
template <class T> void f(T) { template for (int e : {1, 2}) ; }
#endif
