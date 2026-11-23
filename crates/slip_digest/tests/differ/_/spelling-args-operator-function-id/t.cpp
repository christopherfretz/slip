#include "side.h"
// §4.4 "Spellings", rule 11, audit (c): an operator-function-id with explicit template arguments: side B adds `f(long)`, which the found set of `f` there does not hold.
struct X {};
template<int N> int operator+(X, int);
template<int N> int operator+(X, double);
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int g(T t, X x) { return operator+<sizeof(sizeof(f(t)))>(x, t); }
