#include "side.h"
// §4.4 "Spellings", rule 11, audit (c): explicit template arguments of a `CXXDependentScopeMemberExpr`: side B adds `f(long)`, which the found set of `f` there does not hold.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int g(T t) { return t.template m<sizeof(sizeof(f(t)))>(); }
