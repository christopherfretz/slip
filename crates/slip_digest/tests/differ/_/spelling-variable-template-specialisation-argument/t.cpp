#include "side.h"
// §4.4 "Spellings", rule 1: the written arguments of a resolved reference to a variable template specialisation, read as written (the reviewer's w12; kills N21).
template<int N> struct A {};
template<int N> constexpr int vt = 1;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> A<vt<sizeof(sizeof(f(t)))>>;
