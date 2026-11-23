// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, templated: a local class in a generic lambda at namespace scope (`spelling/using-enum`).
template<int N> struct Q { using I = int; enum E { e1 }; };
template<int N> struct X { };
int *p;
X<8> *q;
int f(int);
#ifdef B
long f(long);
#endif
auto l = [](auto x) { using T = decltype(x); struct L { void k() { using enum Q<sizeof(sizeof(f(T())))>::E; } }; };
