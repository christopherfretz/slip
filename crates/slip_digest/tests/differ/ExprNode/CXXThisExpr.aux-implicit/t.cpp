#include "side.h"
// §7 ExprNode `CXXThisExpr.aux-implicit`: written for an
// instantiation-dependent node only, here a member of a class template
// (the pattern unit); outside a template a redundant `this->` changes
// nothing (`equal/_/expr-qualifier`, pitfalls E07).
template<class T> struct Th { int m;
#ifdef B
int n = m; };
#else
int n = this->m; };
#endif
