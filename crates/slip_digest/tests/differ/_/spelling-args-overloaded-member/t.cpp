#include "side.h"
// §4.4 "Spellings", rule 11, audit (c): explicit template arguments of an overloaded member (`member_overload`): side B adds `f(long)`, which the found set of `f` there does not hold.
struct M { template<int N> static int k(int); template<int N> static int k(double); };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int g(T t, M m) { return m.k<sizeof(sizeof(f(t)))>(t); }
