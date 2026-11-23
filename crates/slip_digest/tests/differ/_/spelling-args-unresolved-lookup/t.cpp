#include "side.h"
// §4.4 "Spellings", rule 11, audit (c): explicit template arguments of an unresolved name (`UnresolvedLookupExpr`, `expr_targs`): side B adds `f(long)`, which the found set of `f` there does not hold.
template<int N> int h(int);
template<int N> int h(double);
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int g(T t) { return h<sizeof(sizeof(f(t)))>(t); }
