#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXPseudoDestructorExpr.aux-qualified`: the qualifier, written inside an instantiation-dependent expression, which substitution reads.
typedef int I;
I *p;
template <class T> struct A {
#ifdef B
  static inline int v = (p->I::~I(), sizeof(T));
#else
  static inline int v = (p->~I(), sizeof(T));
#endif
};
