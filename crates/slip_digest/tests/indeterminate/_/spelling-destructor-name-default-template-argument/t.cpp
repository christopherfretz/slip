// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11: the same in a default template argument (`spelling/destructor-name`).
template<int N> struct Q { using I = int; enum E { e1 }; };
template<int N> struct X { };
template<int N> struct Bk { };
int *p;
X<8> *q;
int f(int);
#ifdef B
long f(long);
#endif
template<class T, class = decltype(q->~X<sizeof(sizeof(f(T())))>())> void g(T);
